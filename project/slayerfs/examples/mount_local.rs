use slayerfs::cadapter::client::ObjectClient;
use slayerfs::cadapter::localfs::LocalFsBackend;
use slayerfs::chuck::chunk::ChunkLayout;
use slayerfs::chuck::store::ObjectBlockStore;
use slayerfs::fuse::mount::mount_vfs_unprivileged;
use slayerfs::meta::create_meta_store_from_url;
use slayerfs::vfs::fs::VFS;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!(
            "This mount demo only works on Linux (FUSE).\nIf you're on Windows, please run under WSL/WSL2 or a Linux host."
        );
        std::process::exit(2);
    }

    #[cfg(target_os = "linux")]
    {
        let mut args = std::env::args().skip(1);
        let data_dir = match args.next() {
            Some(p) => p,
            None => {
                eprintln!(
                    "Usage: slayerfs mount-local <data_dir> <mount_point>\n\n  data_dir: backend localfs data directory (will be created if not exist)\n  mount_point: empty directory to mount slayerfs\n\nExample:\n  slayerfs mount-local /tmp/slayerfs-data /tmp/slayerfs-mnt"
                );
                std::process::exit(2);
            }
        };
        let mount_point = match args.next() {
            Some(p) => p,
            None => {
                eprintln!("Usage: slayerfs mount-local <data_dir> <mount_point>");
                std::process::exit(2);
            }
        };

        // Prepare backend
        let layout = ChunkLayout::default();
        let client = ObjectClient::new(LocalFsBackend::new(std::path::Path::new(&data_dir)));
        let store = ObjectBlockStore::new(client);

        // Create meta store using memory SQLite
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .expect("create meta store");
        let fs = VFS::new(layout, store, meta).await.expect("create VFS");

        // Ensure mount point exists
        if let Err(e) = std::fs::create_dir_all(&mount_point) {
            eprintln!("create mount point failed: {e}");
            std::process::exit(1);
        }

        println!("Mounting SlayerFS at {mount_point} (backend: {data_dir})...");
        println!("Press Ctrl+C to unmount and exit.");
        let handle = match mount_vfs_unprivileged(fs, std::path::Path::new(&mount_point)).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!(
                    "mount failed: {e}\n\nHint: ensure you are on Linux with FUSE (fusermount3) available."
                );
                std::process::exit(1);
            }
        };

        // Wait for Ctrl+C
        if let Err(e) = tokio::signal::ctrl_c().await {
            eprintln!("signal error: {e}");
        }

        println!("Unmounting...");
        if let Err(e) = handle.unmount().await {
            eprintln!("unmount error: {e}");
        }
        println!("Bye.~");
    }
}
