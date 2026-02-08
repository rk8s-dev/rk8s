use crate::commands::pod::PodInfo;
use crate::commands::{Exec, ExecPod};
use crate::commands::{delete, exec, load_container, start, state};
use crate::pod_task::get_cni;
use anyhow::{Result, anyhow};
use liboci_cli::{Delete, Start, State};
use libruntime::rootpath;
use tracing::{error, info};

use libcontainer::syscall::syscall::create_syscall;

pub fn delete_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;

    // Try to get PID from pause container for network cleanup
    let mut pause_pid = None;
    if let Ok(container) = load_container(root_path.clone(), &pod_info.pod_sandbox_id) {
        pause_pid = container.state.pid;
    }

    // Remove network if we have pause PID
    if let Some(pid) = pause_pid
        && let Err(e) = remove_pod_network(pid)
    {
        error!("Failed to remove network for Pod {}: {}", pod_name, e);
    }

    // Delete all containers
    for container_name in &pod_info.container_names {
        let delete_args = Delete {
            container_id: container_name.clone(),
            force: true,
        };
        if let Err(delete_err) = delete(delete_args, root_path.clone()) {
            error!(
                "Failed to delete container {}: {}",
                container_name, delete_err
            );
        } else {
            info!("Container deleted: {}", container_name);
        }
    }

    // Delete pause container
    let delete_args = Delete {
        container_id: pod_info.pod_sandbox_id.clone(),
        force: true,
    };
    if let Err(delete_err) = delete(delete_args, root_path.clone()) {
        error!(
            "Failed to delete PodSandbox {}: {}",
            pod_info.pod_sandbox_id, delete_err
        );
    } else {
        info!("PodSandbox deleted: {}", pod_info.pod_sandbox_id);
    }

    // Delete pod file
    PodInfo::delete(&root_path, pod_name)?;
    info!("Pod {} deleted successfully", pod_name);
    Ok(())
}

pub fn remove_pod_network(pid: i32) -> Result<(), anyhow::Error> {
    let mut cni = get_cni()?;
    cni.load_default_conf();

    let netns_path = format!("/proc/{pid}/ns/net");
    let id = pid.to_string();
    cni.remove(id, netns_path.clone())
        .map_err(|e| anyhow::anyhow!("Failed to remove CNI network: {}", e))?;

    Ok(())
}

pub fn run_pod(pod_yaml: &str) -> Result<(), anyhow::Error> {
    let mut runner = crate::pod_task::TaskRunner::from_file(pod_yaml)?;
    let pod_name = runner.task.metadata.name.clone();
    let root_path = rootpath::determine(None, &*create_syscall())?;

    // Check if pod already exists
    if PodInfo::load(&root_path, &pod_name).is_ok() {
        info!("Pod {} already exists, cleaning up first", pod_name);
        // Try to delete existing pod
        if let Err(e) = delete_pod(&pod_name) {
            error!("Failed to cleanup existing pod {}: {}", pod_name, e);
            return Err(anyhow!(
                "Pod {} already exists and cleanup failed: {}",
                pod_name,
                e
            ));
        }
    }

    // Also check if containers exist at the youki level without pod info
    // This handles the case where pod info was lost but containers still exist
    let pod_sandbox_id_check = pod_name.clone();
    if let Ok(_container) = load_container(root_path.clone(), &pod_sandbox_id_check) {
        info!(
            "Found orphaned container {}, cleaning up",
            pod_sandbox_id_check
        );
        let delete_args = Delete {
            container_id: pod_sandbox_id_check.clone(),
            force: true,
        };
        if let Err(e) = delete(delete_args, root_path.clone()) {
            error!("Failed to delete orphaned pause container: {}", e);
        }
    }

    // Check for orphaned application containers
    for container in &runner.task.spec.containers {
        let container_id = container.name.clone();
        if let Ok(_container) = load_container(root_path.clone(), &container_id) {
            info!("Found orphaned container {}, cleaning up", container_id);
            let delete_args = Delete {
                container_id: container_id.clone(),
                force: true,
            };
            if let Err(e) = delete(delete_args, root_path.clone()) {
                error!(
                    "Failed to delete orphaned container {}: {}",
                    container_id, e
                );
            }
        }
    }

    let (pod_sandbox_id, pod_ip) = runner.run()?;

    // Save pod info for future management
    let mut container_ids = Vec::new();
    for container in &runner.task.spec.containers {
        container_ids.push(container.name.clone());
    }

    let pod_info = PodInfo {
        pod_sandbox_id: pod_sandbox_id.clone(),
        container_names: container_ids,
    };
    pod_info.save(&root_path, &pod_name)?;

    info!(
        "Pod {} started successfully with PodSandbox ID: {} and IP: {}",
        pod_name, pod_sandbox_id, pod_ip
    );
    Ok(())
}

pub fn create_pod(pod_yaml: &str) -> Result<(), anyhow::Error> {
    let mut task_runner = crate::pod_task::TaskRunner::from_file(pod_yaml)?;
    let pod_name = task_runner.task.metadata.name.clone();
    let root_path = rootpath::determine(None, &*create_syscall())?;

    // Check if pod already exists
    if PodInfo::load(&root_path, &pod_name).is_ok() {
        return Err(anyhow!("Pod {} already exists", pod_name));
    }

    // Also check if containers exist at the youki level without pod info
    let pod_sandbox_id_check = pod_name.clone();
    if let Ok(_container) = load_container(root_path.clone(), &pod_sandbox_id_check) {
        return Err(anyhow!(
            "Container {} already exists (orphaned). Please clean up first with 'rkforge delete {}'",
            pod_sandbox_id_check,
            pod_sandbox_id_check
        ));
    }

    let pod_request = task_runner.build_run_pod_sandbox_request();
    let config = pod_request
        .config
        .as_ref()
        .ok_or_else(|| anyhow!("PodSandbox config is required"))?;
    task_runner.sandbox_config = Some(config.clone());
    let (pod_response, _) = task_runner.run_pod_sandbox(pod_request)?;
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
            task_runner.build_create_container_request(&pod_sandbox_id, container)?;
        let create_response = task_runner.create_container(create_request)?;
        container_ids.push(create_response.container_id.clone());
        info!(
            "Container created: {} (ID: {})",
            container.name, create_response.container_id
        );
    }

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

    println!("Pod: {}", pod_name);
    println!();

    println!("PodSandbox ID: {}", pod_info.pod_sandbox_id);
    let _ = state(
        State {
            container_id: pod_info.pod_sandbox_id.clone(),
        },
        root_path.clone(),
    );

    println!();
    println!("Containers:");
    for container_name in &pod_info.container_names {
        println!("  Container: {}", container_name);
        let _container_state = state(
            State {
                container_id: container_name.clone(),
            },
            root_path.clone(),
        );
        println!();
    }

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
