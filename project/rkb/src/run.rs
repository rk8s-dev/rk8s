use crate::exec::Task;
use crate::overlayfs::MountConfig;
use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose};
use ipc_channel::ipc::{IpcOneShotServer, IpcSender};
use std::process::Command;

pub fn exec_task(mount_config: MountConfig, task: Task) -> Result<()> {
    let need_cleanup = matches!(&task, Task::Run { .. });

    let config_json =
        serde_json::to_string(&mount_config).context("Failed to serialize mount config to json")?;
    let config_base64 = general_purpose::STANDARD.encode(config_json);

    let (parent_server, parent_server_name) = IpcOneShotServer::new()?;
    let (child_server, child_server_name) = IpcOneShotServer::<String>::new()?;
    let mut mount_command = Command::new(std::env::current_exe()?);
    mount_command
        .arg("mount")
        .arg("--config-base64")
        .arg(&config_base64)
        .env("PARENT_SERVER_NAME", &parent_server_name)
        .env("CHILD_SERVER_NAME", &child_server_name);
    let mut mount_process = mount_command
        .spawn()
        .context("Failed to spawn mount process")?;

    let (_, tx1): (_, IpcSender<String>) = parent_server
        .accept()
        .context("Failed to accept connection on parent server")?;
    let (_, msg) = child_server
        .accept()
        .context("Failed to accept connection on child server")?;
    assert_eq!(msg, "ready");

    let mount_pid = mount_process.id();
    let task_json = serde_json::to_string(&task).context("Failed to serialize task to json")?;
    let task_base64 = general_purpose::STANDARD.encode(task_json);
    let mut exec_command = Command::new(std::env::current_exe()?);
    exec_command
        .arg("exec")
        .arg("--mountpoint")
        .arg(mount_config.mountpoint.to_string_lossy().into_owned())
        .arg("--task-base64")
        .arg(&task_base64)
        .env("MOUNT_PID", mount_pid.to_string());
    let exec_status = exec_command
        .status()
        .context("Failed to run exec process")?;

    if need_cleanup {
        let mut cleanup_command = Command::new(std::env::current_exe()?);
        cleanup_command
            .arg("cleanup")
            .arg("--mountpoint")
            .arg(mount_config.mountpoint.to_string_lossy().into_owned())
            .env("MOUNT_PID", mount_pid.to_string());
        let cleanup_status = cleanup_command
            .status()
            .context("Failed to run cleanup command")?;
        if !cleanup_status.success() {
            tracing::error!("Cleanup command exited with status: {cleanup_status}");
        }
    }

    tx1.send("exit".to_string())
        .context("Failed to send exit message to mount process")?;
    mount_process
        .wait()
        .context("Failed to wait for mount process")?;

    if !exec_status.success() {
        tracing::error!("Exec process exited with status: {exec_status}");
    }
    Ok(())
}
