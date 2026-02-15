use crate::commands::pod::PodInfo;
use crate::commands::{Exec, ExecPod};
use crate::commands::{delete, exec, kill, load_container, start, state};
use crate::task::{self, TaskRunner};
use anyhow::{Result, anyhow};
use libcontainer::container::ContainerStatus;
use liboci_cli::{Delete, Kill, Start, State};
use libruntime::rootpath;
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use libcontainer::syscall::syscall::create_syscall;

/// Kills a container and waits for it to stop.
/// Returns Ok(()) if the container is stopped (either was already stopped or successfully killed).
fn kill_and_wait_container(root_path: &std::path::Path, container_name: &str) -> Result<()> {
    let container = match load_container(root_path, container_name) {
        Ok(c) => c,
        Err(e) => {
            debug!(
                "Container {} not found, skipping kill: {}",
                container_name, e
            );
            return Ok(());
        }
    };

    if container.status() == ContainerStatus::Stopped {
        debug!("Container {} already stopped", container_name);
        return Ok(());
    }

    let kill_args = Kill {
        container_id: container_name.to_string(),
        signal: "SIGKILL".to_string(),
        all: true,
    };

    if let Err(e) = kill(kill_args, root_path.to_path_buf()) {
        warn!("Failed to kill container {}: {}", container_name, e);
    }

    for i in 0..20 {
        thread::sleep(Duration::from_millis(500));
        let container = match load_container(root_path, container_name) {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };
        if container.status() == ContainerStatus::Stopped {
            debug!(
                "Container {} stopped after {}ms",
                container_name,
                (i + 1) * 500
            );
            return Ok(());
        }
    }

    warn!("Container {} did not stop within timeout", container_name);
    Ok(())
}

pub fn delete_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;
    let container = load_container(root_path.clone(), pod_name)
        .map_err(|e| anyhow!("Failed to load container {}: {}", pod_name, e))?;
    let pid_i32 = container
        .state
        .pid
        .ok_or_else(|| anyhow!("PID not found for container {}", pod_name))?;
    remove_pod_network(pid_i32)?;

    // First, kill all containers and wait for them to stop
    for container_name in &pod_info.container_names {
        if let Err(e) = kill_and_wait_container(&root_path, container_name) {
            warn!("Failed to kill container {}: {}", container_name, e);
        }
    }

    // Kill the pause container
    if let Err(e) = kill_and_wait_container(&root_path, &pod_info.pod_sandbox_id) {
        warn!(
            "Failed to kill pause container {}: {}",
            pod_info.pod_sandbox_id, e
        );
    }

    // Now delete all containers
    for container_name in &pod_info.container_names {
        let delete_args = Delete {
            container_id: container_name.clone(),
            force: true,
        };
        let root_path = rootpath::determine(None, &*create_syscall())?;
        if let Err(delete_err) = delete(delete_args, root_path.clone()) {
            error!(
                "Failed to delete container {}: {}",
                container_name, delete_err
            );
        } else {
            info!("Container deleted: {}", container_name);
        }
    }

    // delete pause container
    let delete_args = Delete {
        container_id: pod_info.pod_sandbox_id.clone(),
        force: true,
    };
    let root_path = rootpath::determine(None, &*create_syscall())?;
    if let Err(delete_err) = delete(delete_args, root_path.clone()) {
        error!(
            "Failed to delete PodSandbox {}: {}",
            pod_info.pod_sandbox_id, delete_err
        );
    } else {
        info!("PodSandbox deleted: {}", pod_info.pod_sandbox_id);
    }

    // delete pod file
    PodInfo::delete(&root_path, pod_name)?;
    info!("Pod {} deleted successfully", pod_name);
    Ok(())
}

pub fn remove_pod_network(pid: i32) -> Result<(), anyhow::Error> {
    let mut cni = task::get_cni()?;
    cni.load_default_conf();

    let netns_path = format!("/proc/{pid}/ns/net");
    let id = pid.to_string();
    cni.remove(id, netns_path.clone())
        .map_err(|e| anyhow::anyhow!("Failed to remove CNI network: {}", e))?;

    Ok(())
}

pub fn create_pod(pod_yaml: &str) -> Result<(), anyhow::Error> {
    let mut task_runner = TaskRunner::from_file(pod_yaml)?;
    let pod_name = task_runner.task.metadata.name.clone();

    let pod_request = task_runner.build_run_pod_sandbox_request();
    let config = pod_request
        .config
        .as_ref()
        .ok_or_else(|| anyhow!("PodSandbox config is required"))?;
    task_runner.sandbox_config = Some(config.clone());
    let (pod_response, _) = task_runner.sync_run_pod_sandbox(pod_request)?;
    let pod_sandbox_id = pod_response.pod_sandbox_id;

    let pause_pid = task_runner.pause_pid.ok_or_else(|| {
        anyhow!(
            "Pause container PID not found for PodSandbox ID: {}",
            pod_sandbox_id
        )
    })?;
    info!(
        "PodSandbox (Pause) created: {}, pid: {}\n",
        pod_sandbox_id, pause_pid
    );

    let mut container_ids = Vec::new();
    for container in &task_runner.task.spec.containers {
        let create_request =
            task_runner.sync_build_create_container_request(&pod_sandbox_id, container)?;
        let create_response = task_runner.create_container(create_request)?;
        container_ids.push(create_response.container_id.clone());
        info!(
            "Container created: {} (ID: {})",
            container.name, create_response.container_id
        );
    }

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo {
        pod_sandbox_id,
        container_names: container_ids,
    };
    pod_info.save(&root_path, &pod_name)?;

    info!("Pod {} created successfully", pod_name);
    Ok(())
}

pub fn start_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;

    if pod_info.container_names.is_empty() {
        return Err(anyhow!("No containers found for Pod {}", pod_name));
    }

    for container_name in &pod_info.container_names {
        let start_args = Start {
            container_id: container_name.clone(),
        };
        start(start_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to start container {}: {}", container_name, e))?;
        info!("Container started: {}", container_name);
    }

    info!("Pod {} started successfully", pod_name);
    Ok(())
}

pub fn state_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;

    info!("Pod: {pod_name}");

    info!("PodSandbox ID: {}", pod_info.pod_sandbox_id);
    let _ = state(
        State {
            container_id: pod_info.pod_sandbox_id.clone(),
        },
        root_path.clone(),
    );

    info!("Containers:");
    for container_name in &pod_info.container_names {
        let _container_state = state(
            State {
                container_id: container_name.clone(),
            },
            root_path.clone(),
        );
    }

    // TODO: show probe status

    Ok(())
}

pub fn exec_pod(args: ExecPod) -> Result<i32> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info_path = root_path.join("pods").join(&args.pod_name);
    if !pod_info_path.exists() {
        return Err(anyhow::anyhow!("Pod {} not found", args.pod_name));
    }
    let args = Exec::from(args);
    let exit_code = exec(args, root_path)?;
    Ok(exit_code)
}
