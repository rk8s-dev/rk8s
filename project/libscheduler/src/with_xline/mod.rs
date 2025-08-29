use etcd_client::{Client, EventType, WatchOptions, WatchResponse};
use tokio::{select, sync::mpsc::UnboundedReceiver};
pub mod model;
mod utils;

use crate::{
    models::Assignment,
    plugins::{Plugins, node_resources_fit::ScoringStrategy},
    scheduler::Scheduler,
    with_xline::utils::{get_node_from_kv, get_pod_from_kv, list_nodes, list_pods},
};

/// Start a scheduler with xline watcher
///
/// # Argument
/// - unassume_rx: a receiver passing pod's name that bind failed.
pub async fn run_scheduler_with_xline(
    endpoints: &[&str],
    strategy: ScoringStrategy,
    plugins: Plugins,
    mut unassume_rx: UnboundedReceiver<String>,
) -> Result<UnboundedReceiver<Result<Assignment, anyhow::Error>>, anyhow::Error> {
    let mut client = Client::connect(endpoints, None).await?;
    let mut scheduler = Scheduler::new(strategy, plugins);
    let exist_nodes = list_nodes(&mut client).await?;
    let exist_pods = list_pods(&mut client).await?;
    scheduler.set_cache_node(exist_nodes).await;
    for p in exist_pods {
        scheduler.update_cache_pod(p).await;
    }

    let (_, mut nodes_watch_stream) = client
        .watch(
            "/registry/nodes/".to_string(),
            Some(WatchOptions::new().with_prefix()),
        )
        .await?;
    let (_, mut pods_watch_stream) = client
        .watch(
            "/registry/pods/".to_string(),
            Some(WatchOptions::new().with_prefix()),
        )
        .await?;

    let rx = scheduler.run();
    tokio::spawn(async move {
        loop {
            select! {
                pod_msg = pods_watch_stream.message() => {
                    handle_pod_update(&mut scheduler, pod_msg).await;
                }
                node_msg = nodes_watch_stream.message() => {
                    handle_node_update(&mut scheduler, node_msg).await;

                }
                to_unassume = unassume_rx.recv() => {
                    if let Some(name) = to_unassume {
                        scheduler.unassume(&name).await;
                    }
                }
            }
        }
    });
    Ok(rx)
}

async fn handle_pod_update(
    scheduler: &mut Scheduler,
    pod_msg: Result<Option<WatchResponse>, etcd_client::Error>,
) {
    if let Ok(msg) = pod_msg {
        if let Some(resp) = msg {
            for e in resp.events() {
                if let Some(kv) = e.kv() {
                    match e.event_type() {
                        EventType::Put => {
                            let pod_res = get_pod_from_kv(kv);
                            if let Ok(pod) = pod_res {
                                scheduler.update_cache_pod(pod).await;
                            }
                        }
                        EventType::Delete => {
                            let name = String::from_utf8_lossy(kv.key()).to_string();
                            let node_name = name.split('/').filter(|s| !s.is_empty()).next_back();
                            if let Some(n) = node_name {
                                scheduler.remove_cache_pod(n).await;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn handle_node_update(
    scheduler: &mut Scheduler,
    node_msg: Result<Option<WatchResponse>, etcd_client::Error>,
) {
    if let Ok(msg) = node_msg {
        if let Some(resp) = msg {
            for e in resp.events() {
                if let Some(kv) = e.kv() {
                    match e.event_type() {
                        EventType::Put => {
                            let node_res = get_node_from_kv(kv);
                            if let Ok(node) = node_res {
                                scheduler.update_cache_node(node).await;
                            }
                        }
                        EventType::Delete => {
                            let name = String::from_utf8_lossy(kv.key()).to_string();
                            let node_name = name.split('/').filter(|s| !s.is_empty()).next_back();
                            if let Some(n) = node_name {
                                scheduler.remove_cache_node(n).await;
                            }
                        }
                    }
                }
            }
        }
    }
}
