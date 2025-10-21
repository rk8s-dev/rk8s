pub mod sync_loop;
use sync_loop::SyncLoop;
pub mod client;
pub mod static_pods;

use crate::commands::pod::TLSConnectionArgs;
use tracing::{error, info};

pub fn main(tls_cfg: TLSConnectionArgs) -> Result<(), anyhow::Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            //tokio::spawn(status_access::init());

            tokio::spawn(async move {
                if let Err(e) = client::run_forever(tls_cfg).await {
                    error!("[daemon] rks client exited with error: {e:?}");
                }
            });
            tokio::spawn(async {
                let sync_loop = SyncLoop::default().register_event(static_pods::handler);
                sync_loop.run().await;
                error!("[daemon] sync_loop exited unexpectedly");
            });
            tokio::signal::ctrl_c().await?;
            info!("[daemon] received Ctrl-C, shutting down");
            Ok(())
        })
}
