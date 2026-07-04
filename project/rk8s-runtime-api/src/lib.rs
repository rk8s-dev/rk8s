//! Runtime provider traits and portable request types for rk8s.

use std::collections::BTreeMap;
use std::path::PathBuf;

pub use async_trait::async_trait;
use rk8s_oci::{ImageReference, Platform};
use serde::{Deserialize, Serialize};

/// Result type used by runtime adapters.
pub type Result<T> = std::result::Result<T, Error>;

/// Runtime API errors.
#[derive(Debug, thiserror::Error, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum Error {
    /// A request field is invalid.
    #[error("invalid runtime request: {0}")]
    Invalid(String),
    /// Runtime does not support the requested feature.
    #[error("runtime feature is unsupported: {0}")]
    Unsupported(String),
    /// Runtime failed to find the requested resource.
    #[error("runtime resource was not found: {0}")]
    NotFound(String),
    /// Runtime operation failed.
    #[error("runtime operation failed: {0}")]
    Operation(String),
}

/// Runtime-scoped container identifier.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContainerId(String);

impl ContainerId {
    /// Creates a validated container id.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(Error::Invalid("container id cannot be empty".to_owned()));
        }
        if value == "." || value == ".." {
            return Err(Error::Invalid("container id cannot be a path".to_owned()));
        }
        if value.contains('/') || value.contains('\\') || value.contains("..") {
            return Err(Error::Invalid(
                "container id cannot contain path separators or traversal".to_owned(),
            ));
        }
        if !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))
        {
            return Err(Error::Invalid(
                "container id contains unsupported characters".to_owned(),
            ));
        }
        Ok(Self(value))
    }

    /// Returns the id as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ContainerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Portable source for creating a container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ContainerSource {
    /// Runtime should resolve and prepare an image reference.
    Image { image: ImageReference },
    /// Runtime should use an existing OCI runtime bundle directory.
    OciBundle { path: PathBuf },
    /// Runtime should use an existing root filesystem.
    Rootfs { path: PathBuf },
}

impl ContainerSource {
    /// Creates an image source.
    pub fn image(image: ImageReference) -> Self {
        Self::Image { image }
    }

    /// Creates an OCI bundle source.
    pub fn oci_bundle(path: impl Into<PathBuf>) -> Self {
        Self::OciBundle { path: path.into() }
    }

    /// Creates a rootfs source.
    pub fn rootfs(path: impl Into<PathBuf>) -> Self {
        Self::Rootfs { path: path.into() }
    }
}

/// Process settings shared by create and exec operations.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProcessSpec {
    /// Command arguments.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Environment variables.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Optional working directory inside the container.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Whether the process should allocate a terminal.
    #[serde(default, skip_serializing_if = "is_false")]
    pub terminal: bool,
}

impl ProcessSpec {
    /// Adds an argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Adds an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Sets the working directory.
    pub fn with_cwd(mut self, cwd: impl Into<String>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    /// Enables terminal allocation.
    pub fn with_terminal(mut self, terminal: bool) -> Self {
        self.terminal = terminal;
        self
    }
}

/// Request to create a container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateContainerRequest {
    /// Container id.
    pub id: ContainerId,
    /// Container source.
    pub source: ContainerSource,
    /// Process settings.
    #[serde(default)]
    pub process: ProcessSpec,
    /// Optional platform selector.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    /// Runtime-specific annotations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

impl CreateContainerRequest {
    /// Creates a request from an id and source.
    pub fn new(id: ContainerId, source: ContainerSource) -> Self {
        Self {
            id,
            source,
            process: ProcessSpec::default(),
            platform: None,
            annotations: BTreeMap::new(),
        }
    }

    /// Sets the target platform.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Adds a process argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.process.args.push(arg.into());
        self
    }

    /// Adds an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.process.env.insert(key.into(), value.into());
        self
    }

    /// Adds an annotation.
    pub fn with_annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }
}

/// Request to start a container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartContainerRequest {
    /// Container id.
    pub id: ContainerId,
}

impl StartContainerRequest {
    /// Creates a start request.
    pub fn new(id: ContainerId) -> Self {
        Self { id }
    }
}

/// Request to stop a container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopContainerRequest {
    /// Container id.
    pub id: ContainerId,
    /// Optional timeout in seconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

impl StopContainerRequest {
    /// Creates a stop request.
    pub fn new(id: ContainerId) -> Self {
        Self {
            id,
            timeout_secs: None,
        }
    }

    /// Sets the graceful stop timeout.
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = Some(timeout_secs);
        self
    }
}

/// Request to delete a container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeleteContainerRequest {
    /// Container id.
    pub id: ContainerId,
    /// Whether the runtime should force deletion.
    #[serde(default, skip_serializing_if = "is_false")]
    pub force: bool,
}

impl DeleteContainerRequest {
    /// Creates a delete request.
    pub fn new(id: ContainerId) -> Self {
        Self { id, force: false }
    }

    /// Enables forced deletion.
    pub fn force(mut self) -> Self {
        self.force = true;
        self
    }
}

/// Request to inspect one container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerStatusRequest {
    /// Container id.
    pub id: ContainerId,
}

impl ContainerStatusRequest {
    /// Creates an inspect request.
    pub fn new(id: ContainerId) -> Self {
        Self { id }
    }
}

/// Request to list containers.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ListContainersRequest {
    /// Optional state filter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<ContainerState>,
}

impl ListContainersRequest {
    /// Creates an empty list request.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filters containers by state.
    pub fn with_state(mut self, state: ContainerState) -> Self {
        self.state = Some(state);
        self
    }
}

/// Request to execute a process in a container.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecContainerRequest {
    /// Container id.
    pub id: ContainerId,
    /// Process settings.
    #[serde(default)]
    pub process: ProcessSpec,
}

impl ExecContainerRequest {
    /// Creates an exec request.
    pub fn new(id: ContainerId) -> Self {
        Self {
            id,
            process: ProcessSpec::default(),
        }
    }

    /// Adds a process argument.
    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.process.args.push(arg.into());
        self
    }

    /// Adds an environment variable.
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.process.env.insert(key.into(), value.into());
        self
    }
}

/// Result of an exec operation.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    /// Process exit code.
    pub exit_code: i32,
    /// Captured stdout bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stderr: Vec<u8>,
}

impl ExecResult {
    /// Creates a successful exec result.
    pub fn success() -> Self {
        Self {
            exit_code: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }

    /// Sets stdout bytes.
    pub fn with_stdout(mut self, stdout: impl Into<Vec<u8>>) -> Self {
        self.stdout = stdout.into();
        self
    }

    /// Sets stderr bytes.
    pub fn with_stderr(mut self, stderr: impl Into<Vec<u8>>) -> Self {
        self.stderr = stderr.into();
        self
    }
}

/// Container lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContainerState {
    /// Container metadata exists but it has not started.
    Created,
    /// Container is running.
    Running,
    /// Container has stopped.
    Stopped,
    /// Container runtime reported an error state.
    Failed,
    /// Container state is not known to the adapter.
    Unknown,
}

/// Portable container status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerStatus {
    /// Container id.
    pub id: ContainerId,
    /// Current lifecycle state.
    pub state: ContainerState,
    /// Optional host process id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// Optional runtime-specific message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl ContainerStatus {
    /// Creates a status value.
    pub fn new(id: ContainerId, state: ContainerState) -> Self {
        Self {
            id,
            state,
            pid: None,
            message: None,
        }
    }

    /// Sets the host process id.
    pub fn with_pid(mut self, pid: u32) -> Self {
        self.pid = Some(pid);
        self
    }

    /// Sets a runtime message.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Runtime feature advertised by an adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeFeature {
    /// Runtime can resolve image references.
    Image,
    /// Runtime can consume OCI bundle directories.
    OciBundle,
    /// Runtime can consume rootfs directories.
    Rootfs,
    /// Runtime supports exec.
    Exec,
    /// Runtime supports terminal allocation.
    Terminal,
}

/// Runtime health and capability status.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    /// Runtime adapter name.
    pub name: String,
    /// Optional runtime version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Whether the runtime is ready to accept requests.
    pub healthy: bool,
    /// Runtime capabilities.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<RuntimeFeature>,
    /// Optional status message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl RuntimeStatus {
    /// Creates a runtime status with `healthy` set to false.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            healthy: false,
            features: Vec::new(),
            message: None,
        }
    }

    /// Sets the runtime version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Adds a feature if it is not already present.
    pub fn with_feature(mut self, feature: RuntimeFeature) -> Self {
        if !self.features.contains(&feature) {
            self.features.push(feature);
        }
        self
    }

    /// Marks the runtime healthy.
    pub fn healthy(mut self) -> Self {
        self.healthy = true;
        self
    }

    /// Sets a status message.
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Runtime provider contract implemented by concrete adapters.
#[async_trait]
pub trait ContainerRuntime: Send + Sync {
    /// Human-readable adapter name.
    fn name(&self) -> &str;

    /// Reports runtime health and capabilities.
    async fn status(&self) -> Result<RuntimeStatus>;

    /// Creates container metadata and storage from a portable request.
    async fn create(&self, request: CreateContainerRequest) -> Result<ContainerStatus>;

    /// Starts a created container.
    async fn start(&self, request: StartContainerRequest) -> Result<ContainerStatus>;

    /// Stops a running container.
    async fn stop(&self, request: StopContainerRequest) -> Result<ContainerStatus>;

    /// Deletes a container.
    async fn delete(&self, request: DeleteContainerRequest) -> Result<()>;

    /// Executes a process in a container.
    async fn exec(&self, request: ExecContainerRequest) -> Result<ExecResult>;

    /// Inspects one container.
    async fn inspect(&self, request: ContainerStatusRequest) -> Result<ContainerStatus>;

    /// Lists containers visible to the adapter.
    async fn list(&self, request: ListContainersRequest) -> Result<Vec<ContainerStatus>>;
}

fn is_false(value: &bool) -> bool {
    !*value
}
