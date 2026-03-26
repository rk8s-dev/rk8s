mod cadapter;
mod chunk;
mod daemon;
#[allow(dead_code)]
mod fs;
mod fuse;
mod meta;
mod posix;
mod utils;
#[allow(dead_code)]
mod vfs;

#[cfg(all(feature = "jemalloc", target_os = "linux"))]
use tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "jemalloc", target_os = "linux"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[cfg(feature = "profiling")]
use std::fs::File;
#[cfg(feature = "profiling")]
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(feature = "profiling")]
use std::sync::{LazyLock, Mutex as StdMutex};

pub mod config;
use config::*;

use clap::Parser;
#[cfg(not(feature = "profiling"))]
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::cadapter::s3::{S3Backend, S3Config};
use crate::chunk::layout::ChunkLayout;
use crate::chunk::store::{BlockStore, ObjectBlockStore};
use crate::fuse::mount::mount_vfs_unprivileged;
use crate::meta::MetaStore;
use crate::meta::config::{CacheConfig, ClientOptions, Config, DatabaseConfig, DatabaseType};
use crate::meta::factory::MetaStoreFactory;
use crate::meta::stores::{DatabaseMetaStore, EtcdMetaStore, RedisMetaStore};
use crate::vfs::fs::VFS;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let result = match cli.cmd {
        Command::Mount(args) => mount_cmd(MountConfig::from_sources(args)?).await,
    };
    shutdown_flame();
    shutdown_chrome();
    result
}

#[cfg(feature = "profiling")]
fn init_tracing() {
    let flame_layer = std::env::var("SLAYERFS_TRACE_FLAME").ok().and_then(|path| {
        let path_for_log = path.clone();
        match tracing_flame::FlameLayer::with_file(path) {
            Ok((layer, guard)) => {
                let layer = layer.with_empty_samples(false).with_threads_collapsed(true);
                eprintln!("[slayerfs] tracing-flame enabled: {}", path_for_log);
                register_flame_guard(guard);
                Some(layer)
            }
            Err(err) => {
                eprintln!(
                    "[slayerfs] failed to enable tracing-flame for {}: {err}",
                    path_for_log
                );
                None
            }
        }
    });
    let chrome_layer = std::env::var("SLAYERFS_TRACE_CHROME").ok().map(|path| {
        let path_for_log = path.clone();
        let builder = tracing_chrome::ChromeLayerBuilder::new()
            .file(path)
            .trace_style(tracing_chrome::TraceStyle::Async)
            .include_args(true);
        let (layer, guard) = builder.build();
        eprintln!("[slayerfs] tracing-chrome enabled: {}", path_for_log);
        register_chrome_guard(guard);
        layer
    });
    let env_filter = tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "slayerfs=info".to_string()),
    );
    let console_layer = std::env::var_os("TOKIO_CONSOLE").map(|_| console_subscriber::spawn());

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().pretty())
        .with(env_filter)
        .with(flame_layer)
        .with(chrome_layer)
        .with(console_layer)
        .init();
}

#[cfg(not(feature = "profiling"))]
fn init_tracing() {
    let env_filter = tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "slayerfs=info".to_string()),
    );

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .pretty()
                .with_span_events(FmtSpan::CLOSE),
        )
        .with(env_filter)
        .init();
}

async fn mount_cmd(args: MountConfig) -> anyhow::Result<()> {
    if !args.mount_point.exists() {
        std::fs::create_dir_all(&args.mount_point)?;
    }
    if !args.mount_point.is_dir() {
        anyhow::bail!("mount point must be a directory");
    }

    if args.chunk_size < args.block_size as u64 {
        anyhow::bail!("chunk_size must be >= block_size");
    }

    let layout = ChunkLayout {
        chunk_size: args.chunk_size,
        block_size: args.block_size,
    };

    let meta_store = create_meta_store(&args).await?;

    match args.data_backend {
        DataBackendKind::LocalFs => {
            let client = create_localfs_client(&args)?;
            let store = ObjectBlockStore::new(client);
            mount_with_store(layout, store, meta_store, &args.mount_point).await
        }
        DataBackendKind::S3 => {
            let client = create_s3_client(&args).await?;
            let store = ObjectBlockStore::new(client);
            mount_with_store(layout, store, meta_store, &args.mount_point).await
        }
    }
}

fn create_localfs_client(args: &MountConfig) -> anyhow::Result<ObjectClient<LocalFsBackend>> {
    if !args.data_dir.exists() {
        std::fs::create_dir_all(&args.data_dir)?;
    }
    if !args.data_dir.is_dir() {
        anyhow::bail!("data dir must be a directory");
    }
    Ok(ObjectClient::new(LocalFsBackend::new(&args.data_dir)))
}

async fn create_s3_client(args: &MountConfig) -> anyhow::Result<ObjectClient<S3Backend>> {
    let bucket = args
        .s3_bucket
        .clone()
        .ok_or_else(|| anyhow::anyhow!("s3 bucket must be set when data backend is s3"))?;

    if args.s3_part_size == 0 {
        anyhow::bail!("--s3-part-size must be greater than 0");
    }
    if args.s3_max_concurrency == 0 {
        anyhow::bail!("--s3-max-concurrency must be greater than 0");
    }

    let config = S3Config {
        bucket,
        region: args.s3_region.clone(),
        part_size: args.s3_part_size,
        max_concurrency: args.s3_max_concurrency,
        endpoint: args.s3_endpoint.clone(),
        force_path_style: args.s3_force_path_style,
        ..Default::default()
    };

    let backend = S3Backend::with_config(config).await?;
    Ok(ObjectClient::new(backend))
}

async fn mount_with_store<S>(
    layout: ChunkLayout,
    store: S,
    meta_store: Arc<dyn MetaStore>,
    mount_point: &PathBuf,
) -> anyhow::Result<()>
where
    S: BlockStore + Send + Sync + 'static,
{
    let fs = VFS::new(layout, store, meta_store)
        .await
        .map_err(anyhow::Error::from)?;
    let handle = mount_vfs_unprivileged(fs, mount_point).await?;

    println!("mounted at {}", mount_point.display());
    tokio::signal::ctrl_c().await?;
    println!("unmounting...");
    handle.unmount().await?;
    Ok(())
}

#[cfg(feature = "profiling")]
static FLAME_GUARD: LazyLock<StdMutex<Option<tracing_flame::FlushGuard<BufWriter<File>>>>> =
    LazyLock::new(|| StdMutex::new(None));
#[cfg(feature = "profiling")]
static CHROME_GUARD: LazyLock<StdMutex<Option<tracing_chrome::FlushGuard>>> =
    LazyLock::new(|| StdMutex::new(None));

#[cfg(feature = "profiling")]
fn register_flame_guard(guard: tracing_flame::FlushGuard<BufWriter<File>>) {
    if let Ok(mut slot) = FLAME_GUARD.lock() {
        *slot = Some(guard);
    }
}

#[cfg(feature = "profiling")]
fn shutdown_flame() {
    if let Ok(mut slot) = FLAME_GUARD.lock()
        && let Some(guard) = slot.take()
        && let Err(err) = guard.flush()
    {
        eprintln!("tracing-flame flush failed: {err}");
    }
}

#[cfg(not(feature = "profiling"))]
fn shutdown_flame() {}

#[cfg(feature = "profiling")]
fn register_chrome_guard(guard: tracing_chrome::FlushGuard) {
    if let Ok(mut slot) = CHROME_GUARD.lock() {
        *slot = Some(guard);
    }
}

#[cfg(feature = "profiling")]
fn shutdown_chrome() {
    if let Ok(mut slot) = CHROME_GUARD.lock() {
        slot.take();
    }
}

#[cfg(not(feature = "profiling"))]
fn shutdown_chrome() {}

async fn create_meta_store(args: &MountConfig) -> anyhow::Result<Arc<dyn MetaStore>> {
    match args.meta_backend {
        MetaBackendKind::Sqlx => {
            let client = ClientOptions::default();

            let config = Config {
                database: DatabaseConfig {
                    db_config: database_type_from_url(&args.meta_url),
                },
                cache: CacheConfig::default(),
                client,
            };
            let handle = MetaStoreFactory::<DatabaseMetaStore>::create_from_config(config).await?;
            Ok(handle.store() as Arc<dyn MetaStore>)
        }
        MetaBackendKind::Etcd => {
            if args.meta_etcd_urls.is_empty() {
                anyhow::bail!("etcd urls must be set when meta backend is etcd");
            }

            let client = ClientOptions::default();

            let config = Config {
                database: DatabaseConfig {
                    db_config: DatabaseType::Etcd {
                        urls: args.meta_etcd_urls.clone(),
                    },
                },
                cache: CacheConfig::default(),
                client,
            };
            let handle = MetaStoreFactory::<EtcdMetaStore>::create_from_config(config).await?;
            Ok(handle.store() as Arc<dyn MetaStore>)
        }
        MetaBackendKind::Redis => {
            let client = ClientOptions::default();

            let config = Config {
                database: DatabaseConfig {
                    db_config: DatabaseType::Redis {
                        url: args.meta_url.clone(),
                    },
                },
                cache: CacheConfig::default(),
                client,
            };
            let handle = MetaStoreFactory::<RedisMetaStore>::create_from_config(config).await?;
            Ok(handle.store() as Arc<dyn MetaStore>)
        }
    }
}

fn database_type_from_url(url: &str) -> DatabaseType {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        DatabaseType::Postgres {
            url: url.to_string(),
        }
    } else {
        DatabaseType::Sqlite {
            url: url.to_string(),
        }
    }
}
