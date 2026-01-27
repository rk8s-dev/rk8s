use crate::commands::ExecPod;
use crate::commands::pod::standalone::{exec_pod, run_pod, start_pod, state_pod};
use anyhow::{Result, anyhow};
use clap::Subcommand;
use libruntime::rootpath;
use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::Path;

use common::PodTask;
use libcontainer::syscall::syscall::create_syscall;

pub mod standalone;

#[derive(Debug, Clone)]
pub struct PodRunResult {
    #[allow(unused)]
    pub pod_sandbox_id: String,
    pub pod_ip: String,
    #[allow(unused)]
    pub container_names: Vec<String>,
    pub pod_task: PodTask,
}

#[derive(Subcommand, Debug)]
pub enum PodCommand {
    #[command(about = "Run a pod from a YAML file using rkb run pod.yaml")]
    Run {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
    },
    #[command(about = "Create a pod from a YAML file using rkb create pod.yaml")]
    Create {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,
    },
    #[command(about = "Start a pod with a pod-name using rkb start pod-name")]
    Start {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },

    #[command(about = "Delete a pod with a pod-name using rkb delete pod-name")]
    Delete {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },

    #[command(about = "Get the state of a pod using rkb state pod-name")]
    State {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },

    #[command(about = "Execute a command inside a specific container of a pod")]
    Exec(Box<ExecPod>),

    #[command(about = "List all of pods")]
    List {},
}

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

pub fn pod_execute(cmd: PodCommand) -> Result<()> {
    match cmd {
        PodCommand::Run { pod_yaml } => run_pod(&pod_yaml),
        PodCommand::Create { pod_yaml } => standalone::create_pod(&pod_yaml),
        PodCommand::Start { pod_name } => start_pod(&pod_name),
        PodCommand::Delete { pod_name } => standalone::delete_pod(&pod_name),
        PodCommand::State { pod_name } => state_pod(&pod_name),
        PodCommand::Exec(exec) => {
            let exit_code = exec_pod(*exec)?;
            std::process::exit(exit_code);
        }
        PodCommand::List {} => pod_list(),
    }
}

fn pod_list() -> Result<()> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pods_dir = root_path.join("pods");

    if !pods_dir.exists() {
        println!("No pods found");
        return Ok(());
    }

    let entries = fs::read_dir(pods_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().map(|ft| ft.is_file()).unwrap_or(false))
        .collect::<Vec<_>>();

    if entries.is_empty() {
        println!("No pods found");
        return Ok(());
    }

    println!("{:<20} {:<40} {:<20}", "NAME", "SANDBOX_ID", "CONTAINERS");
    println!("{:-<80}", "");

    for entry in entries {
        let pod_name = entry.file_name().to_string_lossy().to_string();
        match PodInfo::load(&root_path, &pod_name) {
            Ok(pod_info) => {
                let containers = pod_info.container_names.join(", ");
                let sandbox_id_short = if pod_info.pod_sandbox_id.len() > 12 {
                    &pod_info.pod_sandbox_id[..12]
                } else {
                    &pod_info.pod_sandbox_id
                };
                println!(
                    "{:<20} {:<40} {:<20}",
                    pod_name, sandbox_id_short, containers
                );
            }
            Err(_) => {
                println!("{:<20} {:<40} {:<20}", pod_name, "<error>", "<error>");
            }
        }
    }

    Ok(())
}
