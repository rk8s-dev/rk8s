use clap::Parser;
use libfuse_fs::overlayfs::mount_fs;
use tokio::signal;

#[derive(Parser)]
#[command(author, version, about)]
struct Args {
    #[arg(long)]
    mountpoint: String,
    #[arg(long)]
    upperdir: String,
    #[arg(long)]
    lowerdir: Vec<String>,
    #[arg(long, default_value_t = false)]
    not_unprivileged: bool,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let mut mount_handle = mount_fs(
        args.mountpoint,
        args.upperdir,
        args.lowerdir,
        args.not_unprivileged,
    )
    .await;
    let handle = &mut mount_handle;

    tokio::select! {
        res = handle => res.unwrap(),
        _ = signal::ctrl_c() => {
            mount_handle.unmount().await.unwrap()
        }
    }
}
