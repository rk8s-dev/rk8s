use slayerfs::sdk_fs::{Client, OpenOptions};
use slayerfs::{ChunkLayout, LocalClient};
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let root: PathBuf = if let Some(p) = args.next() {
        PathBuf::from(p)
    } else {
        std::env::temp_dir().join("slayerfs-sdk-fs-demo")
    };

    let layout = ChunkLayout::default();
    let cli = LocalClient::new_local(&root, layout)
        .await
        .expect("init LocalClient");
    let client = Client::new(Arc::new(cli));

    println!("=== Basic File Operations ===");
    client.create_dir_all("/demo").await?;

    // Create/truncate + write
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(true);
    let f = client.open(&opts, "/demo/hello.txt").await?;
    f.write_all(b"hello").await?;
    f.write_all(b" world").await?;

    // Seek + read
    f.seek(SeekFrom::Start(0)).await?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).await?;
    println!("read back: {}", String::from_utf8_lossy(&buf));

    // Append
    let mut a = OpenOptions::new();
    a.append(true).create(true);
    let af = client.open(&a, "/demo/hello.txt").await?;
    af.write_all(b"!").await?;

    let s = client.read_to_string("/demo/hello.txt").await?;
    println!("after append: {s}");

    println!("\n=== Metadata Operations ===");
    // Check exists
    println!(
        "exists /demo/hello.txt: {}",
        client.exists("/demo/hello.txt").await
    );
    println!(
        "exists /nonexistent: {}",
        client.exists("/nonexistent").await
    );

    // Get metadata
    let meta = client.metadata("/demo/hello.txt").await?;
    println!("file size: {} bytes", meta.len());
    println!("is_file: {}, is_dir: {}", meta.is_file(), meta.is_dir());
    println!(
        "mode: {:o}, uid: {}, gid: {}",
        meta.mode(),
        meta.uid(),
        meta.gid()
    );
    println!("mtime: {}", meta.mtime());

    println!("\n=== Directory Operations ===");
    // Create nested directories
    client.create_dir_all("/demo/sub/nested").await?;
    client
        .write("/demo/sub/nested/file.txt", b"nested content")
        .await?;

    // Directory listing
    let mut rd = client.read_dir("/demo").await?;
    println!("Contents of /demo:");
    while let Some(ent) = rd.next_entry().await? {
        let type_str = if ent.file_type().is_dir() {
            "DIR"
        } else {
            "FILE"
        };
        println!("  [{type_str}] {} ({})", ent.file_name(), ent.path());
    }

    // Copy file
    client
        .copy("/demo/hello.txt", "/demo/hello_copy.txt")
        .await?;
    println!("\nCopied /demo/hello.txt to /demo/hello_copy.txt");

    // Rename file
    client
        .rename("/demo/hello_copy.txt", "/demo/hello_renamed.txt")
        .await?;
    println!("Renamed to /demo/hello_renamed.txt");

    println!("\n=== Cleanup with remove_dir_all ===");
    // Clean up nested directory structure
    client.remove_dir_all("/demo/sub").await?;
    println!("Removed /demo/sub recursively");

    // Verify it's gone
    println!("exists /demo/sub: {}", client.exists("/demo/sub").await);

    // Clean up remaining files
    client.remove_file("/demo/hello.txt").await?;
    client.remove_file("/demo/hello_renamed.txt").await?;
    client.remove_dir("/demo").await?;
    println!("Cleanup complete!");

    Ok(())
}
