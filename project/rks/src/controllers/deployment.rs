use crate::api::xlinestore::XlineStore;
use crate::controllers::manager::{Controller, ResourceWatchResponse, WatchEvent};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::Utc;
use common::*;
use log::{debug, error, info};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

pub struct DeploymentController {
    store: Arc<XlineStore>,
}

impl DeploymentController {
    pub fn new(store: Arc<XlineStore>) -> Self {
        Self { store }
    }

    /// Reconcile a single deployment by name
    async fn reconcile_by_name(&self, name: &str) -> Result<()> {
        let yaml = self.store.get_deployment_yaml(name).await?;

        if yaml.is_none() {
            info!("Deployment {} not found, skipping reconciliation", name);
            return Ok(());
        }

        let deployment: Deployment = serde_yaml::from_str(&yaml.unwrap())?;
        self.reconcile_deployment(deployment).await
    }

    async fn reconcile_deployment(&self, deployment: Deployment) -> Result<()> {
        let deploy_name = deployment.metadata.name.clone();
        // Check if deployment is being deleted
        if deployment.metadata.deletion_timestamp.is_some() {
            info!("Deployment {} is being deleted", deploy_name);
            return self.handle_deletion(&deployment).await;
        }

        info!("Reconciling deployment: {}", deploy_name);

        // Get all ReplicaSets owned by this deployment
        let all_rs = self.store.list_replicasets().await?;
        let owned_rs: Vec<ReplicaSet> = all_rs
            .into_iter()
            .filter(|rs| self.is_owned_by(&rs.metadata, &deployment.metadata))
            .collect();

        // Separate new and old ReplicaSets
        let (new_rs_opt, old_rss) = self.get_new_and_old_replicasets(&deployment, &owned_rs)?;

        // Execute update strategy
        match &deployment.spec.strategy {
            DeploymentStrategy::Recreate => {
                self.recreate_update(&deployment, &new_rs_opt, &old_rss)
                    .await?
            }
            DeploymentStrategy::RollingUpdate { rolling_update } => {
                self.rolling_update(&deployment, &new_rs_opt, &old_rss, rolling_update)
                    .await?
            }
        }

        // Check progress deadline
        self.check_progress_deadline(&deployment).await?;
        // Clean old ReplicaSets beyond revision history limit
        self.clean_old_replicasets(&deployment, &new_rs_opt, &old_rss)
            .await?;
        // Update deployment status
        self.update_deployment_status(&deployment).await?;
        // Update observed_generation
        self.update_observed_generation(&deployment).await?;
        Ok(())
    }

    async fn handle_deletion(&self, deployment: &Deployment) -> Result<()> {
        // The garbage collector will handle cascading deletion of owned ReplicaSets
        info!(
            "Deployment {} deletion is handled by garbage collector",
            deployment.metadata.name
        );
        // need a duration to wait gc to clean child resource?
        Ok(())
    }
    /// check if child resource is owned by parent resource
    fn is_owned_by(&self, child_meta: &ObjectMeta, parent_meta: &ObjectMeta) -> bool {
        if let Some(owner_refs) = &child_meta.owner_references {
            owner_refs.iter().any(|owner_ref| {
                owner_ref.uid == parent_meta.uid && owner_ref.kind == ResourceKind::Deployment
            })
        } else {
            false
        }
    }

    async fn get_or_create_replicaset(
        &self,
        deployment: &Deployment,
        owned_rs: &[ReplicaSet],
    ) -> Result<ReplicaSet> {
        // Check if a ReplicaSet with the current template already exists
        for rs in owned_rs {
            if self.replicaset_matches_deployment(rs, deployment) {
                info!("Found existing ReplicaSet: {}", rs.metadata.name);
                return Ok(rs.clone());
            }
        }

        // Create new ReplicaSet
        info!(
            "Creating new ReplicaSet for deployment: {}",
            deployment.metadata.name
        );
        let new_rs = self.create_replicaset(deployment).await?;
        Ok(new_rs)
    }

    /// Check if a ReplicaSet's pod template matches the Deployment's pod template
    fn replicaset_matches_deployment(&self, rs: &ReplicaSet, deployment: &Deployment) -> bool {
        let rs_yaml = serde_yaml::to_string(&rs.spec.template.spec).unwrap_or_default();
        let deploy_yaml = serde_yaml::to_string(&deployment.spec.template.spec).unwrap_or_default();
        rs_yaml == deploy_yaml
    }

    /// Separate new ReplicaSet (matching current template, just one) from old ones (maybe a lot of)
    /// This handles Roll Over: all non-matching RSs are considered "old"\
    /// Because when v1 → v2 is happening, then v2->v3
    fn get_new_and_old_replicasets(
        &self,
        deployment: &Deployment,
        owned_rs: &[ReplicaSet],
    ) -> Result<(Option<ReplicaSet>, Vec<ReplicaSet>)> {
        let mut new_rs = None;
        let mut old_rss = Vec::new();

        for rs in owned_rs {
            if self.replicaset_matches_deployment(rs, deployment) {
                new_rs = Some(rs.clone());
            } else {
                old_rss.push(rs.clone());
            }
        }

        Ok((new_rs, old_rss))
    }

    async fn create_replicaset(&self, deployment: &Deployment) -> Result<ReplicaSet> {
        let deploy_name = &deployment.metadata.name;
        let collision_count = deployment.status.collision_count;
        let template_hash = self.generate_hash(&deployment.spec.template, collision_count);
        let rs_name = format!("{}-{}", deploy_name, template_hash);

        let existing_rs_yaml = self.store.get_replicaset_yaml(&rs_name).await?;
        if let Some(existing_yaml) = existing_rs_yaml {
            let existing_rs: ReplicaSet = serde_yaml::from_str(&existing_yaml)?;

            // Check if it's owned by this deployment
            if let Some(owner_refs) = &existing_rs.metadata.owner_references
                && owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment && owner.uid == deployment.metadata.uid
                })
            {
                // Already exists and owned by us, check if template matches
                if self.replicaset_matches_deployment(&existing_rs, deployment) {
                    info!(
                        "ReplicaSet {} already exists for deployment {}",
                        rs_name, deploy_name
                    );
                    return Ok(existing_rs);
                } else {
                    // Hash collision: increment collision_count and retry
                    info!(
                        "Hash collision detected for {}, incrementing collision_count",
                        rs_name
                    );
                    self.increment_collision_count(deployment).await?;
                    return Err(anyhow!(
                        "Hash collision detected, collision_count incremented, will retry on next reconcile"
                    ));
                }
            }

            // If RS exists but not owned by this deployment, it's an unexpected state
            info!(
                "ReplicaSet {} exists but not owned by deployment {}, incrementing collision_count",
                rs_name, deploy_name
            );
            self.increment_collision_count(deployment).await?;
            return Err(anyhow!(
                "ReplicaSet name conflict detected, collision_count incremented"
            ));
        }

        let mut rs_metadata = ObjectMeta {
            name: rs_name.clone(),
            namespace: deployment.metadata.namespace.clone(),
            labels: deployment.spec.selector.match_labels.clone(),
            ..Default::default()
        };

        // Set owner reference to enable garbage collection
        rs_metadata.owner_references = Some(vec![OwnerReference {
            api_version: deployment.api_version.clone(),
            kind: ResourceKind::Deployment,
            name: deploy_name.clone(),
            uid: deployment.metadata.uid,
            controller: true,
            block_owner_deletion: Some(true),
        }]);

        // Prepare pod template with merged labels
        let mut template = deployment.spec.template.clone();
        for (k, v) in &deployment.spec.selector.match_labels {
            template.metadata.labels.insert(k.clone(), v.clone());
        }

        let rs = ReplicaSet {
            api_version: "v1".to_string(),
            kind: "ReplicaSet".to_string(),
            metadata: rs_metadata,
            spec: ReplicaSetSpec {
                replicas: 0,
                selector: deployment.spec.selector.clone(),
                template,
            },
            status: ReplicaSetStatus::default(),
        };

        let rs_yaml = serde_yaml::to_string(&rs)?;
        self.store
            .insert_replicaset_yaml(&rs_name, &rs_yaml)
            .await?;

        info!(
            "Created ReplicaSet {} for deployment {}",
            rs_name, deploy_name
        );
        Ok(rs)
    }

    /// Generate a stable hash of the pod template for replicaset name
    /// Use collision_count to avoid hash collisions
    /// One pod template always maps to one hash
    fn generate_hash(&self, template: &PodTemplateSpec, collision_count: i32) -> String {
        let template_yaml = serde_yaml::to_string(&template.spec).unwrap_or_default();
        let mut hasher = DefaultHasher::new();
        template_yaml.hash(&mut hasher);

        // Add collision_count to hash if non-zero
        if collision_count > 0 {
            collision_count.hash(&mut hasher);
        }

        let hash = hasher.finish();

        // Convert to hex and take first 10 chars
        format!("{:x}", hash).chars().take(10).collect()
    }

    /// Recreate strategy: scale down all old RSs to 0, then scale up new RS    
    async fn recreate_update(
        &self,
        deployment: &Deployment,
        new_rs_opt: &Option<ReplicaSet>,
        old_rss: &[ReplicaSet],
    ) -> Result<()> {
        // Scale down all old ReplicaSets to 0
        for old_rs in old_rss {
            if old_rs.spec.replicas > 0 {
                self.scale_replicaset(old_rs, 0).await?;
            }
        }

        let new_rs = match new_rs_opt {
            Some(rs) => rs.clone(),
            None => {
                let all_rs = self.store.list_replicasets().await?;
                let owned_rs: Vec<ReplicaSet> = all_rs
                    .into_iter()
                    .filter(|rs| self.is_owned_by(&rs.metadata, &deployment.metadata))
                    .collect();
                self.get_or_create_replicaset(deployment, &owned_rs).await?
            }
        };

        // Scale up new ReplicaSet to desired replicas
        if new_rs.spec.replicas != deployment.spec.replicas {
            self.scale_replicaset(&new_rs, deployment.spec.replicas)
                .await?;
        }

        Ok(())
    }

    /// Rolling update: gradually replace old pods with new ones
    async fn rolling_update(
        &self,
        deployment: &Deployment,
        new_rs_opt: &Option<ReplicaSet>,
        old_rss: &[ReplicaSet],
        strategy: &RollingUpdateStrategy,
    ) -> Result<()> {
        let desired = deployment.spec.replicas;

        let max_surge = strategy.max_surge.resolve(desired);
        let max_unavailable = strategy.max_unavailable.resolve(desired);

        // maxSurge and maxUnavailable cannot both be 0 so rkl need to check
        info!(
            "Rolling update for {}: desired={}, maxSurge={}, maxUnavailable={}",
            deployment.metadata.name, desired, max_surge, max_unavailable
        );
        let new_rs = match new_rs_opt {
            Some(rs) => rs.clone(),
            None => {
                let all_rs = self.store.list_replicasets().await?;
                let owned_rs: Vec<ReplicaSet> = all_rs
                    .into_iter()
                    .filter(|rs| self.is_owned_by(&rs.metadata, &deployment.metadata))
                    .collect();
                self.get_or_create_replicaset(deployment, &owned_rs).await?
            }
        };

        // get some state,total(all pods in new and old RSs), available pods(ready pods in new and old RSs)...
        let mut all_rss = vec![new_rs.clone()];
        all_rss.extend_from_slice(old_rss);
        let (total, available, _) = self.calculate_replica(&all_rss);
        let max_total = desired + max_surge;
        let min_available = (desired - max_unavailable).max(0);

        info!(
            "Current state: total={}, available={}, max_total={}, min_available={}",
            total, available, max_total, min_available
        );

        // Scale up new RS
        let mut scaled_up = false;
        if new_rs.spec.replicas < desired {
            let can_scale_up = (max_total - total).max(0);
            if can_scale_up > 0 {
                let new_replicas = (new_rs.spec.replicas + can_scale_up).min(desired);
                info!(
                    "Scaling up new RS {} from {} to {} (can_scale_up={})",
                    new_rs.metadata.name, new_rs.spec.replicas, new_replicas, can_scale_up
                );
                self.scale_replicaset(&new_rs, new_replicas).await?;
                scaled_up = true;
            } else if max_surge == 0 && total >= max_total {
                // If maxSurge=0, should scale down old RS first
                info!(
                    "maxSurge=0 and at capacity (total={}, max_total={}), scaling down old RSs first",
                    total, max_total
                );
            }
        }

        // Scale down old RSs
        if !old_rss.is_empty() {
            // Recalculate state
            let all_rss_updated = self.get_all_replicasets_for_deployment(deployment).await?;
            let (total_updated, available_updated, _) = self.calculate_replica(&all_rss_updated);

            // Determine how many old pods can be scaled down
            // based on both available and total constraints (min)
            let delete_num1 = if available_updated > min_available {
                available_updated - min_available
            } else {
                0
            };

            let delete_num2 = if total_updated > desired {
                total_updated - desired
            } else {
                0
            };

            let delete_num = delete_num1.min(delete_num2);

            if delete_num > 0 {
                info!(
                    "Scaling down old RSs: total={}, available={}, min_available={}, delete_num={}",
                    total_updated, available_updated, min_available, delete_num
                );
                self.scale_down_old_replicasets(old_rss, delete_num).await?;

                // If we couldn't scale up earlier due to maxSurge=0, try again after scaling down
                if !scaled_up && max_surge == 0 && new_rs.spec.replicas < desired {
                    let all_rss_final = self.get_all_replicasets_for_deployment(deployment).await?;
                    let (total_final, _, _) = self.calculate_replica(&all_rss_final);
                    let can_scale_up_now = (max_total - total_final).max(0);

                    if can_scale_up_now > 0 {
                        let new_replicas = (new_rs.spec.replicas + can_scale_up_now).min(desired);
                        info!(
                            "After scaling down, scaling up new RS {} from {} to {}",
                            new_rs.metadata.name, new_rs.spec.replicas, new_replicas
                        );
                        self.scale_replicaset(&new_rs, new_replicas).await?;
                    }
                }
            }
        }

        Ok(())
    }

    /// Get all ReplicaSets owned by this deployment
    async fn get_all_replicasets_for_deployment(
        &self,
        deployment: &Deployment,
    ) -> Result<Vec<ReplicaSet>> {
        let all_rs = self.store.list_replicasets().await?;
        let owned_rs: Vec<ReplicaSet> = all_rs
            .into_iter()
            .filter(|rs| self.is_owned_by(&rs.metadata, &deployment.metadata))
            .collect();
        Ok(owned_rs)
    }

    /// Calculate total, available, and ready replica counts
    fn calculate_replica(&self, rss: &[ReplicaSet]) -> (i32, i32, i32) {
        let mut total = 0;
        let mut available = 0;
        let mut ready = 0;

        for rs in rss {
            total += rs.spec.replicas;
            available += rs.status.available_replicas;
            ready += rs.status.ready_replicas;
        }

        (total, available, ready)
    }

    /// Scale a ReplicaSet to the specified number of replicas
    async fn scale_replicaset(&self, rs: &ReplicaSet, new_replicas: i32) -> Result<()> {
        let rs_name = &rs.metadata.name;

        if rs.spec.replicas == new_replicas {
            return Ok(());
        }

        info!(
            "Scaling ReplicaSet {} from {} to {} replicas",
            rs_name, rs.spec.replicas, new_replicas
        );

        let rs_yaml = self
            .store
            .get_replicaset_yaml(rs_name)
            .await?
            .ok_or_else(|| anyhow!("ReplicaSet {} not found", rs_name))?;

        let mut updated_rs: ReplicaSet = serde_yaml::from_str(&rs_yaml)?;
        updated_rs.spec.replicas = new_replicas;

        let updated_yaml = serde_yaml::to_string(&updated_rs)?;
        self.store
            .insert_replicaset_yaml(rs_name, &updated_yaml)
            .await?;

        Ok(())
    }

    /// Scale down old ReplicaSets proportionally
    /// Scale down unhealthy pods first (achieve after podcondition is ok)
    async fn scale_down_old_replicasets(
        &self,
        old_rss: &[ReplicaSet],
        scale_down_count: i32,
    ) -> Result<()> {
        let mut active_old: Vec<&ReplicaSet> =
            old_rss.iter().filter(|rs| rs.spec.replicas > 0).collect();

        if active_old.is_empty() {
            return Ok(());
        }

        // Sort by replicas descending (scale down largest first)
        active_old.sort_by(|a, b| b.spec.replicas.cmp(&a.spec.replicas));

        let mut remaining = scale_down_count;
        for rs in active_old {
            if remaining <= 0 {
                break;
            }

            let can_scale_down = rs.spec.replicas.min(remaining);
            let new_replicas = rs.spec.replicas - can_scale_down;

            self.scale_replicaset(rs, new_replicas).await?;
            remaining -= can_scale_down;
        }

        Ok(())
    }

    /// Check if deployment has exceeded progress deadline
    async fn check_progress_deadline(&self, deployment: &Deployment) -> Result<()> {
        let deadline_seconds = deployment.spec.progress_deadline_seconds;

        // Find the last Progressing condition
        if let Some(progressing_cond) = deployment
            .status
            .conditions
            .iter()
            .find(|c| c.condition_type == "Progressing")
            && progressing_cond.status == "True"
        {
            // Parse last transition time
            if let Ok(last_time) =
                chrono::DateTime::parse_from_rfc3339(&progressing_cond.last_transition_time)
            {
                let elapsed = Utc::now().signed_duration_since(last_time.with_timezone(&Utc));

                if elapsed.num_seconds() > deadline_seconds {
                    info!(
                        "Deployment {} exceeded progress deadline ({}s)",
                        deployment.metadata.name, deadline_seconds
                    );
                    self.update_condition(
                        deployment,
                        DeploymentCondition {
                            condition_type: "Progressing".to_string(),
                            status: "False".to_string(),
                            reason: Some("ProgressDeadlineExceeded".to_string()),
                            message: Some(format!(
                                "ReplicaSet update exceeded {}s",
                                deadline_seconds
                            )),
                            last_transition_time: Utc::now().to_rfc3339(),
                            last_update_time: Some(Utc::now().to_rfc3339()),
                        },
                    )
                    .await?;
                }
            }
        }

        Ok(())
    }

    /// Update or add a condition in deployment status
    async fn update_condition(
        &self,
        deployment: &Deployment,
        new_condition: DeploymentCondition,
    ) -> Result<()> {
        let deploy_name = &deployment.metadata.name;

        let yaml = self
            .store
            .get_deployment_yaml(deploy_name)
            .await?
            .ok_or_else(|| anyhow!("Deployment {} not found", deploy_name))?;

        let mut deploy: Deployment = serde_yaml::from_str(&yaml)?;

        // Find existing condition of same type
        if let Some(existing) = deploy
            .status
            .conditions
            .iter_mut()
            .find(|c| c.condition_type == new_condition.condition_type)
        {
            *existing = new_condition;
        } else {
            deploy.status.conditions.push(new_condition);
        }

        let updated_yaml = serde_yaml::to_string(&deploy)?;
        self.store
            .insert_deployment_yaml(deploy_name, &updated_yaml)
            .await?;

        Ok(())
    }

    /// Clean old ReplicaSets beyond revision history limit
    async fn clean_old_replicasets(
        &self,
        deployment: &Deployment,
        _new_rs_opt: &Option<ReplicaSet>,
        old_rss: &[ReplicaSet],
    ) -> Result<()> {
        let limit = deployment.spec.revision_history_limit;

        // Keep RSs with replicas > 0
        let mut zero_replicas: Vec<&ReplicaSet> =
            old_rss.iter().filter(|rs| rs.spec.replicas == 0).collect();

        if zero_replicas.len() as i32 <= limit {
            return Ok(());
        }

        // oldest first
        zero_replicas.sort_by_key(|rs| &rs.metadata.creation_timestamp);

        let to_delete = zero_replicas.len() as i32 - limit;
        for rs in zero_replicas.iter().take(to_delete as usize) {
            info!(
                "Deleting old ReplicaSet {} (revision history cleanup)",
                rs.metadata.name
            );
            self.store.delete_replicaset(&rs.metadata.name).await?;
        }

        Ok(())
    }

    /// Update observed_generation to track processed spec changes
    async fn update_observed_generation(&self, deployment: &Deployment) -> Result<()> {
        let deploy_name = &deployment.metadata.name;
        let generation = deployment.metadata.generation.unwrap_or(0);

        // Check if already up to date
        if deployment.status.observed_generation == Some(generation) {
            return Ok(());
        }

        let yaml = self
            .store
            .get_deployment_yaml(deploy_name)
            .await?
            .ok_or_else(|| anyhow!("Deployment {} not found", deploy_name))?;

        let mut deploy: Deployment = serde_yaml::from_str(&yaml)?;
        deploy.status.observed_generation = Some(generation);

        let updated_yaml = serde_yaml::to_string(&deploy)?;
        self.store
            .insert_deployment_yaml(deploy_name, &updated_yaml)
            .await?;

        info!(
            "Updated observed_generation to {} for deployment {}",
            generation, deploy_name
        );

        Ok(())
    }

    /// Increment collision_count in deployment status for hash collision resolution
    async fn increment_collision_count(&self, deployment: &Deployment) -> Result<()> {
        let deploy_name = &deployment.metadata.name;

        let yaml = self
            .store
            .get_deployment_yaml(deploy_name)
            .await?
            .ok_or_else(|| anyhow!("Deployment {} not found", deploy_name))?;

        let mut deploy: Deployment = serde_yaml::from_str(&yaml)?;
        let old_count = deploy.status.collision_count;
        let new_count = old_count + 1;
        deploy.status.collision_count = new_count;

        let updated_yaml = serde_yaml::to_string(&deploy)?;
        self.store
            .insert_deployment_yaml(deploy_name, &updated_yaml)
            .await?;
        info!(
            "Incremented collision_count to {} for deployment {}",
            new_count, deploy_name
        );
        Ok(())
    }
    /// In the end, update the deployment status based on its ReplicaSets
    async fn update_deployment_status(&self, deployment: &Deployment) -> Result<()> {
        let deploy_name = &deployment.metadata.name;

        // Get all ReplicaSets owned by this deployment
        let all_rs = self.store.list_replicasets().await?;
        let owned_rs: Vec<ReplicaSet> = all_rs
            .into_iter()
            .filter(|rs| self.is_owned_by(&rs.metadata, &deployment.metadata))
            .collect();

        // Calculate status from all owned ReplicaSets
        let mut total_replicas = 0;
        let mut ready_replicas = 0;
        let mut available_replicas = 0;
        let mut updated_replicas = 0;

        for rs in &owned_rs {
            total_replicas += rs.status.replicas;
            ready_replicas += rs.status.ready_replicas;
            available_replicas += rs.status.available_replicas;

            // For now, consider all replicas as "updated" since we only have one RS
            if self.replicaset_matches_deployment(rs, deployment) {
                updated_replicas = rs.status.replicas;
            }
        }

        // Update deployment status
        let yaml = self
            .store
            .get_deployment_yaml(deploy_name)
            .await?
            .ok_or_else(|| anyhow!("Deployment {} not found", deploy_name))?;

        let mut deploy: Deployment = serde_yaml::from_str(&yaml)?;
        deploy.status.replicas = total_replicas;
        deploy.status.ready_replicas = ready_replicas;
        deploy.status.available_replicas = available_replicas;
        deploy.status.updated_replicas = updated_replicas;
        deploy.status.unavailable_replicas = (deployment.spec.replicas - available_replicas).max(0);

        let updated_yaml = serde_yaml::to_string(&deploy)?;
        self.store
            .insert_deployment_yaml(deploy_name, &updated_yaml)
            .await?;

        info!(
            "Updated status for deployment {}: replicas={}/{}, ready={}, available={}",
            deploy_name,
            total_replicas,
            deployment.spec.replicas,
            ready_replicas,
            available_replicas
        );

        Ok(())
    }
}

#[async_trait]
impl Controller for DeploymentController {
    fn name(&self) -> &'static str {
        "deployment"
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![ResourceKind::Deployment, ResourceKind::ReplicaSet]
    }

    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        match response.kind {
            ResourceKind::Deployment => {
                debug!(
                    "DeploymentController handling Deployment event: key={}",
                    response.key
                );

                // Reconcile on Add events or when the spec has changed
                let mut should_reconcile = false;
                match &response.event {
                    WatchEvent::Add { .. } => {
                        should_reconcile = true;
                    }
                    WatchEvent::Update { old_yaml, new_yaml } => {
                        let old_deploy: Deployment = serde_yaml::from_str(old_yaml)?;
                        let new_deploy: Deployment = serde_yaml::from_str(new_yaml)?;
                        // Only reconcile if spec changed (ignore status-only updates)
                        let old_spec_yaml =
                            serde_yaml::to_string(&old_deploy.spec).unwrap_or_default();
                        let new_spec_yaml =
                            serde_yaml::to_string(&new_deploy.spec).unwrap_or_default();

                        if old_spec_yaml != new_spec_yaml {
                            // Spec changed, increment generation
                            self.increment_generation(&new_deploy).await?;
                            should_reconcile = true;
                        }
                    }
                    WatchEvent::Delete { .. } => {}
                }

                if should_reconcile && let Err(e) = self.reconcile_by_name(&response.key).await {
                    error!("Failed to reconcile deployment {}: {}", response.key, e);
                    return Err(e);
                }
            }
            ResourceKind::ReplicaSet => {
                debug!(
                    "DeploymentController handling ReplicaSet event: key={}",
                    response.key
                );

                // When ReplicaSet status changes, update parent Deployment status
                match &response.event {
                    WatchEvent::Update { old_yaml, new_yaml } => {
                        let old_rs: ReplicaSet = serde_yaml::from_str(old_yaml)?;
                        let new_rs: ReplicaSet = serde_yaml::from_str(new_yaml)?;

                        // Only update if status changed (compare using YAML serialization)
                        let old_status_yaml =
                            serde_yaml::to_string(&old_rs.status).unwrap_or_default();
                        let new_status_yaml =
                            serde_yaml::to_string(&new_rs.status).unwrap_or_default();
                        if old_status_yaml != new_status_yaml
                            && let Err(e) = self.update_deployment_for_replicaset(&new_rs).await
                        {
                            error!(
                                "Failed to update deployment for ReplicaSet {}: {}",
                                response.key, e
                            );
                        }
                    }
                    WatchEvent::Add { yaml } => {
                        let rs: ReplicaSet = serde_yaml::from_str(yaml)?;
                        if let Err(e) = self.update_deployment_for_replicaset(&rs).await {
                            error!(
                                "Failed to update deployment for new ReplicaSet {}: {}",
                                response.key, e
                            );
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl DeploymentController {
    /// Increment generation when spec changes
    async fn increment_generation(&self, deployment: &Deployment) -> Result<()> {
        let deploy_name = &deployment.metadata.name;

        let yaml = self
            .store
            .get_deployment_yaml(deploy_name)
            .await?
            .ok_or_else(|| anyhow!("Deployment {} not found", deploy_name))?;

        let mut deploy: Deployment = serde_yaml::from_str(&yaml)?;
        let old_gen = deploy.metadata.generation.unwrap_or(0);
        deploy.metadata.generation = Some(old_gen + 1);

        let updated_yaml = serde_yaml::to_string(&deploy)?;
        self.store
            .insert_deployment_yaml(deploy_name, &updated_yaml)
            .await?;

        info!(
            "Incremented generation to {} for deployment {}",
            old_gen + 1,
            deploy_name
        );

        Ok(())
    }

    /// Update deployment status when ReplicaSet changes
    async fn update_deployment_for_replicaset(&self, rs: &ReplicaSet) -> Result<()> {
        // Find the owning Deployment
        if let Some(owner_refs) = &rs.metadata.owner_references {
            for owner_ref in owner_refs {
                if owner_ref.kind == ResourceKind::Deployment && owner_ref.controller {
                    let deployment_name = &owner_ref.name;

                    if let Some(yaml) = self.store.get_deployment_yaml(deployment_name).await? {
                        let deployment: Deployment = serde_yaml::from_str(&yaml)?;
                        self.update_deployment_status(&deployment).await?;
                        debug!(
                            "Updated status for deployment {} due to ReplicaSet {} change",
                            deployment_name, rs.metadata.name
                        );
                    }
                    break;
                }
            }
        }
        Ok(())
    }
}
