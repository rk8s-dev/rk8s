mod cadapter;
mod chuck;
mod daemon;
mod fuse;
mod meta;
mod vfs;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("demo-localfs") => {
            let dir = match args.next() {
                Some(p) => p,
                None => {
                    eprintln!("Usage: slayerfs demo-localfs <dir>");
                    std::process::exit(2);
                }
            };
            match vfs::demo::e2e_localfs_demo(dir).await {
                Ok(()) => println!("demo-localfs: OK"),
                Err(e) => {
                    eprintln!("demo-localfs failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!("Hello, I'm SlayerFS!\nUsage:\n  slayerfs demo-localfs <dir>");
        }
    }
}
