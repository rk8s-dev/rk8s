use crate::api::xlinestore::XlineStore;
use crate::controllers::Controller;
use crate::controllers::manager::{ResourceWatchResponse, WatchEvent};
use anyhow::Result;
use async_trait::async_trait;
use common::{
    LabelSelectorOperator, OwnerReference, PodTask, PodTemplateSpec, ReplicaSet, ResourceKind,
};
use rand::random;
use std::collections::HashSet;
use std::sync::Arc;
use uuid::Uuid;

pub struct ReplicaSetController {
    store: Arc<XlineStore>,
}

impl ReplicaSetController {
    pub fn new(store: Arc<XlineStore>) -> Self {
        Self { store }
    }

    /// Match selector: supports MatchLabels and MatchExpressions.
    pub fn selector_match(rs: &ReplicaSet, pod: &PodTask) -> bool {
        // namespace must match
        if rs.metadata.namespace != pod.metadata.namespace {
            return false;
        }
        let sel = &rs.spec.selector;
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

    fn owns_or_can_adopt_pod(rs: &ReplicaSet, pod: &PodTask) -> bool {
        match pod.metadata.owner_references.as_ref() {
            Some(owners) => owners.iter().any(|owner| {
                if owner.kind != ResourceKind::ReplicaSet {
                    return false;
                }

                if owner.uid == rs.metadata.uid {
                    return true;
                }

                owner.controller && owner.name == rs.metadata.name
            }),
            None => true,
        }
    }

    /// Generate a unique pod name based on base name and random suffix.
    pub async fn generate_unique_name(base: &str, store: &XlineStore) -> Result<String> {
        loop {
            let rnd: u32 = random();
            let name = format!("{}-{:08x}", base, rnd);

            if store.get_pod_yaml(&name).await?.is_none() {
                return Ok(name);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// Reconcile given ReplicaSet: ensure desired number of pods exist, update status.
    pub async fn reconcile(&self, rs: &mut ReplicaSet) -> Result<()> {
        let pods = self.store.list_pods().await?;

        // Separate owned pods and orphan pods using owns_or_can_adopt_pod
        let mut owned_pods = Vec::new();
        let mut orphan_pods = Vec::new();

        for pod in pods {
            if !Self::selector_match(rs, &pod) {
                continue;
            }

            // Check if this pod is already owned by this ReplicaSet
            let is_owned = pod
                .metadata
                .owner_references
                .as_ref()
                .is_some_and(|owners| {
                    owners.iter().any(|owner| {
                        owner.kind == ResourceKind::ReplicaSet && owner.uid == rs.metadata.uid
                    })
                });

            if is_owned {
                owned_pods.push(pod);
            } else if Self::owns_or_can_adopt_pod(rs, &pod) {
                // Can adopt but not yet owned (orphan pod)
                orphan_pods.push(pod);
            }
            // else: owned by other ReplicaSet, ignore
        }

        let mut matching = owned_pods.clone();
        let desired = rs.spec.replicas;

        log::debug!(
            "ReplicaSet {} reconcile start: owned={} orphans={} desired={}",
            rs.metadata.name,
            owned_pods.len(),
            orphan_pods.len(),
            desired
        );

        // Always claim ALL matching orphan pods, regardless of replica count
        for orphan_pod in orphan_pods {
            // Add ownerReference to orphan pod
            let mut pod = orphan_pod.clone();
            pod.metadata.owner_references = Some(vec![OwnerReference {
                api_version: rs.api_version.clone(),
                kind: ResourceKind::ReplicaSet,
                name: rs.metadata.name.clone(),
                uid: rs.metadata.uid,
                controller: true,
                block_owner_deletion: Some(true),
            }]);

            let yaml = serde_yaml::to_string(&pod)?;
            self.store
                .insert_pod_yaml(&pod.metadata.name, &yaml)
                .await?;
            log::info!(
                "ReplicaSet {} adopted orphan pod {}",
                rs.metadata.name,
                pod.metadata.name
            );

            matching.push(pod);
        }

        let actual = matching.len() as i32;

        // update status counters
        rs.status.replicas = actual;
        rs.status.fully_labeled_replicas = matching.len() as i32;

        // Now consider all matching Pods as ready
        // TODO: replace with real readiness check after integrating kubelet PodStatus updates
        rs.status.ready_replicas = matching.len() as i32;
        rs.status.available_replicas = matching.len() as i32;

        if actual < desired {
            let to_create = (desired - actual) as usize;
            for _ in 0..to_create {
                let tpl: PodTemplateSpec = rs.spec.template.clone();
                // build PodTask from template
                let mut pod = PodTask {
                    api_version: "v1".to_string(),
                    kind: "Pod".to_string(),
                    metadata: tpl.metadata.clone(),
                    spec: tpl.spec.clone(),
                    status: Default::default(),
                };
                // ensure name unique
                let name =
                    Self::generate_unique_name(&rs.metadata.name, self.store.as_ref()).await?;
                pod.metadata.name = name.clone();
                // ensure uid unique
                pod.metadata.uid = Uuid::new_v4();
                // ensure selector labels present on pod
                for (k, v) in rs.spec.selector.match_labels.iter() {
                    pod.metadata.labels.insert(k.clone(), v.clone());
                }
                pod.metadata.owner_references = Some(vec![OwnerReference {
                    api_version: rs.api_version.clone(),
                    kind: ResourceKind::ReplicaSet,
                    name: rs.metadata.name.clone(),
                    uid: rs.metadata.uid,
                    controller: true,
                    block_owner_deletion: Some(true),
                }]);
                let yaml = serde_yaml::to_string(&pod)?;
                self.store.insert_pod_yaml(&name, &yaml).await?;
                log::debug!(
                    "ReplicaSet {} created pod {} while reconciling",
                    rs.metadata.name,
                    name
                );
                matching.push(pod);
            }
        } else if actual > desired {
            let to_delete = (actual - desired) as usize;
            // prefer deleting pods that are not ready
            matching.sort_by_key(|p| p.status.pod_ip.is_some()); // not ready first (None => true sorts before)
            for pod in matching.into_iter().take(to_delete) {
                let pod_name = pod.metadata.name.clone();
                self.store.delete_pod(&pod_name).await?;
                log::info!(
                    "ReplicaSet {} deleted pod {} while reconciling",
                    rs.metadata.name,
                    pod_name
                );
            }
        }

        Ok(())
    }

    // Implement Controller trait wrapper: load ReplicaSet by name then call reconcile above and persist status.
    pub async fn reconcile_by_name(&self, key: &str) -> Result<()> {
        let mut attempts = 0;
        loop {
            let Some((yaml, revision)) = self.store.get_replicaset_yaml_with_revision(key).await?
            else {
                return Ok(());
            };

            let mut rs: ReplicaSet = serde_yaml::from_str(&yaml)?;
            let name = rs.metadata.name.clone();

            self.reconcile(&mut rs).await?;

            let new_yaml = serde_yaml::to_string(&rs)?;
            if self
                .store
                .compare_and_set_replicaset_yaml(&name, revision, &new_yaml)
                .await?
            {
                return Ok(());
            }

            attempts += 1;
            if attempts >= 5 {
                log::warn!(
                    "ReplicaSetController reconcile_by_name {} failed due to concurrent updates",
                    key
                );
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }
}

#[async_trait]
impl Controller for ReplicaSetController {
    fn name(&self) -> &'static str {
        "replicaset"
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![ResourceKind::ReplicaSet, ResourceKind::Pod]
    }

    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        match response.kind {
            ResourceKind::ReplicaSet => {
                log::debug!(
                    "ReplicaSetController handling ReplicaSet event: key={}",
                    response.key
                );
                // reconcile on Add events or when the spec has changed
                let mut should_reconcile = false;
                match &response.event {
                    WatchEvent::Add { yaml: _ } => {
                        should_reconcile = true;
                    }
                    WatchEvent::Update { old_yaml, new_yaml } => {
                        let old_rs: ReplicaSet = serde_yaml::from_str(old_yaml)?;
                        let new_rs: ReplicaSet = serde_yaml::from_str(new_yaml)?;
                        if old_rs.spec != new_rs.spec {
                            should_reconcile = true;
                        }
                    }
                    WatchEvent::Delete { yaml: _ } => {
                        should_reconcile = false;
                    }
                }
                if should_reconcile {
                    self.reconcile_by_name(&response.key).await?;
                }
            }
            ResourceKind::Pod => {
                log::debug!(
                    "ReplicaSetController handling Pod event: key={}",
                    response.key
                );
                let mut pods_to_check: Vec<PodTask> = Vec::new();
                match &response.event {
                    WatchEvent::Add { yaml } => {
                        pods_to_check.push(serde_yaml::from_str::<PodTask>(yaml)?);
                    }
                    WatchEvent::Update { old_yaml, new_yaml } => {
                        pods_to_check.push(serde_yaml::from_str::<PodTask>(new_yaml)?);
                        pods_to_check.push(serde_yaml::from_str::<PodTask>(old_yaml)?);
                    }
                    WatchEvent::Delete { yaml } => {
                        pods_to_check.push(serde_yaml::from_str::<PodTask>(yaml)?);
                    }
                }

                if pods_to_check.is_empty() {
                    return Ok(());
                }

                let mut reconciled: HashSet<String> = HashSet::new();
                let mut replicasets_cache: Option<Vec<ReplicaSet>> = None;

                for pod in pods_to_check.iter() {
                    let mut owner_triggered = false;

                    if let Some(owner_refs) = pod.metadata.owner_references.as_ref() {
                        for owner in owner_refs
                            .iter()
                            .filter(|o| o.kind == ResourceKind::ReplicaSet)
                        {
                            owner_triggered = true;
                            if reconciled.insert(owner.name.clone()) {
                                log::debug!(
                                    "Pod {} owned by ReplicaSet {}, triggering reconcile",
                                    pod.metadata.name,
                                    owner.name
                                );
                                self.reconcile_by_name(&owner.name).await?;
                            }
                        }
                    }

                    if owner_triggered {
                        continue;
                    }

                    let replicasets = match replicasets_cache.as_ref() {
                        Some(rs) => rs,
                        None => {
                            let rs = self.store.list_replicasets().await?;
                            log::debug!(
                                "ReplicaSetController label-matching Pod event against {} ReplicaSets",
                                rs.len()
                            );
                            replicasets_cache = Some(rs);
                            replicasets_cache.as_ref().unwrap()
                        }
                    };

                    for rs in replicasets.iter() {
                        if Self::selector_match(rs, pod)
                            && reconciled.insert(rs.metadata.name.clone())
                        {
                            log::debug!(
                                "Pod {} label-matched ReplicaSet {}, triggering reconcile",
                                pod.metadata.name,
                                rs.metadata.name
                            );
                            self.reconcile_by_name(&rs.metadata.name).await?;
                        }
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}
