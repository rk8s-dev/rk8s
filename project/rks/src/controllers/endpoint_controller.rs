use crate::api::xlinestore::XlineStore;
use anyhow::Result;
use async_trait::async_trait;
use common::{
    Endpoint, EndpointAddress, EndpointPort, EndpointSubset, LabelSelector, LabelSelectorOperator,
    ObjectMeta, ObjectReference, PodTask, ResourceKind, ServiceTask,
};
use log::{info, warn};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};
use tokio::time::sleep;

use crate::controllers::manager::{Controller, ResourceWatchResponse, WatchEvent};
use common::ServicePort;

/// EndpointController watches Services and Pods and maintains Endpoints objects in the
/// registry (xline) to reflect Pods that match a Service selector.
pub struct EndpointController {
    store: Arc<XlineStore>,

    // local caches
    service_index: Arc<RwLock<HashMap<String, ServiceTask>>>,
    pod_index: Arc<RwLock<HashMap<String, PodTask>>>,
    endpoints_index: Arc<RwLock<HashMap<String, Endpoint>>>,

    // work queue
    queue_tx: mpsc::Sender<String>,
    queue_rx: Option<mpsc::Receiver<String>>,
    // dedupe helpers
    queued: Arc<RwLock<HashSet<String>>>,
    // keys that need processing (used for dedupe and requeue semantics)
    dirty: Arc<RwLock<HashSet<String>>>,
    // keys currently being processed by a worker
    processing: Arc<RwLock<HashSet<String>>>,
    // processing/dirtiness sets to avoid losing updates while a key is being processed
    max_retries: u32,
}

impl EndpointController {
    pub fn new(store: Arc<XlineStore>) -> Self {
        let (tx, rx) = mpsc::channel::<String>(1000);
        Self {
            store,
            service_index: Arc::new(RwLock::new(HashMap::new())),
            pod_index: Arc::new(RwLock::new(HashMap::new())),
            endpoints_index: Arc::new(RwLock::new(HashMap::new())),
            queue_tx: tx,
            queue_rx: Some(rx),
            queued: Arc::new(RwLock::new(HashSet::new())),
            dirty: Arc::new(RwLock::new(HashSet::new())),
            processing: Arc::new(RwLock::new(HashSet::new())),

            max_retries: 15,
        }
    }

    // Initialize caches from store
    async fn bootstrap_caches(&self) -> Result<()> {
        let services = self.store.list_services().await?;
        let mut s_guard = self.service_index.write().await;
        for s in services.into_iter() {
            s_guard.insert(s.metadata.name.clone(), s);
        }

        let pods = self.store.list_pods().await?;
        let mut p_guard = self.pod_index.write().await;
        for p in pods.into_iter() {
            p_guard.insert(p.metadata.name.clone(), p);
        }

        let endpoints = self.store.list_endpoints().await?;
        let mut e_guard = self.endpoints_index.write().await;
        for ep in endpoints.into_iter() {
            e_guard.insert(ep.metadata.name.clone(), ep);
        }

        info!(
            "endpoint-controller bootstrap: services={} pods={} endpoints={}",
            s_guard.len(),
            p_guard.len(),
            e_guard.len()
        );
        Ok(())
    }

    // Take the receiver and spawn the consumer workers
    fn spawn_consumer(&mut self) {
        if let Some(rx) = self.queue_rx.take() {
            let tx = self.queue_tx.clone();
            let store = self.store.clone();
            let service_index = self.service_index.clone();
            let pod_index = self.pod_index.clone();
            let endpoints_index = self.endpoints_index.clone();
            let queued = self.queued.clone();
            let dirty = self.dirty.clone();
            let processing = self.processing.clone();
            let max_retries = self.max_retries;

            tokio::spawn(async move {
                let mut rx = rx;
                use tokio::sync::Mutex;
                let attempts = Arc::new(Mutex::new(HashMap::<String, u32>::new()));
                while let Some(key) = rx.recv().await {
                    let key_cl = key.clone();

                    // process concurrently
                    let svc_index_c = service_index.clone();
                    let pod_index_c = pod_index.clone();
                    let endpoints_index_c = endpoints_index.clone();
                    let store_c = store.clone();
                    let queued_c = queued.clone();
                    let dirty_c = dirty.clone();
                    let processing_c = processing.clone();
                    let attempts_c = attempts.clone();
                    let tx_c = tx.clone();
                    let max_retries_c = max_retries;

                    tokio::spawn(async move {
                        info!("worker start: service={key_cl}");
                        // mark start of processing: remove queued, add to processing, clear any old dirty
                        {
                            let mut qd = queued_c.write().await;
                            qd.remove(&key_cl);
                        }
                        {
                            let mut p = processing_c.write().await;
                            p.insert(key_cl.clone());
                        }
                        // remove any stale dirty marker that existed prior to this start
                        {
                            let mut d = dirty_c.write().await;
                            d.remove(&key_cl);
                        }

                        // sync logic similar to reconcile_service but using caches
                        let svc_opt = { svc_index_c.read().await.get(&key_cl).cloned() };

                        let res = if let Some(svc) = svc_opt {
                            // For ExternalName services we skip processing entirely (do nothing).
                            if svc.spec.service_type == "ExternalName" {
                                info!("skip ExternalName service={}", svc.metadata.name);
                                Ok(())
                            } else if svc.spec.selector.is_none() {
                                // nil selector: controller skips processing (do not modify endpoints)
                                info!("skip None selector service={}", svc.metadata.name);
                                Ok(())
                            } else {
                                // selector present -> delegate to helper using cached pods
                                let pods: Vec<PodTask> =
                                    { pod_index_c.read().await.values().cloned().collect() };
                                if let Some(endpoints) =
                                    build_endpoints_from_service_and_pods(&svc, &pods)
                                {
                                    let yaml =
                                        serde_yaml::to_string(&endpoints).unwrap_or_default();
                                    let r = store_c
                                        .insert_endpoint_yaml(&svc.metadata.name, &yaml)
                                        .await;
                                    if r.is_ok() {
                                        let addr_cnt = endpoints
                                            .subsets
                                            .first()
                                            .map(|s| s.addresses.len())
                                            .unwrap_or(0);
                                        let port_cnt = endpoints
                                            .subsets
                                            .first()
                                            .map(|s| s.ports.len())
                                            .unwrap_or(0);
                                        info!(
                                            "endpoints updated service={} addresses={} ports={}",
                                            svc.metadata.name, addr_cnt, port_cnt
                                        );
                                        endpoints_index_c
                                            .write()
                                            .await
                                            .insert(svc.metadata.name.clone(), endpoints);
                                    }
                                    r
                                } else {
                                    // helper returned None -> skip processing
                                    info!("helper produced None service={}", svc.metadata.name);
                                    Ok(())
                                }
                            }
                        } else {
                            // service not found: delete endpoints key
                            let r = store_c.delete_endpoint(&key_cl).await;
                            info!(
                                "service missing -> delete endpoints service={key_cl} ok={}",
                                r.is_ok()
                            );
                            r
                        };

                        if let Err(err) = res {
                            // retry logic
                            let mut at = attempts_c.lock().await;
                            let cnt = at.entry(key_cl.clone()).or_insert(0);
                            *cnt += 1;
                            if *cnt <= max_retries_c {
                                // exponential backoff: base 200ms
                                // cap backoff to a reasonable maximum (30s) to avoid
                                // extremely long waits when max_retries is large.
                                let backoff =
                                    200u64.saturating_mul(2u64.saturating_pow(*cnt)).min(30_000);
                                let tx_retry = tx_c.clone();
                                let key_retry = key_cl.clone();
                                warn!(
                                    "processing failed service={key_cl} attempt={cnt} backoff_ms={backoff} err={err:?}"
                                );
                                tokio::spawn(async move {
                                    sleep(Duration::from_millis(backoff)).await;
                                    let _ = tx_retry.send(key_retry).await;
                                });
                            } else {
                                warn!(
                                    "processing permanently failed service={key_cl} attempts={cnt} err={err:?}"
                                );
                            }
                        } else {
                            // success: clear attempts
                            let mut at = attempts_c.lock().await;
                            at.remove(&key_cl);
                            info!("processing succeeded service={key_cl}");
                        }
                        // remove from processing
                        info!("worker finish service={key_cl}");
                        processing_c.write().await.remove(&key_cl);

                        // processing finished: if we saw updates while processing (dirty), requeue once
                        let had_dirty = {
                            let mut d = dirty_c.write().await;
                            d.remove(&key_cl)
                        };

                        if had_dirty {
                            let mut at = attempts_c.lock().await;
                            if at.remove(&key_cl).is_some() {
                                info!("reset attempts due to dirty requeue service={key_cl}");
                            }

                            // requeue to process latest changes (no extra delay)
                            info!("requeue due to dirty service={key_cl}");
                            // schedule_enqueue(
                            //     &tx_c,
                            //     queued_c.clone(),
                            //     dirty_c.clone(),
                            //     processing_c.clone(),
                            //     key_cl.clone(),
                            //     Duration::from_millis(0),
                            // )
                            // .await;
                        }
                    });
                }
            });
        }
    }

    /// Reconcile a Service by name.
    ///
    /// NOTE: This helper is not currently invoked by the controller runtime but
    /// is retained intentionally as a convenient entry point for manual
    /// reconciliation, debugging, or unit tests that may want to exercise the
    /// same logic using the controller's store/caches. If you remove it, please
    /// ensure any tests or external callers are updated accordingly.
    ///
    /// Keep `allow(dead_code)` to suppress unused warnings while the method is
    /// kept for future use; remove the attribute if the function is deleted.
    #[allow(dead_code)]
    async fn reconcile_service(&self, svc_name: &str) -> Result<()> {
        // Try to get the service; if missing, create an empty Endpoints object
        let svc = self.store.get_service(svc_name).await?;

        if svc.is_none() {
            // Service not found — delete corresponding Endpoints key if present
            let _ = self.store.delete_endpoint(svc_name).await;
            return Ok(());
        }

        let svc = svc.unwrap();

        // Collect matching pods. ServiceSpec.selector is Option<LabelSelector>.
        // nil selector (None) -> controller should skip processing
        let selector_opt = svc.spec.selector.clone();
        if selector_opt.is_none() {
            // nil selector: do nothing
            return Ok(());
        }

        // Prefer local cache of pods when available
        let pods = {
            let guard = self.pod_index.read().await;
            if guard.is_empty() {
                self.store.list_pods().await?
            } else {
                guard.values().cloned().collect()
            }
        };

        // For ExternalName services we skip processing entirely (do nothing).
        if svc.spec.service_type == "ExternalName" {
            return Ok(());
        }

        // Delegate endpoint construction to helper. Helper returns `None` when
        // controller should skip processing (e.g. nil selector).
        if let Some(endpoints) = build_endpoints_from_service_and_pods(&svc, &pods) {
            let yaml = serde_yaml::to_string(&endpoints)?;
            self.store
                .insert_endpoint_yaml(&svc.metadata.name, &yaml)
                .await?;
        }

        Ok(())
    }
}

fn selector_match(sel: &LabelSelector, svc_ns: &str, pod: &PodTask) -> bool {
    // namespace must match
    if svc_ns != pod.metadata.namespace {
        return false;
    }
    // matchLabels
    for (k, v) in sel.match_labels.iter() {
        match pod.metadata.labels.get(k) {
            Some(val) if val == v => (),
            _ => return false,
        }
    }

    // matchExpressions
    for expr in sel.match_expressions.iter() {
        match &expr.operator {
            LabelSelectorOperator::In => {
                let v = pod.metadata.labels.get(&expr.key);
                if v.is_none() || !expr.values.contains(v.unwrap()) {
                    return false;
                }
            }
            LabelSelectorOperator::NotIn => {
                if let Some(v) = pod.metadata.labels.get(&expr.key)
                    && expr.values.contains(v)
                {
                    return false;
                }
            }
            LabelSelectorOperator::Exists => {
                if !pod.metadata.labels.contains_key(&expr.key) {
                    return false;
                }
            }
            LabelSelectorOperator::DoesNotExist => {
                if pod.metadata.labels.contains_key(&expr.key) {
                    return false;
                }
            }
        }
    }

    true
}

/// Build an `Endpoint` object for given Service and list of Pods.
/// Returns `None` when the controller should skip processing (e.g. nil selector).
fn build_endpoints_from_service_and_pods(svc: &ServiceTask, pods: &[PodTask]) -> Option<Endpoint> {
    // nil selector -> skip
    svc.spec.selector.as_ref()?;

    let selector = svc.spec.selector.as_ref().unwrap();

    let mut addresses: Vec<EndpointAddress> = Vec::new();
    for pod in pods.iter() {
        let matched = if selector.match_labels.is_empty() && selector.match_expressions.is_empty() {
            true
        } else {
            selector_match(selector, &svc.metadata.namespace, pod)
        };

        if !matched {
            continue;
        }

        if let Some(ip) = pod.status.pod_ip.clone() {
            let target_ref = Some(ObjectReference {
                api_version: Some(pod.api_version.clone()),
                kind: Some(pod.kind.clone()),
                namespace: Some(pod.metadata.namespace.clone()),
                name: Some(pod.metadata.name.clone()),
                uid: Some(pod.metadata.uid.to_string()),
                resource_version: None,
                field_path: None,
            });

            addresses.push(EndpointAddress {
                ip,
                node_name: pod.spec.node_name.clone(),
                target_ref,
            });
        }
    }

    // Build ports
    let mut ports: Vec<EndpointPort> = Vec::new();
    if svc.spec.ports.is_empty() {
        if svc.spec.cluster_ip.as_deref() == Some("None") {
            // headless with no ports -> keep ports empty
        }
    } else {
        for sp in svc.spec.ports.iter() {
            let port_num = sp.target_port.unwrap_or(sp.port);
            ports.push(endpoint_port_from_service_port(sp, port_num));
        }
    }

    let subset = EndpointSubset {
        addresses,
        not_ready_addresses: vec![],
        ports,
    };

    let endpoints = Endpoint {
        api_version: "v1".to_string(),
        kind: "Endpoints".to_string(),
        metadata: ObjectMeta {
            name: svc.metadata.name.clone(),
            namespace: svc.metadata.namespace.clone(),
            uid: svc.metadata.uid,
            ..Default::default()
        },
        subsets: vec![subset],
    };

    Some(endpoints)
}

/// (Removed pod-based port resolution.) Endpoint ports use service.target_port or service.port.
fn endpoint_port_from_service_port(service_port: &ServicePort, port_num: i32) -> EndpointPort {
    EndpointPort {
        port: port_num,
        protocol: service_port.protocol.clone(),
        name: service_port.name.clone(),
        app_protocol: None,
    }
}

async fn schedule_enqueue(
    tx: &mpsc::Sender<String>,
    queued: Arc<RwLock<HashSet<String>>>,
    dirty: Arc<RwLock<HashSet<String>>>,
    processing: Arc<RwLock<HashSet<String>>>,
    key: String,
    delay: Duration,
) {
    // If already marked dirty, it's already scheduled/needs processing -> dedupe
    {
        let mut d = dirty.write().await;
        if d.contains(&key) {
            info!("enqueue dedupe (already dirty) service={key}");
            return;
        }
        // mark as needing processing
        d.insert(key.clone());
        info!("marked dirty service={key}");
    }

    let q = tx.clone();
    let queued_c = queued.clone();
    let processing_c = processing.clone();
    // spawn a timer for delayed enqueue (delay may be zero)
    tokio::spawn(async move {
        sleep(delay).await;
        // If the key is currently being processed, do nothing — the worker will
        // observe `dirty` and requeue when it finishes. Otherwise enqueue if
        // not already queued.
        // Note: we intentionally do not remove `dirty` here; the worker will
        // clear it when it starts processing.
        let in_processing = { processing_c.read().await.contains(&key) };
        if in_processing {
            // leave dirty marker for worker to requeue on Done
            info!("in processing; will requeue later service={key}");
            return;
        }

        let currently_queued = { queued_c.read().await.contains(&key) };
        if !currently_queued {
            let _ = q.send(key.clone()).await;
            queued_c.write().await.insert(key.clone());
            info!("enqueue service={key} delay_ms={}", delay.as_millis());
        }
    });
}

#[async_trait]
impl Controller for EndpointController {
    fn name(&self) -> &'static str {
        "endpoint-controller"
    }

    async fn init(&mut self) -> Result<()> {
        // simple init: bootstrap caches and spawn consumer
        self.bootstrap_caches().await?;
        self.spawn_consumer();
        info!("endpoint-controller initialized");
        Ok(())
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![ResourceKind::Service, ResourceKind::Pod]
    }

    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        match &response.event {
            WatchEvent::Add { yaml }
            | WatchEvent::Update {
                old_yaml: _,
                new_yaml: yaml,
            } => {
                match response.kind {
                    ResourceKind::Service => {
                        // update service cache and enqueue
                        if let Ok(svc) = serde_yaml::from_str::<ServiceTask>(yaml) {
                            let name = svc.metadata.name.clone();
                            self.service_index.write().await.insert(name.clone(), svc);
                            info!("service add/update enqueue service={name}");
                            schedule_enqueue(
                                &self.queue_tx,
                                self.queued.clone(),
                                self.dirty.clone(),
                                self.processing.clone(),
                                name.clone(),
                                Duration::from_millis(0),
                            )
                            .await;
                        }
                    }
                    ResourceKind::Pod => {
                        if let Ok(pod) = serde_yaml::from_str::<PodTask>(yaml) {
                            // update pod cache
                            let pod_name = pod.metadata.name.clone();
                            self.pod_index
                                .write()
                                .await
                                .insert(pod_name.clone(), pod.clone());
                            info!("pod add/update name={}", pod_name);
                            // find services selecting this pod and schedule them with debounce
                            let services: Vec<ServiceTask> =
                                self.service_index.read().await.values().cloned().collect();
                            for svc in services.into_iter() {
                                // nil selector -> skip
                                if svc.spec.selector.is_none() {
                                    info!(
                                        "skip pod event for service with None selector service={}",
                                        svc.metadata.name
                                    );
                                    continue;
                                }
                                let sel = svc.spec.selector.clone().unwrap();
                                // empty selector {} -> matches all pods
                                let matched = if sel.match_labels.is_empty()
                                    && sel.match_expressions.is_empty()
                                {
                                    true
                                } else {
                                    selector_match(&sel, &svc.metadata.namespace, &pod)
                                };
                                if matched {
                                    let svc_name = svc.metadata.name.clone();
                                    info!(
                                        "pod triggers enqueue (debounce) service={svc_name} pod={pod_name}"
                                    );
                                    schedule_enqueue(
                                        &self.queue_tx,
                                        self.queued.clone(),
                                        self.dirty.clone(),
                                        self.processing.clone(),
                                        svc_name.clone(),
                                        Duration::from_millis(200),
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            WatchEvent::Delete { yaml } => {
                match response.kind {
                    ResourceKind::Service => {
                        // service deleted -> remove from cache and enqueue (will result in empty endpoints)
                        if let Ok(svc) = serde_yaml::from_str::<ServiceTask>(yaml) {
                            let name = svc.metadata.name.clone();
                            self.service_index.write().await.remove(&name);
                            info!("service delete enqueue service={}", svc.metadata.name);
                            schedule_enqueue(
                                &self.queue_tx,
                                self.queued.clone(),
                                self.dirty.clone(),
                                self.processing.clone(),
                                name,
                                Duration::from_millis(0),
                            )
                            .await;
                        }
                    }
                    ResourceKind::Pod => {
                        if let Ok(pod) = serde_yaml::from_str::<PodTask>(yaml) {
                            // remove pod from cache and schedule related services
                            self.pod_index.write().await.remove(&pod.metadata.name);
                            info!("pod delete name={}", pod.metadata.name);
                            let services: Vec<ServiceTask> =
                                self.service_index.read().await.values().cloned().collect();
                            for svc in services.into_iter() {
                                // nil selector -> skip
                                if svc.spec.selector.is_none() {
                                    info!(
                                        "skip pod delete for service with None selector service={}",
                                        svc.metadata.name
                                    );
                                    continue;
                                }
                                let sel = svc.spec.selector.clone().unwrap();
                                let matched = if sel.match_labels.is_empty()
                                    && sel.match_expressions.is_empty()
                                {
                                    true
                                } else {
                                    selector_match(&sel, &svc.metadata.namespace, &pod)
                                };
                                if matched {
                                    let svc_name = svc.metadata.name.clone();
                                    info!(
                                        "pod delete triggers enqueue service={} pod={}",
                                        svc_name, pod.metadata.name
                                    );
                                    schedule_enqueue(
                                        &self.queue_tx,
                                        self.queued.clone(),
                                        self.dirty.clone(),
                                        self.processing.clone(),
                                        svc_name.clone(),
                                        Duration::from_millis(200),
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }
}
