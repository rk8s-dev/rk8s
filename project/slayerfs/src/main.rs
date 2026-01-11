mod cadapter;
mod chuck;
mod daemon;
mod fuse;
mod meta;
mod utils;
mod vfs;

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand, ValueEnum};

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
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "slayerfs=info".to_string()))
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Command::Mount(args) => mount_cmd(args).await?,
    }

    Ok(())
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

async fn create_meta_store(args: &MountArgs) -> anyhow::Result<Arc<dyn MetaStore>> {
    match args.meta_backend {
        MetaBackendKind::Sqlx => {
            let config = Config {
                database: DatabaseConfig {
                    db_config: database_type_from_url(&args.meta_url),
                },
                cache: CacheConfig::default(),
                client: ClientOptions::default(),
            };
            let handle = MetaStoreFactory::<DatabaseMetaStore>::create_from_config(config).await?;
            Ok(handle.store() as Arc<dyn MetaStore>)
        }
        MetaBackendKind::Etcd => {
            if args.meta_etcd_urls.is_empty() {
                anyhow::bail!("--meta-etcd-urls must be set when --meta-backend etcd");
            }
            let config = Config {
                database: DatabaseConfig {
                    db_config: DatabaseType::Etcd {
                        urls: args.meta_etcd_urls.clone(),
                    },
                },
                cache: CacheConfig::default(),
                client: ClientOptions::default(),
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
