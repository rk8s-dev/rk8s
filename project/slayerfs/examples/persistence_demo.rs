use clap::Parser;
use slayerfs::cadapter::client::ObjectClient;
use slayerfs::cadapter::localfs::LocalFsBackend;
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
    #[arg(short, long)]
    config: PathBuf,

    /// Mount point path
    #[arg(short, long, default_value = "/tmp/mount")]
    mount: PathBuf,

    /// Backend storage path
    #[arg(short, long, default_value = "/tmp/db")]
    storage: PathBuf,
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
                "postgres" => {
                    println!("Using PostgreSQL database backend");
                    Ok(config_content.to_string())
                }
                "etcd" => {
                    println!("Using etcd distributed backend");
                    Ok(config_content.to_string())
                }
                "redis" => {
                    println!("Using Redis metadata backend");
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
        let config_file = args.config;
        let mount_point = args.mount;
        let backend_storage = args.storage;

        println!("=== SlayerFS Persistence Demo ===");
        println!("Config file: {}", config_file.display());
        println!("Data storage: {}", backend_storage.display());
        println!("Mount point: {}", mount_point.display());
        println!();

        if !config_file.exists() {
            eprintln!(
                "Error: Config file {} does not exist",
                config_file.display()
            );
            eprintln!();
            eprintln!("Please create a config file or use existing ones:");
            eprintln!("  slayerfs-sqlite.yml   # SQLite database backend");
            eprintln!("  slayerfs-etcd.yml    # etcd distributed backend");
            std::process::exit(1);
        }

        std::fs::create_dir_all(&mount_point).map_err(|e| {
            format!(
                "Cannot create mount point directory {}: {}",
                mount_point.display(),
                e
            )
        })?;
        std::fs::create_dir_all(&backend_storage).map_err(|e| {
            format!(
                "Cannot create storage directory {}: {}",
                backend_storage.display(),
                e
            )
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(&mount_point, permissions)?;
        }

        let layout = ChunkLayout::default();
        let client = ObjectClient::new(LocalFsBackend::new(&backend_storage));
        let store = ObjectBlockStore::new(client.clone());

        let config_content = std::fs::read_to_string(&config_file)
            .map_err(|e| format!("Cannot read config file: {}", e))?;

        let meta_config_dir = backend_storage.join(".slayerfs");
        std::fs::create_dir_all(&meta_config_dir)?;

        let target_config_path = meta_config_dir.join("slayerfs.yml");
        let processed_config = process_config_for_backend(&config_content, &meta_config_dir)?;
        std::fs::write(&target_config_path, processed_config)?;

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

        let fs = VFS::new(layout, store, meta_store.clone())
            .await
            .expect("create VFS");

        println!("Starting garbage collector...");

        let gc_handle = tokio::spawn({
            let meta_store = meta_store.clone();
            let object_client = client.clone();
            async move {
                use slayerfs::daemon::worker::start_gc;
                use std::sync::Arc;

                start_gc(meta_store, Arc::new(object_client), None).await;
            }
        });

        println!("Mounting filesystem...");

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

        let handle = mount_vfs_unprivileged(fs, &mount_point)
            .await
            .map_err(|e| format!("Failed to mount filesystem: {}", e))?;

        println!(
            "SlayerFS successfully mounted at: {}",
            mount_point.display()
        );

        println!("Press Ctrl+C to exit and unmount filesystem...");

        tokio::select! {
            _ = signal::ctrl_c() => {
                println!("\nUnmounting filesystem...");

                handle.unmount().await?;
                println!("Filesystem unmounted");
                gc_handle.abort();
                let _ = gc_handle.await;
            }
        }

        Ok(())
    }
}
