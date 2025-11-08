use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use async_trait::async_trait;
use common::ResourceKind;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::sleep;

pub static CONTROLLER_MANAGER: Lazy<Arc<ControllerManager>> =
    Lazy::new(|| Arc::new(ControllerManager::new()));

/// A watch event.
/// Contains the resource yaml.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    Add { yaml: String },
    Update { old_yaml: String, new_yaml: String },
    Delete { yaml: String },
}

/// A watch response.
/// Contains the resource kind, key, and event.
#[derive(Debug, Clone)]
pub struct ResourceWatchResponse {
    pub kind: ResourceKind,
    pub key: String,
    pub event: WatchEvent,
}

/// Controller trait defines the contract for controllers managed by ControllerManager.
#[async_trait]
pub trait Controller: Send + Sync + 'static {
    /// Name used for identifying the controller.
    fn name(&self) -> &'static str;

    /// Initialize the controller.
    async fn init(&mut self) -> Result<()> {
        Ok(())
    }

    /// The resources that the controller needs to watch.
    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![]
    }

    #[allow(unused)]
    /// Watch response handler.
    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        Ok(())
    }
}

/// Simple ControllerManager: registers controllers, provides enqueue, and starts watch.
pub struct ControllerManager {
    controllers: RwLock<HashMap<String, Arc<RwLock<dyn Controller>>>>,
    // a work queue per controller.
    queues: RwLock<HashMap<String, mpsc::Sender<ResourceWatchResponse>>>,
    // use for avoiding duplicates and avoid the same key gets into queue twice.
    inflight: RwLock<HashMap<String, HashSet<String>>>,
    // use for stopping the manager.
    stop_tx: watch::Sender<bool>,
}

impl ControllerManager {
    // Initialize a new ControllerManager.
    pub fn new() -> Self {
        let (stop_tx, _) = watch::channel(false);
        Self {
            controllers: RwLock::new(HashMap::new()),
            queues: RwLock::new(HashMap::new()),
            inflight: RwLock::new(HashMap::new()),
            stop_tx,
        }
    }

    // Register a controller and spawn a dispatcher task that consumes its work queue.
    // Each controller gets its own queue, and this function starts the async loop.
    pub async fn register(
        self: Arc<Self>,
        controller: Arc<RwLock<dyn Controller>>,
        workers: usize, // max number of concurrent handle watch response workers
    ) -> Result<()> {
        controller.write().await.init().await?;
        // create workqueue
        let name = controller.read().await.name().to_string();
        let (tx, mut rx) = mpsc::channel::<ResourceWatchResponse>(1000);

        // register this controller and its queue in the manager
        self.controllers
            .write()
            .await
            .insert(name.clone(), controller.clone());
        self.queues.write().await.insert(name.clone(), tx.clone());
        // initialize inflight set for this controller
        self.inflight
            .write()
            .await
            .insert(name.clone(), HashSet::new());

        // use semaphore to limit the number of concurrent handle watch response workers
        let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));

        // subscribe to the global stop signal so this dispatcher can exit.
        let mut stop_sub = self.stop_tx.subscribe();

        let manager_clone = self.clone();
        let controller_clone = controller.clone();
        let name_clone = name.clone();

        // spawn the dispatcher loop which continuously receives keys from the queue
        // and spawns handle watch response tasks for them.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_sub.changed() => {
                        break;
                    }

                    opt = rx.recv() => {
                        match opt {
                            Some(resp) => {

                                // need to acquire a permit from the semaphore
                                let permit = semaphore.clone().acquire_owned().await.unwrap();

                                let controller = controller_clone.clone();
                                let name = name_clone.clone();
                                let _manager = manager_clone.clone(); // Keep the manager alive for lifetime safety

                                // spawn a new task to handle the watch response
                                tokio::spawn(async move {
                                    if let Err(e) = retry_with_backoff(|| async {
                                        controller.write().await.handle_watch_response(&resp).await?;
                                        Ok(())
                                    }).await {
                                        log::error!(
                                            "controller {} handle watch response {} failed: {:?}",
                                            name, resp.key, e
                                        );
                                    }

                                    // remove from inflight set when done
                                    let mut inflight_map = _manager.inflight.write().await;
                                    if let Some(set) = inflight_map.get_mut(&name) {
                                        set.remove(&resp.key);
                                    }

                                    // release the permit when the task is done.
                                    drop(permit);
                                });
                            }

                            None => break,
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Start watch for pods and replicasets. Events are broadcast to controllers who need to watch these resources.
    pub async fn start_watch(self: Arc<Self>, store: Arc<XlineStore>) -> Result<()> {
        // pods informer with reconnect loop
        let mgr_p = self.clone();
        let store_p = store.clone();

        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_p.pods_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        // broadcast snapshot items to controllers who need to watch pods.
                        for (name, _yaml) in items.into_iter() {
                            let senders = mgr_p.get_senders_by_kind(ResourceKind::Pod).await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::Pod,
                                        key: name.clone(),
                                        event: WatchEvent::Add {
                                            yaml: _yaml.clone(),
                                        },
                                    })
                                    .await;
                            }
                        }

                        // start watch from snapshot revision and broadcast events to controllers who need to watch pods.
                        match store_p.watch_pods(rev).await {
                            Ok((_watcher, mut stream)) => {
                                // reset backoff on successful watch
                                backoff_ms = 100;
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/pods/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_p
                                                        .get_senders_by_kind(ResourceKind::Pod)
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::Pod,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!("pod watch stream closed, will reconnect");
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!("pod watch error: {:?}, will reconnect", e);
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start pod watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot pods: {:?}", e);
                    }
                }

                // backoff before retry
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });

        // replicasets informer with reconnect loop (use snapshot_with_rev to obtain a starting revision)
        let mgr_rs = self.clone();
        let store_rs = store.clone();
        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_rs.replicasets_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        for (name, _yaml) in items.into_iter() {
                            // broadcast snapshot items to controllers who need to watch replicasets.
                            let senders =
                                mgr_rs.get_senders_by_kind(ResourceKind::ReplicaSet).await;
                            for sender in senders {
                                let _ = sender
                                    .send(ResourceWatchResponse {
                                        kind: ResourceKind::ReplicaSet,
                                        key: name.clone(),
                                        event: WatchEvent::Add {
                                            yaml: _yaml.clone(),
                                        },
                                    })
                                    .await;
                            }
                        }

                        // start watch from snapshot revision and broadcast events to controllers who need to watch replicasets.
                        match store_rs.watch_replicasets(rev).await {
                            Ok((_watcher, mut stream)) => {
                                backoff_ms = 100;
                                loop {
                                    match stream.message().await {
                                        Ok(Some(resp)) => {
                                            for ev in resp.events() {
                                                if let Some(kv) = ev.kv() {
                                                    let key = String::from_utf8_lossy(kv.key())
                                                        .replace("/registry/replicasets/", "");
                                                    let event_opt = match ev.event_type() {
                                                        etcd_client::EventType::Put => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Update {
                                                                    old_yaml:
                                                                        String::from_utf8_lossy(
                                                                            prev_kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                    new_yaml:
                                                                        String::from_utf8_lossy(
                                                                            kv.value(),
                                                                        )
                                                                        .to_string(),
                                                                })
                                                            } else {
                                                                Some(WatchEvent::Add {
                                                                    yaml: String::from_utf8_lossy(
                                                                        kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            }
                                                        }
                                                        etcd_client::EventType::Delete => {
                                                            if let Some(prev_kv) = ev.prev_kv() {
                                                                Some(WatchEvent::Delete {
                                                                    yaml: String::from_utf8_lossy(
                                                                        prev_kv.value(),
                                                                    )
                                                                    .to_string(),
                                                                })
                                                            } else {
                                                                log::warn!(
                                                                    "watch delete event missing prev_kv for key {}",
                                                                    key
                                                                );
                                                                None
                                                            }
                                                        }
                                                    };
                                                    let Some(event) = event_opt else {
                                                        continue;
                                                    };
                                                    let senders = mgr_rs
                                                        .get_senders_by_kind(
                                                            ResourceKind::ReplicaSet,
                                                        )
                                                        .await;
                                                    for sender in senders {
                                                        let _ = sender
                                                            .send(ResourceWatchResponse {
                                                                kind: ResourceKind::ReplicaSet,
                                                                key: key.clone(),
                                                                event: event.clone(),
                                                            })
                                                            .await;
                                                    }
                                                }
                                            }
                                        }
                                        Ok(None) => {
                                            log::info!(
                                                "replicaset watch stream closed, will reconnect"
                                            );
                                            break;
                                        }
                                        Err(e) => {
                                            log::error!(
                                                "replicaset watch error: {:?}, will reconnect",
                                                e
                                            );
                                            break;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                log::error!("failed to start replicaset watch: {:?}", e);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("failed to snapshot replicasets: {:?}", e);
                    }
                }

                // backoff before retry
                sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(30_000);
            }
        });

        Ok(())
    }

    pub fn shutdown(&self) {
        let _ = self.stop_tx.send(true);
    }

    async fn get_senders_by_kind(
        &self,
        kind: ResourceKind,
    ) -> Vec<mpsc::Sender<ResourceWatchResponse>> {
        let mut ret = Vec::new();
        for (name, ctrl) in self.controllers.read().await.iter() {
            if ctrl.read().await.watch_resources().contains(&kind)
                && let Some(tx) = self.queues.read().await.get(name)
            {
                ret.push(tx.clone());
            }
        }
        ret
    }
}

impl Default for ControllerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ControllerManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn retry_with_backoff<F, Fut>(mut f: F) -> Result<()>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let mut attempts = 0u32;
    loop {
        match f().await {
            Ok(_) => return Ok(()),
            Err(e) => {
                attempts += 1;
                if attempts >= 5 {
                    return Err(e);
                }
                let backoff = 2u64.pow(attempts.min(6)) * 100;
                sleep(Duration::from_millis(backoff)).await;
                continue;
            }
        }
    }
}
