use crate::sandbox::types::{
    SandboxCreateOptions, SandboxExecOptions, SandboxExecResult, SandboxExecSpec,
    SandboxExecTarget, SandboxMetadata,
};
use crate::sandbox::{ExecRequest, SandboxBox, SandboxRuntime, SandboxRuntimeBuilder};
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct SandboxClientBuilder {
    root: Option<PathBuf>,
}

impl SandboxClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.root = Some(root.into());
        self
    }

    pub fn build(self) -> Result<SandboxClient> {
        let mut builder = SandboxRuntimeBuilder::new();
        if let Some(root) = self.root {
            builder = builder.with_root(root);
        }
        let runtime = builder.build()?;
        Ok(SandboxClient { runtime })
    }
}

#[derive(Clone)]
pub struct SandboxClient {
    runtime: SandboxRuntime,
}

impl SandboxClient {
    pub fn builder() -> SandboxClientBuilder {
        SandboxClientBuilder::new()
    }

    pub fn new() -> Result<Self> {
        Self::builder().build()
    }

    pub fn root(&self) -> &Path {
        self.runtime.root()
    }

    pub fn create(&self, options: SandboxCreateOptions) -> Result<SandboxHandle> {
        let sandbox = self.runtime.create(options.into())?;
        Ok(SandboxHandle { inner: sandbox })
    }

    pub fn get(&self, id: impl Into<String>) -> Result<Option<SandboxHandle>> {
        self.runtime
            .get(id.into())?
            .map(|inner| Ok(SandboxHandle { inner }))
            .transpose()
    }

    pub fn inspect(&self, id: &str) -> Result<SandboxMetadata> {
        let info = self
            .runtime
            .get_info(id)?
            .ok_or_else(|| anyhow!("sandbox not found"))?;
        Ok(info.into())
    }

    pub fn list(&self) -> Result<Vec<SandboxMetadata>> {
        Ok(self.runtime.list()?.into_iter().map(Into::into).collect())
    }

    pub async fn remove(&self, id: &str, force: bool) -> Result<()> {
        self.runtime.remove(id, force).await
    }
}

#[derive(Clone)]
pub struct SandboxHandle {
    inner: SandboxBox,
}

impl SandboxHandle {
    pub fn id(&self) -> &str {
        &self.inner.id
    }

    pub fn inspect(&self) -> Result<SandboxMetadata> {
        Ok(self.inner.info()?.into())
    }

    pub async fn start(&self) -> Result<()> {
        self.inner.start().await
    }

    pub async fn exec(
        &self,
        command: impl Into<String>,
        args: Vec<String>,
        options: SandboxExecOptions,
    ) -> Result<SandboxExecResult> {
        let mut spec = SandboxExecSpec::command(command, args);
        spec.timeout_secs = options.timeout_secs;
        self.execute(spec).await
    }

    pub async fn exec_python(
        &self,
        code: impl Into<String>,
        options: SandboxExecOptions,
    ) -> Result<SandboxExecResult> {
        let mut spec = SandboxExecSpec::python(code);
        spec.timeout_secs = options.timeout_secs;
        self.execute(spec).await
    }

    pub async fn execute(&self, spec: SandboxExecSpec) -> Result<SandboxExecResult> {
        let mut request = match spec.target {
            SandboxExecTarget::Command { command, args } => {
                let mut request = ExecRequest::new(self.inner.id.clone(), command);
                request.args = args;
                request
            }
            SandboxExecTarget::Python { code } => {
                let mut request = ExecRequest::new(self.inner.id.clone(), "python3");
                request.inline_code = Some(code);
                request.language = Some("python".to_string());
                request
            }
        };
        request.timeout_secs = spec.timeout_secs;
        Ok(self.inner.exec_request(request).await?.into())
    }

    pub async fn stop(&self) -> Result<()> {
        self.inner.stop().await
    }

    pub async fn remove(&self, force: bool) -> Result<()> {
        self.inner.runtime.remove(&self.inner.id, force).await
    }
}
