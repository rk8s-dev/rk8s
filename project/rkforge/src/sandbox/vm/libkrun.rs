use crate::sandbox::protocol::GuestReadyEvent;
use crate::sandbox::read_message;
use crate::sandbox::runtime_assets::{
    RuntimeAssetBundle, discover_libkrun_library, discover_libkrunfw_path,
};
use crate::sandbox::vm::{
    VmBackend, VmInstanceHandle, VmInstanceSpec, VmmKind, load_shim_failure, shim_failure_path,
    shim_log_path, spawn_shim_process,
};
use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use libloading::Library;
use nix::sys::signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use std::ffi::{CString, OsString};
use std::fs;
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{debug, warn};
use uuid::Uuid;

const DEFAULT_BOOT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(100);
const KRUN_KERNEL_FORMAT_RAW: u32 = 0;
const GUEST_AGENT_BINARY_PATH: &str = "/usr/local/bin/rkforge-sandbox-agent";

type KrunCreateCtxFn = unsafe extern "C" fn() -> c_int;
type KrunSetVmConfigFn = unsafe extern "C" fn(u32, u8, u32) -> c_int;
type KrunSetKernelFn =
    unsafe extern "C" fn(u32, *const c_char, u32, *const c_char, *const c_char) -> c_int;
type KrunSetWorkdirFn = unsafe extern "C" fn(u32, *const c_char) -> c_int;
type KrunSetExecFn =
    unsafe extern "C" fn(u32, *const c_char, *const *const c_char, *const *const c_char) -> c_int;
type KrunAddDiskFn = unsafe extern "C" fn(u32, *const c_char, *const c_char, bool) -> c_int;
type KrunSetRootDiskRemountFn =
    unsafe extern "C" fn(u32, *const c_char, *const c_char, *const c_char) -> c_int;
type KrunAddVsockPort2Fn = unsafe extern "C" fn(u32, u32, *const c_char, bool) -> c_int;
type KrunStartEnterFn = unsafe extern "C" fn(u32) -> c_int;

#[derive(Debug, Clone)]
pub struct LibkrunVmBackend {
    root: PathBuf,
    shim_binary: PathBuf,
    boot_timeout: Duration,
}

impl LibkrunVmBackend {
    pub fn new(root: PathBuf) -> Result<Self> {
        let assets = RuntimeAssetBundle::prepare(&root)?;
        let shim_binary = assets.shim_binary().to_path_buf();
        Ok(Self {
            root,
            shim_binary,
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

    fn save_spec(&self, spec: &VmInstanceSpec) -> Result<()> {
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
        let work_dir = spec_path
            .parent()
            .ok_or_else(|| anyhow!("shim spec path has no parent: {}", spec_path.display()))?;
        spawn_shim_process(&self.shim_binary, spec_path, work_dir)
    }
}

#[async_trait]
impl VmBackend for LibkrunVmBackend {
    async fn boot(&self, spec: &VmInstanceSpec) -> Result<VmInstanceHandle> {
        debug!(
            sandbox_id=%spec.sandbox_id,
            work_dir=%spec.work_dir.display(),
            guest_image=?spec.guest_image_path,
            kernel=?spec.kernel_path,
            initrd=?spec.initrd_path,
            "booting libkrun vm"
        );
        validate_vm_spec(spec)?;
        fs::create_dir_all(&spec.work_dir)
            .with_context(|| format!("failed to create {}", spec.work_dir.display()))?;

        cleanup_runtime_paths(spec)?;
        spawn_ready_listener(spec)?;
        self.save_spec(spec)?;

        let shim_pid = self.spawn_shim(&self.spec_path(&spec.sandbox_id))?;
        debug!(sandbox_id=%spec.sandbox_id, shim_pid, "spawned libkrun shim");
        let runtime_state = wait_for_runtime_state(
            &self.runtime_state_path(&spec.sandbox_id),
            &spec.work_dir,
            self.boot_timeout,
        )?;
        debug!(
            sandbox_id=%spec.sandbox_id,
            vmm_pid=runtime_state.vmm_pid,
            libkrun_library=?runtime_state.libkrun_library,
            libkrunfw_path=?runtime_state.libkrunfw_path,
            "received libkrun runtime state"
        );

        let handle = VmInstanceHandle {
            sandbox_id: spec.sandbox_id.clone(),
            vm_id: format!("krun-{}", Uuid::new_v4().simple()),
            pid: Some(runtime_state.vmm_pid),
            shim_pid: Some(shim_pid),
            control_socket: None,
            ready_file: spec.ready_file.clone(),
            work_dir: spec.work_dir.clone(),
            vmm_kind: VmmKind::Libkrun,
            vsock_uds_path: None,
            agent_socket_path: Some(spec.agent_socket_path.clone()),
            ready_socket_path: Some(spec.ready_socket_path.clone()),
        };
        self.save_handle(&handle)?;
        Ok(handle)
    }

    async fn wait_ready(&self, handle: &VmInstanceHandle) -> Result<GuestReadyEvent> {
        let deadline = Instant::now() + self.boot_timeout;
        debug!(
            sandbox_id=%handle.sandbox_id,
            ready_file=%handle.ready_file.display(),
            timeout_secs=self.boot_timeout.as_secs(),
            "waiting for libkrun guest ready event"
        );
        while Instant::now() < deadline {
            if handle.ready_file.exists() {
                let bytes = fs::read(&handle.ready_file).with_context(|| {
                    format!("failed to read ready file {}", handle.ready_file.display())
                })?;
                let event: GuestReadyEvent = serde_json::from_slice(&bytes)
                    .with_context(|| "failed to parse guest ready event")?;
                debug!(
                    sandbox_id=%handle.sandbox_id,
                    stage=?event.stage,
                    transport=%event.transport,
                    "received libkrun guest ready event"
                );
                return Ok(event);
            }
            if let Some(failure) = load_shim_failure(&handle.work_dir)? {
                warn!(
                    sandbox_id=%handle.sandbox_id,
                    error=%failure.error,
                    "libkrun shim failed before guest ready"
                );
                return Err(anyhow!(
                    "sandbox shim failed before guest ready: {}",
                    failure.error
                ));
            }
            tokio::time::sleep(DEFAULT_POLL_INTERVAL).await;
        }
        warn!(
            sandbox_id=%handle.sandbox_id,
            ready_file=%handle.ready_file.display(),
            "timed out waiting for libkrun guest ready event"
        );
        Err(anyhow!(
            "timed out waiting for guest ready signal for sandbox {}",
            handle.sandbox_id
        ))
    }

    async fn stop(&self, handle: &VmInstanceHandle) -> Result<()> {
        debug!(
            sandbox_id=%handle.sandbox_id,
            pid=?handle.pid,
            shim_pid=?handle.shim_pid,
            "sending stop signals to libkrun vm"
        );
        if let Some(pid) = handle.pid {
            let _ = signal::kill(Pid::from_raw(pid as i32), signal::Signal::SIGTERM);
        }
        if let Some(shim_pid) = handle.shim_pid
            && Some(shim_pid) != handle.pid
        {
            let _ = signal::kill(Pid::from_raw(shim_pid as i32), signal::Signal::SIGTERM);
        }
        let handle_path = self.handle_path(&handle.sandbox_id);
        if handle_path.exists() {
            let _ = fs::remove_file(handle_path);
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ShimState {
    spec: VmInstanceSpec,
    libkrun_library: Option<PathBuf>,
    libkrunfw_path: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RuntimeState {
    vmm_pid: u32,
    libkrun_library: Option<PathBuf>,
    libkrunfw_path: Option<PathBuf>,
}

pub fn run_libkrun_shim(spec: VmInstanceSpec) -> Result<()> {
    debug!(
        sandbox_id=%spec.sandbox_id,
        work_dir=%spec.work_dir.display(),
        guest_image=?spec.guest_image_path,
        kernel=?spec.kernel_path,
        initrd=?spec.initrd_path,
        "starting libkrun shim"
    );
    validate_vm_spec(&spec)?;
    fs::create_dir_all(&spec.work_dir)
        .with_context(|| format!("failed to create {}", spec.work_dir.display()))?;
    fs::write(
        spec.work_dir.join("shim.pid"),
        std::process::id().to_string(),
    )?;

    let libkrun_library = find_libkrun_library();
    let libkrunfw_path = resolve_libkrunfw_path(&spec, libkrun_library.as_deref())?;
    debug!(
        sandbox_id=%spec.sandbox_id,
        libkrun_library=?libkrun_library,
        libkrunfw_path=?libkrunfw_path,
        "resolved libkrun runtime assets"
    );

    configure_runtime_library_path(libkrun_library.as_deref(), libkrunfw_path.as_deref())?;

    let shim_state = ShimState {
        spec: spec.clone(),
        libkrun_library: libkrun_library.clone(),
        libkrunfw_path: libkrunfw_path.clone(),
    };
    fs::write(
        spec.work_dir.join("shim-state.json"),
        serde_json::to_vec_pretty(&shim_state)?,
    )?;

    let runtime_state = RuntimeState {
        vmm_pid: std::process::id(),
        libkrun_library: libkrun_library.clone(),
        libkrunfw_path: libkrunfw_path.clone(),
    };
    fs::write(
        spec.work_dir.join("runtime-state.json"),
        serde_json::to_vec_pretty(&runtime_state)?,
    )?;

    if let Some(parent_pid) = spec.parent_pid {
        start_parent_watchdog(parent_pid);
    }

    // start to link libkrun library to current shim process
    let symbols = unsafe { LibkrunSymbols::load(libkrun_library.as_deref()) }?;
    debug!(sandbox_id=%spec.sandbox_id, "loaded libkrun shared library symbols");
    let ctx = unsafe { configure_ctx(&symbols, &spec) }?;
    debug!(sandbox_id=%spec.sandbox_id, ctx, "configured libkrun context");
    let rc = unsafe { (symbols.start_enter)(ctx) };
    if rc < 0 {
        bail!("krun_start_enter failed with errno {}", -rc);
    }
    debug!(sandbox_id=%spec.sandbox_id, "libkrun context entered guest execution");
    Ok(())
}

unsafe fn configure_ctx(symbols: &LibkrunSymbols, spec: &VmInstanceSpec) -> Result<u32> {
    let ctx = unsafe { krun_call("krun_create_ctx", (symbols.create_ctx)())? as u32 };
    let num_vcpus =
        u8::try_from(spec.cpus).map_err(|_| anyhow!("libkrun supports at most 255 vCPUs"))?;
    unsafe {
        krun_call(
            "krun_set_vm_config",
            (symbols.set_vm_config)(ctx, num_vcpus, spec.memory_mib),
        )?
    };
    debug!(
        sandbox_id=%spec.sandbox_id,
        ctx,
        cpus=spec.cpus,
        memory_mib=spec.memory_mib,
        "configured libkrun vm config"
    );

    if let Some(kernel_path) = &spec.kernel_path {
        let kernel_path = path_cstring(kernel_path)?;
        let initrd_path = spec
            .initrd_path
            .as_ref()
            .map(|path| path_cstring(path.as_path()))
            .transpose()?;
        let boot_args = CString::new(default_boot_args(spec))
            .context("failed to encode libkrun boot arguments")?;
        unsafe {
            krun_call(
                "krun_set_kernel",
                (symbols.set_kernel)(
                    ctx,
                    kernel_path.as_ptr(),
                    KRUN_KERNEL_FORMAT_RAW,
                    initrd_path
                        .as_ref()
                        .map(|value| value.as_ptr())
                        .unwrap_or(std::ptr::null()),
                    boot_args.as_ptr(),
                ),
            )?
        };
        debug!(
            sandbox_id=%spec.sandbox_id,
            kernel=%kernel_path.to_string_lossy(),
            initrd=?spec.initrd_path,
            "configured explicit libkrun kernel"
        );
    }

    let guest_image = spec.guest_image_path.as_ref().ok_or_else(|| {
        anyhow!("RKFORGE_SANDBOX_GUEST_IMAGE must be configured for libkrun boot")
    })?;
    let block_id = CString::new("rootfs").unwrap();
    let guest_image = path_cstring(guest_image)?;
    unsafe {
        krun_call(
            "krun_add_disk",
            (symbols.add_disk)(ctx, block_id.as_ptr(), guest_image.as_ptr(), false),
        )?
    };
    debug!(sandbox_id=%spec.sandbox_id, ctx, "configured libkrun root disk");

    let guest_device = CString::new("/dev/vda").unwrap();
    let fstype = CString::new("ext4").unwrap();
    let mount_options = CString::new("rw").unwrap();
    unsafe {
        krun_call(
            "krun_set_root_disk_remount",
            (symbols.set_root_disk_remount)(
                ctx,
                guest_device.as_ptr(),
                fstype.as_ptr(),
                mount_options.as_ptr(),
            ),
        )?
    };
    debug!(sandbox_id=%spec.sandbox_id, ctx, "configured libkrun root disk remount");

    let ready_socket = path_cstring(&spec.ready_socket_path)?;
    unsafe {
        krun_call(
            "krun_add_vsock_port2(ready)",
            (symbols.add_vsock_port2)(ctx, spec.ready_vsock_port, ready_socket.as_ptr(), false),
        )?
    };
    debug!(
        sandbox_id=%spec.sandbox_id,
        ctx,
        ready_vsock_port=spec.ready_vsock_port,
        ready_socket=%spec.ready_socket_path.display(),
        "configured libkrun ready vsock bridge"
    );

    let agent_socket = path_cstring(&spec.agent_socket_path)?;
    unsafe {
        krun_call(
            "krun_add_vsock_port2(agent)",
            (symbols.add_vsock_port2)(ctx, spec.agent_vsock_port, agent_socket.as_ptr(), true),
        )?
    };
    debug!(
        sandbox_id=%spec.sandbox_id,
        ctx,
        agent_vsock_port=spec.agent_vsock_port,
        agent_socket=%spec.agent_socket_path.display(),
        "configured libkrun agent vsock bridge"
    );

    unsafe { configure_guest_entrypoint(symbols, spec, ctx)? };

    Ok(ctx)
}

unsafe fn configure_guest_entrypoint(
    symbols: &LibkrunSymbols,
    spec: &VmInstanceSpec,
    ctx: u32,
) -> Result<()> {
    let workdir = CString::new("/").unwrap();
    unsafe {
        krun_call(
            "krun_set_workdir",
            (symbols.set_workdir)(ctx, workdir.as_ptr()),
        )?
    };

    let exec_path = CString::new(GUEST_AGENT_BINARY_PATH).unwrap();
    let argv_storage = [
        CString::new("--vsock-port").unwrap(),
        CString::new(spec.agent_vsock_port.to_string()).unwrap(),
        CString::new("--ready-vsock-port").unwrap(),
        CString::new(spec.ready_vsock_port.to_string()).unwrap(),
        CString::new("--sandbox-id").unwrap(),
        CString::new(spec.sandbox_id.as_str()).context("sandbox id contains interior NUL")?,
    ];
    let mut argv_ptrs: Vec<*const c_char> = argv_storage.iter().map(|arg| arg.as_ptr()).collect();
    argv_ptrs.push(std::ptr::null());

    let env_ptrs = [std::ptr::null()];
    unsafe {
        krun_call(
            "krun_set_exec",
            (symbols.set_exec)(
                ctx,
                exec_path.as_ptr(),
                argv_ptrs.as_ptr(),
                env_ptrs.as_ptr(),
            ),
        )?
    };
    debug!(
        sandbox_id=%spec.sandbox_id,
        ctx,
        exec_path=GUEST_AGENT_BINARY_PATH,
        workdir="/",
        agent_vsock_port=spec.agent_vsock_port,
        ready_vsock_port=spec.ready_vsock_port,
        "configured libkrun guest entrypoint"
    );

    Ok(())
}

unsafe fn krun_call(name: &str, rc: c_int) -> Result<c_int> {
    if rc < 0 {
        bail!("{name} failed with errno {}", -rc);
    }
    Ok(rc)
}

fn cleanup_runtime_paths(spec: &VmInstanceSpec) -> Result<()> {
    let paths = vec![
        spec.ready_file.clone(),
        spec.agent_socket_path.clone(),
        spec.ready_socket_path.clone(),
        spec.work_dir.join("runtime-state.json"),
        spec.work_dir.join("shim.pid"),
        spec.work_dir.join("shim-state.json"),
        shim_failure_path(&spec.work_dir),
        shim_log_path(&spec.work_dir),
    ];
    for path in paths {
        if path.exists() {
            debug!(
                sandbox_id=%spec.sandbox_id,
                path=%path.display(),
                "cleaning stale libkrun runtime path"
            );
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
    if let Some(kernel_path) = spec.kernel_path.as_ref()
        && !kernel_path.exists()
    {
        bail!("kernel path does not exist: {}", kernel_path.display());
    }
    if let Some(initrd_path) = spec.initrd_path.as_ref()
        && !initrd_path.exists()
    {
        bail!("initrd path does not exist: {}", initrd_path.display());
    }
    if spec.initrd_path.is_some() && spec.kernel_path.is_none() {
        bail!("RKFORGE_SANDBOX_INITRD requires RKFORGE_SANDBOX_KERNEL for libkrun boot");
    }
    let guest_image = spec.guest_image_path.as_ref().ok_or_else(|| {
        anyhow!("RKFORGE_SANDBOX_GUEST_IMAGE must be configured for libkrun boot")
    })?;
    if !guest_image.exists() {
        bail!("guest image path does not exist: {}", guest_image.display());
    }
    Ok(())
}

// Detect the runtime-state.json's existence, when the runtime-state.json is written to
// specific path, then the shim process is start up successfully
fn wait_for_runtime_state(path: &Path, work_dir: &Path, timeout: Duration) -> Result<RuntimeState> {
    let deadline = Instant::now() + timeout;
    debug!(
        path=%path.display(),
        timeout_secs=timeout.as_secs(),
        "waiting for libkrun runtime state file"
    );
    while Instant::now() < deadline {
        if path.exists() {
            let bytes =
                fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
            let state: RuntimeState =
                serde_json::from_slice(&bytes).with_context(|| "failed to parse runtime state")?;
            debug!(path=%path.display(), vmm_pid=state.vmm_pid, "loaded libkrun runtime state");
            return Ok(state);
        }
        if let Some(failure) = load_shim_failure(work_dir)? {
            return Err(anyhow!(
                "sandbox shim failed before runtime state was written: {}",
                failure.error
            ));
        }
        std::thread::sleep(DEFAULT_POLL_INTERVAL);
    }
    Err(anyhow!(
        "timed out waiting for runtime state {}",
        path.display()
    ))
}

fn default_boot_args(spec: &VmInstanceSpec) -> String {
    spec.boot_args.clone().unwrap_or_else(|| {
        format!(
            "console=ttyS0 reboot=k panic=1 rkforge.sandbox_id={}",
            spec.sandbox_id
        )
    })
}

// Create ready_listener from `ready_socket_path` at host end to make sure host get the ready signal
fn spawn_ready_listener(spec: &VmInstanceSpec) -> Result<()> {
    let listener = UnixListener::bind(&spec.ready_socket_path).with_context(|| {
        format!(
            "failed to bind ready socket {}",
            spec.ready_socket_path.display()
        )
    })?;
    let ready_file = spec.ready_file.clone();
    let sandbox_id = spec.sandbox_id.clone();
    debug!(
        sandbox_id=%sandbox_id,
        ready_socket=%spec.ready_socket_path.display(),
        ready_file=%ready_file.display(),
        "spawned libkrun ready listener"
    );
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept()
            && let Ok(event) = read_message::<GuestReadyEvent>(&mut stream)
            && let Ok(bytes) = serde_json::to_vec_pretty(&event)
        {
            debug!(
                sandbox_id=%sandbox_id,
                stage=?event.stage,
                transport=%event.transport,
                "libkrun ready listener received event"
            );
            let _ = fs::write(&ready_file, bytes);
        }
    });
    Ok(())
}

fn path_cstring(path: &Path) -> Result<CString> {
    CString::new(path.as_os_str().as_bytes())
        .with_context(|| format!("path contains an interior NUL byte: {}", path.display()))
}

// To prevent shim-process become a orphan process
fn start_parent_watchdog(parent_pid: u32) {
    std::thread::spawn(move || {
        let parent = Pid::from_raw(parent_pid as i32);
        let self_pid = Pid::this();
        loop {
            if signal::kill(parent, None).is_err() {
                let _ = signal::kill(self_pid, signal::Signal::SIGTERM);
                std::thread::sleep(Duration::from_secs(3));
                let _ = signal::kill(self_pid, signal::Signal::SIGKILL);
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

// Make sure the `LD_LIBARY_PATH` is setted in advance
fn configure_runtime_library_path(
    libkrun_library: Option<&Path>,
    libkrunfw_path: Option<&Path>,
) -> Result<()> {
    let mut dirs = Vec::new();
    if let Some(path) = libkrun_library.and_then(Path::parent) {
        dirs.push(path.to_path_buf());
    }
    if let Some(path) = libkrunfw_path.and_then(Path::parent) {
        dirs.push(path.to_path_buf());
    }
    if dirs.is_empty() {
        return Ok(());
    }

    if let Some(current) = std::env::var_os("LD_LIBRARY_PATH") {
        dirs.extend(std::env::split_paths(&current));
    }
    let joined =
        std::env::join_paths(dirs).context("failed to compose LD_LIBRARY_PATH for libkrun shim")?;
    // SAFETY: the shim updates its own environment before spawning any helper threads or loading
    // libkrun.
    unsafe {
        std::env::set_var("LD_LIBRARY_PATH", joined);
    }
    debug!(
        libkrun_library=?libkrun_library,
        libkrunfw_path=?libkrunfw_path,
        "configured shim LD_LIBRARY_PATH for libkrun assets"
    );
    Ok(())
}

fn resolve_libkrunfw_path(
    spec: &VmInstanceSpec,
    libkrun_library: Option<&Path>,
) -> Result<Option<PathBuf>> {
    if spec.kernel_path.is_some() {
        return Ok(find_libkrunfw_path(libkrun_library));
    }
    find_libkrunfw_path(libkrun_library).map(Some).ok_or_else(|| {
        anyhow!(
            "libkrun boot requires either RKFORGE_SANDBOX_KERNEL or a discoverable libkrunfw asset in the sandbox runtime bundle or standard library locations"
        )
    })
}

// First will try to get library path by RuntimeAssetBundle.
// If not, try the Env variable again
fn find_libkrun_library() -> Option<PathBuf> {
    if let Ok(Some(bundle)) = RuntimeAssetBundle::discover_from_current_process()
        && let Some(path) = bundle.libkrun_library()
    {
        return Some(path.to_path_buf());
    }
    discover_libkrun_library()
}

fn find_libkrunfw_path(libkrun_library: Option<&Path>) -> Option<PathBuf> {
    if let Ok(Some(bundle)) = RuntimeAssetBundle::discover_from_current_process()
        && let Some(path) = bundle.libkrunfw_path()
    {
        return Some(path.to_path_buf());
    }
    discover_libkrunfw_path(libkrun_library)
}

struct LibkrunSymbols {
    _library: Library,
    create_ctx: KrunCreateCtxFn,
    set_vm_config: KrunSetVmConfigFn,
    set_kernel: KrunSetKernelFn,
    set_workdir: KrunSetWorkdirFn,
    set_exec: KrunSetExecFn,
    add_disk: KrunAddDiskFn,
    set_root_disk_remount: KrunSetRootDiskRemountFn,
    add_vsock_port2: KrunAddVsockPort2Fn,
    start_enter: KrunStartEnterFn,
}

impl LibkrunSymbols {
    // Multi-path fallback to load the libkrun path
    unsafe fn load(explicit_path: Option<&Path>) -> Result<Self> {
        let mut attempts = Vec::new();

        if let Some(path) = explicit_path {
            attempts.push(path.as_os_str().to_os_string());
        }
        for name in ["libkrun.so", "libkrun.so.1"] {
            attempts.push(OsString::from(name));
        }

        let mut errors = Vec::new();
        for attempt in attempts {
            // Library:new which represents `dlopen`
            let library = unsafe { Library::new(&attempt) };
            match library {
                Ok(library) => return unsafe { Self::from_library(library) },
                Err(err) => errors.push(format!("{}: {err}", PathBuf::from(&attempt).display())),
            }
        }

        bail!(
            "failed to load libkrun shared library from the sandbox runtime bundle or standard library locations. attempts: {}",
            errors.join("; ")
        )
    }

    unsafe fn from_library(library: Library) -> Result<Self> {
        unsafe {
            Ok(Self {
                create_ctx: *library
                    .get(b"krun_create_ctx\0")
                    .context("failed to resolve krun_create_ctx")?,
                set_vm_config: *library
                    .get(b"krun_set_vm_config\0")
                    .context("failed to resolve krun_set_vm_config")?,
                set_kernel: *library
                    .get(b"krun_set_kernel\0")
                    .context("failed to resolve krun_set_kernel")?,
                set_workdir: *library
                    .get(b"krun_set_workdir\0")
                    .context("failed to resolve krun_set_workdir")?,
                set_exec: *library
                    .get(b"krun_set_exec\0")
                    .context("failed to resolve krun_set_exec")?,
                add_disk: *library
                    .get(b"krun_add_disk\0")
                    .context("failed to resolve krun_add_disk")?,
                set_root_disk_remount: *library
                    .get(b"krun_set_root_disk_remount\0")
                    .context("failed to resolve krun_set_root_disk_remount")?,
                add_vsock_port2: *library
                    .get(b"krun_add_vsock_port2\0")
                    .context("failed to resolve krun_add_vsock_port2")?,
                start_enter: *library
                    .get(b"krun_start_enter\0")
                    .context("failed to resolve krun_start_enter")?,
                _library: library,
            })
        }
    }
}
