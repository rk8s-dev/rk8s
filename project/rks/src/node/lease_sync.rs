use crate::network::manager::LocalManager;
use crate::{network::lease::LeaseWatchResult, node::NodeRegistry};
use common::RksMessage;
use common::lease::Lease;
use libcni::ip::route::Route;
use libnetwork::route;
use log::{debug, error, info, warn};
use std::sync::Arc;
use tokio::sync::mpsc;

pub(crate) struct LeaseSynchronizer {
    manager: Arc<LocalManager>,
    registry: Arc<NodeRegistry>,
}

impl LeaseSynchronizer {
    pub(crate) fn spawn(manager: Arc<LocalManager>, registry: Arc<NodeRegistry>) {
        tokio::spawn(async move {
            info!(
                target: "rks::node::lease",
                "synchronizer task spawned"
            );
            Self { manager, registry }.run().await;
        });
    }

    async fn run(self) {
        let Self { manager, registry } = self;
        info!(
            target: "rks::node::lease",
            "subscribing to lease updates"
        );
        // Channel for receiving lease watch results
        let mut lease_rx = Self::subscribe(manager);

        while let Some(results) = lease_rx.recv().await {
            Self::process_results(registry.clone(), results).await;
        }
        warn!(
            target: "rks::node::lease",
            "lease update channel closed; stopping synchronizer"
        );
    }

    fn subscribe(manager: Arc<LocalManager>) -> mpsc::Receiver<Vec<LeaseWatchResult>> {
        let (lease_tx, lease_rx) = mpsc::channel::<Vec<LeaseWatchResult>>(16);
        // Spawn task to propagate lease updates to workers
        tokio::spawn(async move {
            match manager.watch_leases(lease_tx).await {
                Ok(_) => info!(
                    target: "rks::node::lease",
                    "watch_leases stream ended"
                ),
                Err(e) => error!("watch_leases terminated with error: {e:?}"),
            }
        });
        lease_rx
    }

    async fn process_results(registry: Arc<NodeRegistry>, results: Vec<LeaseWatchResult>) {
        let leases = results
            .iter()
            .flat_map(|r| r.snapshot.clone())
            .collect::<Vec<_>>();

        info!("received all leases: {leases:?}");

        let node_ids: Vec<String> = leases.iter().map(|l| l.attrs.node_id.clone()).collect();

        if node_ids.is_empty() {
            debug!(
                target: "rks::node::lease",
                "no routed nodes in current snapshot"
            );
            return;
        }

        for node_id in node_ids {
            let routes = calculate_routes_for_node(&node_id, &leases);

            info!("sending routes to {node_id}: {routes:?}");

            let msg = RksMessage::UpdateRoutes(node_id.clone(), routes);
            if let Some(worker) = registry.get(&node_id).await {
                if let Err(e) = worker.tx.try_send(msg) {
                    error!("Failed to enqueue message for {node_id}: {e:?}");
                }
            } else {
                error!("No active worker for {node_id}");
            }
        }
    }
}

/// Calculate routes for a node from all current leases.
fn calculate_routes_for_node(node_id: &str, leases: &[Lease]) -> Vec<Route> {
    leases
        .iter()
        .filter(|lease| lease.attrs.node_id != node_id)
        .flat_map(|lease| {
            route::get_route_from_lease(lease)
                .into_iter()
                .chain(route::get_v6_route_from_lease(lease))
        })
        .collect()
}
