use crate::api::xlinestore::XlineStore;
use crate::controllers::manager::{Controller, ResourceWatchResponse, WatchEvent};
use anyhow::{Result, anyhow};
use async_trait::async_trait;
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

        // Get or create the ReplicaSet for current spec
        let current_rs = self
            .get_or_create_replicaset(&deployment, &owned_rs)
            .await?;

        // Scale the current ReplicaSet to match deployment's desired replicas
        self.ensure_replicas(&current_rs, deployment.spec.replicas)
            .await?;

        // Update deployment status
        self.update_deployment_status(&deployment).await?;

        info!("Successfully reconciled deployment: {}", deploy_name);
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

    fn replicaset_matches_deployment(&self, rs: &ReplicaSet, deployment: &Deployment) -> bool {
        // Compare pod template specs using YAML serialization
        let rs_yaml = serde_yaml::to_string(&rs.spec.template.spec).unwrap_or_default();
        let deploy_yaml = serde_yaml::to_string(&deployment.spec.template.spec).unwrap_or_default();
        rs_yaml == deploy_yaml
    }

    async fn create_replicaset(&self, deployment: &Deployment) -> Result<ReplicaSet> {
        let deploy_name = &deployment.metadata.name;

        let collision_count = deployment.status.collision_count;
        let template_hash = self.generate_hash(&deployment.spec.template, collision_count);
        let rs_name = format!("{}-{}", deploy_name, template_hash);

        // Check if ReplicaSet with this name already exists
        if let Some(existing_yaml) = self.store.get_replicaset_yaml(&rs_name).await? {
            let existing_rs: ReplicaSet = serde_yaml::from_str(&existing_yaml)?;

            // Check if it's owned by this deployment
            if let Some(owner_refs) = &existing_rs.metadata.owner_references {
                if owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment && owner.uid == deployment.metadata.uid
                }) {
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
                } else {
                    // Owned by different deployment - rare hash collision
                    info!(
                        "ReplicaSet {} exists but owned by different deployment, incrementing collision_count",
                        rs_name
                    );
                    self.increment_collision_count(deployment).await?;
                    return Err(anyhow!(
                        "Hash collision with different deployment, collision_count incremented"
                    ));
                }
            }
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
                replicas: deployment.spec.replicas,
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

    /// Increment collision_count in deployment status for hash collision resolution
    async fn increment_collision_count(&self, deployment: &Deployment) -> Result<()> {
        let deploy_name = &deployment.metadata.name;

        let yaml = self
            .store
            .get_deployment_yaml(deploy_name)
            .await?
            .ok_or_else(|| anyhow!("Deployment {} not found", deploy_name))?;

        let mut deploy: Deployment = serde_yaml::from_str(&yaml)?;
        let new_count = deploy.status.collision_count + 1;
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

    /// Ensure the ReplicaSet has the desired number of replicas
    /// Just scale up/down the replicas count
    async fn ensure_replicas(&self, rs: &ReplicaSet, desired_replicas: i32) -> Result<()> {
        let rs_name = &rs.metadata.name;

        if rs.spec.replicas == desired_replicas {
            info!(
                "ReplicaSet {} already has desired replicas: {}",
                rs_name, desired_replicas
            );
            return Ok(());
        }

        info!(
            "Scaling ReplicaSet {} from {} to {} replicas",
            rs_name, rs.spec.replicas, desired_replicas
        );

        // Update ReplicaSet replicas
        let rs_yaml = self
            .store
            .get_replicaset_yaml(rs_name)
            .await?
            .ok_or_else(|| anyhow!("ReplicaSet {} not found", rs_name))?;

        let mut updated_rs: ReplicaSet = serde_yaml::from_str(&rs_yaml)?;
        updated_rs.spec.replicas = desired_replicas;

        let updated_yaml = serde_yaml::to_string(&updated_rs)?;
        self.store
            .insert_replicaset_yaml(rs_name, &updated_yaml)
            .await?;

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
                        let old_template_yaml =
                            serde_yaml::to_string(&old_deploy.spec.template.spec)
                                .unwrap_or_default();
                        let new_template_yaml =
                            serde_yaml::to_string(&new_deploy.spec.template.spec)
                                .unwrap_or_default();
                        if old_deploy.spec.replicas != new_deploy.spec.replicas
                            || old_template_yaml != new_template_yaml
                        {
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
