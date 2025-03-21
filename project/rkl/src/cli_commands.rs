use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::path::PathBuf;
use anyhow::{Result, anyhow};
use liboci_cli::{Create, Start,State, Kill, Delete};
use crate::commands::{create, start, state,kill,delete,load_container};
use crate::task::task::TaskRunner;
use crate::rootpath;

// store infomation of pod
#[derive(Debug)]
pub struct PodInfo {
    pub pod_sandbox_id: String,
    pub container_names: Vec<String>,
}

impl PodInfo {
    pub fn load(root_path: &Path, pod_name: &str) -> Result<Self> {
        // 使用 Path::join 构造路径
        let pod_info_path = root_path.join("pods").join(format!("{}.txt", pod_name));
        let mut file = File::open(&pod_info_path)
            .map_err(|_| anyhow!("Pod {} not found", pod_name))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let mut pod_sandbox_id = None;
        let mut container_names = Vec::new();
        for line in contents.lines() {
            if line.starts_with("PodSandbox ID: ") {
                pod_sandbox_id = Some(line.trim_start_matches("PodSandbox ID: ").to_string());
            } else if line.starts_with("- ") {
                let container_name = line.trim_start_matches("- ").to_string();
                container_names.push(container_name);
            }
        }

        let pod_sandbox_id = pod_sandbox_id.ok_or_else(|| anyhow!("PodSandbox ID not found for Pod {}", pod_name))?;
        Ok(PodInfo {
            pod_sandbox_id,
            container_names,
        })
    }

    pub fn save(&self, root_path: &Path, pod_name: &str) -> Result<()> {
        // 使用 Path::join 构造路径
        let pods_dir = root_path.join("pods");
        let pod_info_path = pods_dir.join(format!("{}.txt", pod_name));

        // 检查 pods 目录是否存在
        if pods_dir.exists() {
            if !pods_dir.is_dir() {
                return Err(anyhow!("{} exists but is not a directory", pods_dir.display()));
            }
        } else {
            fs::create_dir_all(&pods_dir)?;
        }

        // 检查 pod_info_path 是否已存在
        if pod_info_path.exists() {
            return Err(anyhow!("Pod {} already exists at {}", pod_name, pod_info_path.display()));
        }

        let mut file = File::create(&pod_info_path)?;
        writeln!(file, "PodSandbox ID: {}", self.pod_sandbox_id)?;
        writeln!(file, "Containers:")?;
        for container_name in &self.container_names {
            writeln!(file, "- {}", container_name)?;
        }
        Ok(())
    }

    pub fn delete(root_path: &Path, pod_name: &str) -> Result<()> {
        // 使用 Path::join 构造路径
        let pod_info_path = root_path.join("pods").join(format!("{}.txt", pod_name));
        fs::remove_file(&pod_info_path)?;
        Ok(())
    }
}

pub fn run_pod(pod_yaml: &str) -> Result<(), anyhow::Error> {
    let mut task_runner = TaskRunner::from_file(pod_yaml)?;
    let pod_name = task_runner.task.metadata.name.clone();
    let pod_sandbox_id = task_runner.run()?;
    println!("PodSandbox ID: {}", pod_sandbox_id);

    let container_names: Vec<String> = task_runner.task.spec.containers
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let root_path = rootpath::determine(None)?;
    let pod_info = PodInfo {
        pod_sandbox_id,
        container_names,
    };
    pod_info.save(&root_path, &pod_name)?;

    println!("Pod {} created and started successfully", pod_name);
    Ok(())
}

pub fn create_pod(pod_yaml: &str) -> Result<(), anyhow::Error> {
    let mut task_runner = TaskRunner::from_file(pod_yaml)?;
    let pod_name = task_runner.task.metadata.name.clone();

    let pod_request = task_runner.build_run_pod_sandbox_request();
    let config = pod_request.config.as_ref().ok_or_else(|| anyhow!("PodSandbox config is required"))?;
    task_runner.sandbox_config = Some(config.clone());
    let pod_response = task_runner.run_pod_sandbox(pod_request)?;
    let pod_sandbox_id = pod_response.pod_sandbox_id;

    let pause_pid = task_runner.pause_pid
        .ok_or_else(|| anyhow!("Pause container PID not found for PodSandbox ID: {}", pod_sandbox_id))?;
    println!("PodSandbox (Pause) created: {}, pid: {}\n", pod_sandbox_id, pause_pid);

    let mut container_ids = Vec::new();
    for container in &task_runner.task.spec.containers {
        let create_request = task_runner.build_create_container_request(
            &pod_sandbox_id,
            container,
        )?;
        let create_response = task_runner.create_container(create_request)?;
        container_ids.push(create_response.container_id.clone());
        println!("Container created: {} (ID: {})", container.name, create_response.container_id);
    }

    let root_path = rootpath::determine(None)?;
    let pod_info = PodInfo {
        pod_sandbox_id,
        container_names: container_ids,
    };
    pod_info.save(&root_path, &pod_name)?;

    println!("Pod {} created successfully", pod_name);
    Ok(())
}

pub fn start_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None)?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;

    if pod_info.container_names.is_empty() {
        return Err(anyhow!("No containers found for Pod {}", pod_name));
    }

    for container_name in &pod_info.container_names {
        let start_args = Start {
            container_id: container_name.clone(),
        };
        start::start(start_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to start container {}: {}", container_name, e))?;
        println!("Container started: {}", container_name);
    }

    println!("Pod {} started successfully", pod_name);
    Ok(())
}

pub fn delete_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None)?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;

    // 先删除所有容器
    for container_name in &pod_info.container_names {
        let delete_args = Delete {
            container_id: container_name.clone(),
            force: true, // 强制删除，即使容器正在运行
        };
        let root_path = rootpath::determine(None)?;
        if let Err(delete_err) = delete::delete(delete_args, root_path.clone()) {
            eprintln!("Failed to delete container {}: {}", container_name, delete_err);
        } else {
            println!("Container deleted: {}", container_name);
        }
    }

    // 删除 PodSandbox
    let delete_args = Delete {
        container_id: pod_info.pod_sandbox_id.clone(),
        force: true, // 强制删除，即使 PodSandbox 正在运行
    };
    let root_path = rootpath::determine(None)?;
    if let Err(delete_err) = delete::delete(delete_args, root_path.clone()) {
        eprintln!("Failed to delete PodSandbox {}: {}", pod_info.pod_sandbox_id, delete_err);
    } else {
        println!("PodSandbox deleted: {}", pod_info.pod_sandbox_id);
    }

    // 删除 Pod 信息文件
    PodInfo::delete(&root_path, pod_name)?;
    println!("Pod {} deleted successfully", pod_name);
    Ok(())
}

pub fn state_pod(pod_name: &str) -> Result<(), anyhow::Error> {
    let root_path = rootpath::determine(None)?;
    let pod_info = PodInfo::load(&root_path, pod_name)?;

    // 打印 Pod 基本信息
    println!("Pod: {}", pod_name);

    // 查询 PodSandbox 状态
    println!("PodSandbox ID: {}", pod_info.pod_sandbox_id);
    state::state(State { container_id: pod_info.pod_sandbox_id.clone() }, root_path.clone());
    

    // 查询容器状态
    println!("Containers:");
    for container_name in &pod_info.container_names {
        let container_state = state::state(State { container_id: container_name.clone() }, root_path.clone());
    }

    Ok(())
}