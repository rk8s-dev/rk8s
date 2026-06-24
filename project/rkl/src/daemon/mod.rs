pub mod client;
pub mod pod_worker;
// pub mod probe;
pub mod static_pods;
pub mod status;
pub mod sync_loop;
pub mod tty;

use std::{sync::Arc, time::Duration};

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

            let client_tls_cfg = (*tls_cfg).clone();
            tokio::spawn(async move {
                if let Err(e) = client::run_forever(client_tls_cfg).await {
                    error!("[daemon] rks client exited with error: {e:?}");
                }
            });

            let mut status_manager = StatusManager::new().await;
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
                let mut pleg = PLEG::new(Duration::from_secs(10));
                let pleg_event_rx = pleg.run();

                let mut pod_worker =
                    PodWorker::new(pleg_event_rx, probe_manager.clone(), status_manager.clone());
                pod_worker.run();

                let probe_restore_manager = probe_manager.clone();
                tokio::spawn(async move {
                    if let Err(e) = restore_existing_probes(probe_restore_manager).await {
                        warn!("[daemon] failed to restore probes: {e}");
                    }
                });

                let sync_loop = SyncLoop::default().register_event(static_pods::handler);
                sync_loop.run().await;
                error!("[daemon] sync_loop exited unexpectedly");
            });

            tokio::signal::ctrl_c().await?;
            info!("[daemon] received Ctrl-C, shutting down");
            Ok(())
        })
}
