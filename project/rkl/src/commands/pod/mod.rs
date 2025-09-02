use crate::commands::pod::standalone::{exec_pod, start_pod, state_pod};
use crate::rootpath;
use crate::task::TaskRunner;
use crate::{PodCommand, daemon};
use anyhow::{Result, anyhow};
use daemonize::Daemonize;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

pub mod cluster;
pub mod standalone;

// store infomation of pod
#[derive(Debug)]
pub struct PodInfo {
    pub pod_sandbox_id: String,
    pub container_names: Vec<String>,
}

impl PodInfo {
    pub fn load(root_path: &Path, pod_name: &str) -> Result<Self> {
        // get path like pods/podname
        let pod_info_path = root_path.join("pods").join(pod_name);
        let mut file =
            File::open(&pod_info_path).map_err(|_| anyhow!("Pod {} not found", pod_name))?;
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

        let pod_sandbox_id = pod_sandbox_id
            .ok_or_else(|| anyhow!("PodSandbox ID not found for Pod {}", pod_name))?;
        Ok(PodInfo {
            pod_sandbox_id,
            container_names,
        })
    }

    pub fn save(&self, root_path: &Path, pod_name: &str) -> Result<()> {
        let pods_dir = root_path.join("pods");
        let pod_info_path = pods_dir.join(pod_name);

        if pods_dir.exists() {
            if !pods_dir.is_dir() {
                return Err(anyhow!(
                    "{} exists but is not a directory",
                    pods_dir.display()
                ));
            }
        } else {
            fs::create_dir_all(&pods_dir)?;
        }

        if pod_info_path.exists() {
            return Err(anyhow!(
                "Pod {} already exists at {}",
                pod_name,
                pod_info_path.display()
            ));
        }

        let mut file = File::create(&pod_info_path)?;
        writeln!(file, "PodSandbox ID: {}", self.pod_sandbox_id)?;
        writeln!(file, "Containers:")?;
        for container_name in &self.container_names {
            writeln!(file, "- {container_name}")?;
        }
        Ok(())
    }

    pub fn delete(root_path: &Path, pod_name: &str) -> Result<()> {
        let pod_info_path = root_path.join("pods").join(pod_name);
        fs::remove_file(&pod_info_path)?;
        Ok(())
    }
}

pub fn run_pod_from_taskrunner(mut task_runner: TaskRunner) -> Result<(), anyhow::Error> {
    let pod_name = task_runner.task.metadata.name.clone();
    let pod_sandbox_id = task_runner.run()?;
    info!("PodSandbox ID: {}", pod_sandbox_id);

    let container_names: Vec<String> = task_runner
        .task
        .spec
        .containers
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let root_path = rootpath::determine(None)?;
    let pod_info = PodInfo {
        pod_sandbox_id,
        container_names,
    };
    pod_info.save(&root_path, &pod_name)?;

    info!("Pod {} created and started successfully", pod_name);
    Ok(())
}

pub fn run_pod(pod_yaml: &str) -> Result<(), anyhow::Error> {
    let task_runner = TaskRunner::from_file(pod_yaml)?;
    run_pod_from_taskrunner(task_runner)
}

pub fn set_daemonize() -> Result<(), anyhow::Error> {
    let log_path = PathBuf::from("/var/log/rk8s/");
    if !log_path.exists() {
        std::fs::create_dir(log_path)?;
    }

    let time_stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let out = File::create(format!("/var/log/rk8s/log_{time_stamp}.out")).unwrap();
    let err = File::create(format!("/var/log/rk8s/log_{time_stamp}.err")).unwrap();
    let pid = format!("/tmp/rkl_{time_stamp}.pid");
    let daemonize = Daemonize::new().pid_file(&pid).stdout(out).stderr(err);
    daemonize.start()?;
    Ok(())
}

pub fn start_daemon() -> Result<(), anyhow::Error> {
    let manifest_path = Path::new("/etc/rk8s/manifests");
    if !manifest_path.exists() {
        std::fs::create_dir_all(manifest_path)?;
    }
    #[cfg(not(debug_assertions))]
    set_daemonize()?;
    daemon::main()
}

pub fn pod_execute(cmd: PodCommand) -> Result<()> {
    match cmd {
        PodCommand::Run { pod_yaml } => run_pod(&pod_yaml),
        PodCommand::Create { pod_yaml, cluster } => pod_create(&pod_yaml, cluster),
        PodCommand::Start { pod_name } => start_pod(&pod_name),
        PodCommand::Delete { pod_name, cluster } => pod_delete(&pod_name, cluster),
        PodCommand::State { pod_name } => state_pod(&pod_name),
        PodCommand::Exec(exec) => {
            let exit_code = exec_pod(*exec)?;
            std::process::exit(exit_code);
        }
        PodCommand::Daemon => start_daemon(),
        PodCommand::List { .. } => pod_list(),
    }
}

fn pod_list() -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cluster::list_pod())
}

fn pod_delete(pod_name: &str, cluster: bool) -> Result<()> {
    let is_cluster: bool = env::var("RKL_POD_CLUSTER")
        .unwrap_or("false".to_string())
        .parse()?;
    match cluster || is_cluster {
        false => standalone::delete_pod(pod_name),
        true => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(cluster::delete_pod(pod_name))
        }
    }
}

fn pod_create(pod_yaml: &str, cluster: bool) -> Result<()> {
    let is_cluster: bool = env::var("RKL_POD_CLUSTER")
        .unwrap_or("false".to_string())
        .parse()?;
    match cluster || is_cluster {
        false => standalone::create_pod(pod_yaml),
        true => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(cluster::create_pod(pod_yaml))
        }
    }
}
