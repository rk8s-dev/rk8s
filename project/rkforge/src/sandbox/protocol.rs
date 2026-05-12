use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReadyStage {
    VmmReady,
    GuestAgentReady,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestReadyEvent {
    pub sandbox_id: String,
    pub stage: ReadyStage,
    pub agent_version: String,
    pub transport: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestExecRequest {
    pub request_id: String,
    pub sandbox_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_secs: Option<u64>,
    pub inline_code: Option<String>,
    pub language: Option<String>,
}

impl GuestExecRequest {
    pub fn new(sandbox_id: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            request_id: Uuid::new_v4().to_string(),
            sandbox_id: sandbox_id.into(),
            command: command.into(),
            args: Vec::new(),
            timeout_secs: None,
            inline_code: None,
            language: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuestExecResponse {
    pub request_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}
