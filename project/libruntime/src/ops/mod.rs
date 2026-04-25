use std::convert::TryInto;
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Local, Utc};
use libcontainer::container::Container;
use libcontainer::container::ContainerStatus;
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::container::state;
use libcontainer::signal::Signal;
use libcontainer::syscall::syscall::SyscallType;
use liboci_cli::{Create, Delete, Kill, List, Start, State};
use nix::sys::wait::{WaitStatus, waitpid};
use tabwriter::TabWriter;
use tracing::info;

pub fn construct_container_root<P: AsRef<Path>>(
    root_path: P,
    container_id: &str,
) -> Result<PathBuf> {
    // resolves relative paths, symbolic links etc. and get complete path
    let root_path = fs::canonicalize(&root_path).with_context(|| {
        format!(
            "failed to canonicalize {} for container {}",
            root_path.as_ref().display(),
            container_id
        )
    })?;
    // the state of the container is stored in a directory named after the container id
    Ok(root_path.join(container_id))
}

pub fn load_container<P: AsRef<Path>>(root_path: P, container_id: &str) -> Result<Container> {
    let container_root = construct_container_root(root_path.as_ref(), container_id)?;
    if !container_root.exists() {
        bail!("container {} does not exist.", container_id)
    }

    Container::load(container_root)
        .with_context(|| format!("could not load state for container {container_id}"))
}

pub fn container_exists<P: AsRef<Path>>(root_path: P, container_id: &str) -> Result<bool> {
    let container_root = construct_container_root(root_path.as_ref(), container_id)?;
    Ok(container_root.exists())
}

pub fn start(args: Start, root_path: PathBuf) -> Result<()> {
    let mut container = load_container(root_path, &args.container_id)?;
    container
        .start()
        .map_err(|e| anyhow!("failed to start container {}, {}", args.container_id, e))
}

pub fn state(args: State, root_path: PathBuf) -> Result<()> {
    let container = load_container(root_path, &args.container_id)?;
    info!("{}", serde_json::to_string_pretty(&container.state)?);
    Ok(())
}

/// lists all existing containers
pub fn list(_: List, root_path: PathBuf) -> Result<()> {
    let root_path = fs::canonicalize(root_path)?;
    let mut content = String::new();
    // all containers' data is stored in their respective dir in root directory
    // so we iterate through each and print the various info
    for container_dir in fs::read_dir(root_path)? {
        let container_dir = container_dir?.path();
        let state_file = state::State::file_path(&container_dir);
        if !state_file.exists() {
            continue;
        }

        let container = Container::load(container_dir)?;
        let pid = if let Some(pid) = container.pid() {
            pid.to_string()
        } else {
            "".to_string()
        };

        let user_name = container.creator().unwrap_or_default();

        let created = if let Some(utc) = container.created() {
            let local: DateTime<Local> = DateTime::from(utc);
            local.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        } else {
            "".to_string()
        };

        let _ = writeln!(
            content,
            "{}	{}	{}	{}	{}	{}",
            container.id(),
            pid,
            container.status(),
            container.bundle().display(),
            created,
            user_name.to_string_lossy()
        );
    }

    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "ID\tPID\tSTATUS\tBUNDLE\tCREATED\tCREATOR")?;
    write!(&mut tab_writer, "{content}")?;
    tab_writer.flush()?;

    Ok(())
}

pub fn kill(args: Kill, root_path: PathBuf) -> Result<()> {
    let mut container = load_container(root_path, &args.container_id)?;
    let signal: Signal = args.signal.as_str().try_into()?;
    match container.kill(signal, args.all) {
        Ok(_) => Ok(()),
        Err(e) => {
            // see https://github.com/containers/youki/issues/1314
            if container.status() == ContainerStatus::Stopped {
                return Err(anyhow!(e).context("container not running"));
            }
            Err(anyhow!(e).context("failed to kill container"))
        }
    }
}

pub fn create(args: Create, root_path: PathBuf, systemd_cgroup: bool) -> Result<()> {
    ContainerBuilder::new(args.container_id.clone(), SyscallType::default())
        .with_executor(libcontainer::workload::default::DefaultExecutor {})
        .with_pid_file(args.pid_file.as_ref())?
        .with_console_socket(args.console_socket.as_ref())
        .with_root_path(root_path)?
        .with_preserved_fds(args.preserve_fds)
        .validate_id()?
        .as_init(&args.bundle)
        .with_systemd(systemd_cgroup)
        .with_detach(true)
        .with_no_pivot(args.no_pivot)
        .build()?;

    Ok(())
}

/// Create a container and redirect its stdout/stderr to a CRI-format log file.
///
/// Two modes depending on `console_socket`:
///
/// ## Pipe mode (`console_socket = None`)
/// Spawns two background threads that drain stdout/stderr pipes and write
/// timestamped CRI-format lines to `log_path`:
///   `<RFC3339Nano> stdout F <message>\n`
///
/// ## PTY mode (`console_socket = Some(path)`)
/// The container is created with `process.terminal = true`. After `build()`,
/// the pty master fd is received from `console_socket` via SCM_RIGHTS.
/// The master fd (as `OwnedFd`) is returned so the caller can register it in a
/// TTY store and take responsibility for teeing it to logs / attach sessions.
///
/// Returns `Some(OwnedFd)` in PTY mode, `None` in pipe mode.
pub fn create_with_log(
    args: Create,
    root_path: PathBuf,
    log_path: PathBuf,
) -> Result<Option<std::os::fd::OwnedFd>> {
    use std::io::BufRead;
    use std::os::unix::io::{FromRawFd, IntoRawFd};

    // Ensure log directory exists
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent)?;
    }

    if let Some(sock_path) = args.console_socket.as_deref() {
        // PTY mode
        // 1. Create and bind the console socket so libcontainer can connect.
        use nix::sys::socket::{
            self as nix_sock, AddressFamily, Backlog, SockFlag, SockType, UnixAddr,
        };
        use std::os::fd::AsRawFd;

        if let Some(parent) = sock_path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Remove stale socket file if present
        let _ = fs::remove_file(sock_path);

        let sock_fd = nix_sock::socket(
            AddressFamily::Unix,
            SockType::Stream,
            SockFlag::empty(),
            None,
        )?;
        nix_sock::bind(sock_fd.as_raw_fd(), &UnixAddr::new(sock_path)?)?;
        nix_sock::listen(&sock_fd, Backlog::new(1)?)?;

        // 2. Build container with the console socket path.
        ContainerBuilder::new(args.container_id.clone(), SyscallType::default())
            .with_executor(libcontainer::workload::default::DefaultExecutor {})
            .with_pid_file(args.pid_file.as_ref())?
            .with_console_socket(Some(sock_path))
            .with_root_path(root_path)?
            .with_preserved_fds(args.preserve_fds)
            .validate_id()?
            .as_init(&args.bundle)
            .with_systemd(false)
            .with_detach(true)
            .with_no_pivot(args.no_pivot)
            .build()?;

        // 3. Accept the connection from libcontainer and receive the master fd.
        let conn_fd = nix_sock::accept(sock_fd.as_raw_fd())?;
        let mut dummy_buf = [0u8; 64];
        let mut iov = [std::io::IoSliceMut::new(&mut dummy_buf)];
        let mut cmsg_buf = nix::cmsg_space!([std::os::unix::io::RawFd; 1]);
        let msg = nix_sock::recvmsg::<UnixAddr>(
            conn_fd,
            &mut iov,
            Some(&mut cmsg_buf),
            nix::sys::socket::MsgFlags::empty(),
        )?;
        let master_raw: std::os::unix::io::RawFd = msg
            .cmsgs()?
            .filter_map(|cmsg| {
                if let nix::sys::socket::ControlMessageOwned::ScmRights(fds) = cmsg {
                    fds.into_iter().next()
                } else {
                    None
                }
            })
            .next()
            .ok_or_else(|| anyhow!("PTY mode: no master fd received from console socket"))?;

        // Clean up the socket file
        let _ = fs::remove_file(sock_path);

        let master_owned = unsafe { std::os::fd::OwnedFd::from_raw_fd(master_raw) };
        Ok(Some(master_owned))
    } else {
        // Pipe mode (original behaviour)
        use nix::unistd::pipe;

        let (stdout_r, stdout_w) = pipe()?;
        let (stderr_r, stderr_w) = pipe()?;

        let log_path_clone = log_path.clone();

        // Spawn background thread to drain stdout pipe -> log file
        std::thread::spawn(move || {
            let reader = unsafe { std::fs::File::from_raw_fd(stdout_r.into_raw_fd()) };
            let mut log_file = match fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path_clone)
            {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("failed to open log file {:?}: {e}", log_path_clone);
                    return;
                }
            };
            let buf = std::io::BufReader::new(reader);
            for line in buf.lines() {
                match line {
                    Ok(l) => {
                        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
                        let _ = writeln!(log_file, "{ts} stdout F {l}");
                    }
                    Err(_) => break,
                }
            }
        });

        // Spawn background thread to drain stderr pipe -> same log file
        std::thread::spawn(move || {
            let reader = unsafe { std::fs::File::from_raw_fd(stderr_r.into_raw_fd()) };
            let mut log_file = match fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
            {
                Ok(f) => f,
                Err(e) => {
                    tracing::error!("failed to open log file {:?}: {e}", log_path);
                    return;
                }
            };
            let buf = std::io::BufReader::new(reader);
            for line in buf.lines() {
                match line {
                    Ok(l) => {
                        let ts = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
                        let _ = writeln!(log_file, "{ts} stderr F {l}");
                    }
                    Err(_) => break,
                }
            }
        });

        ContainerBuilder::new(args.container_id.clone(), SyscallType::default())
            .with_executor(libcontainer::workload::default::DefaultExecutor {})
            .with_pid_file(args.pid_file.as_ref())?
            .with_console_socket(args.console_socket.as_ref())
            .with_root_path(root_path)?
            .with_preserved_fds(args.preserve_fds)
            .with_stdout(stdout_w)
            .with_stderr(stderr_w)
            .validate_id()?
            .as_init(&args.bundle)
            .with_systemd(false)
            .with_detach(true)
            .with_no_pivot(args.no_pivot)
            .build()?;

        Ok(None)
    }
}

pub fn delete(args: Delete, root_path: PathBuf) -> Result<()> {
    tracing::debug!("start deleting {}", args.container_id);
    if !container_exists(&root_path, &args.container_id)? && args.force {
        return Ok(());
    }

    let mut container = load_container(root_path, &args.container_id)?;
    container
        .delete(args.force)
        .with_context(|| format!("failed to delete container {}", args.container_id))
}

pub fn exec(args: Exec, root_path: PathBuf) -> Result<i32> {
    let pid = ContainerBuilder::new(args.container_id.clone(), SyscallType::default())
        .with_executor(libcontainer::workload::default::DefaultExecutor {})
        .with_root_path(root_path)?
        .with_console_socket(args.console_socket.as_ref())
        .with_pid_file(args.pid_file.as_ref())?
        .validate_id()?
        .as_tenant()
        .with_detach(args.detach)
        .with_cwd(args.cwd.as_ref())
        .with_env(args.env.clone().into_iter().collect())
        .with_process(args.process.as_ref())
        .with_no_new_privs(args.no_new_privs)
        .with_container_args(args.command.clone())
        .build()?;

    if args.detach {
        return Ok(0);
    }

    match waitpid(pid, None)? {
        WaitStatus::Exited(_, status) => Ok(status),
        WaitStatus::Signaled(_, sig, _) => Ok(sig as i32),
        _ => Ok(0),
    }
}

#[derive(Debug, Clone)]
pub struct Exec {
    pub pod_name: Option<String>,
    pub container_id: String,
    pub command: Vec<String>,
    pub console_socket: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub tty: bool,
    pub user: Option<(u32, Option<u32>)>,
    pub additional_gids: Vec<u32>,
    pub process: Option<PathBuf>,
    pub detach: bool,
    pub pid_file: Option<PathBuf>,
    pub process_label: Option<String>,
    pub apparmor: Option<String>,
    pub no_new_privs: bool,
    pub cap: Vec<String>,
    pub preserve_fds: i32,
    pub ignore_paused: bool,
    pub cgroup: Option<String>,
}

pub fn parse_env<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid VAR=value: no `=` found in `{s}`"))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}

pub fn parse_user<T, U>(s: &str) -> Result<(T, Option<U>), Box<dyn Error + Send + Sync + 'static>>
where
    T: FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    if let Some(pos) = s.find(':') {
        Ok((s[..pos].parse()?, Some(s[pos + 1..].parse()?)))
    } else {
        Ok((s.parse()?, None))
    }
}
