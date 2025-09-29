use clap::Parser;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{Context, Result, anyhow, bail};
use libcontainer::container::Container;
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::container::state;
use libcontainer::syscall::syscall::SyscallType;
use liboci_cli::Create;
use liboci_cli::Delete;
use liboci_cli::Kill;
use liboci_cli::{List, Start, State};
use nix::sys::wait::WaitStatus;
use nix::sys::wait::waitpid;
use std::error::Error;

use std::fmt::Write as _;
use std::io::Write;

use libcontainer::container::ContainerStatus;
use libcontainer::signal::Signal;
use std::convert::TryInto;

use chrono::{DateTime, Local};
use tabwriter::TabWriter;

pub mod compose;
pub mod container;
pub mod pod;
pub mod utils;

fn construct_container_root<P: AsRef<Path>>(root_path: P, container_id: &str) -> Result<PathBuf> {
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

fn container_exists<P: AsRef<Path>>(root_path: P, container_id: &str) -> Result<bool> {
    let container_root = construct_container_root(root_path.as_ref(), container_id)?;
    Ok(container_root.exists())
}

pub fn start(args: Start, root_path: PathBuf) -> Result<()> {
    let mut container = load_container(root_path, &args.container_id)?;
    container
        .start()
        .map_err(|e| anyhow!("failed to start container {}, {}", args.container_id, e))
}

//
pub fn state(args: State, root_path: PathBuf) -> Result<()> {
    let container = load_container(root_path, &args.container_id)?;
    println!("{}", serde_json::to_string_pretty(&container.state)?);
    //std::process::exit(0);
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
            "".to_owned()
        };

        let user_name = container.creator().unwrap_or_default();

        let created = if let Some(utc) = container.created() {
            let local: DateTime<Local> = DateTime::from(utc);
            local.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
        } else {
            "".to_owned()
        };

        let _ = writeln!(
            content,
            "{}\t{}\t{}\t{}\t{}\t{}",
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

// One thing to note is that in the end, container is just another process in Linux
// it has specific/different control group, namespace, using which program executing in it
// can be given impression that is is running on a complete system, but on the system which
// it is running, it is just another process, and has attributes such as pid, file descriptors, etc.
// associated with it like any other process.
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

    // See https://github.com/containers/youki/pull/1252 for a detailed explanation
    // basically, if there is any error in starting exec, the build above will return error
    // however, if the process does start, and detach is given, we do not wait for it
    // if not detached, then we wait for it using waitpid below
    if args.detach {
        return Ok(0);
    }

    match waitpid(pid, None)? {
        WaitStatus::Exited(_, status) => Ok(status),
        WaitStatus::Signaled(_, sig, _) => Ok(sig as i32),
        _ => Ok(0),
    }
}

#[allow(dead_code)]
/// exec cmd implements
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

/// Execute a process within an existing container
/// Reference: https://github.com/opencontainers/runc/blob/main/man/runc-exec.8.md
#[derive(Parser, Debug, Clone)]
pub struct ExecBase {
    // /// Pod name
    // #[arg(value_name = "POD_NAME")]
    // #[clap(required = true)]
    // pub pod_name: String,
    /// Unix socket (file) path , which will receive file descriptor of the writing end of the pseudoterminal
    #[clap(long)]
    pub console_socket: Option<PathBuf>,
    #[clap(long)]
    /// Current working directory of the container
    pub cwd: Option<PathBuf>,
    /// Environment variables that should be set in the container
    #[clap(short, long, value_parser = parse_env::<String, String>, number_of_values = 1)]
    pub env: Vec<(String, String)>,
    #[clap(short, long)]
    pub tty: bool,
    /// Run the command as a user
    #[clap(short, long, value_parser = parse_user::<u32, u32>)]
    pub user: Option<(u32, Option<u32>)>,
    /// Add additional group IDs. Can be specified multiple times
    #[clap(long, short = 'g', number_of_values = 1)]
    pub additional_gids: Vec<u32>,
    /// Path to process.json
    #[clap(short, long)]
    pub process: Option<PathBuf>,
    /// Detach from the container process
    #[clap(short, long)]
    pub detach: bool,
    #[clap(long)]
    /// The file to which the pid of the container process should be written to
    pub pid_file: Option<PathBuf>,
    /// Set the asm process label for the process commonly used with selinux
    #[clap(long)]
    pub process_label: Option<String>,
    /// Set the apparmor profile for the process
    #[clap(long)]
    pub apparmor: Option<String>,
    /// Prevent the process from gaining additional privileges
    #[clap(long)]
    pub no_new_privs: bool,
    /// Add a capability to the bounding set for the process
    #[clap(long, number_of_values = 1)]
    pub cap: Vec<String>,
    /// Pass N additional file descriptors to the container
    #[clap(long, default_value = "0")]
    pub preserve_fds: i32,
    /// Allow exec in a paused container
    #[clap(long)]
    pub ignore_paused: bool,
    /// Execute a process in a sub-cgroup
    #[clap(long)]
    pub cgroup: Option<String>,
    // /// Identifier of the container
    // #[clap(value_parser = clap::builder::NonEmptyStringValueParser::new(), required = true)]
    // pub container_id: String,

    // /// Command that should be executed in the container
    // #[clap(required = false)]
    // pub command: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct ExecContainer {
    #[arg(value_name = "CONTAINER_ID")]
    #[clap(value_parser = clap::builder::NonEmptyStringValueParser::new(), required = true)]
    pub container_id: String,

    /// Command that should be executed in the container
    #[clap(required = false)]
    pub command: Vec<String>,

    #[clap(long, required = false)]
    pub root_path: Option<String>,

    #[clap(flatten)]
    pub base: ExecBase,
}

#[derive(Parser, Debug, Clone)]
pub struct ExecPod {
    /// Pod name
    #[arg(value_name = "POD_NAME")]
    #[clap(required = true)]
    pub pod_name: String,

    #[arg(value_name = "CONTAINER_ID")]
    #[clap(value_parser = clap::builder::NonEmptyStringValueParser::new(), required = true)]
    pub container_id: String,

    /// Command that should be executed in the container
    #[clap(required = false)]
    pub command: Vec<String>,

    #[clap(flatten)]
    pub base: ExecBase,
}

// Support both pod and container
impl From<ExecPod> for Exec {
    fn from(cli: ExecPod) -> Self {
        Exec {
            pod_name: Some(cli.pod_name),
            container_id: cli.container_id,
            command: cli.command,
            console_socket: cli.base.console_socket,
            cwd: cli.base.cwd,
            env: cli.base.env,
            tty: cli.base.tty,
            user: cli.base.user,
            additional_gids: cli.base.additional_gids,
            process: cli.base.process,
            detach: cli.base.detach,
            pid_file: cli.base.pid_file,
            process_label: cli.base.process_label,
            apparmor: cli.base.apparmor,
            no_new_privs: cli.base.no_new_privs,
            cap: cli.base.cap,
            preserve_fds: cli.base.preserve_fds,
            ignore_paused: cli.base.ignore_paused,
            cgroup: cli.base.cgroup,
        }
    }
}

impl From<ExecContainer> for Exec {
    fn from(cli: ExecContainer) -> Self {
        Exec {
            pod_name: None,
            container_id: cli.container_id,
            command: cli.command,
            console_socket: cli.base.console_socket,
            cwd: cli.base.cwd,
            env: cli.base.env,
            tty: cli.base.tty,
            user: cli.base.user,
            additional_gids: cli.base.additional_gids,
            process: cli.base.process,
            detach: cli.base.detach,
            pid_file: cli.base.pid_file,
            process_label: cli.base.process_label,
            apparmor: cli.base.apparmor,
            no_new_privs: cli.base.no_new_privs,
            cap: cli.base.cap,
            preserve_fds: cli.base.preserve_fds,
            ignore_paused: cli.base.ignore_paused,
            cgroup: cli.base.cgroup,
        }
    }
}

fn parse_env<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
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

fn parse_user<T, U>(s: &str) -> Result<(T, Option<U>), Box<dyn Error + Send + Sync + 'static>>
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
