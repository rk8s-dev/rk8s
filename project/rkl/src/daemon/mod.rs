pub mod sync_loop;
use sync_loop::SyncLoop;
pub mod static_pods;

#[tokio::main]
pub async fn main() -> Result<(), anyhow::Error> {
    let sync_loop = SyncLoop::new().register_event(static_pods::handler);
    sync_loop.run().await;
    Ok(())
}
