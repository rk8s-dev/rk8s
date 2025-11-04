use crate::api::xlinestore::XlineStore;
use common::{Node, Taint, TaintEffect, TaintKey};
use log::{error, info, warn};
use std::sync::Arc;
use std::time::Duration;

pub fn watch(xline_store: Arc<XlineStore>, grace: Duration, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await;
            if let Err(e) = handle_node_heartbeats(xline_store.clone(), grace).await {
                error!("heartbeat monitor failed: {e:?}");
            }
        }
    });
}

async fn handle_node_heartbeats(
    xline_store: Arc<XlineStore>,
    grace: Duration,
) -> anyhow::Result<()> {
    for node_id in xline_store.list_node_names().await? {
        if let Some(mut node) = xline_store.get_node(&node_id).await?
            && process_heartbeat_timeout(&mut node, grace)
        {
            persist_node(&xline_store, &node_id, &node).await?;
        }
    }
    Ok(())
}

fn process_heartbeat_timeout(node: &mut Node, grace: Duration) -> bool {
    if node.update_ready_status_on_timeout(grace) {
        // If expired is true, it means out of date; change status only when timeout is reached
        node.spec.taints = Node::derive_taints_from_conditions(&node.status.conditions);
        return true;
    }
    false
}

async fn persist_node(
    xline_store: &Arc<XlineStore>,
    node_id: &str,
    node: &Node,
) -> anyhow::Result<()> {
    xline_store.insert_node(node).await?;
    warn!("Node {node_id} marked Ready=Unknown (timeout)");
    evict_pods_for_node(node_id, xline_store.clone()).await;
    Ok(())
}

/// Evict all pods running on a given node if they don't tolerate NoExecute taints.
pub(crate) async fn evict_pods_for_node(node_id: &str, xline_store: Arc<XlineStore>) {
    let pods = match xline_store.list_pod_names().await {
        Ok(names) => names,
        Err(e) => {
            warn!("Failed to list pods for eviction: {e:?}");
            return;
        }
    };

    for pod_name in pods {
        let pod = match xline_store.get_pod(&pod_name).await {
            Ok(Some(pod)) => pod,
            Ok(None) => continue,
            Err(e) => {
                warn!("Failed to load pod {pod_name}: {e:?}");
                continue;
            }
        };

        if pod.spec.node_name.as_deref() != Some(node_id) {
            continue;
        }

        let taint = Taint::new(TaintKey::NodeNotReady, TaintEffect::NoExecute);
        // Check if pod has a matching toleration
        let has_toleration = pod.spec.tolerations.iter().any(|tol| tol.tolerate(&taint));

        if !has_toleration {
            // Evict if no toleration found
            info!("Evicting pod {} from node {}", pod.metadata.name, node_id);
            if let Err(e) = xline_store.delete_pod(&pod.metadata.name).await {
                error!("Failed to evict pod {}: {:?}", pod.metadata.name, e);
            }
        }
    }
}
