use crate::image::build_runtime::{BuildHostEntry, BuildNetworkMode, BuildUlimit};
use crate::overlayfs::MountConfig;
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose};
use ipc_channel::ipc::{IpcOneShotServer, IpcSender};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
};
use tracing::trace;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RunTask {
    pub commands: Vec<String>,
    pub envp: Vec<String>,
    /// Working directory for command execution. If None, defaults to "/".
    pub working_dir: Option<String>,
    /// User to run the command as. Format: "user", "uid", "user:group", "uid:gid", etc.
    /// If None, runs as root.
    pub user: Option<String>,
    /// Suppress stdout while keeping stderr visible.
    pub quiet: bool,
    /// Additional host mappings injected into build-time /etc/hosts.
    pub add_hosts: Vec<BuildHostEntry>,
    /// Optional /dev/shm size in bytes for build-time RUN containers.
    pub shm_size: Option<u64>,
    /// Optional ulimit settings applied before command exec.
    pub ulimits: Vec<BuildUlimit>,
    /// Network mode for build-time RUN containers.
    pub network_mode: BuildNetworkMode,
    /// Optional cgroup parent for build-time RUN containers.
    pub cgroup_parent: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CopyTask {
    pub src: Vec<PathBuf>,
    pub dest: PathBuf,
    /// Suppress stdout while keeping stderr visible.
    pub quiet: bool,
}

/// A trait for tasks to be executed during building an image.
pub trait TaskExec {
    fn need_cleanup(&self) -> bool {
        false
    }

    fn run(&self, session: &mut MountSession) -> Result<()>;

    fn execute(&self, cfg: &mut MountConfig) -> Result<()> {
        cfg.prepare()?;

        let mut session = MountSession::start(cfg).context("Failed to start mount session")?;

        let result = self.run(&mut session);

        if self.need_cleanup()
            && let Err(e) = session.cleanup()
        {
            tracing::error!("Cleanup failed: {e:?}");
        }

        drop(session);

        if let Err(e) = result {
            cfg.finish()?;
            return Err(e);
        }
        cfg.finish()?;
        Ok(())
    }
}

impl TaskExec for RunTask {
    fn need_cleanup(&self) -> bool {
        true
    }

    fn run(&self, session: &mut MountSession) -> Result<()> {
        let mut command = session.create_command("exec-internal")?;

        let commands_json = serde_json::to_string(&self.commands)
            .context("Failed to serialize commands to json")?;
        let commands_base64 = general_purpose::STANDARD.encode(commands_json);
        let envp_json =
            serde_json::to_string(&self.envp).context("Failed to serialize envp to json")?;
        let envp_base64 = general_purpose::STANDARD.encode(envp_json);
        let add_hosts_base64 = if self.add_hosts.is_empty() {
            None
        } else {
            let add_hosts_json = serde_json::to_string(&self.add_hosts)
                .context("Failed to serialize add-host mappings to json")?;
            Some(general_purpose::STANDARD.encode(add_hosts_json))
        };
        let ulimits_base64 = if self.ulimits.is_empty() {
            None
        } else {
            let ulimits_json = serde_json::to_string(&self.ulimits)
                .context("Failed to serialize ulimits to json")?;
            Some(general_purpose::STANDARD.encode(ulimits_json))
        };

        trace!(
            "Run commands: {:?}, envp: {:?}, working_dir: {:?}, user: {:?}, add_hosts: {:?}, shm_size: {:?}, ulimits: {:?}",
            self.commands,
            self.envp,
            self.working_dir,
            self.user,
            self.add_hosts,
            self.shm_size,
            self.ulimits
        );
        command
            .arg("--mountpoint")
            .arg(&session.mountpoint)
            .arg("--envp-base64")
            .arg(&envp_base64)
            .arg("--network")
            .arg(self.network_mode.as_cli_value())
            .arg("--commands-base64")
            .arg(commands_base64);

        if let Some(add_hosts_base64) = &add_hosts_base64 {
            command.arg("--add-hosts-base64").arg(add_hosts_base64);
        }
        if let Some(ulimits_base64) = &ulimits_base64 {
            command.arg("--ulimits-base64").arg(ulimits_base64);
        }

        // Add working directory if specified
        if let Some(ref working_dir) = self.working_dir {
            command.arg("--working-dir").arg(working_dir);
        }

        // Add user if specified
        if let Some(ref user) = self.user {
            command.arg("--user").arg(user);
        }

        if let Some(shm_size) = self.shm_size {
            command.arg("--shm-size").arg(shm_size.to_string());
        }
        if let Some(cgroup_parent) = &self.cgroup_parent {
            command.arg("--cgroup-parent").arg(cgroup_parent);
        }

        if self.quiet {
            command.stdout(Stdio::null());
        }

        let status = command.status().context("Failed to run run command")?;
        if !status.success() {
            anyhow::bail!("Run command exited with status: {status}");
        }
        Ok(())
    }
}

impl TaskExec for CopyTask {
    fn run(&self, session: &mut MountSession) -> Result<()> {
        let mut command = session.create_command("copy")?;
        command.arg("--dest").arg(&self.dest);
        for src in &self.src {
            command.arg("--src").arg(src);
        }
        trace!("Running command: {:?}", command);
        if self.quiet {
            command.stdout(Stdio::null());
        }

        let status = command.status().context("Failed to run copy command")?;
        if !status.success() {
            anyhow::bail!("Copy command exited with status: {status}");
        }
        Ok(())
    }
}

/// A session that manages the mount process and allows running tasks processes within the mount.
pub(crate) struct MountSession {
    tx: IpcSender<String>,
    mount_process: Child,
    mount_pid: u32,
    mountpoint: PathBuf,
}

impl MountSession {
    pub fn start(cfg: &MountConfig) -> Result<Self> {
        let cfg_json =
            serde_json::to_string(&cfg).context("Failed to serialize mount config to json")?;
        let cfg_base64 = general_purpose::STANDARD.encode(cfg_json);

        let (parent_server, parent_server_name) = IpcOneShotServer::new()?;
        let (child_server, child_server_name) = IpcOneShotServer::<String>::new()?;
        let mut mount_command = Command::new(std::env::current_exe()?);
        if cfg.libfuse {
            mount_command
                .arg("mount")
                .arg("--config-base64")
                .arg(&cfg_base64)
                .arg("--libfuse")
                .env("PARENT_SERVER_NAME", &parent_server_name)
                .env("CHILD_SERVER_NAME", &child_server_name);
        } else {
            mount_command
                .arg("mount")
                .arg("--config-base64")
                .arg(&cfg_base64)
                .env("PARENT_SERVER_NAME", &parent_server_name)
                .env("CHILD_SERVER_NAME", &child_server_name);
        }

        let mount_process = mount_command
            .spawn()
            .context("Failed to spawn mount process")?;

        let (_, tx): (_, IpcSender<String>) = parent_server
            .accept()
            .context("Failed to accept connection on parent server")?;
        let (_, msg) = child_server
            .accept()
            .context("Failed to accept connection on child server")?;
        if msg != "ready" {
            anyhow::bail!("Unexpected message from mount process: {msg}");
        }

        let mount_pid = mount_process.id();

        Ok(MountSession {
            tx,
            mount_process,
            mount_pid,
            mountpoint: cfg.mountpoint.clone(),
        })
    }

    pub fn create_command(&self, sub_command: &str) -> Result<Command> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg(sub_command)
            .env("MOUNT_PID", self.mount_pid.to_string());
        Ok(command)
    }

    pub fn cleanup(&self) -> Result<()> {
        let mut cleanup_command = Command::new(std::env::current_exe()?);
        cleanup_command
            .arg("cleanup")
            .arg("--mountpoint")
            .arg(&self.mountpoint)
            .env("MOUNT_PID", self.mount_pid.to_string());
        let cleanup_status = cleanup_command
            .status()
            .context("Failed to run cleanup command")?;
        if !cleanup_status.success() {
            tracing::error!("Cleanup command exited with status: {cleanup_status}");
        }
        Ok(())
    }
}

impl Drop for MountSession {
    fn drop(&mut self) {
        if let Err(e) = self.tx.send("exit".to_string()) {
            tracing::error!("Failed to send exit message to mount process: {e}");
        }
        if let Err(e) = self.mount_process.wait() {
            tracing::error!("Failed to wait for mount process: {e}");
        }
    }
}
