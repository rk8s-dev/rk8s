use crate::sandbox::{ExecResult, SandboxInfo, SandboxOptions};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxCreateOptions {
    pub image: String,
    pub cpus: u32,
    pub memory_mib: u32,
    pub persistent: bool,
    pub name: Option<String>,
}

impl Default for SandboxCreateOptions {
    fn default() -> Self {
        Self {
            image: "python:3.12-slim".to_string(),
            cpus: 1,
            memory_mib: 256,
            persistent: false,
            name: None,
        }
    }
}

impl SandboxCreateOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn image(mut self, image: impl Into<String>) -> Self {
        self.image = image.into();
        self
    }

    pub fn cpus(mut self, cpus: u32) -> Self {
        self.cpus = cpus;
        self
    }

    pub fn memory_mib(mut self, memory_mib: u32) -> Self {
        self.memory_mib = memory_mib;
        self
    }

    pub fn persistent(mut self, persistent: bool) -> Self {
        self.persistent = persistent;
        self
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

impl From<SandboxCreateOptions> for SandboxOptions {
    fn from(value: SandboxCreateOptions) -> Self {
        Self {
            image: value.image,
            cpus: value.cpus,
            memory_mib: value.memory_mib,
            persistent: value.persistent,
            name: value.name,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxExecOptions {
    pub timeout_secs: Option<u64>,
}

impl SandboxExecOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SandboxExecTarget {
    Command { command: String, args: Vec<String> },
    Python { code: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxExecSpec {
    pub target: SandboxExecTarget,
    pub timeout_secs: Option<u64>,
}

impl SandboxExecSpec {
    pub fn command(command: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            target: SandboxExecTarget::Command {
                command: command.into(),
                args,
            },
            timeout_secs: None,
        }
    }

    pub fn python(code: impl Into<String>) -> Self {
        Self {
            target: SandboxExecTarget::Python { code: code.into() },
            timeout_secs: None,
        }
    }

    pub fn timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxExecResult {
    pub request_id: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl From<ExecResult> for SandboxExecResult {
    fn from(value: ExecResult) -> Self {
        Self {
            request_id: value.request_id,
            stdout: value.stdout,
            stderr: value.stderr,
            exit_code: value.exit_code,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxMetadata {
    pub info: SandboxInfo,
}

impl From<SandboxInfo> for SandboxMetadata {
    fn from(info: SandboxInfo) -> Self {
        Self { info }
    }
}
