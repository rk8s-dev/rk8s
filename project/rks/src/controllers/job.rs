use crate::api::xlinestore::XlineStore;
use crate::controllers::manager::{Controller, ResourceWatchResponse, WatchEvent};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use common::{
    CompletionMode, ConditionStatus, Job, JobCondition, JobConditionType, OwnerReference, PodPhase,
    PodTask, ResourceKind,
};
use log::{debug, info, warn};
use rand::random;
use std::sync::Arc;
use uuid::Uuid;

pub struct JobController {
    store: Arc<XlineStore>,
}

impl JobController {
    pub fn new(store: Arc<XlineStore>) -> Self {
        Self { store }
    }

    // load the Job and drive its state machine.
    async fn reconcile_by_name(&self, name: &str) -> Result<()> {
        let job = match self.store.get_job(name).await? {
            Some(j) => j,
            None => {
                debug!("Job {} not found, skipping reconcile", name);
                return Ok(());
            }
        };

        // deleted by gc
        if job.metadata.deletion_timestamp.is_some() {
            info!("Job {} is being deleted, skipping reconcile", name);
            return Ok(());
        }

        self.reconcile_job(job).await
    }

    async fn reconcile_job(&self, mut job: Job) -> Result<()> {
        let job_name = job.metadata.name.clone();
        info!("Reconciling job: {}", job_name);

        // Set start_time on first reconcile.
        if job.status.start_time.is_none() {
            job.status.start_time = Some(Utc::now());
        }

        // Gather owned Pods
        let all_pods = self.store.list_pods().await?;
        let owned_pods: Vec<&PodTask> = all_pods
            .iter()
            .filter(|pod| self.is_owned_by_job(pod, &job))
            .collect();

        let completion_mode = job.spec.completion_mode.clone();

        let succeeded: i32;
        let failed: i32;
        let active: i32;

        let mut succeeded_indices: Option<Vec<bool>> = None;
        let mut failed_counts: Option<Vec<i32>> = None;
        let mut active_indices: Option<Vec<bool>> = None;

        if completion_mode == CompletionMode::Indexed {
            let completion_count = job.spec.completions.max(0) as usize;
            if completion_count == 0 {
                if !self.has_condition(&job, &JobConditionType::Complete) {
                    self.mark_job_complete(&mut job).await?;
                } else {
                    self.maybe_cleanup_by_ttl(&job).await?;
                }
                return Ok(());
            }

            let mut s = vec![false; completion_count];
            let mut f = vec![0i32; completion_count];
            let mut a = vec![false; completion_count];

            for pod in &owned_pods {
                let Some(idx) = Self::pod_completion_index(pod) else {
                    continue;
                };
                if idx >= completion_count {
                    continue;
                }

                match pod.status.phase {
                    PodPhase::Succeeded => s[idx] = true,
                    PodPhase::Failed => f[idx] += 1,
                    PodPhase::Running | PodPhase::Pending => a[idx] = true,
                    _ => {}
                }
            }

            succeeded = s.iter().filter(|&&b| b).count() as i32;
            failed = f.iter().sum::<i32>();
            active = a.iter().filter(|&&b| b).count() as i32;

            succeeded_indices = Some(s);
            failed_counts = Some(f);
            active_indices = Some(a);
        } else {
            // Non-indexed mode: only total counts matter.
            succeeded = owned_pods
                .iter()
                .filter(|p| p.status.phase == PodPhase::Succeeded)
                .count() as i32;
            failed = owned_pods
                .iter()
                .filter(|p| p.status.phase == PodPhase::Failed)
                .count() as i32;
            active = owned_pods
                .iter()
                .filter(|p| {
                    p.status.phase == PodPhase::Running || p.status.phase == PodPhase::Pending
                })
                .count() as i32;
        }

        // Update status counters.
        job.status.succeeded = succeeded;
        job.status.failed = failed;
        job.status.active = active;

        debug!(
            "Job {} counts: active={} succeeded={} failed={}",
            job_name, active, succeeded, failed
        );

        // Check conditions

        // Active-deadline exceeded.
        if let Some(deadline_secs) = job.spec.active_deadline_seconds
            && let Some(start) = job.status.start_time
        {
            let elapsed = Utc::now().signed_duration_since(start).num_seconds();
            if elapsed >= deadline_secs {
                warn!(
                    "Job {} exceeded active_deadline_seconds ({}s), marking Failed",
                    job_name, deadline_secs
                );
                self.terminate_active_pods(&owned_pods).await?;
                self.mark_job_failed(
                    &mut job,
                    "DeadlineExceeded",
                    &format!(
                        "Job exceeded the specified active deadline of {}s",
                        deadline_secs
                    ),
                )
                .await?;
                return Ok(());
            }
        }

        // Completion reached -> Complete
        if completion_mode == CompletionMode::Indexed {
            let completion_count = job.spec.completions.max(0) as usize;
            if (succeeded as usize) == completion_count {
                if !self.has_condition(&job, &JobConditionType::Complete) {
                    info!(
                        "Job {} (Indexed) all {} indices succeeded, marking Complete",
                        job_name, completion_count
                    );
                    self.mark_job_complete(&mut job).await?;
                } else {
                    self.maybe_cleanup_by_ttl(&job).await?;
                }
                return Ok(());
            }
        } else if succeeded >= job.spec.completions {
            if !self.has_condition(&job, &JobConditionType::Complete) {
                info!(
                    "Job {} reached {} completions, marking Complete",
                    job_name, succeeded
                );
                self.mark_job_complete(&mut job).await?;
            } else {
                // Already complete; only run TTL cleanup.
                self.maybe_cleanup_by_ttl(&job).await?;
            }
            return Ok(());
        }

        // Backoff limit exceeded -> Failed
        if completion_mode == CompletionMode::Indexed {
            let backoff_limit = job.spec.backoff_limit;
            let failed_counts = failed_counts.as_ref().expect("indexed");
            let exceeded = failed_counts
                .iter()
                .enumerate()
                .find(|&(_idx, c)| *c > backoff_limit);

            if let Some((idx, &cnt)) = exceeded {
                warn!(
                    "Job {} (Indexed) index {} exceeded backoff_limit ({} > {}), marking Failed",
                    job_name, idx, cnt, backoff_limit
                );
                self.terminate_active_pods(&owned_pods).await?;
                self.mark_job_failed(
                    &mut job,
                    "BackoffLimitExceeded",
                    &format!(
                        "Indexed completion index {} exceeded backoff limit ({} > {})",
                        idx, cnt, backoff_limit
                    ),
                )
                .await?;
                return Ok(());
            }
        } else if failed > job.spec.backoff_limit {
            warn!(
                "Job {} exceeded backoff_limit ({} > {}), marking Failed",
                job_name, failed, job.spec.backoff_limit
            );
            let backoff_limit = job.spec.backoff_limit;
            self.mark_job_failed(
                &mut job,
                "BackoffLimitExceeded",
                &format!(
                    "Job has reached the specified backoff limit of {}",
                    backoff_limit
                ),
            )
            .await?;
            return Ok(());
        }

        // reached here only if conditions already set
        if self.has_condition(&job, &JobConditionType::Complete)
            || self.has_condition(&job, &JobConditionType::Failed)
        {
            self.maybe_cleanup_by_ttl(&job).await?;
            return Ok(());
        }

        // Create new Pods to fill up parallelism
        // need = min(parallelism, completions - succeeded) - active
        if completion_mode == CompletionMode::Indexed {
            let completion_count = job.spec.completions.max(0) as usize;
            let succeeded_indices = succeeded_indices.as_ref().expect("indexed");
            let failed_counts = failed_counts.as_ref().expect("indexed");
            let active_indices = active_indices.as_ref().expect("indexed");

            let parallelism = job.spec.parallelism.max(0);
            let remaining = (job.spec.completions - succeeded).max(0);
            let want_active = parallelism.min(remaining);
            let to_create = (want_active - active).max(0) as usize;

            if to_create > 0 {
                let backoff_limit = job.spec.backoff_limit;
                let mut created = 0usize;
                for idx in 0..completion_count {
                    if created >= to_create {
                        break;
                    }
                    if succeeded_indices[idx] {
                        continue;
                    }
                    if active_indices[idx] {
                        continue;
                    }
                    if failed_counts[idx] > backoff_limit {
                        continue;
                    }

                    let attempt = failed_counts[idx] + 1;
                    self.create_job_pod_indexed(&job, idx, attempt).await?;
                    created += 1;
                }
            }
        } else {
            let parallelism = job.spec.parallelism.max(0);
            let remaining = (job.spec.completions - succeeded).max(0);
            let want_active = parallelism.min(remaining);
            let to_create = (want_active - active).max(0) as usize;

            for _ in 0..to_create {
                self.create_job_pod(&job).await?;
            }
        }

        // Persist updated status.
        let yaml = serde_yaml::to_string(&job)?;
        self.store.insert_job_yaml(&job_name, &yaml).await?;
        Ok(())
    }

    fn is_owned_by_job(&self, pod: &PodTask, job: &Job) -> bool {
        pod.metadata
            .owner_references
            .as_ref()
            .map(|refs| {
                refs.iter()
                    .any(|r| r.kind == ResourceKind::Job && r.uid == job.metadata.uid)
            })
            .unwrap_or(false)
    }

    fn has_condition(&self, job: &Job, kind: &JobConditionType) -> bool {
        job.status
            .conditions
            .iter()
            .any(|c| &c.condition_type == kind && c.status == ConditionStatus::True)
    }

    /// Extract completion index from Pod template env:
    /// - `JOB_COMPLETION_INDEX` injected by Indexed JobController.
    fn pod_completion_index(pod: &PodTask) -> Option<usize> {
        let idx_str = pod
            .spec
            .containers
            .iter()
            .chain(pod.spec.init_containers.iter())
            .filter_map(|c| c.env.as_ref())
            .flatten()
            .find_map(|e| {
                if e.name == "JOB_COMPLETION_INDEX" {
                    e.value.as_ref().cloned()
                } else {
                    None
                }
            });

        idx_str
            .and_then(|s| s.parse::<i32>().ok())
            .and_then(|i| if i >= 0 { Some(i as usize) } else { None })
    }

    async fn mark_job_complete(&self, job: &mut Job) -> Result<()> {
        job.status.completion_time = Some(Utc::now());
        job.status.conditions.push(JobCondition {
            condition_type: JobConditionType::Complete,
            status: ConditionStatus::True,
            last_transition_time: Some(Utc::now()),
            reason: Some("JobComplete".to_string()),
            message: Some(format!(
                "Job completed with {} succeeded pod(s)",
                job.status.succeeded
            )),
        });
        let yaml = serde_yaml::to_string(&*job)?;
        self.store
            .insert_job_yaml(&job.metadata.name, &yaml)
            .await?;
        info!("Job {} marked as Complete", job.metadata.name);
        self.maybe_cleanup_by_ttl(job).await?;
        Ok(())
    }

    async fn mark_job_failed(&self, job: &mut Job, reason: &str, message: &str) -> Result<()> {
        job.status.completion_time = Some(Utc::now());
        job.status.conditions.push(JobCondition {
            condition_type: JobConditionType::Failed,
            status: ConditionStatus::True,
            last_transition_time: Some(Utc::now()),
            reason: Some(reason.to_string()),
            message: Some(message.to_string()),
        });
        let yaml = serde_yaml::to_string(&*job)?;
        self.store
            .insert_job_yaml(&job.metadata.name, &yaml)
            .await?;
        warn!("Job {} marked as Failed ({})", job.metadata.name, reason);
        self.maybe_cleanup_by_ttl(job).await?;
        Ok(())
    }

    /// Delete all still-active (Pending/Running) Pods owned by this Job.
    async fn terminate_active_pods(&self, owned_pods: &[&PodTask]) -> Result<()> {
        for pod in owned_pods {
            if (pod.status.phase == PodPhase::Running || pod.status.phase == PodPhase::Pending)
                && let Err(e) = self.store.delete_pod(&pod.metadata.name).await
            {
                warn!(
                    "Failed to delete active pod {} during job termination: {}",
                    pod.metadata.name, e
                );
            }
        }
        Ok(())
    }

    /// If `ttl_seconds_after_finished` is set and has elapsed, delete the Job.
    async fn maybe_cleanup_by_ttl(&self, job: &Job) -> Result<()> {
        let ttl = match job.spec.ttl_seconds_after_finished {
            Some(t) => t,
            None => return Ok(()),
        };
        let finish_time = match job.status.completion_time.or(job.status.start_time) {
            Some(t) => t,
            None => return Ok(()),
        };
        let elapsed = Utc::now().signed_duration_since(finish_time).num_seconds();
        if elapsed >= ttl {
            info!("Job {} TTL ({}s) expired, deleting", job.metadata.name, ttl);
            self.store.delete_job(&job.metadata.name).await?;
        }
        Ok(())
    }

    /// Create a single Pod for the given Job.
    async fn create_job_pod(&self, job: &Job) -> Result<()> {
        let pod_name = self.generate_pod_name(&job.metadata.name).await?;

        let mut pod_meta = job.spec.template.metadata.clone();
        pod_meta.name = pod_name.clone();
        if pod_meta.namespace.is_empty() || pod_meta.namespace == "default" {
            pod_meta.namespace = job.metadata.namespace.clone();
        }
        pod_meta.uid = Uuid::new_v4();
        pod_meta.creation_timestamp = Some(Utc::now());
        pod_meta.owner_references = Some(vec![OwnerReference {
            api_version: job.api_version.clone(),
            kind: ResourceKind::Job,
            name: job.metadata.name.clone(),
            uid: job.metadata.uid,
            controller: true,
            block_owner_deletion: Some(true),
        }]);

        let mut pod_spec = job.spec.template.spec.clone();
        // Job's Pods must not restart automatically because the JobController handles retries.
        pod_spec.restart_policy = common::RestartPolicy::Never;

        let pod = PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: pod_meta,
            spec: pod_spec,
            status: common::PodStatus::default(),
        };

        let yaml = serde_yaml::to_string(&pod)?;
        self.store.insert_pod_yaml(&pod_name, &yaml).await?;
        info!("Job {} created Pod {}", job.metadata.name, pod_name);
        Ok(())
    }

    /// Create a Pod for Indexed completion mode at a specific index.
    /// Injects `JOB_COMPLETION_INDEX` into all containers.
    async fn create_job_pod_indexed(&self, job: &Job, index: usize, attempt: i32) -> Result<()> {
        let idx = index;
        let mut a = attempt;
        let job_name = job.metadata.name.clone();

        // Ensure pod name uniqueness in xline.
        let pod_name = loop {
            let name = format!("{}-idx{}-a{}", job_name, idx, a);
            if self.store.get_pod_yaml(&name).await?.is_none() {
                break name;
            }
            a += 1;
        };

        let mut pod_meta = job.spec.template.metadata.clone();
        pod_meta.name = pod_name.clone();
        if pod_meta.namespace.is_empty() || pod_meta.namespace == "default" {
            pod_meta.namespace = job.metadata.namespace.clone();
        }
        pod_meta.uid = Uuid::new_v4();
        pod_meta.creation_timestamp = Some(Utc::now());
        pod_meta.owner_references = Some(vec![OwnerReference {
            api_version: job.api_version.clone(),
            kind: ResourceKind::Job,
            name: job.metadata.name.clone(),
            uid: job.metadata.uid,
            controller: true,
            block_owner_deletion: Some(true),
        }]);

        let mut pod_spec = job.spec.template.spec.clone();
        // Job Pods must not restart automatically because the JobController handles retries.
        pod_spec.restart_policy = common::RestartPolicy::Never;

        let idx_str = idx.to_string();
        let inject_index = |container_spec: &mut common::ContainerSpec| {
            let envs = container_spec.env.get_or_insert_with(Vec::new);
            if let Some(existing) = envs.iter_mut().find(|e| e.name == "JOB_COMPLETION_INDEX") {
                existing.value = Some(idx_str.clone());
            } else {
                envs.push(common::EnvVar {
                    name: "JOB_COMPLETION_INDEX".to_string(),
                    value: Some(idx_str.clone()),
                });
            }
        };

        for c in &mut pod_spec.containers {
            inject_index(c);
        }
        for c in &mut pod_spec.init_containers {
            inject_index(c);
        }

        let pod = PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: pod_meta,
            spec: pod_spec,
            status: common::PodStatus::default(),
        };

        let yaml = serde_yaml::to_string(&pod)?;
        self.store.insert_pod_yaml(&pod_name, &yaml).await?;
        info!(
            "Job {} created indexed Pod {} (index={})",
            job.metadata.name, pod_name, idx
        );
        Ok(())
    }

    async fn generate_pod_name(&self, job_name: &str) -> Result<String> {
        loop {
            let suffix: u32 = random();
            let name = format!("{}-{:08x}", job_name, suffix);
            if self.store.get_pod_yaml(&name).await?.is_none() {
                return Ok(name);
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    /// Extract the Job name from a Pod's owner references.
    fn owning_job_name(pod: &PodTask) -> Option<String> {
        pod.metadata
            .owner_references
            .as_ref()?
            .iter()
            .find_map(|r| {
                if r.kind == ResourceKind::Job {
                    Some(r.name.clone())
                } else {
                    None
                }
            })
    }
}

#[async_trait]
impl Controller for JobController {
    fn name(&self) -> &'static str {
        "job-controller"
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        vec![ResourceKind::Job, ResourceKind::Pod]
    }

    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        match response.kind {
            // A Job was added or updated — reconcile it directly.
            ResourceKind::Job => match &response.event {
                WatchEvent::Add { yaml } | WatchEvent::Update { new_yaml: yaml, .. } => {
                    match serde_yaml::from_str::<Job>(yaml) {
                        Ok(job) => {
                            if let Err(e) = self.reconcile_by_name(&job.metadata.name.clone()).await
                            {
                                log::error!(
                                    "JobController: reconcile failed for job {}: {}",
                                    job.metadata.name,
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "JobController: failed to parse Job yaml from event: {}",
                                e
                            );
                        }
                    }
                }
                WatchEvent::Delete { .. } => {
                    // Deletion is handled by GC; nothing to do.
                }
            },

            // A Pod changed — if it belongs to a Job, re-reconcile that Job.
            ResourceKind::Pod => {
                let yaml = match &response.event {
                    WatchEvent::Add { yaml } => yaml,
                    WatchEvent::Update { new_yaml, .. } => new_yaml,
                    WatchEvent::Delete { yaml } => yaml,
                };
                if let Ok(pod) = serde_yaml::from_str::<PodTask>(yaml)
                    && let Some(job_name) = Self::owning_job_name(&pod)
                {
                    debug!(
                        "JobController: Pod {} event triggered reconcile for job {}",
                        pod.metadata.name, job_name
                    );
                    if let Err(e) = self.reconcile_by_name(&job_name).await {
                        log::error!(
                            "JobController: reconcile failed for job {} (triggered by pod {}): {}",
                            job_name,
                            pod.metadata.name,
                            e
                        );
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
