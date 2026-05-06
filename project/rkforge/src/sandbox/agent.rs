use crate::sandbox::protocol::{GuestExecRequest, GuestExecResponse, GuestReadyEvent, ReadyStage};
use crate::sandbox::{read_message, write_message};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::{Args, Parser};
use std::io::{Read, Write};
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use tracing::{debug, warn};

#[derive(Debug, Args, Clone)]
pub struct SandboxAgentArgs {
    #[arg(long)]
    pub vsock_port: u32,
    #[arg(long)]
    pub ready_vsock_port: Option<u32>,
    #[arg(long)]
    pub sandbox_id: Option<String>,
}

pub fn run_command(args: SandboxAgentArgs) -> Result<()> {
    run_vsock_agent(args)
}

#[derive(Parser)]
struct SandboxAgentBinaryCli {
    #[command(flatten)]
    args: SandboxAgentArgs,
}

pub fn run_binary() -> Result<()> {
    run_command(SandboxAgentBinaryCli::parse().args)
}

fn run_vsock_agent(args: SandboxAgentArgs) -> Result<()> {
    debug!(
        sandbox_id=args.sandbox_id.as_deref().unwrap_or("unknown"),
        vsock_port=args.vsock_port,
        ready_vsock_port=?args.ready_vsock_port,
        "starting sandbox guest agent"
    );
    let listener = VsockListener::bind(args.vsock_port)?;
    if let Some(ready_vsock_port) = args.ready_vsock_port {
        notify_host_ready(
            args.sandbox_id.as_deref().unwrap_or("unknown"),
            args.vsock_port,
            ready_vsock_port,
        )?;
    }

    loop {
        let mut stream = listener.accept()?;
        debug!("accepted guest agent client connection");
        if let Err(err) = handle_client(&mut stream) {
            warn!(error=%err, "sandbox agent failed to handle client");
            let response = GuestExecResponse {
                request_id: String::new(),
                stdout: String::new(),
                stderr: format!("sandbox agent error: {err:#}"),
                exit_code: 1,
            };
            let _ = write_message(&mut stream, &response);
        }
    }
}

fn notify_host_ready(sandbox_id: &str, agent_vsock_port: u32, ready_vsock_port: u32) -> Result<()> {
    debug!(
        sandbox_id,
        agent_vsock_port, ready_vsock_port, "notifying host that guest agent is ready"
    );
    let mut stream = VsockStream::connect(libc::VMADDR_CID_HOST, ready_vsock_port)?;
    let event = GuestReadyEvent {
        sandbox_id: sandbox_id.to_string(),
        stage: ReadyStage::GuestAgentReady,
        agent_version: "rkforge-sandbox-agent".to_string(),
        transport: format!("vsock://{}", agent_vsock_port),
        timestamp: Utc::now(),
    };
    write_message(&mut stream, &event)?;
    stream.flush()?;
    Ok(())
}

fn handle_client(stream: &mut VsockStream) -> Result<()> {
    let request: GuestExecRequest = read_message(stream)?;
    debug!(
        sandbox_id=%request.sandbox_id,
        request_id=%request.request_id,
        command=%request.command,
        args=?request.args,
        timeout_secs=?request.timeout_secs,
        inline_code=request.inline_code.is_some(),
        "sandbox agent received exec request"
    );
    let response = execute_request(request);
    write_message(stream, &response)?;
    stream.flush()?;
    Ok(())
}

fn execute_request(request: GuestExecRequest) -> GuestExecResponse {
    match execute_request_inner(&request) {
        Ok(response) => response,
        Err(err) => GuestExecResponse {
            request_id: request.request_id.clone(),
            stdout: String::new(),
            stderr: err.to_string(),
            exit_code: 1,
        },
    }
}

fn execute_request_inner(request: &GuestExecRequest) -> Result<GuestExecResponse> {
    let executable = if request.inline_code.is_some() {
        let language = request.language.as_deref().unwrap_or("python");
        match language {
            "python" => resolve_python_executable().to_string(),
            other => bail!("unsupported inline language `{other}`"),
        }
    } else {
        request.command.clone()
    };
    let mut command = Command::new(&executable);
    if let Some(code) = request.inline_code.as_ref() {
        command.arg("-c").arg(code);
    } else {
        command.args(&request.args);
    }

    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .with_context(|| format!("failed to spawn command `{executable}`"))?;
    debug!(
        sandbox_id=%request.sandbox_id,
        request_id=%request.request_id,
        command=%executable,
        pid=?child.id(),
        "spawned guest command"
    );

    let status = if let Some(timeout_secs) = request.timeout_secs {
        wait_with_timeout(&mut child, Duration::from_secs(timeout_secs))?
    } else {
        child.wait()?
    };
    let output = child
        .wait_with_output()
        .context("failed to collect command output")?;

    Ok(GuestExecResponse {
        request_id: request.request_id.clone(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: exit_code(&status),
    })
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    timeout: Duration,
) -> Result<std::process::ExitStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            warn!(timeout_secs=timeout.as_secs(), pid=?child.id(), "guest command timed out");
            let _ = child.kill();
            let status = child.wait()?;
            return Ok(status);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn exit_code(status: &std::process::ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or_default())
}

fn resolve_python_executable() -> &'static str {
    const CANDIDATES: [&str; 4] = [
        "/usr/local/bin/python3",
        "/usr/bin/python3",
        "/bin/python3",
        "python3",
    ];

    for candidate in CANDIDATES {
        if candidate == "python3" || Path::new(candidate).is_file() {
            return candidate;
        }
    }

    "python3"
}

struct VsockListener {
    fd: OwnedFd,
}

impl VsockListener {
    fn bind(port: u32) -> Result<Self> {
        let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("failed to create vsock socket");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let sockaddr = libc::sockaddr_vm {
            svm_family: libc::AF_VSOCK as libc::sa_family_t,
            svm_reserved1: 0,
            svm_port: port,
            svm_cid: libc::VMADDR_CID_ANY,
            svm_zero: [0; 4],
        };

        let rc = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                &sockaddr as *const libc::sockaddr_vm as *const libc::sockaddr,
                size_of::<libc::sockaddr_vm>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("failed to bind vsock port {}", port));
        }

        let rc = unsafe { libc::listen(fd.as_raw_fd(), 16) };
        if rc < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("failed to listen on vsock port {}", port));
        }

        debug!(port, "sandbox agent bound vsock listener");
        Ok(Self { fd })
    }

    fn accept(&self) -> Result<VsockStream> {
        let fd = unsafe {
            libc::accept(
                self.fd.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error())
                .context("failed to accept vsock connection");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        Ok(VsockStream { fd })
    }
}

struct VsockStream {
    fd: OwnedFd,
}

impl VsockStream {
    fn connect(cid: u32, port: u32) -> Result<Self> {
        let fd = unsafe { libc::socket(libc::AF_VSOCK, libc::SOCK_STREAM, 0) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error()).context("failed to create vsock socket");
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let sockaddr = libc::sockaddr_vm {
            svm_family: libc::AF_VSOCK as libc::sa_family_t,
            svm_reserved1: 0,
            svm_port: port,
            svm_cid: cid,
            svm_zero: [0; 4],
        };

        let rc = unsafe {
            libc::connect(
                fd.as_raw_fd(),
                &sockaddr as *const libc::sockaddr_vm as *const libc::sockaddr,
                size_of::<libc::sockaddr_vm>() as libc::socklen_t,
            )
        };
        if rc < 0 {
            return Err(std::io::Error::last_os_error())
                .with_context(|| format!("failed to connect to vsock {}:{}", cid, port));
        }

        debug!(cid, port, "connected vsock stream");
        Ok(Self { fd })
    }
}

impl Read for VsockStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let rc = unsafe { libc::read(self.fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
        if rc < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }
}

impl Write for VsockStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let rc = unsafe { libc::write(self.fd.as_raw_fd(), buf.as_ptr().cast(), buf.len()) };
        if rc < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
