use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Local};
use libcontainer::container::builder::ContainerBuilder;
use libcontainer::container::{Container, ContainerStatus, state};
use libcontainer::signal::Signal;
use libcontainer::syscall::syscall::SyscallType;
use liboci_cli::{Create, Delete, Kill, List, Start, State};
use std::fmt::Write as _;
use std::fs::{self};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tabwriter::TabWriter;
use tracing::info;

pub mod config;
pub mod container;
pub mod cri_api;

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
    info!("{}", serde_json::to_string_pretty(&container.state)?);
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

// pub fn exec(args: Exec, root_path: PathBuf) -> Result<i32> {
//     let pid = ContainerBuilder::new(args.container_id.clone(), SyscallType::default())
//         .with_executor(libcontainer::workload::default::DefaultExecutor {})
//         .with_root_path(root_path)?
//         .with_console_socket(args.console_socket.as_ref())
//         .with_pid_file(args.pid_file.as_ref())?
//         .validate_id()?
//         .as_tenant()
//         .with_detach(args.detach)
//         .with_cwd(args.cwd.as_ref())
//         .with_env(args.env.clone().into_iter().collect())
//         .with_process(args.process.as_ref())
//         .with_no_new_privs(args.no_new_privs)
//         .with_container_args(args.command.clone())
//         .build()?;

//     // See https://github.com/containers/youki/pull/1252 for a detailed explanation
//     // basically, if there is any error in starting exec, the build above will return error
//     // however, if the process does start, and detach is given, we do not wait for it
//     // if not detached, then we wait for it using waitpid below
//     if args.detach {
//         return Ok(0);
//     }

//     match waitpid(pid, None)? {
//         WaitStatus::Exited(_, status) => Ok(status),
//         WaitStatus::Signaled(_, sig, _) => Ok(sig as i32),
//         _ => Ok(0),
//     }
// }
