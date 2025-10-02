use clap::Parser;
use slayerfs::cadapter::client::ObjectClient;
use slayerfs::cadapter::localfs::LocalFsBackend;
use slayerfs::chuck::chunk::ChunkLayout;
use slayerfs::chuck::store::ObjectBlockStore;
use slayerfs::fuse::mount::mount_vfs_unprivileged;
use slayerfs::meta::MetaStore;
use slayerfs::vfs::fs::VFS;
use std::path::PathBuf;
use tokio::signal;

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
                "xline" => {
                    println!("Using xline distributed backend");
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
    env_logger::init();

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("This demo only works on Linux (requires FUSE support).");
        eprintln!("If you're on Windows, please run under WSL/WSL2 or Linux host.");
        std::process::exit(2);
    }

    #[cfg(target_os = "linux")]
    {
        let args = Args::parse();
        let program_name = std::env::args()
            .next()
            .unwrap_or_else(|| "persistence_demo".to_string());

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
            eprintln!("  slayerfs-xline.yml    # xline distributed backend");
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

        println!("Starting SlayerFS...");
        println!("Backend storage location: {}", backend_storage.display());
        let layout = ChunkLayout::default();
        let client = ObjectClient::new(LocalFsBackend::new(&backend_storage));
        let store = ObjectBlockStore::new(client);

        println!("Reading config file: {}", config_file.display());
        let config_content = std::fs::read_to_string(&config_file)
            .map_err(|e| format!("Cannot read config file: {}", e))?;

        let meta_config_dir = backend_storage.join(".slayerfs");
        std::fs::create_dir_all(&meta_config_dir)?;

        let target_config_path = meta_config_dir.join("slayerfs.yml");
        let processed_config = process_config_for_backend(&config_content, &meta_config_dir)?;
        std::fs::write(&target_config_path, processed_config)?;

        println!("Initializing metadata storage...");
        let config = slayerfs::meta::config::Config::from_file(&target_config_path)
            .map_err(|e| format!("Failed to load config file: {}", e))?;
        let meta = slayerfs::meta::factory::MetaStoreFactory::create_from_config(config)
            .await
            .map_err(|e| format!("Failed to initialize metadata storage: {}", e))?;

        println!("Verifying metadata storage status...");
        let root_entries = meta
            .readdir(1)
            .await
            .map_err(|e| format!("readdir failed: {}", e))?;
        println!("Root directory contains {} entries", root_entries.len());
        if !root_entries.is_empty() {
            println!("Existing entries:");
            for entry in &root_entries {
                println!(
                    "  - {} (inode: {}, type: {:?})",
                    entry.name, entry.ino, entry.kind
                );
            }
            println!("Detected existing data, persistence working!");
        } else {
            println!("This is a new filesystem");
        }

        println!("Creating VFS instance...");
        let fs = VFS::new(layout, store, meta).await.expect("create VFS");
        println!("VFS instance created successfully");

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
        println!(
            "Mount point permissions: {:?}",
            std::fs::metadata(&mount_point)?.permissions()
        );

        println!();
        println!("You can now test file operations in another terminal:");
        println!("  ls {}", mount_point.display());
        println!("  echo 'hello world' > {}/test.txt", mount_point.display());
        println!("  cat {}/test.txt", mount_point.display());
        println!("  mkdir {}/testdir", mount_point.display());
        println!();
        println!("Persistence testing:");
        println!("  1. Create some files and directories");
        println!("  2. Press Ctrl+C to stop the program");
        println!("  3. Start the program again with same parameters:");
        println!(
            "     {} --config {} --mount {} --storage {}",
            program_name,
            config_file.display(),
            mount_point.display(),
            backend_storage.display()
        );
        println!("  4. Check if your data is still there!");
        println!();
        println!("Backend switching test:");
        println!("  1. Stop the program");
        println!("  2. Start with different config files:");
        println!(
            "     {} --config slayerfs-sqlite.yml --mount {} --storage {}",
            program_name,
            mount_point.display(),
            backend_storage.display()
        );
        println!(
            "     {} --config slayerfs-etcd.yml --mount {} --storage {}",
            program_name,
            mount_point.display(),
            backend_storage.display()
        );
        println!();
        println!("Press Ctrl+C to exit and unmount filesystem...");

        signal::ctrl_c().await?;
        println!("\nUnmounting filesystem...");

        handle.unmount().await?;
        println!("Filesystem unmounted");
        println!();
        println!("Tips:");
        println!("  - Re-run the same command to verify data persistence");
        println!("  - Try different config files to test multi-backend support");
        println!(
            "  - Restart command: {} --config {} --mount {} --storage {}",
            program_name,
            config_file.display(),
            mount_point.display(),
            backend_storage.display()
        );

        Ok(())
    }
}
