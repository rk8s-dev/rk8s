use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum JobState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GcJobResult {
    pub dry_run: bool,
    pub orphan_slice_count: u64,
    pub orphan_object_count: u64,
    pub deleted_object_count: u64,
    pub error_count: u64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum JobOutcome {
    Gc(GcJobResult),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct JobInfo {
    pub job_id: String,
    pub state: JobState,
    pub detail: Option<String>,
    pub outcome: Option<JobOutcome>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Default)]
pub struct JobManager {
    jobs: DashMap<String, JobInfo>,
}

impl JobManager {
    pub async fn create_gc_job(&self, dry_run: bool) -> String {
        let job_id = Uuid::now_v7().to_string();
        let now = Utc::now();

        self.jobs.insert(
            job_id.clone(),
            JobInfo {
                job_id: job_id.clone(),
                state: JobState::Pending,
                detail: if dry_run {
                    Some("dry-run".to_string())
                } else {
                    None
                },
                outcome: None,
                created_at: now,
                updated_at: now,
            },
        );

        job_id
    }

    pub async fn get(&self, job_id: &str) -> Option<JobInfo> {
        self.jobs.get(job_id).map(|entry| entry.clone())
    }

    pub async fn mark_running(&self, job_id: &str) -> Result<()> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow!("job not found: {job_id}"))?;
        job.state = JobState::Running;
        job.updated_at = Utc::now();
        Ok(())
    }

    pub async fn finish(&self, job_id: &str, result: GcJobResult) -> Result<()> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow!("job not found: {job_id}"))?;
        job.state = JobState::Succeeded;
        job.detail = result.detail.clone();
        job.outcome = Some(JobOutcome::Gc(result));
        job.updated_at = Utc::now();
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn fail(&self, job_id: &str, detail: String) -> Result<()> {
        let mut job = self
            .jobs
            .get_mut(job_id)
            .ok_or_else(|| anyhow!("job not found: {job_id}"))?;
        job.state = JobState::Failed;
        job.detail = Some(detail);
        job.updated_at = Utc::now();
        Ok(())
    }
}
