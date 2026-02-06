mod cadapter;
mod chuck;
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

use clap::{Args, Parser, Subcommand, ValueEnum};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::chunk::{ChunkLayout, DEFAULT_BLOCK_SIZE, DEFAULT_CHUNK_SIZE};
use crate::chuck::store::ObjectBlockStore;
use crate::fuse::mount::mount_vfs_unprivileged;
use crate::meta::MetaStore;
use crate::meta::config::{CacheConfig, ClientOptions, Config, DatabaseConfig, DatabaseType};
use crate::meta::factory::MetaStoreFactory;
use crate::meta::stores::{DatabaseMetaStore, EtcdMetaStore};
use crate::vfs::fs::VFS;

#[derive(Parser)]
#[command(name = "slayerfs", version, about = "SlayerFS FUSE CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Mount SlayerFS via FUSE.
    Mount(MountArgs),
}

#[derive(Args)]
struct MountArgs {
    /// Directory to mount the filesystem.
    #[arg(value_name = "MOUNT_POINT")]
    mount_point: PathBuf,

    /// Local directory used as object storage backend.
    #[arg(long, value_name = "DIR", default_value = "./data")]
    data_dir: PathBuf,

    /// Metadata backend (sqlx or etcd).
    #[arg(long, value_enum, default_value_t = MetaBackendKind::Sqlx)]
    meta_backend: MetaBackendKind,

    /// Metadata backend URL (sqlx only, e.g. sqlite::memory: or postgres://...).
    #[arg(long, value_name = "URL", default_value = "sqlite::memory:")]
    meta_url: String,

    /// Etcd endpoint URLs (comma-separated).
    #[arg(long, value_name = "URLS", value_delimiter = ',')]
    meta_etcd_urls: Vec<String>,

    /// Chunk size in bytes.
    #[arg(long, default_value_t = DEFAULT_CHUNK_SIZE)]
    chunk_size: u64,

    /// Block size in bytes.
    #[arg(long, default_value_t = DEFAULT_BLOCK_SIZE)]
    block_size: u32,
}

#[derive(ValueEnum, Clone, Copy)]
enum MetaBackendKind {
    Sqlx,
    Etcd,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let result = match cli.cmd {
        Command::Mount(args) => mount_cmd(args).await,
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

async fn mount_cmd(args: MountArgs) -> anyhow::Result<()> {
    if !args.mount_point.exists() {
        std::fs::create_dir_all(&args.mount_point)?;
    }
    if !args.mount_point.is_dir() {
        anyhow::bail!("mount point must be a directory");
    }

    if !args.data_dir.exists() {
        std::fs::create_dir_all(&args.data_dir)?;
    }
    if !args.data_dir.is_dir() {
        anyhow::bail!("data dir must be a directory");
    }

    if args.chunk_size < args.block_size as u64 {
        anyhow::bail!("chunk_size must be >= block_size");
    }

    let layout = ChunkLayout {
        chunk_size: args.chunk_size,
        block_size: args.block_size,
    };

    let client = ObjectClient::new(LocalFsBackend::new(&args.data_dir));
    let store = ObjectBlockStore::new(client);
    let meta_store = create_meta_store(&args).await?;

    let fs = VFS::new(layout, store, meta_store)
        .await
        .map_err(anyhow::Error::from)?;
    let handle = mount_vfs_unprivileged(fs, &args.mount_point).await?;

    println!("mounted at {}", args.mount_point.display());
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

async fn create_meta_store(args: &MountArgs) -> anyhow::Result<Arc<dyn MetaStore>> {
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
                anyhow::bail!("--meta-etcd-urls must be set when --meta-backend etcd");
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
