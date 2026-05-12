pub mod agent;
pub mod cli;
pub mod guest;
mod guest_image;
pub mod protocol;
mod runtime_assets;
pub mod sdk;
pub mod types;
pub mod vm;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use protocol::{GuestExecRequest, GuestExecResponse, ReadyStage};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};
use uuid::Uuid;
use vm::{
    FirecrackerVmBackend, LibkrunVmBackend, VmBackend, VmInstanceHandle, VmmKind, build_vm_spec,
};

/// Current Single Sandbox State
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SandboxState {
    Creating,
    Booting,
    Ready,
    Running,
    Stopped,
    Failed,
}

impl fmt::Display for SandboxState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Creating => "creating",
            Self::Booting => "booting",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        };
        write!(f, "{value}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxOptions {
    pub image: String,
    pub cpus: u32,
    pub memory_mib: u32,
    pub persistent: bool,
    pub name: Option<String>,
}

impl Default for SandboxOptions {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSpec {
    pub image: String,
    pub cpus: u32,
    pub memory_mib: u32,
    pub persistent: bool,
    pub name: Option<String>,
}

impl From<SandboxOptions> for SandboxSpec {
    fn from(value: SandboxOptions) -> Self {
        Self {
            image: value.image,
            cpus: value.cpus,
            memory_mib: value.memory_mib,
            persistent: value.persistent,
            name: value.name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxInfo {
    pub id: String,
    pub spec: SandboxSpec,
    pub state: SandboxState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

pub type ExecRequest = GuestExecRequest;
pub type ExecResult = GuestExecResponse;

#[async_trait]
pub trait SandboxBackend: Send + Sync {
    async fn create(&self, info: &SandboxInfo) -> Result<()>;
    async fn start(&self, info: &SandboxInfo) -> Result<()>;
    async fn exec(&self, info: &SandboxInfo, request: &ExecRequest) -> Result<ExecResult>;
    async fn stop(&self, info: &SandboxInfo) -> Result<()>;
    async fn remove(&self, info: &SandboxInfo, force: bool) -> Result<()>;
}

pub struct MicroVmSandboxBackend {
    vm_backend: Arc<dyn VmBackend>,
    root: PathBuf,
    vmm_kind: VmmKind,
}

impl MicroVmSandboxBackend {
    pub fn new(root: PathBuf) -> Result<Self> {
        let vmm_kind = VmmKind::from_env()?;
        Self::new_with_vmm(root, vmm_kind)
    }

    pub fn new_with_vmm(root: PathBuf, vmm_kind: VmmKind) -> Result<Self> {
        let vm_backend: Arc<dyn VmBackend> = match vmm_kind {
            VmmKind::Firecracker => Arc::new(FirecrackerVmBackend::new(root.clone())?),
            VmmKind::Libkrun => Arc::new(LibkrunVmBackend::new(root.clone())?),
        };
        debug!(root=%root.display(), ?vmm_kind, "initialized microvm sandbox backend");
        Ok(Self {
            vm_backend,
            root,
            vmm_kind,
        })
    }

    fn handle_path(&self, sandbox_id: &str) -> PathBuf {
        self.root
            .join("instances")
            .join(sandbox_id)
            .join("vm-handle.json")
    }

    fn save_handle(&self, handle: &VmInstanceHandle) -> Result<()> {
        fs::create_dir_all(&handle.work_dir)
            .with_context(|| format!("failed to create {}", handle.work_dir.display()))?;
        fs::write(
            self.handle_path(&handle.sandbox_id),
            serde_json::to_vec_pretty(handle)?,
        )
        .with_context(|| format!("failed to persist vm handle for {}", handle.sandbox_id))?;
        Ok(())
    }

    fn load_handle(&self, sandbox_id: &str) -> Result<Option<VmInstanceHandle>> {
        let path = self.handle_path(sandbox_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read vm handle {}", path.display()))?;
        let handle = serde_json::from_slice(&bytes).with_context(|| "failed to parse vm handle")?;
        Ok(Some(handle))
    }
}

#[async_trait]
impl SandboxBackend for MicroVmSandboxBackend {
    async fn create(&self, info: &SandboxInfo) -> Result<()> {
        let existing = self.load_handle(&info.id)?;
        if existing.is_some() {
            debug!(sandbox_id=%info.id, "reusing existing vm handle");
            return Ok(());
        }
        debug!(
            sandbox_id=%info.id,
            image=%info.spec.image,
            cpus=info.spec.cpus,
            memory_mib=info.spec.memory_mib,
            persistent=info.spec.persistent,
            ?self.vmm_kind,
            "building vm spec and booting sandbox vm"
        );
        let spec = build_vm_spec(
            &self.root,
            &info.id,
            &info.spec.image,
            info.spec.cpus,
            info.spec.memory_mib,
            info.spec.persistent,
            self.vmm_kind,
        )?;
        debug!(
            sandbox_id=%info.id,
            work_dir=%spec.work_dir.display(),
            ready_file=%spec.ready_file.display(),
            guest_image=?spec.guest_image_path,
            kernel=?spec.kernel_path,
            initrd=?spec.initrd_path,
            "vm spec prepared"
        );
        let handle = self.vm_backend.boot(&spec).await?;
        debug!(
            sandbox_id=%info.id,
            vm_id=%handle.vm_id,
            pid=?handle.pid,
            shim_pid=?handle.shim_pid,
            "vm boot returned handle"
        );
        self.save_handle(&handle)?;
        Ok(())
    }

    async fn start(&self, info: &SandboxInfo) -> Result<()> {
        let handle = self
            .load_handle(&info.id)?
            .ok_or_else(|| anyhow!("vm handle missing for sandbox {}", info.id))?;
        debug!(
            sandbox_id=%info.id,
            ready_file=%handle.ready_file.display(),
            "waiting for sandbox readiness"
        );
        let _ready = self.vm_backend.wait_ready(&handle).await?;
        debug!(sandbox_id=%info.id, "sandbox reported ready");
        Ok(())
    }

    async fn exec(&self, info: &SandboxInfo, request: &ExecRequest) -> Result<ExecResult> {
        let handle = self
            .load_handle(&info.id)?
            .ok_or_else(|| anyhow!("vm handle missing for sandbox {}", info.id))?;
        debug!(
            sandbox_id=%info.id,
            command=%request.command,
            args=?request.args,
            timeout_secs=?request.timeout_secs,
            inline_code=request.inline_code.is_some(),
            ?handle.vmm_kind,
            "executing request in sandbox"
        );

        match handle.vmm_kind {
            VmmKind::Libkrun => {
                let socket_path = handle
                    .agent_socket_path
                    .as_ref()
                    .ok_or_else(|| anyhow!("agent socket path missing for sandbox {}", info.id))?;
                debug!(
                    sandbox_id=%info.id,
                    socket_path=%socket_path.display(),
                    "connecting to libkrun guest agent socket"
                );
                let mut stream = connect_agent_socket(socket_path)?;
                write_message(&mut stream, request)?;
                stream.flush()?;
                let response: ExecResult = read_message(&mut stream)?;
                debug!(
                    sandbox_id=%info.id,
                    request_id=%response.request_id,
                    exit_code=response.exit_code,
                    stdout_len=response.stdout.len(),
                    stderr_len=response.stderr.len(),
                    "received guest exec response"
                );
                Ok(response)
            }
            VmmKind::Firecracker => {
                debug!(sandbox_id=%info.id, "serving firecracker exec stub response");
                let mut stderr = String::new();
                stderr.push_str(
                    "sandbox reached phase-1 VMM readiness, but Firecracker guest-agent exec is not wired yet\n",
                );
                if request.inline_code.is_some() {
                    stderr.push_str(
                        "python code was accepted by the host runtime stub and was not executed inside the guest\n",
                    );
                }
                Ok(ExecResult {
                    request_id: request.request_id.clone(),
                    stdout: format!(
                        "sandbox {} is {:?}; guest-agent integration is the next step\n",
                        info.id,
                        ReadyStage::VmmReady
                    ),
                    stderr,
                    exit_code: 0,
                })
            }
        }
    }

    async fn stop(&self, info: &SandboxInfo) -> Result<()> {
        if let Some(handle) = self.load_handle(&info.id)? {
            debug!(
                sandbox_id=%info.id,
                pid=?handle.pid,
                shim_pid=?handle.shim_pid,
                "stopping sandbox vm"
            );
            self.vm_backend.stop(&handle).await?;
        }
        Ok(())
    }

    async fn remove(&self, info: &SandboxInfo, force: bool) -> Result<()> {
        let _ = force;
        debug!(sandbox_id=%info.id, force, "removing sandbox instance");
        self.stop(info).await?;
        let instance_dir = self.root.join("instances").join(&info.id);
        if instance_dir.exists() {
            fs::remove_dir_all(&instance_dir).with_context(|| {
                format!(
                    "failed to remove instance directory {}",
                    instance_dir.display()
                )
            })?;
        }
        Ok(())
    }
}

fn connect_agent_socket(path: &Path) -> Result<std::os::unix::net::UnixStream> {
    const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
    const RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

    let deadline = std::time::Instant::now() + CONNECT_TIMEOUT;
    loop {
        match std::os::unix::net::UnixStream::connect(path) {
            Ok(stream) => {
                debug!(socket_path=%path.display(), "connected to guest agent socket");
                return Ok(stream);
            }
            Err(err) if std::time::Instant::now() < deadline => {
                let _ = err;
                std::thread::sleep(RETRY_INTERVAL);
            }
            Err(err) => {
                return Err(err).with_context(|| {
                    format!(
                        "failed to connect to guest agent socket {}; ensure the guest image starts `rkforge sandbox-agent`",
                        path.display()
                    )
                });
            }
        }
    }
}

fn write_message<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<()> {
    let payload = serde_json::to_vec(value)?;
    let len = u32::try_from(payload.len()).context("message too large")?;
    writer.write_all(&len.to_be_bytes())?;
    writer.write_all(&payload)?;
    Ok(())
}

fn read_message<T: for<'de> Deserialize<'de>>(reader: &mut impl Read) -> Result<T> {
    let mut len_buf = [0_u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0_u8; len];
    reader.read_exact(&mut payload)?;
    let value = serde_json::from_slice(&payload)?;
    Ok(value)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SandboxRecord {
    info: SandboxInfo,
}

#[derive(Debug, Clone)]
struct SandboxStore {
    root: PathBuf,
}

impl SandboxStore {
    fn new(root: PathBuf) -> Result<Self> {
        let store = Self { root };
        fs::create_dir_all(store.sandbox_dir())
            .with_context(|| format!("failed to create {}", store.sandbox_dir().display()))?;
        Ok(store)
    }

    fn default_root() -> Result<PathBuf> {
        if let Some(path) = dirs::data_local_dir() {
            return Ok(path.join("rkforge").join("sandbox"));
        }
        //
        if let Some(home) = dirs::home_dir() {
            return Ok(home
                .join(".local")
                .join("share")
                .join("rkforge")
                .join("sandbox"));
        }
        bail!("failed to resolve local data directory for sandbox store")
    }

    fn sandbox_dir(&self) -> PathBuf {
        self.root.join("sandboxes")
    }

    fn record_path(&self, id: &str) -> PathBuf {
        self.sandbox_dir().join(format!("{id}.json"))
    }

    fn save(&self, info: &SandboxInfo) -> Result<()> {
        let record = SandboxRecord { info: info.clone() };
        let bytes = serde_json::to_vec_pretty(&record)?;
        fs::write(self.record_path(&info.id), bytes)
            .with_context(|| format!("failed to persist sandbox {}", info.id))?;
        Ok(())
    }

    fn load(&self, id: &str) -> Result<Option<SandboxInfo>> {
        let path = self.record_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes =
            fs::read(&path).with_context(|| format!("failed to read sandbox record {}", id))?;
        let record: SandboxRecord = serde_json::from_slice(&bytes)
            .with_context(|| format!("failed to parse sandbox record {}", id))?;
        Ok(Some(record.info))
    }

    fn list(&self) -> Result<Vec<SandboxInfo>> {
        let mut values = Vec::new();
        for entry in fs::read_dir(self.sandbox_dir())? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path)
                .with_context(|| format!("failed to read sandbox record {}", path.display()))?;
            let record: SandboxRecord = serde_json::from_slice(&bytes)
                .with_context(|| format!("failed to parse sandbox record {}", path.display()))?;
            values.push(record.info);
        }
        values.sort_by_key(|a| a.created_at);
        Ok(values)
    }

    fn delete(&self, id: &str) -> Result<()> {
        let path = self.record_path(id);
        if path.exists() {
            fs::remove_file(&path)
                .with_context(|| format!("failed to remove sandbox record {}", id))?;
        }
        Ok(())
    }
}

pub struct SandboxRuntimeBuilder {
    root: Option<PathBuf>,
    backend: Option<Arc<dyn SandboxBackend>>,
}

impl Default for SandboxRuntimeBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SandboxRuntimeBuilder {
    pub fn new() -> Self {
        Self {
            root: None,
            backend: None,
        }
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    pub fn with_backend(mut self, backend: Arc<dyn SandboxBackend>) -> Self {
        self.backend = Some(backend);
        self
    }

    pub fn build(self) -> Result<SandboxRuntime> {
        let root = match self.root {
            Some(root) => root,
            None => SandboxStore::default_root()?,
        };
        let backend = match self.backend {
            Some(backend) => backend,
            None => Arc::new(MicroVmSandboxBackend::new(root.clone())?),
        };
        SandboxRuntime::new_with_backend(root, backend)
    }
}

#[derive(Clone)]
pub struct SandboxRuntime {
    inner: Arc<SandboxRuntimeInner>,
}

struct SandboxRuntimeInner {
    store: SandboxStore,
    backend: Arc<dyn SandboxBackend>,
}

impl SandboxRuntime {
    pub fn builder() -> SandboxRuntimeBuilder {
        SandboxRuntimeBuilder::new()
    }

    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    pub fn new_with_backend(root: PathBuf, backend: Arc<dyn SandboxBackend>) -> Result<Self> {
        debug!(root=%root.display(), "creating sandbox runtime");
        Ok(Self {
            inner: Arc::new(SandboxRuntimeInner {
                store: SandboxStore::new(root)?,
                backend,
            }),
        })
    }

    pub fn root(&self) -> &Path {
        &self.inner.store.root
    }

    pub fn create(&self, options: SandboxOptions) -> Result<SandboxBox> {
        let now = Utc::now();
        let info = SandboxInfo {
            id: options
                .name
                .clone()
                .unwrap_or_else(|| format!("sbx-{}", Uuid::new_v4().simple())),
            spec: options.into(),
            state: SandboxState::Creating,
            created_at: now,
            updated_at: now,
            last_error: None,
        };

        debug!(
            sandbox_id=%info.id,
            image=%info.spec.image,
            cpus=info.spec.cpus,
            memory_mib=info.spec.memory_mib,
            persistent=info.spec.persistent,
            "creating sandbox record"
        );
        self.inner.store.save(&info)?;
        Ok(SandboxBox {
            runtime: self.clone(),
            id: info.id,
        })
    }

    pub fn get(&self, id: impl Into<String>) -> Result<Option<SandboxBox>> {
        let id = id.into();
        if self.inner.store.load(&id)?.is_some() {
            return Ok(Some(SandboxBox {
                runtime: self.clone(),
                id,
            }));
        }
        Ok(None)
    }

    pub fn get_info(&self, id: &str) -> Result<Option<SandboxInfo>> {
        self.inner.store.load(id)
    }

    pub fn list(&self) -> Result<Vec<SandboxInfo>> {
        self.inner.store.list()
    }

    pub async fn remove(&self, id: &str, force: bool) -> Result<()> {
        debug!(sandbox_id=%id, force, "removing sandbox from runtime");
        let info = self
            .inner
            .store
            .load(id)?
            .ok_or_else(|| anyhow!("sandbox {} not found", id))?;
        self.inner.backend.remove(&info, force).await?;
        self.inner.store.delete(id)?;
        Ok(())
    }

    fn save_info(&self, info: &SandboxInfo) -> Result<()> {
        self.inner.store.save(info)
    }
}

pub type SandboxManager = SandboxRuntime;

#[allow(unused_imports)]
pub use sdk::{SandboxClient, SandboxClientBuilder, SandboxHandle};
#[allow(unused_imports)]
pub use types::{
    SandboxCreateOptions, SandboxExecOptions, SandboxExecResult, SandboxExecSpec,
    SandboxExecTarget, SandboxMetadata,
};

#[derive(Clone)]
pub struct SandboxBox {
    runtime: SandboxRuntime,
    pub id: String,
}

impl SandboxBox {
    pub fn info(&self) -> Result<SandboxInfo> {
        self.runtime
            .get_info(&self.id)?
            .ok_or_else(|| anyhow!("sandbox {} not found", self.id))
    }

    // Update current sandbox's state and try to start sandbox by create and start inner backend
    pub async fn start(&self) -> Result<()> {
        let mut info = self.info()?;
        debug!(sandbox_id=%self.id, state=%info.state.to_string(), "starting sandbox");
        match info.state {
            SandboxState::Creating | SandboxState::Stopped | SandboxState::Failed => {}
            SandboxState::Booting | SandboxState::Ready | SandboxState::Running => return Ok(()),
        }

        info.state = SandboxState::Booting;
        info.updated_at = Utc::now();
        info.last_error = None;
        self.runtime.save_info(&info)?;

        if let Err(err) = self.runtime.inner.backend.create(&info).await {
            return self.fail(info, err);
        }
        if let Err(err) = self.runtime.inner.backend.start(&info).await {
            return self.fail(info, err);
        }

        info.state = SandboxState::Ready;
        info.updated_at = Utc::now();
        self.runtime.save_info(&info)?;
        debug!(sandbox_id=%self.id, "sandbox transitioned to ready");
        Ok(())
    }

    pub async fn exec(&self, command: impl Into<String>, args: Vec<String>) -> Result<ExecResult> {
        let mut request = ExecRequest::new(self.id.clone(), command.into());
        request.args = args;
        self.exec_request(request).await
    }

    pub async fn exec_python(&self, code: impl Into<String>) -> Result<ExecResult> {
        let mut request = ExecRequest::new(self.id.clone(), "python3");
        request.inline_code = Some(code.into());
        request.language = Some("python".to_string());
        self.exec_request(request).await
    }

    pub async fn stop(&self) -> Result<()> {
        let mut info = self.info()?;
        if matches!(info.state, SandboxState::Stopped) {
            return Ok(());
        }
        debug!(sandbox_id=%self.id, state=%info.state.to_string(), "stopping sandbox");
        self.runtime.inner.backend.stop(&info).await?;
        info.state = SandboxState::Stopped;
        info.updated_at = Utc::now();
        self.runtime.save_info(&info)?;
        debug!(sandbox_id=%self.id, "sandbox transitioned to stopped");
        Ok(())
    }

    pub async fn exec_request(&self, request: ExecRequest) -> Result<ExecResult> {
        debug!(
            sandbox_id=%self.id,
            request_id=%request.request_id,
            command=%request.command,
            args=?request.args,
            timeout_secs=?request.timeout_secs,
            inline_code=request.inline_code.is_some(),
            "processing sandbox exec request"
        );

        self.start().await?;

        let mut info = self.info()?;
        info.state = SandboxState::Running;
        info.updated_at = Utc::now();
        self.runtime.save_info(&info)?;

        let exec_result = self.runtime.inner.backend.exec(&info, &request).await;
        match exec_result {
            Ok(result) => {
                info.state = SandboxState::Ready;
                info.updated_at = Utc::now();
                self.runtime.save_info(&info)?;
                debug!(
                    sandbox_id=%self.id,
                    request_id=%result.request_id,
                    exit_code=result.exit_code,
                    "sandbox exec request completed"
                );
                Ok(result)
            }
            Err(err) => self.fail(info, err),
        }
    }

    fn fail<T>(&self, mut info: SandboxInfo, err: anyhow::Error) -> Result<T> {
        warn!(sandbox_id=%self.id, error=%err, "sandbox transitioned to failed");
        info.state = SandboxState::Failed;
        info.updated_at = Utc::now();
        info.last_error = Some(err.to_string());
        let _ = self.runtime.save_info(&info);
        Err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    #[ignore]
    async fn runtime_persists_lifecycle() {
        let dir = tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let runtime = SandboxRuntime::new_with_backend(
            root.clone(),
            Arc::new(MicroVmSandboxBackend::new(root).unwrap()),
        )
        .unwrap();
        let sandbox = runtime
            .create(SandboxOptions {
                image: "python:3.12-slim".to_string(),
                cpus: 2,
                memory_mib: 512,
                persistent: true,
                name: Some("demo-box".to_string()),
            })
            .unwrap();

        let created = sandbox.info().unwrap();
        assert_eq!(created.state, SandboxState::Creating);
        assert_eq!(created.spec.image, "python:3.12-slim");

        let result = sandbox.exec_python("print('hello')").await.unwrap();
        assert_eq!(result.exit_code, 0);

        let ready = sandbox.info().unwrap();
        assert_eq!(ready.state, SandboxState::Ready);

        sandbox.stop().await.unwrap();
        let stopped = sandbox.info().unwrap();
        assert_eq!(stopped.state, SandboxState::Stopped);

        runtime.remove(&sandbox.id, false).await.unwrap();
        assert!(runtime.get_info(&sandbox.id).unwrap().is_none());
    }
}
