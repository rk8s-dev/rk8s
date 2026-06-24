use crate::sandbox::guest_image::GuestImageManager;
use crate::sandbox::protocol::GuestReadyEvent;
use anyhow::{Context, Result};
use async_trait::async_trait;
use clap::{Args, Parser};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tracing::debug;

pub mod firecracker;
pub mod libkrun;

pub use firecracker::FirecrackerVmBackend;
pub use libkrun::LibkrunVmBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInstanceSpec {
    pub sandbox_id: String,
    pub image: String,
    pub cpus: u32,
    pub memory_mib: u32,
    pub persistent: bool,
    pub kernel_path: Option<PathBuf>,
    pub initrd_path: Option<PathBuf>,
    pub guest_image_path: Option<PathBuf>,
    pub work_dir: PathBuf,
    pub ready_file: PathBuf,
    pub parent_pid: Option<u32>,
    pub vmm_kind: VmmKind,
    pub boot_args: Option<String>,
    pub firecracker_api_socket: PathBuf,
    pub vsock_uds_path: PathBuf,
    pub agent_socket_path: PathBuf,
    pub ready_socket_path: PathBuf,
    pub guest_cid: u32,
    pub agent_vsock_port: u32,
    pub ready_vsock_port: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmInstanceHandle {
    pub sandbox_id: String,
    pub vm_id: String,
    pub pid: Option<u32>,
    pub shim_pid: Option<u32>,
    pub control_socket: Option<PathBuf>,
    pub ready_file: PathBuf,
    pub work_dir: PathBuf,
    pub vmm_kind: VmmKind,
    pub vsock_uds_path: Option<PathBuf>,
    pub agent_socket_path: Option<PathBuf>,
    pub ready_socket_path: Option<PathBuf>,
}

#[async_trait]
pub trait VmBackend: Send + Sync {
    async fn boot(&self, spec: &VmInstanceSpec) -> Result<VmInstanceHandle>;
    async fn wait_ready(&self, handle: &VmInstanceHandle) -> Result<GuestReadyEvent>;
    async fn stop(&self, handle: &VmInstanceHandle) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VmmKind {
    Firecracker,
    Libkrun,
}

impl VmmKind {
    pub const DEFAULT: Self = Self::Libkrun;

    pub fn from_env() -> Result<Self> {
        match std::env::var("RKFORGE_SANDBOX_VMM") {
            Ok(value) => Self::parse(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::DEFAULT),
            Err(err) => Err(err).context("failed to read RKFORGE_SANDBOX_VMM"),
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "firecracker" | "fc" => Ok(Self::Firecracker),
            "libkrun" | "krun" => Ok(Self::Libkrun),
            other => anyhow::bail!(
                "unsupported sandbox VMM kind `{other}`; expected `firecracker` or `libkrun`"
            ),
        }
    }
}

#[derive(Debug, Args)]
pub struct SandboxShimArgs {
    #[arg(long)]
    pub spec: PathBuf,
}

#[derive(Parser)]
struct SandboxShimBinaryCli {
    #[command(flatten)]
    args: SandboxShimArgs,
}

const DEFAULT_GUEST_CID: u32 = 3;
const DEFAULT_AGENT_VSOCK_PORT: u32 = 26_950;
const DEFAULT_READY_VSOCK_PORT: u32 = 26_951;
const SHIM_LOG_FILE: &str = "shim.log";
const SHIM_FAILURE_FILE: &str = "shim-failure.json";
const SHIM_INHERIT_STDIO_ENV: &str = "RKFORGE_SANDBOX_INHERIT_STDIO";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShimFailure {
    pub error: String,
}

pub fn build_vm_spec(
    root: &Path,
    sandbox_id: &str,
    image: &str,
    cpus: u32,
    memory_mib: u32,
    persistent: bool,
    vmm_kind: VmmKind,
) -> Result<VmInstanceSpec> {
    let work_dir = root.join("instances").join(sandbox_id);
    let kernel_path = std::env::var_os("RKFORGE_SANDBOX_KERNEL").map(PathBuf::from);
    let initrd_path = std::env::var_os("RKFORGE_SANDBOX_INITRD").map(PathBuf::from);
    let guest_image_path = resolve_guest_image_path(root, image, vmm_kind)?;
    debug!(
        sandbox_id=%sandbox_id,
        root=%root.display(),
        image=%image,
        cpus,
        memory_mib,
        persistent,
        ?vmm_kind,
        guest_image=?guest_image_path,
        kernel=?kernel_path,
        initrd=?initrd_path,
        "building vm instance spec"
    );
    Ok(VmInstanceSpec {
        sandbox_id: sandbox_id.to_string(),
        image: image.to_string(),
        cpus,
        memory_mib,
        persistent,
        kernel_path,
        initrd_path,
        guest_image_path,
        ready_file: work_dir.join("guest-ready.json"),
        work_dir: work_dir.clone(),
        parent_pid: Some(std::process::id()),
        vmm_kind,
        boot_args: None,
        firecracker_api_socket: work_dir.join("firecracker.socket"),
        vsock_uds_path: work_dir.join("guest.vsock"),
        agent_socket_path: work_dir.join("agent.sock"),
        ready_socket_path: work_dir.join("ready.sock"),
        guest_cid: DEFAULT_GUEST_CID,
        agent_vsock_port: DEFAULT_AGENT_VSOCK_PORT,
        ready_vsock_port: DEFAULT_READY_VSOCK_PORT,
    })
}

fn resolve_guest_image_path(
    root: &Path,
    image: &str,
    vmm_kind: VmmKind,
) -> Result<Option<PathBuf>> {
    let explicit_guest_image = std::env::var_os("RKFORGE_SANDBOX_GUEST_IMAGE").map(PathBuf::from);
    if explicit_guest_image.is_some() {
        return Ok(explicit_guest_image);
    }
    match vmm_kind {
        VmmKind::Firecracker => Ok(None),
        VmmKind::Libkrun => {
            let manager = GuestImageManager::new(root.to_path_buf())?;
            manager.ensure_guest_image(image).map(Some)
        }
    }
}

pub fn run_shim_command(args: SandboxShimArgs) -> Result<()> {
    debug!(spec_path=%args.spec.display(), "running sandbox-shim command");
    let bytes = fs::read(&args.spec)
        .with_context(|| format!("failed to read shim spec {}", args.spec.display()))?;
    let spec: VmInstanceSpec =
        serde_json::from_slice(&bytes).with_context(|| "failed to parse shim spec")?;
    debug!(sandbox_id=%spec.sandbox_id, ?spec.vmm_kind, "parsed shim vm spec");

    let result = match spec.vmm_kind {
        VmmKind::Firecracker => firecracker::run_firecracker_shim(spec),
        VmmKind::Libkrun => libkrun::run_libkrun_shim(spec),
    };

    if let Err(err) = &result {
        let _ = write_shim_failure(args.spec.parent().unwrap_or_else(|| Path::new(".")), err);
    }

    result
}

pub fn run_shim_binary() -> Result<()> {
    run_shim_command(SandboxShimBinaryCli::parse().args)
}

pub fn shim_log_path(work_dir: &Path) -> PathBuf {
    work_dir.join(SHIM_LOG_FILE)
}

pub fn shim_failure_path(work_dir: &Path) -> PathBuf {
    work_dir.join(SHIM_FAILURE_FILE)
}

pub fn load_shim_failure(work_dir: &Path) -> Result<Option<ShimFailure>> {
    let path = shim_failure_path(work_dir);
    if !path.exists() {
        return Ok(None);
    }

    let bytes = fs::read(&path)
        .with_context(|| format!("failed to read shim failure {}", path.display()))?;
    let failure = serde_json::from_slice(&bytes).with_context(|| "failed to parse shim failure")?;
    Ok(Some(failure))
}

pub fn write_shim_failure(work_dir: &Path, err: &anyhow::Error) -> Result<()> {
    fs::create_dir_all(work_dir).with_context(|| {
        format!(
            "failed to create shim work directory {}",
            work_dir.display()
        )
    })?;
    let path = shim_failure_path(work_dir);
    let failure = ShimFailure {
        error: format!("{err:#}"),
    };
    fs::write(&path, serde_json::to_vec_pretty(&failure)?)
        .with_context(|| format!("failed to write shim failure {}", path.display()))?;
    append_log_message(
        &Arc::new(Mutex::new(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(shim_log_path(work_dir))
                .with_context(|| {
                    format!(
                        "failed to open shim log {}",
                        shim_log_path(work_dir).display()
                    )
                })?,
        )),
        &format!("\n===== sandbox-shim failure =====\n{}\n", failure.error),
    );
    Ok(())
}

pub fn spawn_shim_process(shim_binary: &Path, spec_path: &Path, work_dir: &Path) -> Result<u32> {
    fs::create_dir_all(work_dir).with_context(|| {
        format!(
            "failed to create shim work directory {}",
            work_dir.display()
        )
    })?;

    let log_path = shim_log_path(work_dir);
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open shim log {}", log_path.display()))?;
    let shared_log = Arc::new(Mutex::new(log_file));
    let inherit_terminal = env_flag(SHIM_INHERIT_STDIO_ENV);
    debug!(
        shim_binary=%shim_binary.display(),
        spec_path=%spec_path.display(),
        work_dir=%work_dir.display(),
        log_path=%log_path.display(),
        inherit_terminal,
        "spawning sandbox shim process"
    );

    let mut child = Command::new(shim_binary)
        .arg("--spec")
        .arg(spec_path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn sandbox shim via {}", shim_binary.display()))?;

    let child_id = child.id();
    append_log_message(
        &shared_log,
        &format!(
            "\n===== spawned sandbox-shim pid={child_id} spec={} =====\n",
            spec_path.display()
        ),
    );

    if let Some(stdout) = child.stdout.take() {
        spawn_shim_output_pump(stdout, shared_log.clone(), inherit_terminal, false);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_shim_output_pump(stderr, shared_log.clone(), inherit_terminal, true);
    }

    Ok(child_id)
}

fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

fn append_log_message(log: &Arc<Mutex<std::fs::File>>, message: &str) {
    if let Ok(mut file) = log.lock() {
        let _ = file.write_all(message.as_bytes());
        let _ = file.flush();
    }
}

fn spawn_shim_output_pump<R>(
    mut reader: R,
    log: Arc<Mutex<std::fs::File>>,
    inherit_terminal: bool,
    is_stderr: bool,
) where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buf = [0_u8; 8192];
        loop {
            let read = match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(err) => {
                    append_log_message(&log, &format!("\n[shim stream read error] {err}\n"));
                    break;
                }
            };
            let chunk = &buf[..read];

            if let Ok(mut file) = log.lock() {
                let _ = file.write_all(chunk);
                let _ = file.flush();
            }

            if inherit_terminal {
                if is_stderr {
                    let mut stderr = std::io::stderr().lock();
                    let _ = stderr.write_all(chunk);
                    let _ = stderr.flush();
                } else {
                    let mut stdout = std::io::stdout().lock();
                    let _ = stdout.write_all(chunk);
                    let _ = stdout.flush();
                }
            }
        }
    });
}
