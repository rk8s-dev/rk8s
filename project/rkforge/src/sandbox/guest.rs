use anyhow::{Context, Result, bail};
use clap::{Args, Parser};
use nix::mount::{MsFlags, mount};
use nix::unistd::execv;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use tracing::debug;

const GUEST_AGENT_BINARY_PATH: &str = "/usr/local/bin/rkforge-sandbox-agent";
const DEFAULT_AGENT_VSOCK_PORT: u32 = 26_950;
const DEFAULT_READY_VSOCK_PORT: u32 = 26_951;

#[derive(Debug, Args, Clone)]
pub struct SandboxGuestInitArgs {
    #[arg(long, default_value_t = DEFAULT_AGENT_VSOCK_PORT)]
    pub vsock_port: u32,
}

pub fn should_run_as_init() -> bool {
    if std::process::id() != 1 {
        return false;
    }

    let argv0 = std::env::args_os().next();
    let Some(argv0) = argv0 else {
        return false;
    };
    let name = Path::new(&argv0)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    matches!(name, "init" | "rkforge-init")
}

pub fn run_as_init() -> Result<()> {
    let port = std::env::var("RKFORGE_SANDBOX_AGENT_VSOCK_PORT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_AGENT_VSOCK_PORT);
    debug!(vsock_port = port, "running rkforge as guest init");
    run_guest_init(SandboxGuestInitArgs { vsock_port: port })
}

pub fn run_command(args: SandboxGuestInitArgs) -> Result<()> {
    run_guest_init(args)
}

#[derive(Parser)]
struct SandboxGuestInitBinaryCli {
    #[command(flatten)]
    args: SandboxGuestInitArgs,
}

pub fn run_binary() -> Result<()> {
    run_command(SandboxGuestInitBinaryCli::parse().args)
}

fn run_guest_init(args: SandboxGuestInitArgs) -> Result<()> {
    debug!(vsock_port = args.vsock_port, "starting guest init");
    prepare_guest_filesystem()?;
    exec_agent(args.vsock_port)
}

fn prepare_guest_filesystem() -> Result<()> {
    debug!("preparing guest filesystem mounts");
    ensure_dir("/proc")?;
    ensure_dir("/sys")?;
    ensure_dir("/dev")?;
    ensure_dir("/run")?;
    ensure_dir("/tmp")?;

    mount_fs(
        Some("proc"),
        "/proc",
        Some("proc"),
        MsFlags::empty(),
        None::<&str>,
    )?;
    mount_fs(
        Some("sysfs"),
        "/sys",
        Some("sysfs"),
        MsFlags::empty(),
        None::<&str>,
    )?;
    mount_fs(
        Some("devtmpfs"),
        "/dev",
        Some("devtmpfs"),
        MsFlags::empty(),
        Some("mode=0755"),
    )?;

    Ok(())
}

fn ensure_dir(path: &str) -> Result<()> {
    fs::create_dir_all(path).with_context(|| format!("failed to create {}", path))
}

fn mount_fs(
    source: Option<&str>,
    target: &str,
    fstype: Option<&str>,
    flags: MsFlags,
    data: Option<&str>,
) -> Result<()> {
    debug!(source=?source, target, fstype=?fstype, data=?data, "mounting guest filesystem");
    match mount(source, target, fstype, flags, data) {
        Ok(()) => Ok(()),
        Err(nix::errno::Errno::EBUSY) => Ok(()),
        Err(err) => Err(err).with_context(|| format!("failed to mount {}", target)),
    }
}

fn exec_agent(vsock_port: u32) -> Result<()> {
    let ready_vsock_port = std::env::var("RKFORGE_SANDBOX_READY_VSOCK_PORT")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(DEFAULT_READY_VSOCK_PORT);
    let sandbox_id =
        read_kernel_cmdline_value("rkforge.sandbox_id").unwrap_or_else(|| "unknown".to_string());
    let current_exe =
        std::env::current_exe().context("failed to resolve current executable inside guest")?;
    let current_exe = canonical_agent_binary_path(current_exe);
    debug!(
        sandbox_id=%sandbox_id,
        vsock_port,
        ready_vsock_port,
        current_exe=%current_exe.display(),
        "execing sandbox agent helper from guest init"
    );
    let current_exe = CString::new(current_exe.as_os_str().as_bytes())
        .context("guest init path contains interior NUL")?;
    let arg0 = CString::new("rkforge-sandbox-agent").unwrap();
    let vsock_port_flag = CString::new("--vsock-port").unwrap();
    let vsock_port = CString::new(vsock_port.to_string()).unwrap();
    let ready_port_flag = CString::new("--ready-vsock-port").unwrap();
    let ready_port = CString::new(ready_vsock_port.to_string()).unwrap();
    let sandbox_id_flag = CString::new("--sandbox-id").unwrap();
    let sandbox_id = CString::new(sandbox_id).context("sandbox id contains interior NUL")?;
    let argv = [
        &arg0,
        &vsock_port_flag,
        &vsock_port,
        &ready_port_flag,
        &ready_port,
        &sandbox_id_flag,
        &sandbox_id,
    ];
    execv(&current_exe, &argv).context("failed to exec sandbox agent helper from guest init")?;
    bail!("execv returned unexpectedly")
}

fn canonical_agent_binary_path(current_exe: PathBuf) -> PathBuf {
    let preferred = PathBuf::from(GUEST_AGENT_BINARY_PATH);
    if preferred.is_file() {
        preferred
    } else {
        current_exe
    }
}

fn read_kernel_cmdline_value(key: &str) -> Option<String> {
    let cmdline = fs::read_to_string("/proc/cmdline").ok()?;
    for token in cmdline.split_whitespace() {
        let (found_key, value) = token.split_once('=')?;
        if found_key == key {
            return Some(value.to_string());
        }
    }
    None
}
