use anyhow::{Result, anyhow};
use clap::Args;
use tracing::debug;

use super::pod::PodInfo;
use super::{Exec, ExecBase, exec};
use libcontainer::syscall::syscall::create_syscall;
use libruntime::rootpath;
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
#[command(override_usage = "\
rkl exec [OPTIONS] <TARGET> [-c <CONTAINER_NAME>] -- [COMMAND...]

TARGET can be a pod name or a container ID (auto-detected)")]
pub struct ExecCommand {
    /// Pod name or container ID
    #[arg(value_name = "TARGET")]
    pub target: String,

    #[arg(long, short = 'c', value_name = "CONTAINER_NAME")]
    pub container: Option<String>,

    #[clap(long)]
    pub root_path: Option<String>,

    #[clap(required = false)]
    pub command: Vec<String>,

    #[clap(flatten)]
    pub base: ExecBase,
}

pub fn exec_execute(cmd: ExecCommand) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(
        cmd.root_path.as_ref().map(PathBuf::from),
        &*create_syscall(),
    )?;

    // Auto-detect whether target is a pod_name or a container_id:
    //   - If root_path/pods/<target> exists as a file  → it's a pod_name
    //   - If root_path/<target>/state.json exists      → it's a container_id
    //   - If both exist                                → treat as pod_name (pods take priority)
    //   - If neither exists                            → report not found
    let pod_info_path = root_path.join("pods").join(&cmd.target);
    let container_state_path = root_path.join(&cmd.target).join("state.json");

    let is_pod = pod_info_path.exists();
    let is_container = container_state_path.exists();

    debug!(
        "exec target='{}' is_pod={} is_container={}",
        cmd.target, is_pod, is_container
    );

    let (container_id, pod_name) = if is_pod {
        // Target is a pod name
        let pod_info = PodInfo::load(&root_path, &cmd.target)
            .map_err(|_| anyhow!("Pod '{}' not found", cmd.target))?;

        let cid = if let Some(ref container_name) = cmd.container {
            // pod + explicit container name → find matching container
            pod_info
                .container_names
                .iter()
                .find(|c| *c == container_name)
                .cloned()
                .ok_or_else(|| {
                    anyhow!(
                        "Container '{}' not found in pod '{}'",
                        container_name,
                        cmd.target
                    )
                })?
        } else {
            // pod only → use the first container
            pod_info
                .container_names
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("Pod '{}' has no containers", cmd.target))?
        };

        (cid, Some(cmd.target.clone()))
    } else if is_container {
        // Target is a container ID
        if cmd.container.is_some() {
            return Err(anyhow!(
                "'-c' is only valid when TARGET is a pod name, but '{}' is a container ID",
                cmd.target
            ));
        }
        (cmd.target.clone(), None)
    } else {
        return Err(anyhow!(
            "'{}' not found as a pod name or container ID",
            cmd.target
        ));
    };

    let args = Exec {
        container_id,
        pod_name,
        command: cmd.command,
        console_socket: cmd.base.console_socket,
        cwd: cmd.base.cwd,
        env: cmd.base.env,
        tty: cmd.base.tty,
        user: cmd.base.user,
        additional_gids: cmd.base.additional_gids,
        process: cmd.base.process,
        detach: cmd.base.detach,
        pid_file: cmd.base.pid_file,
        process_label: cmd.base.process_label,
        apparmor: cmd.base.apparmor,
        no_new_privs: cmd.base.no_new_privs,
        cap: cmd.base.cap,
        preserve_fds: cmd.base.preserve_fds,
        ignore_paused: cmd.base.ignore_paused,
        cgroup: cmd.base.cgroup,
    };

    debug!(
        "exec command args:
{:?}",
        args
    );
    let exit_code = exec(args, root_path)?;
    std::process::exit(exit_code);
}
