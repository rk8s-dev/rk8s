pub mod client;
pub mod pod_worker;
// pub mod probe;
pub mod static_pods;
pub mod status;
pub mod sync_loop;

use std::{env, sync::Arc, time::Duration};

//mod status_access;
use crate::{
    commands::pod::TLSConnectionArgs,
    daemon::{
        pod_worker::PodWorker,
        status::{
            pleg::PLEG,
            probe::probe_manager::{PROBE_MANAGER, ProbeManager, restore_existing_probes},
            status_manager::{STATUS_MANAGER, StatusManager},
        },
    },
};
use sync_loop::SyncLoop;
use tracing::{error, info, warn};

pub fn main(tls_cfg: TLSConnectionArgs) -> Result<(), anyhow::Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async move {
            //tokio::spawn(status_access::init());

            let tls_cfg = Arc::new(tls_cfg.clone());
            let server_addr: String =
                env::var("RKS_ADDRESS").unwrap_or_else(|_| "192.168.73.128:50051".to_string());

            let client_tls_cfg = (*tls_cfg).clone();
            tokio::spawn(async move {
                if let Err(e) = client::run_forever(client_tls_cfg).await {
                    error!("[daemon] rks client exited with error: {e:?}");
                }
            });

            let mut status_manager = StatusManager::try_new(server_addr.clone(), tls_cfg.clone())
                .await
                .expect("Failed to construct StatusManager");
            status_manager.run();
            let status_manager = Arc::new(status_manager);
            STATUS_MANAGER
                .set(status_manager.clone())
                .expect("failed to set global STATUS_MANAGER");

            let probe_manager = Arc::new(ProbeManager::new());
            PROBE_MANAGER
                .set(probe_manager.clone())
                .expect("[daemon] failed to set global PROBE_MANAGER");

            tokio::spawn(async move {
                let mut pleg = PLEG::new(
                    server_addr.clone(),
                    tls_cfg.clone(),
                    Duration::from_secs(10),
                );
                let pleg_event_rx = pleg.run();

                let mut pod_worker = PodWorker::new(
                    server_addr.clone(),
                    tls_cfg.clone(),
                    pleg_event_rx,
                    probe_manager.clone(),
                    status_manager.clone(),
                );
                pod_worker.run();

                if let Err(e) =
                    restore_existing_probes(&server_addr, tls_cfg.clone(), probe_manager.clone())
                        .await
                {
                    warn!("[daemon] failed to restore probes: {e}");
                }

                let sync_loop = SyncLoop::default().register_event(static_pods::handler);
                sync_loop.run().await;
                error!("[daemon] sync_loop exited unexpectedly");
            });
            tokio::signal::ctrl_c().await?;
            info!("[daemon] received Ctrl-C, shutting down");
            Ok(())
        })
}
