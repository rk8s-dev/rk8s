pub mod client;
pub mod pod_worker;
pub mod probe;
pub mod static_pods;
pub mod status;
pub mod sync_loop;

use std::{env, sync::Arc};

//mod status_access;
use crate::{
    commands::pod::TLSConnectionArgs,
    daemon::{
        pod_worker::PodWorker,
        status::{
            pleg::PLEG,
            status_manager::{STATUS_MANAGER, StatusManager},
        },
    },
};
use sync_loop::SyncLoop;
use tracing::{error, info};

pub fn main(tls_cfg: TLSConnectionArgs) -> Result<(), anyhow::Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            //tokio::spawn(status_access::init());

            let tls_cfg_cloned = tls_cfg.clone();
            tokio::spawn(async move {
                if let Err(e) = client::run_forever(tls_cfg_cloned).await {
                    error!("[daemon] rks client exited with error: {e:?}");
                }
            });

            let tls_cfg = Arc::new(tls_cfg.clone());
            let server_addr: String =
                env::var("RKS_ADDRESS").unwrap_or_else(|_| "192.168.73.128:50051".to_string());
            tokio::spawn(async move {
                STATUS_MANAGER
                    .get_or_init(|| async {
                        Arc::new(
                            StatusManager::try_new(server_addr.clone(), tls_cfg.clone())
                                .await
                                .expect("Failed to construct StatusManager"),
                        )
                    })
                    .await;
                let mut pleg = PLEG::new(server_addr.clone(), tls_cfg.clone());
                let pleg_event_rx = pleg.start();
                let mut pod_worker =
                    PodWorker::new(server_addr.clone(), tls_cfg.clone(), pleg_event_rx);
                pod_worker.run();

                let sync_loop = SyncLoop::default().register_event(static_pods::handler);
                sync_loop.run().await;
                error!("[daemon] sync_loop exited unexpectedly");
            });
            tokio::signal::ctrl_c().await?;
            info!("[daemon] received Ctrl-C, shutting down");
            Ok(())
        })
}
