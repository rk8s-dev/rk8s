use slayerfs::chuck::chunk::ChunkLayout;
use slayerfs::sdk_fs::{self, DynClient, OpenOptions};
use slayerfs::vfs::sdk::LocalClient;
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
    let client: DynClient = Arc::new(cli);

    sdk_fs::create_dir_all(Arc::clone(&client), "/demo").await?;

    // Create/truncate + write
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(true);
    let f = opts.open(Arc::clone(&client), "/demo/hello.txt").await?;
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
    let af = a.open(Arc::clone(&client), "/demo/hello.txt").await?;
    af.write_all(b"!").await?;

    let s = sdk_fs::read_to_string(Arc::clone(&client), "/demo/hello.txt").await?;
    println!("after append: {s}");

    // Directory listing
    let mut rd = sdk_fs::read_dir(Arc::clone(&client), "/demo").await?;
    while let Some(ent) = rd.next_entry().await? {
        println!("entry: {} ({})", ent.file_name(), ent.path());
    }

    Ok(())
}
