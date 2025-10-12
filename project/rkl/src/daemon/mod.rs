pub mod sync_loop;
use sync_loop::SyncLoop;
pub mod static_pods;
//mod status_access;
pub mod client;
use client::init_crypto;

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    init_crypto();
    //tokio::spawn(status_access::init());
    tokio::spawn(async {
        if let Err(e) = client::run_forever().await {
            eprintln!("[daemon] rks client exited with error: {e:?}");
        }
    });
    tokio::spawn(async {
        let sync_loop = SyncLoop::default().register_event(static_pods::handler);
        sync_loop.run().await;
        eprintln!("[daemon] sync_loop exited unexpectedly");
    });
    tokio::signal::ctrl_c().await?;
    println!("[daemon] received Ctrl-C, shutting down");
    Ok(())
}
