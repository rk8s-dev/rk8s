use crate::commands::ExecPod;
use crate::daemon;
use crate::task::TaskRunner;
use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use daemonize::Daemonize;
use libcontainer::container::Container;
use libruntime::rootpath;
use std::env;
use std::fs::{self, File};
use std::io::Read;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, warn};

use common::PodTask;
use libcontainer::syscall::syscall::create_syscall;

pub mod cluster;
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

#[derive(Args, Debug, Clone)]
pub struct TLSConnectionArgs {
    #[arg(long, env = "ENABLE_TLS", required = false)]
    pub enable_tls: bool,
    #[arg(long, env = "JOIN_TOKEN")]
    pub join_token: Option<String>,
    #[arg(long, env = "ROOT_CERT_PATH")]
    pub root_cert_path: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum PodCommand {
    #[command(about = "Create a pod from a YAML file using rkl create pod.yaml")]
    Create {
        #[arg(value_name = "POD_YAML")]
        pod_yaml: String,

        #[arg(
            long,
            value_name = "RKS_ADDRESS",
            env = "RKS_ADDRESS",
            required = false
        )]
        cluster: Option<String>,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },
    #[command(about = "Start a pod with a pod-name using rkl start pod-name")]
    Start {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,
    },

    #[command(about = "Delete a pod with a pod-name using rkl delete pod-name")]
    Delete {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,

        #[arg(
            long,
            value_name = "RKS_ADDRESS",
            env = "RKS_ADDRESS",
            required = false
        )]
        cluster: Option<String>,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },

    #[command(about = "Get the state of a pod using rkl state pod-name")]
    State {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,

        #[arg(
            long,
            value_name = "RKS_ADDRESS",
            env = "RKS_ADDRESS",
            required = false
        )]
        cluster: Option<String>,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },

    #[command(about = "Execute a command inside a specific container of a pod")]
    Exec(Box<ExecPod>),

    #[command(about = "List all of pods")]
    List {
        #[arg(
            long,
            value_name = "RKS_ADDRESS",
            env = "RKS_ADDRESS",
            required = false
        )]
        cluster: Option<String>,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },

    #[command(about = "Get details of a specific Pod")]
    Get {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,

        #[arg(
            long,
            value_name = "RKS_ADDRESS",
            env = "RKS_ADDRESS",
            required = false
        )]
        cluster: Option<String>,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },

    // Run as a daemon process.
    // For convenient, I won't remove cli part now.
    #[command(
        about = "Set rkl on daemon mod monitoring the pod.yaml in '/etc/rk8s/manifests' directory"
    )]
    Daemon {
        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },

    #[command(about = "Get logs from a pod's container")]
    Logs {
        #[arg(value_name = "POD_NAME")]
        pod_name: String,

        #[arg(short = 'c', long, value_name = "CONTAINER")]
        container: Option<String>,

        #[arg(short = 'f', long)]
        follow: bool,

        #[arg(long, value_name = "LINES", default_value = "-1")]
        tail: i64,

        #[arg(long, value_name = "TIMESTAMP")]
        since: Option<String>,

        #[arg(long)]
        timestamps: bool,

        #[arg(short = 'p', long)]
        previous: bool,

        #[arg(
            long,
            value_name = "RKS_ADDRESS",
            env = "RKS_ADDRESS",
            required = false
        )]
        cluster: Option<String>,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },
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

    pub fn get_pod_containers(&self, root_path: &Path) -> Result<Vec<Container>> {
        let mut containers = Vec::new();
        for container_name in &self.container_names {
            let container = Container::load(root_path.join(container_name))?;
            containers.push(container);
        }
        Ok(containers)
    }

    pub fn get_pod_sandbox(&self, root_path: &Path) -> Result<Container> {
        let sandbox = Container::load(root_path.join(&self.pod_sandbox_id))?;
        Ok(sandbox)
    }
}

// Currently the rkl's pod command functionality do not be removed yet
// So keep this sync `run_pod_from_taskrunner` api temporaily.
pub fn sync_run_pod_from_taskrunner(
    mut task_runner: TaskRunner,
) -> Result<PodRunResult, anyhow::Error> {
    let pod_name = task_runner.task.metadata.name.clone();
    let (pod_sandbox_id, podip) = task_runner.sync_run()?;
    info!("PodSandbox ID: {}", pod_sandbox_id);

    let container_names: Vec<String> = task_runner
        .task
        .spec
        .containers
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo {
        pod_sandbox_id: pod_sandbox_id.clone(),
        container_names: container_names.clone(),
    };
    pod_info.save(&root_path, &pod_name)?;

    info!("Pod {} created and started successfully", pod_name);
    Ok(PodRunResult {
        pod_sandbox_id,
        pod_ip: podip,
        container_names,
        pod_task: task_runner.task.clone(),
    })
}

pub async fn run_pod_from_taskrunner(
    mut task_runner: TaskRunner,
) -> Result<PodRunResult, anyhow::Error> {
    let pod_name = task_runner.task.metadata.name.clone();
    let (pod_sandbox_id, podip) = task_runner.run().await?;
    info!("PodSandbox ID: {}", pod_sandbox_id);

    let container_names: Vec<String> = task_runner
        .task
        .spec
        .containers
        .iter()
        .map(|c| c.name.clone())
        .collect();

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let pod_info = PodInfo {
        pod_sandbox_id: pod_sandbox_id.clone(),
        container_names: container_names.clone(),
    };
    pod_info.save(&root_path, &pod_name)?;

    info!("Pod {} created and started successfully", pod_name);
    Ok(PodRunResult {
        pod_sandbox_id,
        pod_ip: podip,
        container_names,
        pod_task: task_runner.task.clone(),
    })
}

#[allow(dead_code)]
pub fn set_daemonize() -> Result<(), anyhow::Error> {
    let log_path = PathBuf::from("/var/log/rk8s/");
    if !log_path.exists() {
        std::fs::create_dir(log_path)?;
    }

    let time_stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let out = File::create(format!("/var/log/rk8s/log_{time_stamp}.out"))?;
    let err = File::create(format!("/var/log/rk8s/log_{time_stamp}.err"))?;
    let pid = format!("/tmp/rkl_{time_stamp}.pid");
    let daemonize = Daemonize::new().pid_file(&pid).stdout(out).stderr(err);
    daemonize.start()?;
    Ok(())
}

pub fn start_daemon(tls_cfg: TLSConnectionArgs) -> Result<(), anyhow::Error> {
    let manifest_path = Path::new("/etc/rk8s/manifests");
    if !manifest_path.exists() {
        std::fs::create_dir_all(manifest_path)?;
    }
    #[cfg(not(debug_assertions))]
    set_daemonize()?;
    daemon::main(tls_cfg)
}

pub fn pod_execute(cmd: PodCommand) -> Result<()> {
    match cmd {
        PodCommand::Create {
            pod_yaml,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl apply -f pod.yaml' instead.");
            pod_create(&pod_yaml, cluster, tls_cfg)
        }
        PodCommand::Start { pod_name } => Err(anyhow!(
            "rkl pod start '{}' is not supported in cluster mode; create the Pod through rks with 'rkl apply -f'",
            pod_name
        )),
        PodCommand::Delete {
            pod_name,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl delete pod POD_NAME' instead.");
            pod_delete(&pod_name, cluster, tls_cfg)
        }
        PodCommand::State {
            pod_name,
            cluster: _,
            tls_cfg: _,
        } => Err(anyhow!(
            "rkl pod state '{}' is not a cluster operation yet; use 'rkl get pod POD_NAME' to get pod info instead",
            pod_name
        )),
        PodCommand::Exec(exec) => {
            warn!("This command has been deprecated. Use 'rkl exec POD_NAME -c' instead.");
            Err(anyhow!(
                "rkl pod exec '{}' is not a cluster operation yet; use 'rkl attach' to attach to the container and execute command instead",
                exec.pod_name
            ))
        }
        PodCommand::Daemon { tls_cfg } => start_daemon(tls_cfg),
        PodCommand::List { cluster, tls_cfg } => {
            warn!("This command has been deprecated. Use 'rkl get pods' instead.");
            pod_list(cluster, tls_cfg)
        }
        PodCommand::Get {
            pod_name,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl get pod POD_NAME' instead.");
            pod_get(&pod_name, cluster, tls_cfg)
        }
        PodCommand::Logs {
            pod_name,
            container,
            follow,
            tail,
            since,
            timestamps,
            previous,
            cluster,
            tls_cfg,
        } => pod_logs(
            &pod_name,
            container.as_deref(),
            follow,
            tail,
            since.as_deref(),
            timestamps,
            previous,
            cluster,
            tls_cfg,
        ),
    }
}

pub fn pod_list(addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let rks_addr = resolve_rks_addr(addr)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cluster::list_pod(rks_addr.as_str(), tls_cfg))
}

pub fn pod_get(pod_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let rks_addr = resolve_rks_addr(addr)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cluster::get_pod(pod_name, &rks_addr, tls_cfg))
}

pub fn pod_delete(pod_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let rks_addr = resolve_rks_addr(addr)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cluster::delete_pod(pod_name, rks_addr.as_str(), tls_cfg))
}

pub fn pod_create(pod_yaml: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let rks_addr = resolve_rks_addr(addr)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cluster::create_pod(pod_yaml, rks_addr.as_str(), tls_cfg))
}

#[allow(clippy::too_many_arguments)]
fn pod_logs(
    pod_name: &str,
    container: Option<&str>,
    follow: bool,
    tail: i64,
    since: Option<&str>,
    timestamps: bool,
    previous: bool,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let rks_addr = resolve_rks_addr(addr)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(cluster::get_pod_logs(
        pod_name, container, follow, tail, since, timestamps, previous, &rks_addr, tls_cfg,
    ))
}

pub fn resolve_rks_addr(addr: Option<String>) -> Result<String> {
    addr.or_else(|| env::var("RKS_ADDRESS").ok())
        .ok_or_else(|| anyhow!("No RKS address provided. Set RKS_ADDRESS or use --cluster"))
}
