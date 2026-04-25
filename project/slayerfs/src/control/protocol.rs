use crate::control::job::{JobInfo, JobOutcome, JobState};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ControlRequest {
    Ping,
    GetInfo,
    RunGc { dry_run: bool },
    GetJob { job_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ControlResponse {
    Pong,
    Info {
        pid: u32,
        mount_point: String,
        started_at: i64,
        version: String,
    },
    Accepted {
        job_id: String,
    },
    JobStatus {
        job_id: String,
        state: JobState,
        detail: Option<String>,
        outcome: Option<JobOutcome>,
    },
    Error {
        code: String,
        message: String,
    },
}

impl From<JobInfo> for ControlResponse {
    fn from(job: JobInfo) -> Self {
        Self::JobStatus {
            job_id: job.job_id,
            state: job.state,
            detail: job.detail,
            outcome: job.outcome,
        }
    }
}
