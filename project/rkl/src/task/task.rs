use libcontainer::oci_spec::runtime::ProcessBuilder;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::Read;
use crate::cri::cri::{
    PodSandboxConfig, PodSandboxMetadata, PortMapping, Protocol,
    RunPodSandboxRequest, RunPodSandboxResponse,
    CreateContainerRequest, CreateContainerResponse,
    ContainerConfig, ContainerMetadata, ImageSpec, KeyValue, Mount,
    LinuxContainerConfig, LinuxContainerResources,
    StartContainerRequest, StopPodSandboxRequest, RemovePodSandboxRequest,
    PodSandboxStatusRequest, ContainerStatusRequest,StartContainerResponse,
    StopPodSandboxResponse,RemovePodSandboxResponse
};
use libcontainer::oci_spec::runtime::{
    LinuxBuilder, LinuxNamespaceBuilder, LinuxNamespaceType, Spec,
};
use serde_json::json;
use std::path::PathBuf;
use std::io::BufWriter;
use std::io::Write;
use anyhow::{Result, anyhow};
use liboci_cli::{Create,Start,State,Kill,Delete};
use crate::commands::{create, start, state,kill,delete,load_container};
use crate::rootpath;
use std::path::Path;
// 模拟 Kubernetes Pod 的元数据
#[derive(Debug, Serialize, Deserialize)]
pub struct TypeMeta {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ObjectMeta {
    pub name: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
    #[serde(default)] // 如果 annotations 未指定，默认为空 HashMap
    pub annotations: std::collections::HashMap<String, String>, // 添加 annotations 字段
}

pub fn default_namespace() -> String {
    "default".to_string()
}

// 模拟 Kubernetes PodSpec
#[derive(Debug, Serialize, Deserialize)]
pub struct PodSpec {
    #[serde(default)]
    pub containers: Vec<ContainerSpec>,
    #[serde(default)]
    pub init_containers: Vec<ContainerSpec>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]  
    pub args: Vec<String>,  
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Port {
    #[serde(rename = "containerPort")]
    pub container_port: i32,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(rename = "hostPort", default)]
    pub host_port: i32,
    #[serde(rename = "hostIP", default)]
    pub host_ip: String,
}

fn default_protocol() -> String {
    "TCP".to_string()
}

// 任务运行器，基于 Kubernetes Pod 模型
pub struct TaskRunner {
    pub task: PodTask,
    pub pause_pid: Option<i32>, // 记录 Pause 容器的 PID
    pub sandbox_config: Option<PodSandboxConfig>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PodTask {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: PodSpec,
}

impl TaskRunner {
    pub fn from_file(path: &str) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let task: PodTask = serde_yaml::from_str(&contents)?;
        Ok(TaskRunner { task, pause_pid: None,sandbox_config:None })
    }

    pub fn create_pod_sandbox_config(&self, uid: &str, attempt: u32) -> Result<PodSandboxConfig, anyhow::Error> {
        // 构造 PodSandboxMetadata
        let metadata = PodSandboxMetadata {
            name: self.task.metadata.name.clone(),
            namespace: self.task.metadata.namespace.clone(),
            uid: uid.to_string(),
            attempt,
        };
    
        // 端口映射，仅处理 containers
        let port_mappings = self.task.spec.containers
            .iter()
            .flat_map(|c| c.ports.iter().map(|p| PortMapping {
                protocol: match p.protocol.as_str() {
                    "TCP" => Protocol::Tcp,
                    "UDP" => Protocol::Udp,
                    _ => Protocol::Tcp,
                } as i32,
                container_port: p.container_port,
                host_port: p.host_port,
                host_ip: p.host_ip.clone(),
            }))
            .collect();
    
        // 构造 PodSandboxConfig
        Ok(PodSandboxConfig {
            metadata: Some(metadata),
            hostname: self.task.metadata.name.clone(),
            log_directory: format!("/var/log/pods/{}_{}_{}/", self.task.metadata.namespace, self.task.metadata.name, uid),
            dns_config: None,
            port_mappings,
            labels: self.task.metadata.labels.clone(),
            annotations: self.task.metadata.annotations.clone(),
            linux: None, 
            windows: None,
        })
    }

    pub fn build_run_pod_sandbox_request(&self) -> RunPodSandboxRequest {
        let uid = uuid::Uuid::new_v4().to_string();
        let attempt = 0; // 可动态传入
        RunPodSandboxRequest {
            config: Some(self.create_pod_sandbox_config(&uid, attempt).unwrap_or_default()), 
            runtime_handler: "pause".to_string(), // 表示启动 Pause 容器
        }
    }

    pub fn run_pod_sandbox(
        &mut self,
        request: RunPodSandboxRequest,
    ) -> Result<RunPodSandboxResponse, anyhow::Error> {
        let config = request.config.unwrap_or_default();
        let sandbox_id = format!("{}", config.metadata.unwrap_or_default().name);

        // 从 labels 中读取 Pause 容器的 bundle 路径
        let bundle_path = self.task.metadata.labels
                .get("bundle")
                .cloned()
                .ok_or_else(|| anyhow!("bundle not found in Pod labels"))?;
        let bundle_dir = PathBuf::from(&bundle_path);
        if !bundle_dir.exists() {
            return Err(anyhow!("Bundle directory does not exist"));
        }

        let create_args = Create {
            bundle: bundle_dir.clone(),
            console_socket: None,
            pid_file: None,
            no_pivot: false,
            no_new_keyring: false,
            preserve_fds: 0,
            container_id: sandbox_id.clone(),
        };

        let root_path = rootpath::determine(None)
            .map_err(|e| anyhow!("Failed to determine root path: {}", e))?;

        create::create(create_args, root_path.clone(), false)
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        //thread::sleep(Duration::from_secs(5));

        let start_args = Start {
            container_id: sandbox_id.clone(),
        };
        start::start(start_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to start container: {}", e))?;

        //thread::sleep(Duration::from_secs(5));

        let container = load_container(root_path.clone(), &sandbox_id)
            .map_err(|e| anyhow!("Failed to load container {}: {}", sandbox_id, e))?;
        let pid_i32 = container.state.pid
            .ok_or_else(|| anyhow!("PID not found for container {}", sandbox_id))?;

        self.pause_pid = Some(pid_i32);

        let response = RunPodSandboxResponse {
            pod_sandbox_id: sandbox_id,
        };

        Ok(response)
    }
    
    pub fn build_create_container_request(
        &self,
        pod_sandbox_id: &str,
        container: &ContainerSpec,
    ) -> Result<CreateContainerRequest, anyhow::Error> {
        let config = ContainerConfig {
            metadata: Some(ContainerMetadata {
                name: container.name.clone(),
                attempt: 0, // 默认尝试次数为 0
            }),
            image: Some(ImageSpec {
                image: container.image.clone(),
                annotations: std::collections::HashMap::new(),
                user_specified_image: container.image.clone(),
                runtime_handler: "".to_string(),
            }),//暂时仅占位
            command: vec!["/bin/sh".to_string()],//容器启动的主进程
            args: container.args.clone(),// command 与 args 配合使用
            working_dir: "/".to_string(),//主进程会在working_dir下执行
            envs: vec![KeyValue {
                key: "PATH".to_string(),
                value: "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string(),
            }],
            mounts: vec![
                Mount {
                    container_path: "/proc".to_string(),
                    host_path: "proc".to_string(),
                    readonly: false,
                    selinux_relabel: false,
                    propagation: 0, // 默认 propagation 模式
                    uid_mappings: vec![],
                    gid_mappings: vec![],
                    recursive_read_only: false,
                    image: None,
                },
                Mount {
                    container_path: "/dev".to_string(),
                    host_path: "tmpfs".to_string(),
                    readonly: false,
                    selinux_relabel: false,
                    propagation: 0,
                    uid_mappings: vec![],
                    gid_mappings: vec![],
                    recursive_read_only: false,
                    image: None,
                },
            ],//暂时仅占位
            devices: vec![],
            labels: std::collections::HashMap::new(),
            annotations: std::collections::HashMap::new(),
            log_path: format!("{}/0.log", container.name),
            stdin: false,
            stdin_once: false,
            tty: false,
            linux: None,
            windows: None,
            cdi_devices: vec![],
        };
    
        Ok(CreateContainerRequest {
            pod_sandbox_id: pod_sandbox_id.to_string(),
            config: Some(config),
            sandbox_config: self.sandbox_config.clone(),
        })
    }
   
    pub fn create_container(
        &self,
        request: CreateContainerRequest,
    ) -> Result<CreateContainerResponse, anyhow::Error> {   
        let pod_sandbox_id = request.pod_sandbox_id.clone();
        let config = request.config.as_ref().ok_or_else(|| anyhow!("Container config is required"))?;
        let container_id = config.metadata.as_ref()
        .map(|m| m.name.clone())
        .ok_or_else(|| anyhow!("Container metadata is required"))?;
    
        // 验证 sandbox_config
        if self.sandbox_config.is_none() {
            return Err(anyhow!("PodSandboxConfig is not set"));
        }
        let pause_pid = self.pause_pid.ok_or_else(|| anyhow!("Pause container PID is not set"))?;
        // 构造 OCI Spec
        let mut spec = Spec::default();
        let namespaces = vec![
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Pid)
            .path(format!("/proc/{}/ns/pid", pause_pid))
            .build()?,
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Network)
            .path(format!("/proc/{}/ns/net",pause_pid))
            .build()?,
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Ipc)
            .path(format!("/proc/{}/ns/ipc", pause_pid))
            .build()?,
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Uts)
            .path(format!("/proc/{}/ns/uts", pause_pid))
            .build()?,
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Mount)
            .build()?,
        LinuxNamespaceBuilder::default()
            .typ(LinuxNamespaceType::Cgroup)
            .build()?,
    ];

        let linux = LinuxBuilder::default()
            .namespaces(namespaces)
            .build()?;
        spec.set_linux(Some(linux));

        let container_spec = self.task.spec.containers
            .iter()
            .find(|c| c.name == container_id)
            .ok_or_else(|| anyhow!("Container spec not found for ID: {}", container_id))?;
        let mut process = ProcessBuilder::default()
                                        .args(container_spec.args.clone())
                                        .build()?;

        spec.set_process( Some(process));
        
        let bundle_path = container_spec.image.clone();
        if bundle_path.is_empty() {
            return Err(anyhow!("Bundle path (image) for container {} is empty", container_id));
        }
        let bundle_dir = PathBuf::from(&bundle_path);
        if !bundle_dir.exists() {
            return Err(anyhow!("Bundle directory does not exist"));
        }
        // 写入 config.json
        let config_path = format!("{}/config.json", bundle_path);
        if Path::new(&config_path).exists() {
            fs::remove_file(&config_path)
                .map_err(|e| anyhow!("Failed to remove existing config.json: {}", e))?;
        }
        let file = File::create(&config_path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &spec)?;
        writer.flush()?;
    
        // 构造 Create 参数
        let create_args = Create {
            bundle: bundle_path.clone().into(),
            console_socket: None,
            pid_file: None,
            no_pivot: false,
            no_new_keyring: false,
            preserve_fds: 0,
            container_id: container_id.clone(),
        };
    
        // 获得 root path
        let root_path = rootpath::determine(None)
            .map_err(|e| anyhow!("Failed to determine root path: {}", e))?;
    
        // 创建容器
        create::create(create_args, root_path.clone(), false)
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;
        
        Ok(CreateContainerResponse {
            container_id,
        })
    }
    
    pub fn start_container(
        &self,
        request: StartContainerRequest,
    ) -> Result<StartContainerResponse, anyhow::Error> {
        let container_id = request.container_id;
        let root_path = rootpath::determine(None)?;

        let start_args = Start {
            container_id: container_id.clone(),
        };
        start::start(start_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to start container {}: {}", container_id, e))?;

        Ok(StartContainerResponse {})
    }
    
    pub fn stop_pod_sandbox(
        &self,
        request: StopPodSandboxRequest,
    ) -> Result<StopPodSandboxResponse, anyhow::Error> {
        let pod_sandbox_id = request.pod_sandbox_id;
        let root_path = rootpath::determine(None)?;
        let kill_args = Kill {
                    container_id: pod_sandbox_id.clone(),
                    signal: "SIGKILL".to_string(),
                    all: false,
                };
        kill::kill(kill_args, root_path.clone())
        .map_err(|e| anyhow!("Failed to stop PodSandbox {}: {}", pod_sandbox_id, e))?;
        Ok(StopPodSandboxResponse {})
    }

    pub fn remove_pod_sandbox(
        &self,
        request: RemovePodSandboxRequest,
    ) -> Result<RemovePodSandboxResponse, anyhow::Error> {
        let pod_sandbox_id = request.pod_sandbox_id;
        let root_path = rootpath::determine(None)?;
        let delete_args = Delete {
            container_id: pod_sandbox_id.clone(),
            force:true,
        };
        delete::delete(delete_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to delete PodSandbox {}: {}", pod_sandbox_id, e))?;

        Ok(RemovePodSandboxResponse {})
    }

    pub fn run(&mut self) -> Result<String, anyhow::Error> {
        // 启动 PodSandbox（Pause 容器）
        let pod_request = self.build_run_pod_sandbox_request();
        let config = pod_request.config.as_ref().ok_or_else(|| anyhow!("PodSandbox config is required"))?;
        self.sandbox_config = Some(config.clone());
        let pod_response = self.run_pod_sandbox(pod_request)
            .map_err(|e| anyhow!("Failed to run PodSandbox: {}", e))?;
        let pod_sandbox_id = pod_response.pod_sandbox_id;
        let pause_pid = self.pause_pid
            .ok_or_else(|| anyhow!("Pause container PID not found for PodSandbox ID: {}", pod_sandbox_id))?;
        println!("PodSandbox (Pause) started: {}, pid: {}\n", pod_sandbox_id, pause_pid);
    
        // 记录创建成功的容器 ID，以便在失败时清理
        let mut created_containers = Vec::new();
    
        // 创建阶段：尝试创建所有容器
        for container in &self.task.spec.containers {
            let create_request = self.build_create_container_request(&pod_sandbox_id, container)?;
            match self.create_container(create_request) {
                Ok(create_response) => {
                    created_containers.push(create_response.container_id.clone());
                    println!("Container created: {} (ID: {})", container.name, create_response.container_id);
                }
                Err(e) => {
                    // 创建失败，回滚已创建的容器和 PodSandbox
                    eprintln!("Failed to create container {}: {}", container.name, e);
    
                    // 删除已创建的容器
                    for container_id in &created_containers {
                        let delete_args = Delete {
                            container_id: container_id.clone(),
                            force: true, // 强制删除，即使容器正在运行
                        };
                        let root_path = rootpath::determine(None)?;
                        if let Err(delete_err) = delete::delete(delete_args, root_path.clone()) {
                            eprintln!("Failed to delete container {} during rollback: {}", container_id, delete_err);
                        } else {
                            println!("Container deleted during rollback: {}", container_id);
                        }
                    }
    
                    // 停止 PodSandbox
                    let stop_request = StopPodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(stop_err) = self.stop_pod_sandbox(stop_request) {
                        eprintln!("Failed to stop PodSandbox {} during rollback: {}", pod_sandbox_id, stop_err);
                    } else {
                        println!("PodSandbox stopped during rollback: {}", pod_sandbox_id);
                    }
    
                    // 删除 PodSandbox
                    let remove_request = RemovePodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(remove_err) = self.remove_pod_sandbox(remove_request) {
                        eprintln!("Failed to remove PodSandbox {} during rollback: {}", pod_sandbox_id, remove_err);
                    } else {
                        println!("PodSandbox deleted during rollback: {}", pod_sandbox_id);
                    }
    
                    return Err(anyhow!("Failed to create container {}: {}", container.name, e));
                }
            }
        }
    
        // 启动阶段：启动所有容器
        for container_id in &created_containers {
            let start_request = StartContainerRequest {
                container_id: container_id.clone(),
            };
            match self.start_container(start_request) {
                Ok(_) => {
                    println!("Container started: {}", container_id);
                }
                Err(e) => {
                    // 启动失败，回滚已创建的容器和 PodSandbox
                    eprintln!("Failed to start container {}: {}", container_id, e);
    
                    // 删除已创建的容器
                    for container_id in &created_containers {
                        let delete_args = Delete {
                            container_id: container_id.clone(),
                            force: true, // 强制删除，即使容器正在运行
                        };
                        let root_path = rootpath::determine(None)?;
                        if let Err(delete_err) = delete::delete(delete_args, root_path.clone()) {
                            eprintln!("Failed to delete container {} during rollback: {}", container_id, delete_err);
                        } else {
                            println!("Container deleted during rollback: {}", container_id);
                        }
                    }
    
                    // 停止 PodSandbox
                    let stop_request = StopPodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(stop_err) = self.stop_pod_sandbox(stop_request) {
                        eprintln!("Failed to stop PodSandbox {} during rollback: {}", pod_sandbox_id, stop_err);
                    } else {
                        println!("PodSandbox stopped during rollback: {}", pod_sandbox_id);
                    }
    
                    // 删除 PodSandbox
                    let remove_request = RemovePodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(remove_err) = self.remove_pod_sandbox(remove_request) {
                        eprintln!("Failed to remove PodSandbox {} during rollback: {}", pod_sandbox_id, remove_err);
                    } else {
                        println!("PodSandbox deleted during rollback: {}", pod_sandbox_id);
                    }
    
                    return Err(anyhow!("Failed to start container {}: {}", container_id, e));
                }
            }
        }
    
        Ok(pod_sandbox_id)
    }

}