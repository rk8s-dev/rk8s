use clap::Parser;
use slayerfs::cadapter::client::ObjectClient;
use slayerfs::cadapter::s3::{S3Backend, S3Config};
use slayerfs::chuck::chunk::ChunkLayout;
use slayerfs::chuck::store::ObjectBlockStore;
use slayerfs::fuse::mount::mount_vfs_unprivileged;
use slayerfs::meta::MetaStore;
use slayerfs::meta::config::DatabaseType;
use slayerfs::meta::factory::MetaStoreFactory;
use slayerfs::meta::stores::{DatabaseMetaStore, EtcdMetaStore, RedisMetaStore};
use slayerfs::vfs::fs::VFS;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::signal;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Configuration file path (e.g. slayerfs-sqlite.yml, slayerfs-etcd.yml)
    #[arg(short, long, default_value = "./slayerfs-sqlite.yml")]
    config: PathBuf,

    /// Mount point path
    #[arg(short, long, default_value = "/tmp/mount")]
    mount: PathBuf,

    /// Directory used to persist metadata artifacts (SQLite database, config, etc.)
    #[arg(long, default_value = "./slayerfs-meta")]
    meta_dir: PathBuf,

    /// Target S3 bucket name (can be overridden by S3_BUCKET env var)
    #[arg(long)]
    bucket: Option<String>,

    /// S3-compatible endpoint URL (can be overridden by S3_ENDPOINT env var)
    #[arg(long)]
    endpoint: Option<String>,

    /// Optional AWS region (can be overridden by AWS_REGION env var)
    #[arg(long)]
    region: Option<String>,
}

/// Process config file and adjust SQLite path to absolute path
fn process_config_for_backend(
    config_content: &str,
    meta_dir: &std::path::Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let config: serde_yaml::Value = serde_yaml::from_str(config_content)?;

    if let Some(database) = config.get("database") {
        if let Some(db_type) = database.get("type").and_then(|t| t.as_str()) {
            match db_type {
                "sqlite" => {
                    let db_path = meta_dir.join("metadata.db");
                    let sqlite_url = format!("sqlite://{}?mode=rwc", db_path.display());

                    let processed_config = format!(
                        r#"database:
  type: sqlite
  url: "{}"
"#,
                        sqlite_url
                    );

                    println!("SQLite database path: {}", db_path.display());
                    Ok(processed_config)
                }
                "postgres" | "etcd" => {
                    println!("Using configured database backend: {}", db_type);
                    Ok(config_content.to_string())
                }
                _ => Err(format!("Unsupported database type: {}", db_type).into()),
            }
        } else {
            Err("Missing database.type field in config file".into())
        }
    } else {
        Err("Missing database configuration in config file".into())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载 .env 文件
    dotenv::dotenv().ok();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .or_else(|_| tracing_subscriber::EnvFilter::try_new("info"))
        .unwrap();

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_ansi(true))
        .with(filter)
        .init();

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("This demo only works on Linux (requires FUSE support).");
        eprintln!("If you're on Windows, please run under WSL/WSL2 or Linux host.");
        std::process::exit(2);
    }

    #[cfg(target_os = "linux")]
    {
        let args = Args::parse();

        // 从环境变量获取配置，命令行参数优先
        let bucket = args
            .bucket
            .or_else(|| std::env::var("S3_BUCKET").ok())
            .ok_or("S3 bucket must be specified via --bucket or S3_BUCKET env var")?;

        let endpoint = args
            .endpoint
            .or_else(|| std::env::var("S3_ENDPOINT").ok())
            .unwrap_or_else(|| "http://127.0.0.1:9000".to_string());

        let region = args.region.or_else(|| std::env::var("AWS_REGION").ok());

        println!("=== SlayerFS Persistence + S3 Demo ===");
        println!("Environment variables loaded from .env file");
        println!("Config file: {}", args.config.display());
        println!("Metadata dir: {}", args.meta_dir.display());
        println!("Mount point: {}", args.mount.display());
        println!("S3 bucket: {}", bucket);
        println!("S3 endpoint: {}", endpoint);
        if let Some(ref region) = region {
            println!("S3 region: {}", region);
        }
        println!();
        let config_file = args.config;
        let mount_point = args.mount;
        let meta_dir = args.meta_dir;

        println!("=== SlayerFS Persistence + S3 Demo ===");
        println!("Environment variables loaded from .env file");
        println!("Config file: {}", config_file.display());
        println!("Metadata dir: {}", meta_dir.display());
        println!("Mount point: {}", mount_point.display());
        println!("S3 bucket: {}", bucket);
        println!("S3 endpoint: {}", endpoint);
        if let Some(ref region) = region {
            println!("S3 region: {}", region);
        }
        println!();

        if !config_file.exists() {
            eprintln!(
                "Error: Config file {} does not exist",
                config_file.display()
            );
            std::process::exit(1);
        }

        std::fs::create_dir_all(&mount_point).map_err(|e| {
            format!(
                "Cannot create mount point directory {}: {}",
                mount_point.display(),
                e
            )
        })?;
        std::fs::create_dir_all(&meta_dir).map_err(|e| {
            format!(
                "Cannot create metadata directory {}: {}",
                meta_dir.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&mount_point, permissions)?;
        }

        if std::fs::metadata(&mount_point)
            .map(|m| !m.is_dir())
            .unwrap_or(false)
        {
            return Err(format!("Mount point {} is not a directory", mount_point.display()).into());
        }

        if let Ok(entries) = std::fs::read_dir(&mount_point) {
            let count = entries.count();
            if count > 0 {
                eprintln!(
                    "Warning: Mount point {} is not empty, may already be mounted",
                    mount_point.display()
                );
                eprintln!("Please unmount first or use an empty directory");
                eprintln!("Try: fusermount -u {}", mount_point.display());
                return Err("Mount point not empty".into());
            }
        }

        let config_content = std::fs::read_to_string(&config_file)
            .map_err(|e| format!("Cannot read config file: {}", e))?;

        let meta_work_dir = meta_dir.join(".slayerfs");
        std::fs::create_dir_all(&meta_work_dir)?;

        let target_config_path = meta_work_dir.join("slayerfs.yml");
        let processed_config = process_config_for_backend(&config_content, &meta_work_dir)?;
        std::fs::write(&target_config_path, processed_config)?;

        // AWS 环境变量已从 .env 文件加载

        let config = slayerfs::meta::config::Config::from_file(&target_config_path)
            .map_err(|e| format!("Failed to load config file: {}", e))?;

        let meta_store: Arc<dyn MetaStore> = match &config.database.db_config {
            DatabaseType::Sqlite { .. } | DatabaseType::Postgres { .. } => {
                MetaStoreFactory::<DatabaseMetaStore>::create_from_config(config.clone())
                    .await
                    .map_err(|e| format!("Failed to initialize metadata storage: {}", e))?
                    .store()
            }
            DatabaseType::Redis { .. } => {
                MetaStoreFactory::<RedisMetaStore>::create_from_config(config.clone())
                    .await
                    .map_err(|e| format!("Failed to initialize metadata storage: {}", e))?
                    .store()
            }
            DatabaseType::Etcd { .. } => {
                MetaStoreFactory::<EtcdMetaStore>::create_from_config(config.clone())
                    .await
                    .map_err(|e| format!("Failed to initialize metadata storage: {}", e))?
                    .store()
            }
        };

        let mut s3_config = S3Config {
            bucket: bucket.clone(),
            region: region.clone().or_else(|| Some("us-east-1".to_string())),
            part_size: 16 * 1024 * 1024,
            max_concurrency: 8,
            ..Default::default()
        };
        s3_config.endpoint = Some(endpoint.clone());
        s3_config.force_path_style = true;

        let s3_backend = S3Backend::with_config(s3_config)
            .await
            .map_err(|e| format!("Failed to create S3 backend: {}", e))?;
        let object_client = ObjectClient::new(s3_backend);
        let block_store = ObjectBlockStore::new(object_client);

        let layout = ChunkLayout::default();
        let fs = VFS::new(layout, block_store, meta_store)
            .await
            .map_err(|e| format!("Failed to create VFS: {}", e))?;

        println!("Mounting filesystem...");

        let handle = mount_vfs_unprivileged(fs, &mount_point)
            .await
            .map_err(|e| format!("Failed to mount filesystem: {}", e))?;

        println!(
            "SlayerFS with S3 backend successfully mounted at: {}",
            mount_point.display()
        );
        println!("Press Ctrl+C to exit and unmount filesystem...");

        signal::ctrl_c().await?;
        println!("\nUnmounting filesystem...");

        handle.unmount().await?;
        println!("Filesystem unmounted");
        Ok(())
    }
}
