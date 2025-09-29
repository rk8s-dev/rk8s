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

    let rx = scheduler.run();
    tokio::spawn(async move {
        let mut since: i64 = 0;
        let mut backoff = std::time::Duration::from_millis(100);
        let max_backoff = std::time::Duration::from_secs(5);

        loop {
            let watch_opts = WatchOptions::new()
                .with_prefix()
                .with_prev_key()
                .with_start_revision(since);
            // Watch nodes in Xline
            let (mut _node_watcher, mut nodes_watch_stream) = match client
                .watch("/registry/nodes/".to_string(), Some(watch_opts.clone()))
                .await
            {
                Ok(w) => w,
                Err(_e) => {
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, max_backoff);
                    continue;
                }
            };
            // Watch pods in Xline
            let (mut _pod_watcher, mut pods_watch_stream) = match client
                .watch("/registry/pods/".to_string(), Some(watch_opts.clone()))
                .await
            {
                Ok(w) => w,
                Err(_e) => {
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, max_backoff);
                    continue;
                }
            };

            backoff = std::time::Duration::from_millis(100); // reset backoff

            loop {
                select! {
                    pod_msg = pods_watch_stream.message() => {
                        match pod_msg {
                            Ok(Some(resp)) => {
                                since = resp.header().map(|h| h.revision()).unwrap_or(since);
                                handle_pod_update(&mut scheduler, Ok(Some(resp))).await;
                            }
                            // If stream loses connection , exit the loop and reconnect
                            Ok(None) => {
                                break;
                            }
                            Err(_e) => {
                                break;
                            }
                        }
                    }

                    node_msg = nodes_watch_stream.message() => {
                        match node_msg {
                            Ok(Some(resp)) => {
                                since = resp.header().map(|h| h.revision()).unwrap_or(since);
                                handle_node_update(&mut scheduler, Ok(Some(resp))).await;
                            }
                            Ok(None) => {
                                break;
                            }
                            Err(_e) => {
                                break;
                            }
                        }
                    }

                    to_unassume = unassume_rx.recv() => {
                        if let Some(name) = to_unassume {
                            scheduler.unassume(&name).await;
                        }
                    }
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, max_backoff);
        }
    });
    Ok(rx)
}

async fn handle_pod_update(
    scheduler: &mut Scheduler,
    pod_msg: Result<Option<WatchResponse>, etcd_client::Error>,
) {
    if let Ok(Some(resp)) = pod_msg {
        for e in resp.events() {
            if let Some(kv) = e.kv() {
                match e.event_type() {
                    EventType::Put => {
                        if let Ok(new_pod) = get_pod_from_kv(kv) {
                            let node_name = new_pod.spec.node_name.clone();
                            let prev_pod = e.prev_kv().and_then(|pkv| get_pod_from_kv(pkv).ok());
                            match (node_name, prev_pod) {
                                // Case 1: New pod without nodeName，needs to be schedulerd
                                (None, _) => {
                                    scheduler.update_cache_pod(new_pod).await;
                                }
                                // Case 2: New pod with nodeName,just assumes once
                                (Some(_), None) => {
                                    scheduler.update_cache_pod(new_pod).await;
                                }
                                // Case 3: ignore pod updated (like podip)
                                _ => {}
                            }
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

async fn handle_node_update(
    scheduler: &mut Scheduler,
    node_msg: Result<Option<WatchResponse>, etcd_client::Error>,
) {
    if let Ok(Some(resp)) = node_msg {
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
