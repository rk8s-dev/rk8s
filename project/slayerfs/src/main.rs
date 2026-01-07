mod cadapter;
mod chuck;
mod daemon;
mod fuse;
mod meta;
mod utils;
mod vfs;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let _ = args.next();
    println!("Hello, I'm SlayerFS!");
}
