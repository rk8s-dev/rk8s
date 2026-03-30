pub mod cli;
pub mod protocol;
pub mod vm;

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use protocol::{GuestExecRequest, GuestExecResponse};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;
use vm::{FirecrackerVmBackend, VmBackend, VmInstanceHandle, build_vm_spec};

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
}

impl MicroVmSandboxBackend {
    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self {
            // TODO: Support other VmBackend in the future
            vm_backend: Arc::new(FirecrackerVmBackend::new(root.clone())?),
            root,
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
            return Ok(());
        }
        let spec = build_vm_spec(
            &self.root,
            &info.id,
            &info.spec.image,
            info.spec.cpus,
            info.spec.memory_mib,
            info.spec.persistent,
        );
        let handle = self.vm_backend.boot(&spec).await?;
        self.save_handle(&handle)?;
        Ok(())
    }

    async fn start(&self, info: &SandboxInfo) -> Result<()> {
        let handle = self
            .load_handle(&info.id)?
            .ok_or_else(|| anyhow!("vm handle missing for sandbox {}", info.id))?;
        let _ready = self.vm_backend.wait_ready(&handle).await?;
        Ok(())
    }

    async fn exec(&self, info: &SandboxInfo, request: &ExecRequest) -> Result<ExecResult> {
        let mut stderr = String::new();
        stderr.push_str("microvm is booted, but guest-agent exec is not wired yet\n");
        if request.inline_code.is_some() {
            stderr.push_str("python code was accepted by the host runtime stub\n");
        }
        Ok(ExecResult {
            request_id: request.request_id.clone(),
            stdout: format!("sandbox {} is ready for guest-agent integration\n", info.id),
            stderr,
            exit_code: 0,
        })
    }

    async fn stop(&self, info: &SandboxInfo) -> Result<()> {
        if let Some(handle) = self.load_handle(&info.id)? {
            self.vm_backend.stop(&handle).await?;
        }
        Ok(())
    }

    async fn remove(&self, info: &SandboxInfo, force: bool) -> Result<()> {
        let _ = force;
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

    pub async fn start(&self) -> Result<()> {
        let mut info = self.info()?;
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
        self.runtime.inner.backend.stop(&info).await?;
        info.state = SandboxState::Stopped;
        info.updated_at = Utc::now();
        self.runtime.save_info(&info)?;
        Ok(())
    }

    pub async fn exec_request(&self, request: ExecRequest) -> Result<ExecResult> {
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
                Ok(result)
            }
            Err(err) => self.fail(info, err),
        }
    }

    fn fail<T>(&self, mut info: SandboxInfo, err: anyhow::Error) -> Result<T> {
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
