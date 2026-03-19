use crate::sandbox::protocol::GuestReadyEvent;
use crate::sandbox::vm::{VmBackend, VmInstanceHandle, VmInstanceSpec, VmmKind};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use chrono::Utc;
use clap::Args;
use nix::sys::signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use uuid::Uuid;

const DEFAULT_BOOT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DEFAULT_GUEST_CID: u32 = 3;

#[derive(Debug, Clone)]
pub struct FirecrackerVmBackend {
    root: PathBuf,
    shim_binary: PathBuf,
    #[allow(dead_code)]
    firecracker_binary: PathBuf,
    boot_timeout: Duration,
}

impl FirecrackerVmBackend {
    pub fn new(root: PathBuf) -> Result<Self> {
        let shim_binary = std::env::var_os("RKFORGE_SANDBOX_SHIM_BIN")
            .map(PathBuf::from)
            .unwrap_or(std::env::current_exe().context("failed to resolve current executable")?);
        let firecracker_binary = find_firecracker_binary().ok_or_else(|| {
            anyhow!("failed to locate firecracker binary; set RKFORGE_FIRECRACKER_BIN")
        })?;
        Ok(Self {
            root,
            shim_binary,
            firecracker_binary,
            boot_timeout: DEFAULT_BOOT_TIMEOUT,
        })
    }

    fn handle_path(&self, sandbox_id: &str) -> PathBuf {
        self.root
            .join("instances")
            .join(sandbox_id)
            .join("vm-handle.json")
    }

    fn spec_path(&self, sandbox_id: &str) -> PathBuf {
        self.root
            .join("instances")
            .join(sandbox_id)
            .join("vm-spec.json")
    }

    fn runtime_state_path(&self, sandbox_id: &str) -> PathBuf {
        self.root
            .join("instances")
            .join(sandbox_id)
            .join("runtime-state.json")
    }

    pub fn save_handle(&self, handle: &VmInstanceHandle) -> Result<()> {
        fs::create_dir_all(&handle.work_dir)
            .with_context(|| format!("failed to create {}", handle.work_dir.display()))?;
        fs::write(
            self.handle_path(&handle.sandbox_id),
            serde_json::to_vec_pretty(handle)?,
        )
        .with_context(|| format!("failed to persist vm handle for {}", handle.sandbox_id))?;
        Ok(())
    }

    pub fn load_handle(&self, sandbox_id: &str) -> Result<Option<VmInstanceHandle>> {
        let path = self.handle_path(sandbox_id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)
            .with_context(|| format!("failed to read vm handle {}", path.display()))?;
        let handle = serde_json::from_slice(&bytes).with_context(|| "failed to parse vm handle")?;
        Ok(Some(handle))
    }

    pub fn save_spec(&self, spec: &VmInstanceSpec) -> Result<()> {
        fs::create_dir_all(&spec.work_dir)
            .with_context(|| format!("failed to create {}", spec.work_dir.display()))?;
        fs::write(
            self.spec_path(&spec.sandbox_id),
            serde_json::to_vec_pretty(spec)?,
        )
        .with_context(|| format!("failed to persist vm spec for {}", spec.sandbox_id))?;
        Ok(())
    }

    fn spawn_shim(&self, spec_path: &Path) -> Result<u32> {
        let child = Command::new(&self.shim_binary)
            .arg("sandbox-shim")
            .arg("--spec")
            .arg(spec_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to spawn sandbox shim via {}",
                    self.shim_binary.display()
                )
            })?;
        Ok(child.id())
    }
}

#[async_trait]
impl VmBackend for FirecrackerVmBackend {
    async fn boot(&self, spec: &VmInstanceSpec) -> Result<VmInstanceHandle> {
        validate_vm_spec(spec)?;
        fs::create_dir_all(&spec.work_dir)
            .with_context(|| format!("failed to create {}", spec.work_dir.display()))?;
        cleanup_runtime_paths(spec)?;
        self.save_spec(spec)?;

        let shim_pid = self.spawn_shim(&self.spec_path(&spec.sandbox_id))?;
        let runtime_state = wait_for_runtime_state(
            &self.runtime_state_path(&spec.sandbox_id),
            self.boot_timeout,
        )?;

        let handle = VmInstanceHandle {
            sandbox_id: spec.sandbox_id.clone(),
            vm_id: format!("fc-{}", Uuid::new_v4().simple()),
            pid: Some(runtime_state.firecracker_pid),
            shim_pid: Some(shim_pid),
            control_socket: Some(spec.firecracker_api_socket.clone()),
            ready_file: spec.ready_file.clone(),
            work_dir: spec.work_dir.clone(),
            vmm_kind: VmmKind::Firecracker,
            vsock_uds_path: Some(spec.vsock_uds_path.clone()),
        };
        self.save_handle(&handle)?;
        Ok(handle)
    }

    async fn wait_ready(&self, handle: &VmInstanceHandle) -> Result<GuestReadyEvent> {
        let deadline = Instant::now() + self.boot_timeout;
        while Instant::now() < deadline {
            if handle.ready_file.exists() {
                let bytes = fs::read(&handle.ready_file).with_context(|| {
                    format!("failed to read ready file {}", handle.ready_file.display())
                })?;
                let event: GuestReadyEvent = serde_json::from_slice(&bytes)
                    .with_context(|| "failed to parse guest ready event")?;
                return Ok(event);
            }
            tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
        }
        Err(anyhow!(
            "timed out waiting for guest ready signal for sandbox {}",
            handle.sandbox_id
        ))
    }

    async fn stop(&self, handle: &VmInstanceHandle) -> Result<()> {
        if let Some(pid) = handle.pid {
            let _ = signal::kill(Pid::from_raw(pid as i32), signal::Signal::SIGTERM);
        }
        if let Some(shim_pid) = handle.shim_pid {
            let _ = signal::kill(Pid::from_raw(shim_pid as i32), signal::Signal::SIGTERM);
        }
        let handle_path = self.handle_path(&handle.sandbox_id);
        if handle_path.exists() {
            let _ = fs::remove_file(handle_path);
        }
        Ok(())
    }
}

#[derive(Debug, Args)]
pub struct SandboxShimArgs {
    #[arg(long)]
    pub spec: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct ShimState {
    firecracker_binary: PathBuf,
    spec: VmInstanceSpec,
}

#[derive(Debug, Serialize, Deserialize)]
struct RuntimeState {
    firecracker_pid: u32,
}

pub fn run_shim_command(args: SandboxShimArgs) -> Result<()> {
    let bytes = fs::read(&args.spec)
        .with_context(|| format!("failed to read shim spec {}", args.spec.display()))?;
    let spec: VmInstanceSpec =
        serde_json::from_slice(&bytes).with_context(|| "failed to parse shim spec")?;

    validate_vm_spec(&spec)?;
    fs::create_dir_all(&spec.work_dir)
        .with_context(|| format!("failed to create {}", spec.work_dir.display()))?;
    fs::write(
        spec.work_dir.join("shim.pid"),
        std::process::id().to_string(),
    )?;

    let firecracker_binary = find_firecracker_binary().ok_or_else(|| {
        anyhow!("failed to locate firecracker binary; set RKFORGE_FIRECRACKER_BIN")
    })?;

    let shim_state = ShimState {
        firecracker_binary: firecracker_binary.clone(),
        spec: spec.clone(),
    };
    fs::write(
        spec.work_dir.join("shim-state.json"),
        serde_json::to_vec_pretty(&shim_state)?,
    )?;

    let mut firecracker = spawn_firecracker(&firecracker_binary, &spec)?;
    wait_for_socket(&spec.firecracker_api_socket, DEFAULT_BOOT_TIMEOUT)?;
    configure_firecracker(&spec)?;

    let runtime_state = RuntimeState {
        firecracker_pid: firecracker.id(),
    };
    fs::write(
        spec.work_dir.join("runtime-state.json"),
        serde_json::to_vec_pretty(&runtime_state)?,
    )?;

    let ready = GuestReadyEvent {
        sandbox_id: spec.sandbox_id.clone(),
        agent_version: "rkforge-firecracker-shim".to_string(),
        transport: "firecracker-api".to_string(),
        timestamp: Utc::now(),
    };
    fs::write(&spec.ready_file, serde_json::to_vec_pretty(&ready)?).with_context(|| {
        format!(
            "failed to write ready signal for sandbox {}",
            spec.sandbox_id
        )
    })?;

    supervise_firecracker(&spec, &mut firecracker)
}

pub fn build_vm_spec(
    root: &Path,
    sandbox_id: &str,
    image: &str,
    cpus: u32,
    memory_mib: u32,
    persistent: bool,
) -> VmInstanceSpec {
    let work_dir = root.join("instances").join(sandbox_id);
    let guest_image_path = std::env::var_os("RKFORGE_SANDBOX_GUEST_IMAGE").map(PathBuf::from);
    let kernel_path = std::env::var_os("RKFORGE_SANDBOX_KERNEL").map(PathBuf::from);
    let initrd_path = std::env::var_os("RKFORGE_SANDBOX_INITRD").map(PathBuf::from);
    VmInstanceSpec {
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
        boot_args: None,
        firecracker_api_socket: work_dir.join("firecracker.socket"),
        vsock_uds_path: work_dir.join("guest.vsock"),
        guest_cid: DEFAULT_GUEST_CID,
    }
}

fn cleanup_runtime_paths(spec: &VmInstanceSpec) -> Result<()> {
    let paths = vec![
        spec.ready_file.clone(),
        spec.firecracker_api_socket.clone(),
        spec.vsock_uds_path.clone(),
        spec.work_dir.join("runtime-state.json"),
        spec.work_dir.join("shim.pid"),
        spec.work_dir.join("shim-state.json"),
    ];
    for path in paths {
        if path.exists() {
            if path.is_dir() {
                fs::remove_dir_all(&path)?;
            } else {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

fn validate_vm_spec(spec: &VmInstanceSpec) -> Result<()> {
    let kernel_path = spec
        .kernel_path
        .as_ref()
        .ok_or_else(|| anyhow!("RKFORGE_SANDBOX_KERNEL must be configured for Firecracker boot"))?;
    if !kernel_path.exists() {
        bail!("kernel path does not exist: {}", kernel_path.display());
    }

    let has_rootfs = spec
        .guest_image_path
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);
    let has_initrd = spec
        .initrd_path
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);

    if !has_rootfs && !has_initrd {
        bail!(
            "Firecracker boot requires either RKFORGE_SANDBOX_GUEST_IMAGE (rootfs) or RKFORGE_SANDBOX_INITRD"
        );
    }

    Ok(())
}

fn find_firecracker_binary() -> Option<PathBuf> {
    let explicit = std::env::var_os("RKFORGE_FIRECRACKER_BIN").map(PathBuf::from);
    if let Some(path) = explicit
        && path.exists()
    {
        return Some(path);
    }

    for candidate in [
        "/usr/bin/firecracker",
        "/usr/local/bin/firecracker",
        "/opt/firecracker/firecracker",
    ] {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn spawn_firecracker(binary: &Path, spec: &VmInstanceSpec) -> Result<Child> {
    let mut cmd = Command::new(binary);
    cmd.arg("--api-sock")
        .arg(&spec.firecracker_api_socket)
        .arg("--no-seccomp")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn()
        .with_context(|| format!("failed to spawn firecracker via {}", binary.display()))
}

fn wait_for_socket(socket_path: &Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if socket_path.exists() {
            return Ok(());
        }
        std::thread::sleep(DEFAULT_POLL_INTERVAL);
    }
    Err(anyhow!(
        "timed out waiting for firecracker socket {}",
        socket_path.display()
    ))
}

fn wait_for_runtime_state(path: &Path, timeout: Duration) -> Result<RuntimeState> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            let bytes =
                fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
            let state: RuntimeState =
                serde_json::from_slice(&bytes).with_context(|| "failed to parse runtime state")?;
            return Ok(state);
        }
        std::thread::sleep(DEFAULT_POLL_INTERVAL);
    }
    Err(anyhow!(
        "timed out waiting for runtime state {}",
        path.display()
    ))
}

fn configure_firecracker(spec: &VmInstanceSpec) -> Result<()> {
    let socket = &spec.firecracker_api_socket;

    firecracker_put(
        socket,
        "/machine-config",
        &json!({
            "vcpu_count": spec.cpus,
            "mem_size_mib": spec.memory_mib,
            "smt": false,
            "track_dirty_pages": false
        }),
    )?;

    let boot_args = spec
        .boot_args
        .clone()
        .unwrap_or_else(|| default_boot_args(spec));
    let mut boot_source = json!({
        "kernel_image_path": spec.kernel_path.as_ref().unwrap(),
        "boot_args": boot_args
    });
    if let Some(initrd) = &spec.initrd_path {
        boot_source["initrd_path"] = json!(initrd);
    }
    firecracker_put(socket, "/boot-source", &boot_source)?;

    if let Some(rootfs) = &spec.guest_image_path {
        firecracker_put(
            socket,
            "/drives/rootfs",
            &json!({
                "drive_id": "rootfs",
                "path_on_host": rootfs,
                "is_root_device": true,
                "is_read_only": false
            }),
        )?;
    }

    firecracker_put(
        socket,
        "/vsock",
        &json!({
            "vsock_id": "agent",
            "guest_cid": spec.guest_cid,
            "uds_path": spec.vsock_uds_path
        }),
    )?;

    firecracker_put(
        socket,
        "/actions",
        &json!({
            "action_type": "InstanceStart"
        }),
    )?;

    Ok(())
}

fn default_boot_args(spec: &VmInstanceSpec) -> String {
    if spec.guest_image_path.is_some() {
        "console=ttyS0 reboot=k panic=1 pci=off root=/dev/vda rw".to_string()
    } else {
        "console=ttyS0 reboot=k panic=1 pci=off".to_string()
    }
}

fn firecracker_put(socket_path: &Path, path: &str, body: &serde_json::Value) -> Result<()> {
    let body = serde_json::to_vec(body)?;
    let mut stream = UnixStream::connect(socket_path).with_context(|| {
        format!(
            "failed to connect to firecracker socket {}",
            socket_path.display()
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;

    let request = format!(
        "PUT {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;

    let mut response = Vec::new();
    stream.read_to_end(&mut response)?;
    let response = String::from_utf8_lossy(&response);
    let status_line = response
        .lines()
        .next()
        .ok_or_else(|| anyhow!("firecracker returned an empty HTTP response"))?;
    if !status_line.contains(" 200 ")
        && !status_line.contains(" 204 ")
        && !status_line.contains(" 201 ")
    {
        bail!("firecracker API call to {path} failed: {status_line}");
    }
    Ok(())
}

fn supervise_firecracker(spec: &VmInstanceSpec, firecracker: &mut Child) -> Result<()> {
    loop {
        if let Some(parent_pid) = spec.parent_pid {
            let parent = Pid::from_raw(parent_pid as i32);
            if signal::kill(parent, None).is_err() {
                let _ = terminate_child(firecracker);
                break;
            }
        }

        if let Some(status) = firecracker.try_wait()? {
            if !status.success() {
                return Err(anyhow!("firecracker exited with status {}", status));
            }
            break;
        }

        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(())
}

fn terminate_child(child: &mut Child) -> Result<()> {
    let pid = Pid::from_raw(child.id() as i32);
    let _ = signal::kill(pid, signal::Signal::SIGTERM);
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = signal::kill(pid, signal::Signal::SIGKILL);
    let _ = child.wait();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn build_vm_spec_uses_fixed_guest_paths() {
        let dir = tempdir().unwrap();
        let spec = build_vm_spec(dir.path(), "demo", "python:3.12-slim", 1, 256, false);
        assert_eq!(spec.sandbox_id, "demo");
        assert!(spec.work_dir.ends_with("demo"));
        assert!(spec.ready_file.ends_with("guest-ready.json"));
        assert!(spec.firecracker_api_socket.ends_with("firecracker.socket"));
        assert!(spec.vsock_uds_path.ends_with("guest.vsock"));
    }
}
