use crate::api::xlinestore::XlineStore;
use crate::controllers::manager::{Controller, ResourceWatchResponse};
use crate::node::NodeRegistry;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use common::{self, ResourceKind};
use libnetwork::nftables::{
    generate_service_delete_with_endpoint, generate_service_update,
    generate_service_update_with_old_endpoint, generate_services_discovery_delta,
    generate_services_discovery_refresh,
};
use log::{info, warn};
use serde_json::{Value, json};
use serde_yaml;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, mpsc};
use tokio::time::{Duration, sleep};

/// Watches Services and Endpoints, generates nftables rules, and broadcasts to workers.
pub struct NftablesController {
    xline_store: Arc<XlineStore>,
    node_registry: Arc<NodeRegistry>,
    // Nodes that failed full-sync bootstrap (map_init + full_rules)
    out_of_sync: Arc<Mutex<HashSet<String>>>,
    // Nodes that failed recent incremental/discovery updates (generic retry)
    recent_failed: Arc<Mutex<HashSet<String>>>,

    // work queue for service key level debounce
    queue_tx: mpsc::Sender<String>,
    queue_rx: Option<mpsc::Receiver<String>>,
    // dedupe helpers
    queued: Arc<RwLock<HashSet<String>>>,
    dirty: Arc<RwLock<HashSet<String>>>,
    processing: Arc<RwLock<HashSet<String>>>,
    // max retries for incremental send failures
    max_retries: u32,

    // service state snapshot cache: service_key -> (service_yaml, endpoint_yaml)
    // used for incremental diff generation in the worker
    service_snapshots: Arc<RwLock<std::collections::HashMap<String, (String, String)>>>,
}

impl NftablesController {
    pub fn new(xline_store: Arc<XlineStore>, node_registry: Arc<NodeRegistry>) -> Self {
        let (tx, rx) = mpsc::channel::<String>(1000);
        Self {
            xline_store,
            node_registry,
            out_of_sync: Arc::new(Mutex::new(HashSet::new())),
            recent_failed: Arc::new(Mutex::new(HashSet::new())),
            queue_tx: tx,
            queue_rx: Some(rx),
            queued: Arc::new(RwLock::new(HashSet::new())),
            dirty: Arc::new(RwLock::new(HashSet::new())),
            processing: Arc::new(RwLock::new(HashSet::new())),
            max_retries: 15,
            service_snapshots: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Spawn the consumer worker that processes queued service keys with 200ms debounce window
    fn spawn_consumer(&mut self) {
        if let Some(rx) = self.queue_rx.take() {
            let tx = self.queue_tx.clone();
            let xline_store = self.xline_store.clone();
            let node_registry = self.node_registry.clone();
            let out_of_sync = self.out_of_sync.clone();
            let queued = self.queued.clone();
            let dirty = self.dirty.clone();
            let processing = self.processing.clone();
            let max_retries = self.max_retries;
            let service_snapshots = self.service_snapshots.clone();

            tokio::spawn(async move {
                let mut rx = rx;
                use tokio::sync::Mutex as TokioMutex;
                let attempts = Arc::new(TokioMutex::new(
                    std::collections::HashMap::<String, u32>::new(),
                ));

                while let Some(key) = rx.recv().await {
                    let key_cl = key.clone();

                    // process concurrently
                    let xline_store_c = xline_store.clone();
                    let node_registry_c = node_registry.clone();
                    let out_of_sync_c = out_of_sync.clone();
                    let queued_c = queued.clone();
                    let dirty_c = dirty.clone();
                    let processing_c = processing.clone();
                    let attempts_c = attempts.clone();
                    let tx_c = tx.clone();
                    let max_retries_c = max_retries;
                    let service_snapshots_c = service_snapshots.clone();

                    tokio::spawn(async move {
                        info!("nftables worker start: service_key={key_cl}");

                        // mark start of processing: remove queued, add to processing atomically
                        {
                            let mut qd = queued_c.write().await;
                            let mut p = processing_c.write().await;
                            qd.remove(&key_cl);
                            p.insert(key_cl.clone());
                        }

                        // Fetch current service and endpoint state for incremental processing
                        let res = async {
                            let service_name = key_cl.clone();

                            // Get current service state
                            let current_service_yaml = match xline_store_c.get_service(&service_name).await? {
                                Some(svc) => serde_yaml::to_string(&svc)?,
                                None => {
                                    // Service deleted: prefer snapshot-based incremental cleanup to
                                    // remove stale per-service chains. Fall back to full sync on any
                                    // snapshot/encode error.
                                    info!("nftables worker: service {} not found, trying snapshot-based incremental cleanup", service_name);

                                    let (old_service_yaml, old_endpoint_yaml) = {
                                        let snapshots = service_snapshots_c.read().await;
                                        snapshots.get(&service_name).cloned().unwrap_or_default()
                                    };

                                    let incremental_cleanup_rules = if old_service_yaml.is_empty() {
                                        None
                                    } else {
                                        let old_svc = serde_yaml::from_str::<common::ServiceTask>(&old_service_yaml).ok();
                                        let old_ep = if old_endpoint_yaml.is_empty() {
                                            None
                                        } else {
                                            serde_yaml::from_str::<common::Endpoint>(&old_endpoint_yaml).ok()
                                        };

                                        if let Some(old_svc) = old_svc {
                                            let delete_rules = match generate_service_delete_with_endpoint(&old_svc, old_ep.as_ref()) {
                                                Ok(r) => r,
                                                Err(e) => {
                                                    warn!(
                                                        "nftables worker: failed to generate delete rules for deleted service {}: {}",
                                                        service_name,
                                                        e
                                                    );
                                                    String::new()
                                                }
                                            };

                                            if delete_rules.is_empty() {
                                                None
                                            } else {
                                                let discovery_rules = {
                                                    let (services_raw, _) =
                                                        xline_store_c.services_snapshot_with_rev().await?;
                                                    let mut new_services = Vec::new();
                                                    for (_, yaml) in services_raw {
                                                        match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
                                                            Ok(svc) => new_services.push(svc),
                                                            Err(e) => warn!(
                                                                "nftables worker: failed to parse Service yaml: {}",
                                                                e
                                                            ),
                                                        }
                                                    }

                                                    let mut old_services = new_services.clone();
                                                    if !old_services
                                                        .iter()
                                                        .any(|s| Self::same_service_identity(s, &old_svc))
                                                    {
                                                        old_services.push(old_svc.clone());
                                                    }

                                                    generate_services_discovery_delta(
                                                        &old_services,
                                                        &new_services,
                                                    )?
                                                };

                                                Some(NftablesController::merge_incremental_payloads(&[
                                                    discovery_rules,
                                                    delete_rules,
                                                ])?)
                                            }
                                        } else {
                                            warn!(
                                                "nftables worker: failed to parse cached service snapshot for {}",
                                                service_name
                                            );
                                            None
                                        }
                                    };

                                    if let Some(rules) = incremental_cleanup_rules {
                                        let msg = common::RksMessage::UpdateNftablesRules(rules);
                                        let mode = "incremental_cleanup";
                                        let sessions = node_registry_c.list_sessions().await;
                                        if !sessions.is_empty() {
                                            let mut failed_nodes = Vec::new();

                                            for (node_id, session) in sessions {
                                                if let Err(e) = session.tx.try_send(msg.clone()) {
                                                    warn!(
                                                        "nftables worker: {} broadcast failed for node {} service_key={}: {}",
                                                        mode,
                                                        node_id,
                                                        key_cl,
                                                        e
                                                    );
                                                    failed_nodes.push(node_id);
                                                }
                                            }

                                            if !failed_nodes.is_empty() {
                                                Self::record_out_of_sync_static(&out_of_sync_c, &failed_nodes).await;
                                                return Err(anyhow::anyhow!("broadcast failures for {}", mode));
                                            }
                                        }
                                    } else {
                                        info!("nftables worker: service {} cleanup fallback to full sync", service_name);
                                        let (services_raw, _) = xline_store_c.services_snapshot_with_rev().await?;
                                        let (endpoints_raw, _) = xline_store_c.endpoints_snapshot_with_rev().await?;

                                        let mut services = Vec::new();
                                        for (_, yaml) in services_raw {
                                            match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
                                                Ok(svc) => services.push(svc),
                                                Err(e) => warn!("nftables worker: failed to parse Service yaml: {}", e),
                                            }
                                        }

                                        let mut endpoints = Vec::new();
                                        for (_, yaml) in endpoints_raw {
                                            match serde_yaml::from_str::<common::Endpoint>(&yaml) {
                                                Ok(ep) => endpoints.push(ep),
                                                Err(e) => warn!("nftables worker: failed to parse Endpoint yaml: {}", e),
                                            }
                                        }

                                        let map_init_rules = libnetwork::nftables::generate_verdict_maps_init_raw_json()?;
                                        let full_rules = libnetwork::nftables::generate_nftables_config(&services, &endpoints)?;
                                        let discovery_rules = generate_services_discovery_refresh(&services)?;

                                        NftablesController::broadcast_two_phase_full_rules(
                                            &node_registry_c,
                                            &out_of_sync_c,
                                            map_init_rules,
                                            full_rules,
                                            "worker_deleted_service_full_sync",
                                            discovery_rules.clone(),
                                        )
                                        .await?;

                                        NftablesController::broadcast_discovery_refresh_rules(
                                            &node_registry_c,
                                            &out_of_sync_c,
                                            discovery_rules,
                                            "worker_deleted_service_discovery_refresh",
                                        )
                                        .await?;
                                    }

                                    // Clear snapshot cache for deleted service
                                    let mut snapshots = service_snapshots_c.write().await;
                                    snapshots.remove(&service_name);

                                    return Ok(());
                                }
                            };

                            // Get current endpoint state
                            let current_endpoint_yaml = xline_store_c
                                .get_endpoint_yaml(&service_name)
                                .await?
                                .unwrap_or_default();

                            // Get old snapshot from cache
                            let (old_service_yaml, old_endpoint_yaml) = {
                                let snapshots = service_snapshots_c.read().await;
                                snapshots.get(&service_name).cloned().unwrap_or_default()
                            };

                            // Check if there's actually a change
                            if current_service_yaml == old_service_yaml && current_endpoint_yaml == old_endpoint_yaml {
                                info!("nftables worker: no changes for service_key={}", service_name);
                                return Ok(());
                            }

                            // Parse current state
                            let current_svc: common::ServiceTask = serde_yaml::from_str(&current_service_yaml)?;
                            let current_ep: common::Endpoint = if current_endpoint_yaml.is_empty() {
                                Self::empty_endpoint_for_service(&current_svc)
                            } else {
                                serde_yaml::from_str(&current_endpoint_yaml)?
                            };

                            // Generate rules.
                            let json_rules = if old_service_yaml.is_empty() {
                                // First-seen service path: keep it incremental to avoid
                                // unnecessary full-sync + map_init on every new service event.
                                let service_rules = generate_service_update(&current_svc, &current_ep)?;

                                let (services_raw, _) = xline_store_c.services_snapshot_with_rev().await?;
                                let mut new_services = Vec::new();
                                for (_, yaml) in services_raw {
                                    match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
                                        Ok(svc) => new_services.push(svc),
                                        Err(e) => warn!(
                                            "nftables worker: failed to parse Service yaml: {}",
                                            e
                                        ),
                                    }
                                }

                                // old = snapshot without current service; new = current snapshot.
                                let old_services = new_services
                                    .iter()
                                    .filter(|s| !Self::same_service_identity(s, &current_svc))
                                    .cloned()
                                    .collect::<Vec<_>>();

                                let discovery_rules = generate_services_discovery_delta(
                                    &old_services,
                                    &new_services,
                                )?;

                                NftablesController::merge_incremental_payloads(&[
                                    service_rules,
                                    discovery_rules,
                                ])?
                            } else {
                                // Have previous state - check if spec changed
                                let old_svc: common::ServiceTask = serde_yaml::from_str(&old_service_yaml)?;

                                // If service spec changed, fallback to full sync.
                                // This avoids nftables transaction conflicts from delete+recreate.
                                if old_svc.spec != current_svc.spec {
                                    info!(
                                        "nftables worker: service spec changed for service_key={}; falling back to full sync",
                                        service_name
                                    );
                                    let (services_raw, _) = xline_store_c.services_snapshot_with_rev().await?;
                                    let (endpoints_raw, _) = xline_store_c.endpoints_snapshot_with_rev().await?;

                                    let mut services = Vec::new();
                                    for (_, yaml) in services_raw {
                                        match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
                                            Ok(svc) => services.push(svc),
                                            Err(e) => warn!("nftables worker: failed to parse Service yaml: {}", e),
                                        }
                                    }

                                    let mut endpoints = Vec::new();
                                    for (_, yaml) in endpoints_raw {
                                        match serde_yaml::from_str::<common::Endpoint>(&yaml) {
                                            Ok(ep) => endpoints.push(ep),
                                            Err(e) => warn!("nftables worker: failed to parse Endpoint yaml: {}", e),
                                        }
                                    }

                                    let map_init_rules = libnetwork::nftables::generate_verdict_maps_init_raw_json()?;
                                    let full_rules = libnetwork::nftables::generate_nftables_config(&services, &endpoints)?;
                                    let discovery_rules = generate_services_discovery_refresh(&services)?;

                                    NftablesController::broadcast_two_phase_full_rules(
                                        &node_registry_c,
                                        &out_of_sync_c,
                                        map_init_rules,
                                        full_rules,
                                        "worker_spec_change_full_sync",
                                        discovery_rules.clone(),
                                    )
                                    .await?;

                                    NftablesController::broadcast_discovery_refresh_rules(
                                        &node_registry_c,
                                        &out_of_sync_c,
                                        discovery_rules,
                                        "worker_spec_change_discovery_refresh",
                                    )
                                    .await?;

                                    // Update snapshot cache with new state
                                    {
                                        let mut snapshots = service_snapshots_c.write().await;
                                        snapshots.insert(service_name.clone(), (current_service_yaml, current_endpoint_yaml));
                                    }

                                    return Ok(());
                                }

                                // Service spec unchanged: use endpoint-diff incremental update only.
                                // This avoids delete+recreate for service/mark chains and prevents busy errors.
                                let old_ep: common::Endpoint = if old_endpoint_yaml.is_empty() {
                                    Self::empty_endpoint_for_service(&old_svc)
                                } else {
                                    serde_yaml::from_str(&old_endpoint_yaml)?
                                };

                                generate_service_update_with_old_endpoint(&current_svc, &old_ep, &current_ep)?
                            };

                            // Broadcast incremental rules
                            let sessions = node_registry_c.list_sessions().await;
                            if sessions.is_empty() {
                                info!("nftables worker: no sessions available for service_key={}", service_name);
                            } else {
                                let msg = common::RksMessage::UpdateNftablesRules(json_rules);
                                let mut failed_nodes = Vec::new();

                                for (node_id, session) in sessions {
                                    if let Err(e) = session.tx.try_send(msg.clone()) {
                                        warn!("nftables worker: incremental broadcast failed for node {} service_key={}: {}", node_id, service_name, e);
                                        failed_nodes.push(node_id);
                                    }
                                }

                                if !failed_nodes.is_empty() {
                                    let mut set = out_of_sync_c.lock().await;
                                    for n in failed_nodes {
                                        set.insert(n);
                                    }
                                    return Err(anyhow::anyhow!("broadcast failures on service_key={}", service_name));
                                }
                            }

                            // Update snapshot cache with new state
                            {
                                let mut snapshots = service_snapshots_c.write().await;
                                snapshots.insert(service_name.clone(), (current_service_yaml, current_endpoint_yaml));
                            }

                            Ok(())
                        }
                        .await;

                        if let Err(err) = res {
                            // retry logic
                            let mut at = attempts_c.lock().await;
                            let (cnt, should_retry) = {
                                let entry = at.entry(key_cl.clone()).or_insert(0);
                                *entry += 1;
                                (*entry, *entry <= max_retries_c)
                            };

                            if should_retry {
                                let backoff =
                                    200u64.saturating_mul(2u64.saturating_pow(cnt)).min(30_000);
                                let tx_retry = tx_c.clone();
                                let key_retry = key_cl.clone();
                                info!(
                                    "nftables worker: processing failed service_key={key_cl} attempt={cnt} backoff_ms={backoff} err={err:?}"
                                );
                                tokio::spawn(async move {
                                    sleep(Duration::from_millis(backoff)).await;
                                    let _ = tx_retry.send(key_retry).await;
                                });
                            } else {
                                // Clear retry budget after permanent failure to allow future updates to retry.
                                at.remove(&key_cl);
                                warn!(
                                    "nftables worker: processing permanently failed service_key={key_cl} attempts={cnt} err={err:?}"
                                );
                            }
                        } else {
                            // success: clear attempts
                            let mut at = attempts_c.lock().await;
                            at.remove(&key_cl);
                            info!("nftables worker: processing succeeded service_key={key_cl}");
                        }

                        // remove from processing
                        info!("nftables worker finish: service_key={key_cl}");
                        processing_c.write().await.remove(&key_cl);

                        // if we saw updates while processing (dirty), requeue once
                        let had_dirty = {
                            let mut d = dirty_c.write().await;
                            d.remove(&key_cl)
                        };

                        if had_dirty {
                            let mut at = attempts_c.lock().await;
                            if at.remove(&key_cl).is_some() {
                                info!(
                                    "nftables worker: reset attempts due to dirty requeue service_key={key_cl}"
                                );
                            }

                            let should_enqueue = {
                                let mut q = queued_c.write().await;
                                if q.contains(&key_cl) {
                                    false
                                } else {
                                    q.insert(key_cl.clone());
                                    true
                                }
                            };

                            if should_enqueue {
                                if let Err(e) = tx_c.send(key_cl.clone()).await {
                                    queued_c.write().await.remove(&key_cl);
                                    warn!(
                                        "nftables worker: failed to requeue dirty service_key={} err={}",
                                        key_cl, e
                                    );
                                } else {
                                    info!(
                                        "nftables worker: requeue due to dirty service_key={key_cl}"
                                    );
                                }
                            }
                        }
                    });
                }
            });
        }
    }

    async fn sync_rules(&self) -> Result<()> {
        let (services_raw, _srev) = self.xline_store.services_snapshot_with_rev().await?;
        let (endpoints_raw, _erev) = self.xline_store.endpoints_snapshot_with_rev().await?;

        let mut services = Vec::new();
        for (key, yaml) in services_raw {
            match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
                Ok(svc) => services.push(svc),
                Err(e) => warn!("Failed to parse Service {}: {}", key, e),
            }
        }

        let mut endpoints = Vec::new();
        for (key, yaml) in endpoints_raw {
            match serde_yaml::from_str::<common::Endpoint>(&yaml) {
                Ok(ep) => endpoints.push(ep),
                Err(e) => warn!("Failed to parse Endpoint {}: {}", key, e),
            }
        }

        // Generate two-phase full sync payloads:
        // 1) regular full ruleset payload (contains flush table)
        // 2) raw verdict-map initialization
        let map_init_rules = libnetwork::nftables::generate_verdict_maps_init_raw_json()?;
        let full_rules = libnetwork::nftables::generate_nftables_config(&services, &endpoints)?;
        let discovery_rules = generate_services_discovery_refresh(&services)?;

        // Seed snapshot cache to prevent "first-seen" service updates on subsequent watch events
        // after full sync has already created the chains.
        {
            let mut snapshots = self.service_snapshots.write().await;
            for svc in &services {
                let service_key = svc.metadata.name.clone();
                let svc_yaml = serde_yaml::to_string(svc).unwrap_or_default();
                let ep_yaml = endpoints
                    .iter()
                    .find(|e| e.metadata.name == svc.metadata.name)
                    .map(|e| serde_yaml::to_string(e).unwrap_or_default())
                    .unwrap_or_default();
                snapshots.insert(service_key, (svc_yaml, ep_yaml));
            }
        }

        self.broadcast_full_rules(map_init_rules, full_rules, discovery_rules.clone())
            .await?;

        let _ = self.broadcast_incremental_rules(discovery_rules).await?;

        Ok(())
    }

    async fn broadcast_full_rules(
        &self,
        map_init_rules: String,
        full_rules: String,
        discovery_rules: String,
    ) -> Result<()> {
        Self::broadcast_two_phase_full_rules(
            &self.node_registry,
            &self.out_of_sync,
            map_init_rules,
            full_rules,
            "controller_full_sync",
            discovery_rules,
        )
        .await
    }

    async fn broadcast_two_phase_full_rules(
        node_registry: &Arc<NodeRegistry>,
        out_of_sync: &Arc<Mutex<HashSet<String>>>,
        map_init_rules: String,
        full_rules: String,
        reason: &str,
        discovery_rules: String,
    ) -> Result<()> {
        let sessions = node_registry.list_sessions().await;
        if sessions.is_empty() {
            info!(
                "Broadcasting two-phase nftables rules skipped: no worker nodes connected, reason={}",
                reason
            );
            return Ok(());
        }

        info!(
            "Broadcasting two-phase full rules to {} nodes reason={} map_init_len={} full_len={}",
            sessions.len(),
            reason,
            map_init_rules.len(),
            full_rules.len()
        );

        let full_msg = common::RksMessage::SetNftablesRules(full_rules.clone());
        let map_init_msg = common::RksMessage::SetNftablesRules(map_init_rules.clone());
        let mut failed_nodes = Vec::new();

        const SEND_TIMEOUT: Duration = Duration::from_secs(5);

        for (node_id, session) in sessions {
            info!(
                "two-phase broadcast phase=full_rules node={} reason={}",
                node_id, reason
            );

            let res = tokio::time::timeout(SEND_TIMEOUT, session.tx.send(full_msg.clone())).await;
            if let Err(_) | Ok(Err(_)) = res {
                warn!(
                    "two-phase broadcast failed at phase=full_rules node={} reason={} err=timeout or closed",
                    node_id, reason
                );
                failed_nodes.push(node_id);
                continue;
            }

            info!(
                "two-phase broadcast phase=map_init node={} reason={}",
                node_id, reason
            );
            let res =
                tokio::time::timeout(SEND_TIMEOUT, session.tx.send(map_init_msg.clone())).await;
            if let Err(_) | Ok(Err(_)) = res {
                warn!(
                    "two-phase broadcast failed at phase=map_init node={} reason={} err=timeout or closed",
                    node_id, reason
                );
                failed_nodes.push(node_id);
            }
        }

        if !failed_nodes.is_empty() {
            Self::record_out_of_sync_static(out_of_sync, &failed_nodes).await;

            Self::retry_two_phase_broadcast(
                node_registry.clone(),
                out_of_sync.clone(),
                failed_nodes,
                map_init_rules,
                full_rules,
                reason.to_string(),
                discovery_rules,
            );

            warn!(
                "two-phase broadcast had partial failures for reason={}; queued retries for failed nodes and continuing with discovery refresh for healthy nodes",
                reason
            );
        }

        Ok(())
    }

    fn retry_two_phase_broadcast(
        node_registry: Arc<NodeRegistry>,
        out_of_sync: Arc<Mutex<HashSet<String>>>,
        failed_nodes: Vec<String>,
        map_init_rules: String,
        full_rules: String,
        reason: String,
        discovery_rules: String,
    ) {
        const MAX_ATTEMPTS: usize = 3;
        const BACKOFF_MS: u64 = 300;

        tokio::spawn(async move {
            let mut pending: HashSet<String> = failed_nodes.into_iter().collect();
            let full_msg = common::RksMessage::SetNftablesRules(full_rules);
            let map_init_msg = common::RksMessage::SetNftablesRules(map_init_rules);
            let discovery_msg = common::RksMessage::UpdateNftablesRules(discovery_rules);

            for attempt in 1..=MAX_ATTEMPTS {
                if pending.is_empty() {
                    return;
                }

                sleep(Duration::from_millis(BACKOFF_MS * attempt as u64)).await;

                let nodes: Vec<String> = pending.iter().cloned().collect();
                let mut successes = Vec::new();

                for node_id in nodes {
                    match node_registry.get(&node_id).await {
                        Some(session) => {
                            if let Err(e) = session.tx.try_send(full_msg.clone()) {
                                warn!(
                                    "two-phase retry {}/{} failed at phase=full_rules node={} reason={} err={}",
                                    attempt, MAX_ATTEMPTS, node_id, reason, e
                                );
                                continue;
                            }

                            if let Err(e) = session.tx.try_send(map_init_msg.clone()) {
                                warn!(
                                    "two-phase retry {}/{} failed at phase=map_init node={} reason={} err={}",
                                    attempt, MAX_ATTEMPTS, node_id, reason, e
                                );
                                continue;
                            }

                            // Replay discovery refresh immediately after successful full rules + map_init
                            if let Err(e) = session.tx.try_send(discovery_msg.clone()) {
                                warn!(
                                    "two-phase retry {}/{} failed at phase=discovery_refresh node={} reason={} err={}",
                                    attempt, MAX_ATTEMPTS, node_id, reason, e
                                );
                                continue;
                            }

                            successes.push(node_id);
                        }
                        None => {
                            warn!(
                                "two-phase retry {}/{} no active session node={} reason={}",
                                attempt, MAX_ATTEMPTS, node_id, reason
                            );
                        }
                    }
                }

                if !successes.is_empty() {
                    let mut set = out_of_sync.lock().await;
                    for node_id in successes {
                        pending.remove(&node_id);
                        set.remove(&node_id);
                    }
                }
            }

            if !pending.is_empty() {
                warn!(
                    "two-phase retries exhausted for reason={}; nodes remain out-of-sync: {:?}",
                    reason, pending
                );
            }
        });
    }

    async fn broadcast_discovery_refresh_rules(
        node_registry: &Arc<NodeRegistry>,
        out_of_sync: &Arc<Mutex<HashSet<String>>>,
        json_rules: String,
        reason: &str,
    ) -> Result<()> {
        let sessions = node_registry.list_sessions().await;
        if sessions.is_empty() {
            return Ok(());
        }

        let msg = common::RksMessage::UpdateNftablesRules(json_rules);
        let mut failed_nodes = Vec::new();

        const SEND_TIMEOUT: Duration = Duration::from_secs(5);

        for (node_id, session) in sessions {
            let res = tokio::time::timeout(SEND_TIMEOUT, session.tx.send(msg.clone())).await;
            if let Err(_) | Ok(Err(_)) = res {
                warn!(
                    "discovery refresh broadcast failed node={} reason={} err=timeout or closed",
                    node_id, reason
                );
                failed_nodes.push(node_id);
            }
        }

        if !failed_nodes.is_empty() {
            let mut set = out_of_sync.lock().await;
            for n in failed_nodes {
                set.insert(n);
            }
            return Err(anyhow!(
                "discovery refresh broadcast failures for {}",
                reason
            ));
        }

        Ok(())
    }

    async fn broadcast_incremental_rules(&self, json_rules: String) -> Result<bool> {
        let sessions = self.node_registry.list_sessions().await;
        if sessions.is_empty() {
            info!("Broadcasting nftables incremental rules skipped: no worker nodes connected");
            return Ok(false);
        }

        info!(
            "Broadcasting incremental nftables rules to {} nodes (len={})",
            sessions.len(),
            json_rules.len()
        );

        let msg = common::RksMessage::UpdateNftablesRules(json_rules);

        self.broadcast_message(msg).await
    }

    async fn broadcast_message(&self, msg: common::RksMessage) -> Result<bool> {
        let sessions = self.node_registry.list_sessions().await;
        if sessions.is_empty() {
            info!("Broadcasting nftables rules skipped: no worker nodes connected");
            return Ok(false);
        }

        let mut failed_nodes = Vec::new();

        const SEND_TIMEOUT: Duration = Duration::from_secs(5);

        for (node_id, session) in sessions {
            let res = tokio::time::timeout(SEND_TIMEOUT, session.tx.send(msg.clone())).await;
            if let Err(_) | Ok(Err(_)) = res {
                warn!(
                    "Failed to send rules to node {} (timeout or closed)",
                    node_id
                );
                failed_nodes.push(node_id);
            }
        }

        if !failed_nodes.is_empty() {
            let mut rf = self.recent_failed.lock().await;
            for node_id in &failed_nodes {
                rf.insert(node_id.clone());
            }
            // Start retry background task targeting only these failed nodes
            Self::retry_broadcast_static(
                self.node_registry.clone(),
                self.recent_failed.clone(),
                msg,
            )
            .await;
        }
        Ok(failed_nodes.is_empty())
    }

    async fn record_out_of_sync_static(tracker: &Arc<Mutex<HashSet<String>>>, nodes: &[String]) {
        let mut set = tracker.lock().await;
        for n in nodes {
            set.insert(n.clone());
        }
    }

    async fn retry_broadcast_static(
        registry: Arc<NodeRegistry>,
        tracker: Arc<Mutex<HashSet<String>>>,
        msg: common::RksMessage,
    ) {
        const MAX_ATTEMPTS: usize = 3;
        const BACKOFF_MS: u64 = 300;

        tokio::spawn(async move {
            for attempt in 1..=MAX_ATTEMPTS {
                // simple linear backoff
                sleep(Duration::from_millis(BACKOFF_MS * attempt as u64)).await;

                let nodes: Vec<String> = {
                    let set: tokio::sync::MutexGuard<'_, HashSet<String>> = tracker.lock().await;
                    set.iter().cloned().collect()
                };

                if nodes.is_empty() {
                    return;
                }

                let mut successes = Vec::new();
                for node_id in nodes {
                    match registry.get(&node_id).await {
                        Some(session) => {
                            let res = tokio::time::timeout(
                                Duration::from_secs(5),
                                session.tx.send(msg.clone()),
                            )
                            .await;
                            if let Ok(Ok(_)) = res {
                                info!(
                                    "Retry {}/{} succeeded sending rules to node {}",
                                    attempt, MAX_ATTEMPTS, node_id
                                );
                                successes.push(node_id);
                            } else {
                                warn!(
                                    "Retry {}/{} failed to send rules to node {}",
                                    attempt, MAX_ATTEMPTS, node_id
                                );
                            }
                        }
                        None => {
                            warn!(
                                "Retry {}/{}: no active session for node {}",
                                attempt, MAX_ATTEMPTS, node_id
                            );
                        }
                    }
                }

                if !successes.is_empty() {
                    let mut set: tokio::sync::MutexGuard<'_, HashSet<String>> =
                        tracker.lock().await;
                    for n in successes {
                        set.remove(&n);
                    }
                }

                // If all cleared, stop early
                if tracker.lock().await.is_empty() {
                    return;
                }
            }

            let remaining: Vec<String> = {
                let set: tokio::sync::MutexGuard<'_, HashSet<String>> = tracker.lock().await;
                set.iter().cloned().collect()
            };
            if !remaining.is_empty() {
                warn!(
                    "Exhausted retries for nodes (recent failed): {:?}; incremental rules may be missing",
                    remaining
                );
            }
        });
    }

    fn empty_endpoint_for_service(svc: &common::ServiceTask) -> common::Endpoint {
        common::Endpoint {
            api_version: "v1".to_string(),
            kind: "Endpoints".to_string(),
            metadata: svc.metadata.clone(),
            subsets: Vec::new(),
        }
    }

    fn same_service_identity(a: &common::ServiceTask, b: &common::ServiceTask) -> bool {
        a.metadata.namespace == b.metadata.namespace && a.metadata.name == b.metadata.name
    }

    async fn schedule_enqueue(&self, key: String, delay: Duration) {
        let q = self.queue_tx.clone();
        let queued_c = self.queued.clone();
        let processing_c = self.processing.clone();
        let dirty_c = self.dirty.clone();

        // spawn a timer for delayed enqueue (delay may be zero)
        tokio::spawn(async move {
            sleep(delay).await;

            // If the key is currently being processed, mark it dirty so worker
            // requeues once after finishing. Do not enqueue directly here.
            let in_processing = { processing_c.read().await.contains(&key) };
            if in_processing {
                let inserted = { dirty_c.write().await.insert(key.clone()) };
                if inserted {
                    info!("nftables: marked dirty while processing service_key={key}");
                } else {
                    info!("nftables: enqueue dedupe (already dirty) service_key={key}");
                }
                info!("nftables: in processing; will requeue later service_key={key}");
                return;
            }

            // Deduplicate delayed enqueues atomically under one write lock.
            let should_enqueue = {
                let mut qd = queued_c.write().await;
                if qd.contains(&key) {
                    false
                } else {
                    qd.insert(key.clone());
                    true
                }
            };

            if !should_enqueue {
                info!("nftables: enqueue dedupe (already queued) service_key={key}");
                return;
            }

            if let Err(e) = q.send(key.clone()).await {
                queued_c.write().await.remove(&key);
                warn!("nftables: failed to enqueue service_key={} err={}", key, e);
            } else {
                info!(
                    "nftables: enqueue service_key={key} delay_ms={}",
                    delay.as_millis()
                );
            }
        });
    }

    fn merge_incremental_payloads(payloads: &[String]) -> Result<String> {
        let mut merged = Vec::new();

        for payload in payloads {
            let value: Value = serde_json::from_str(payload).map_err(|e| {
                anyhow!(
                    "failed to parse incremental nftables payload as json: {}",
                    e
                )
            })?;

            let items = value
                .get("nftables")
                .and_then(Value::as_array)
                .ok_or_else(|| anyhow!("incremental nftables payload missing `nftables` array"))?;

            merged.extend(items.iter().cloned());
        }

        serde_json::to_string(&json!({ "nftables": merged })).map_err(|e| anyhow!(e))
    }
}

#[async_trait]
impl Controller for NftablesController {
    fn name(&self) -> &'static str {
        "nftables-controller"
    }

    async fn init(&mut self) -> Result<()> {
        // Spawn the queue consumer worker for 200ms debounce
        self.spawn_consumer();

        info!("Initializing NftablesController, performing initial full sync...");
        self.sync_rules().await
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![ResourceKind::Service, ResourceKind::Endpoint]
    }

    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        info!(
            "NftablesController: received watch event kind={:?} event={:?} key={}",
            response.kind, response.event, response.key
        );

        // Extract service key from the event. For both Service and Endpoint resources,
        // the key identifies the service (Endpoint key = Service name).
        let service_key = response.key.clone();

        // Schedule the service key for processing with 200ms debounce window.
        // All event processing now happens in the spawn_consumer worker.
        self.schedule_enqueue(service_key, Duration::from_millis(200))
            .await;

        Ok(())
    }
}

pub async fn build_rules(xline_store: &XlineStore) -> Result<String> {
    // Use snapshot helpers to avoid many RPCs
    let (services_raw, _srev) = xline_store.services_snapshot_with_rev().await?;
    let (endpoints_raw, _erev) = xline_store.endpoints_snapshot_with_rev().await?;

    let mut services = Vec::new();
    for (key, yaml) in services_raw {
        match serde_yaml::from_str::<common::ServiceTask>(&yaml) {
            Ok(svc) => services.push(svc),
            Err(e) => warn!("Failed to parse Service {}: {}", key, e),
        }
    }

    let mut endpoints = Vec::new();
    for (key, yaml) in endpoints_raw {
        match serde_yaml::from_str::<common::Endpoint>(&yaml) {
            Ok(ep) => endpoints.push(ep),
            Err(e) => warn!("Failed to parse Endpoint {}: {}", key, e),
        }
    }

    generate_nftables_config(&services, &endpoints)
}

// Re-export generation functions from libnetwork for tests
pub use libnetwork::nftables::generate_nftables_config;
