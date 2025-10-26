use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use async_trait::async_trait;
use common::PodTask;
use common::ReplicaSet;
use serde_yaml;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc, watch};
use tokio::time::sleep;

/// Controller trait defines the contract for controllers managed by ControllerManager.
#[async_trait]
pub trait Controller: Send + Sync + 'static {
    // Name used for identifying the controller.
    fn name(&self) -> &'static str;

    // Reconcile the resource identified by key, e.g. resource name.
    async fn reconcile(&self, key: &str, store: Arc<XlineStore>) -> Result<()>;
}

/// Simple ControllerManager: registers controllers, provides enqueue, and starts watch.
pub struct ControllerManager {
    controllers: RwLock<HashMap<String, Arc<dyn Controller>>>,
    // a work queue per controller.
    queues: RwLock<HashMap<String, mpsc::Sender<String>>>,
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
        controller: Arc<dyn Controller>,
        workers: usize, // max number of concurrent reconcile workers
        store: Arc<XlineStore>,
    ) -> Result<()> {
        // create workqueue
        let name = controller.name().to_string();
        let (tx, mut rx) = mpsc::channel::<String>(1000); // each event is a resource key , for example, "registry/pods/pod"

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

        // use semaphore to limit the number of concurrent reconcile workers
        let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));

        // subscribe to the global stop signal so this dispatcher can exit.
        let mut stop_sub = self.stop_tx.subscribe();

        let manager_clone = self.clone();
        let controller_clone = controller.clone();
        let name_clone = name.clone();

        // spawn the dispatcher loop which continuously receives keys from the queue
        // and spawns reconcile tasks for them.
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stop_sub.changed() => {
                        break;
                    }

                    opt = rx.recv() => {
                        match opt {
                            Some(key) => {

                                // need to acquire a permit from the semaphore
                                let permit = semaphore.clone().acquire_owned().await.unwrap();

                                let controller = controller_clone.clone();
                                let store = store.clone();
                                let name = name_clone.clone();
                                let _manager = manager_clone.clone(); // Keep the manager alive for lifetime safety

                                // spawn a new task to run reconcile for this key
                                tokio::spawn(async move {
                                    if let Err(e) = retry_with_backoff(|| async {
                                        controller.reconcile(&key, store.clone()).await
                                    }).await {
                                        log::error!(
                                            "controller {} reconcile {} failed: {:?}",
                                            name, key, e
                                        );
                                    }

                                    // remove from inflight set when done
                                    let mut inflight_map = _manager.inflight.write().await;
                                    if let Some(set) = inflight_map.get_mut(&name) {
                                        set.remove(&key);
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

    // Enqueue a key for a controller, just depend on inflight set to avoid duplicates.
    // the key may mean /registry/replicasets/name.
    pub async fn enqueue(&self, controller_name: &str, key: String) {
        // Only enqueue if not already present in inflight set
        let mut inflight_map = self.inflight.write().await;
        if let Some(set) = inflight_map.get_mut(controller_name) {
            if set.contains(&key) {
                return;
            }
            set.insert(key.clone());
        }

        let queues = self.queues.read().await;
        if let Some(tx) = queues.get(controller_name) {
            let _ = tx.send(key).await;
        } else {
            // cleanup reservation if no queue to receive
            if let Some(set) = inflight_map.get_mut(controller_name) {
                set.remove(&key);
            }
        }
    }

    // Start watch for pods and replicasets. Events are broadcast to all controllers.
    pub async fn start_watch(self: Arc<Self>, store: Arc<XlineStore>) -> Result<()> {
        // pods informer with reconnect loop
        let mgr_p = self.clone();
        let store_p = store.clone();
        // NOTE: This logic manually matches Pods to ReplicaSets by label selectors, because the ownerReferences
        // mechanism is not yet implemented. Once ownerReferences are supported, this section can be simplified to
        // enqueue only the owning ReplicaSet directly from Pod.metadata.ownerReferences.
        tokio::spawn(async move {
            let mut backoff_ms = 100u64;
            loop {
                match store_p.pods_snapshot_with_rev().await {
                    Ok((items, rev)) => {
                        // enqueue snapshot items
                        for (name, _yaml) in items.into_iter() {
                            let queues = mgr_p.queues.read().await;
                            for (ctrl, _) in queues.iter() {
                                let _ = mgr_p.enqueue(ctrl, name.clone()).await;
                            }
                        }

                        // start watch from snapshot revision
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

                                                    // Resolve Pod object: prefer curr kv, fall back to prev_kv or store lookup.
                                                    let mut pod_opt: Option<PodTask> = None;
                                                    let curr_yaml =
                                                        String::from_utf8_lossy(kv.value())
                                                            .to_string();
                                                    if !curr_yaml.trim().is_empty() {
                                                        if let Ok(p) = serde_yaml::from_str::<PodTask>(
                                                            &curr_yaml,
                                                        ) {
                                                            pod_opt = Some(p);
                                                        } else {
                                                            log::debug!(
                                                                "failed to deserialize pod from event for key={}",
                                                                key
                                                            );
                                                        }
                                                    }

                                                    if pod_opt.is_none()
                                                        && let Some(prev_kv) = ev.prev_kv()
                                                    {
                                                        let prev_yaml = String::from_utf8_lossy(
                                                            prev_kv.value(),
                                                        )
                                                        .to_string();
                                                        if !prev_yaml.trim().is_empty()
                                                            && let Ok(p) =
                                                                serde_yaml::from_str::<PodTask>(
                                                                    &prev_yaml,
                                                                )
                                                        {
                                                            pod_opt = Some(p);
                                                        }
                                                    }

                                                    if pod_opt.is_none() {
                                                        match store_p.get_pod_yaml(&key).await {
                                                            Ok(Some(s)) => {
                                                                if let Ok(p) =
                                                                    serde_yaml::from_str::<PodTask>(
                                                                        &s,
                                                                    )
                                                                {
                                                                    pod_opt = Some(p);
                                                                }
                                                            }
                                                            Ok(None) => {
                                                                log::debug!(
                                                                    "no pod yaml found in store for key={}",
                                                                    key
                                                                );
                                                            }
                                                            Err(e) => {
                                                                log::warn!(
                                                                    "error reading pod yaml from store for key={} err={:?}",
                                                                    key,
                                                                    e
                                                                );
                                                            }
                                                        }
                                                    }

                                                    if let Some(pod) = pod_opt {
                                                        // Find matching ReplicaSets and enqueue their names only.
                                                        match store_p.list_replicasets().await {
                                                            Ok(rss) => {
                                                                let queues =
                                                                    mgr_p.queues.read().await;
                                                                for rs in rss.into_iter() {
                                                                    if crate::controllers::ReplicaSetController::selector_match(&rs, &pod) {
                                                                        for (ctrl, _) in queues.iter() {
                                                                            if ctrl == "replicaset" {
                                                                                let _ = mgr_p.enqueue(ctrl, rs.metadata.name.clone()).await;
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            Err(e) => {
                                                                log::warn!(
                                                                    "failed to list replicasets while handling pod event: {:?}",
                                                                    e
                                                                );
                                                            }
                                                        }
                                                    } else {
                                                        // Fallback: broadcast pod key to all controllers (old behavior)
                                                        let queues = mgr_p.queues.read().await;
                                                        for (ctrl, _) in queues.iter() {
                                                            let _ = mgr_p
                                                                .enqueue(ctrl, key.clone())
                                                                .await;
                                                        }
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
                            let queues = mgr_rs.queues.read().await;
                            for (ctrl, _) in queues.iter() {
                                let _ = mgr_rs.enqueue(ctrl, name.clone()).await;
                            }
                        }

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

                                                    // Only enqueue if the spec actually changed.
                                                    // If prev_kv exists, deserialize prev & curr and compare spec.
                                                    let mut should_enqueue = true;
                                                    if let Some(prev_kv) = ev.prev_kv() {
                                                        let prev_yaml = String::from_utf8_lossy(
                                                            prev_kv.value(),
                                                        )
                                                        .to_string();
                                                        let curr_yaml =
                                                            String::from_utf8_lossy(kv.value())
                                                                .to_string();
                                                        let prev_rs: Result<ReplicaSet, _> =
                                                            serde_yaml::from_str(&prev_yaml);
                                                        let curr_rs: Result<ReplicaSet, _> =
                                                            serde_yaml::from_str(&curr_yaml);

                                                        if let (Ok(prev), Ok(curr)) =
                                                            (prev_rs, curr_rs)
                                                            && prev.spec == curr.spec
                                                        {
                                                            should_enqueue = false;
                                                        }
                                                    }

                                                    if should_enqueue {
                                                        let queues = mgr_rs.queues.read().await;
                                                        for (ctrl, _) in queues.iter() {
                                                            let _ = mgr_rs
                                                                .enqueue(ctrl, key.clone())
                                                                .await;
                                                        }
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

    pub async fn shutdown(&self) {
        let _ = self.stop_tx.send(true);
        sleep(Duration::from_millis(200)).await;
    }
}

impl Default for ControllerManager {
    fn default() -> Self {
        Self::new()
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
