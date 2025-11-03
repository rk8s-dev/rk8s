use crate::api::xlinestore::XlineStore;
use crate::controllers::Controller;
use anyhow::Result;
use async_trait::async_trait;
use common::{LabelSelectorOperator, PodTask, PodTemplateSpec, ReplicaSet};
use rand::random;
use std::sync::Arc;

pub struct ReplicaSetController {}

impl ReplicaSetController {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for ReplicaSetController {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplicaSetController {
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
    pub async fn reconcile(&self, rs: &mut ReplicaSet, store: &XlineStore) -> Result<()> {
        let pods = store.list_pods().await?;
        let mut matching: Vec<PodTask> = pods
            .into_iter()
            .filter(|p| Self::selector_match(rs, p))
            .collect();

        let actual = matching.len() as i32;
        let desired = rs.spec.replicas;

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
                let name = Self::generate_unique_name(&rs.metadata.name, store).await?;
                pod.metadata.name = name.clone();
                // ensure selector labels present on pod
                for (k, v) in rs.spec.selector.match_labels.iter() {
                    pod.metadata.labels.insert(k.clone(), v.clone());
                }
                let yaml = serde_yaml::to_string(&pod)?;
                store.insert_pod_yaml(&name, &yaml).await?;
                matching.push(pod);
            }
        } else if actual > desired {
            let to_delete = (actual - desired) as usize;
            // prefer deleting pods that are not ready
            matching.sort_by_key(|p| p.status.pod_ip.is_some()); // not ready first (None => true sorts before)
            for pod in matching.into_iter().take(to_delete) {
                let pod_name = pod.metadata.name.clone();
                store.delete_pod(&pod_name).await?;
            }
        }

        Ok(())
    }

    // Implement Controller trait wrapper: load ReplicaSet by name then call reconcile above and persist status.
    pub async fn reconcile_by_name(&self, key: &str, store: Arc<XlineStore>) -> Result<()> {
        if let Some(yaml) = store.get_replicaset_yaml(key).await? {
            let mut rs: ReplicaSet = serde_yaml::from_str(&yaml)?;
            // perform reconcile logic
            self.reconcile(&mut rs, &store).await?;
            // persist updated replicaset (including status)
            let new_yaml = serde_yaml::to_string(&rs)?;
            store
                .insert_replicaset_yaml(&rs.metadata.name, &new_yaml)
                .await?;
        }
        Ok(())
    }
}

#[async_trait]
impl Controller for ReplicaSetController {
    fn name(&self) -> &'static str {
        "replicaset"
    }

    async fn reconcile(&self, key: &str, store: Arc<XlineStore>) -> Result<()> {
        self.reconcile_by_name(key, store).await
    }
}
