use crate::commands::container::config::ContainerConfigBuilder;
use crate::commands::container::handle_image_typ;
use crate::commands::{create, delete, kill, load_container, start};
use crate::cri::cri_api::{
    CreateContainerRequest, CreateContainerResponse, LinuxContainerConfig, LinuxContainerResources,
    PodSandboxConfig, PodSandboxMetadata, PortMapping, Protocol, RemovePodSandboxRequest,
    RemovePodSandboxResponse, RunPodSandboxRequest, RunPodSandboxResponse, StartContainerRequest,
    StartContainerResponse, StopPodSandboxRequest, StopPodSandboxResponse,
};
use crate::rootpath;
use anyhow::{Result, anyhow};
use common::{ContainerRes, ContainerSpec, PodTask};
use json::JsonValue;
use libcni::rust_cni::cni::Libcni;
use libcontainer::oci_spec::runtime::{
    Capability, LinuxBuilder, LinuxCapabilities, LinuxCpuBuilder, LinuxMemoryBuilder,
    LinuxNamespaceBuilder, LinuxNamespaceType, LinuxResources, LinuxResourcesBuilder,
    ProcessBuilder, Spec,
};
use libcontainer::syscall::syscall::create_syscall;
use liboci_cli::{Create, Delete, Kill, Start};
use oci_spec::runtime::RootBuilder;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

pub struct TaskRunner {
    pub task: PodTask,
    pub pause_pid: Option<i32>, // pid of pause container
    pub sandbox_config: Option<PodSandboxConfig>,
}

impl TaskRunner {
    pub fn from_task(mut task: PodTask) -> Result<Self> {
        let pod_name = task.metadata.name.clone();

        for container in &mut task.spec.containers {
            let original_name = container.name.clone();
            container.name = format!("{pod_name}-{original_name}");
        }

        Ok(TaskRunner {
            task,
            pause_pid: None,
            sandbox_config: None,
        })
    }

    //get information from a file  record in Podtask
    pub fn from_file(path: &str) -> Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let task: PodTask = serde_yaml::from_str(&contents)?;
        debug!("{task:?}");
        Self::from_task(task)
    }

    //get PodSandboxConfig
    pub fn create_pod_sandbox_config(
        &self,
        uid: &str,
        attempt: u32,
    ) -> Result<PodSandboxConfig, anyhow::Error> {
        // create PodSandboxMetadata
        let metadata = PodSandboxMetadata {
            name: self.task.metadata.name.clone(),
            namespace: self.task.metadata.namespace.clone(),
            uid: uid.to_string(),
            attempt,
        };

        let port_mappings = self
            .task
            .spec
            .containers
            .iter()
            .flat_map(|c| {
                c.ports.iter().map(|p| PortMapping {
                    protocol: match p.protocol.as_str() {
                        "TCP" => Protocol::Tcp,
                        "UDP" => Protocol::Udp,
                        _ => Protocol::Tcp,
                    } as i32,
                    container_port: p.container_port,
                    host_port: p.host_port,
                    host_ip: p.host_ip.clone(),
                })
            })
            .collect();

        // create PodSandboxConfig
        //now some data isn't used
        Ok(PodSandboxConfig {
            metadata: Some(metadata),
            hostname: self.task.metadata.name.clone(),
            log_directory: format!(
                "/var/log/pods/{}_{}_{}/",
                self.task.metadata.namespace, self.task.metadata.name, uid
            ),
            dns_config: None,
            port_mappings,
            labels: self.task.metadata.labels.clone(),
            annotations: self.task.metadata.annotations.clone(),
            linux: None,
            windows: None,
        })
    }

    //get RunPodSandboxRequest
    pub fn build_run_pod_sandbox_request(&self) -> RunPodSandboxRequest {
        let uid = uuid::Uuid::new_v4().to_string();
        let attempt = 0;
        RunPodSandboxRequest {
            config: Some(
                self.create_pod_sandbox_config(&uid, attempt)
                    .unwrap_or_default(),
            ),
            runtime_handler: "pause".to_string(), // just mean that pause container is started
        }
    }

    //create pause container and start it
    pub fn run_pod_sandbox(
        &mut self,
        request: RunPodSandboxRequest,
    ) -> Result<(RunPodSandboxResponse, String), anyhow::Error> {
        let config = request.config.unwrap_or_default();
        let sandbox_id = config.metadata.unwrap_or_default().name.to_string();

        // get bundle path of pause container from labels
        let bundle_path = self.task.metadata.labels.get("bundle").cloned().unwrap();
        // .unwrap_or(get_pause_bundle()?);
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

        let root_path = rootpath::determine(None, &*create_syscall())
            .map_err(|e| anyhow!("Failed to determine root path: {}", e))?;

        create(create_args, root_path.clone(), false)
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        let start_args = Start {
            container_id: sandbox_id.clone(),
        };
        start(start_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to start container: {}", e))?;

        let container = load_container(root_path.clone(), &sandbox_id)
            .map_err(|e| anyhow!("Failed to load container {}: {}", sandbox_id, e))?;
        let pid_i32 = container
            .state
            .pid
            .ok_or_else(|| anyhow!("PID not found for container {}", sandbox_id))?;

        let pod_json = Self::setup_pod_network(pid_i32).map_err(|e| {
            let rollback_res = delete(
                Delete {
                    container_id: sandbox_id.clone(),
                    force: true,
                },
                root_path.clone(),
            );
            if let Err(err_rollback) = rollback_res {
                anyhow!("{e}; and failed to rollback: {err_rollback}")
            } else {
                e
            }
        })?;

        let podip = pod_json["ips"][0]["address"]
            .as_str()
            .unwrap_or("")
            .to_string();
        self.pause_pid = Some(pid_i32);
        info!("podip:{podip}");
        let response = RunPodSandboxResponse {
            pod_sandbox_id: sandbox_id,
        };

        Ok((response, podip))
    }

    pub fn setup_pod_network(pid: i32) -> Result<JsonValue, anyhow::Error> {
        let mut cni = get_cni()?;
        cni.load_default_conf();

        let netns_path = format!("/proc/{pid}/ns/net");
        let id = pid.to_string();

        let result = cni
            .setup(id.clone(), netns_path.clone())
            .map_err(|e| anyhow::anyhow!("Failed to add CNI network: {}", e))?;

        Ok(result)
    }

    pub fn build_create_container_request(
        &self,
        pod_sandbox_id: &str,
        container: &ContainerSpec,
    ) -> Result<CreateContainerRequest, anyhow::Error> {
        let (mut config_builder, bundle_path) = handle_image_typ(container)?;

        let config = if let Some(ref mut builder) = config_builder {
            builder.container_spec(container.clone())?;
            builder.images(bundle_path);
            builder.clone().build()
        } else {
            ContainerConfigBuilder::default()
                .container_spec(container.clone())?
                .clone()
                .build()
        };

        Ok(CreateContainerRequest {
            pod_sandbox_id: pod_sandbox_id.to_string(),
            config: Some(config),
            sandbox_config: self.sandbox_config.clone(),
        })
    }

    //create work container
    pub fn create_container(
        &self,
        request: CreateContainerRequest,
    ) -> Result<CreateContainerResponse, anyhow::Error> {
        let _pod_sandbox_id = request.pod_sandbox_id.clone();
        let config = request
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("Container config is required"))?;
        let container_id = config
            .metadata
            .as_ref()
            .map(|m| m.name.clone())
            .ok_or_else(|| anyhow!("Container metadata is required"))?;

        let container_spec = self
            .task
            .spec
            .containers
            .iter()
            .find(|c| c.name == container_id)
            .ok_or_else(|| anyhow!("Container spec not found for ID: {}", container_id))?;

        // check sandbox_config
        if self.sandbox_config.is_none() {
            return Err(anyhow!("PodSandboxConfig is not set"));
        }
        let pause_pid = self
            .pause_pid
            .ok_or_else(|| anyhow!("Pause container PID is not set"))?;
        // create  OCI Spec
        let mut spec = Spec::default();

        let root = RootBuilder::default()
            .readonly(false)
            .build()
            .unwrap_or_default();

        spec.set_root(Some(root));

        let namespaces = vec![
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Pid)
                .path(format!("/proc/{pause_pid}/ns/pid"))
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Network)
                .path(format!("/proc/{pause_pid}/ns/net"))
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Ipc)
                .path(format!("/proc/{pause_pid}/ns/ipc"))
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Uts)
                .path(format!("/proc/{pause_pid}/ns/uts"))
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Mount)
                .build()?,
            LinuxNamespaceBuilder::default()
                .typ(LinuxNamespaceType::Cgroup)
                .build()?,
        ];

        let mut linux = LinuxBuilder::default().namespaces(namespaces);
        if let Some(x) = &config.linux
            && let Some(r) = &x.resources
        {
            linux = linux.resources(r);
        }
        let linux = linux.build()?;
        spec.set_linux(Some(linux));

        let mut process = ProcessBuilder::default().build()?;

        // set args
        let arg = if container_spec.args.is_empty() {
            config.args.clone()
        } else {
            container_spec.args.clone()
        };

        process.set_args(Some(arg));

        let mut capabilities = process.capabilities().clone().unwrap();
        add_cap_net_raw(&mut capabilities);
        process.set_capabilities(Some(capabilities));

        spec.set_process(Some(process));
        // [image_specification] check if config's spec

        let bundle_path = if let Some(image_spec) = &config.image {
            image_spec.image.clone()
        } else {
            container_spec.image.clone()
        };

        if bundle_path.is_empty() {
            return Err(anyhow!(
                "Bundle path (image) for container {} is empty",
                container_id
            ));
        }
        let bundle_dir = PathBuf::from(&bundle_path);
        if !bundle_dir.exists() {
            return Err(anyhow!("Bundle directory does not exist"));
        }
        // write into config.json
        let config_path = format!("{bundle_path}/config.json");
        if Path::new(&config_path).exists() {
            fs::remove_file(&config_path)
                .map_err(|e| anyhow!("Failed to remove existing config.json: {}", e))?;
        }
        let file = File::create(&config_path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &spec)?;
        writer.flush()?;

        let create_args = Create {
            bundle: bundle_path.clone().into(),
            console_socket: None,
            pid_file: None,
            no_pivot: false,
            no_new_keyring: false,
            preserve_fds: 0,
            container_id: container_id.clone(),
        };

        // get root_path
        let root_path = rootpath::determine(None, &*create_syscall())
            .map_err(|e| anyhow!("Failed to determine root path: {}", e))?;

        create(create_args, root_path.clone(), false)
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        Ok(CreateContainerResponse { container_id })
    }

    pub fn start_container(
        &self,
        request: StartContainerRequest,
    ) -> Result<StartContainerResponse, anyhow::Error> {
        let container_id = request.container_id;
        let root_path = rootpath::determine(None, &*create_syscall())?;

        let start_args = Start {
            container_id: container_id.clone(),
        };
        start(start_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to start container {}: {}", container_id, e))?;

        Ok(StartContainerResponse {})
    }

    //stop pause container
    pub fn stop_pod_sandbox(
        &self,
        request: StopPodSandboxRequest,
    ) -> Result<StopPodSandboxResponse, anyhow::Error> {
        let pod_sandbox_id = request.pod_sandbox_id;
        let root_path = rootpath::determine(None, &*create_syscall())?;
        let kill_args = Kill {
            container_id: pod_sandbox_id.clone(),
            signal: "SIGKILL".to_string(),
            all: false,
        };
        kill(kill_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to stop PodSandbox {}: {}", pod_sandbox_id, e))?;
        Ok(StopPodSandboxResponse {})
    }
    //delete pause container
    pub fn remove_pod_sandbox(
        &self,
        request: RemovePodSandboxRequest,
    ) -> Result<RemovePodSandboxResponse, anyhow::Error> {
        let pod_sandbox_id = request.pod_sandbox_id;
        let root_path = rootpath::determine(None, &*create_syscall())?;
        let delete_args = Delete {
            container_id: pod_sandbox_id.clone(),
            force: true,
        };
        delete(delete_args, root_path.clone())
            .map_err(|e| anyhow!("Failed to delete PodSandbox {}: {}", pod_sandbox_id, e))?;

        Ok(RemovePodSandboxResponse {})
    }

    pub fn run(&mut self) -> Result<(String, String), anyhow::Error> {
        // run PodSandbox（Pause container）
        let pod_request = self.build_run_pod_sandbox_request();
        let config = pod_request
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("PodSandbox config is required"))?;
        self.sandbox_config = Some(config.clone());
        let (pod_response, podip) = self
            .run_pod_sandbox(pod_request)
            .map_err(|e| anyhow!("Failed to run PodSandbox: {}", e))?;
        let pod_sandbox_id = pod_response.pod_sandbox_id;
        let pause_pid = self.pause_pid.ok_or_else(|| {
            anyhow!(
                "Pause container PID not found for PodSandbox ID: {}",
                pod_sandbox_id
            )
        })?;
        info!(
            "PodSandbox (Pause) started: {}, pid: {}\n",
            pod_sandbox_id, pause_pid
        );

        //record the container ID if succeed
        // if fail clear all containers created
        let mut created_containers = Vec::new();

        // create all container
        for container in &self.task.spec.containers {
            let create_request = self.build_create_container_request(&pod_sandbox_id, container)?;
            match self.create_container(create_request) {
                Ok(create_response) => {
                    created_containers.push(create_response.container_id.clone());
                    info!(
                        "Container created: {} (ID: {})",
                        container.name, create_response.container_id
                    );
                }
                Err(e) => {
                    error!("Failed to create container {}: {}", container.name, e);

                    // delete container created
                    for container_id in &created_containers {
                        let delete_args = Delete {
                            container_id: container_id.clone(),
                            force: true,
                        };
                        let root_path = rootpath::determine(None, &*create_syscall())?;
                        if let Err(delete_err) = delete(delete_args, root_path.clone()) {
                            error!(
                                "Failed to delete container {} during rollback: {}",
                                container_id, delete_err
                            );
                        } else {
                            info!("Container deleted during rollback: {}", container_id);
                        }
                    }

                    // stop pause
                    let stop_request = StopPodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(stop_err) = self.stop_pod_sandbox(stop_request) {
                        error!(
                            "Failed to stop PodSandbox {} during rollback: {}",
                            pod_sandbox_id, stop_err
                        );
                    } else {
                        info!("PodSandbox stopped during rollback: {}", pod_sandbox_id);
                    }

                    // delete pause
                    let remove_request = RemovePodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(remove_err) = self.remove_pod_sandbox(remove_request) {
                        error!(
                            "Failed to remove PodSandbox {} during rollback: {}",
                            pod_sandbox_id, remove_err
                        );
                    } else {
                        info!("PodSandbox deleted during rollback: {}", pod_sandbox_id);
                    }

                    return Err(anyhow!(
                        "Failed to create container {}: {}",
                        container.name,
                        e
                    ));
                }
            }
        }

        // start all container
        for container_id in &created_containers {
            let start_request = StartContainerRequest {
                container_id: container_id.clone(),
            };
            match self.start_container(start_request) {
                Ok(_) => {
                    info!("Container started: {}", container_id);
                }
                Err(e) => {
                    error!("Failed to start container {}: {}", container_id, e);
                    for container_id in &created_containers {
                        let delete_args = Delete {
                            container_id: container_id.clone(),
                            force: true,
                        };
                        let root_path = rootpath::determine(None, &*create_syscall())?;
                        if let Err(delete_err) = delete(delete_args, root_path.clone()) {
                            error!(
                                "Failed to delete container {} during rollback: {}",
                                container_id, delete_err
                            );
                        } else {
                            info!("Container deleted during rollback: {}", container_id);
                        }
                    }

                    let stop_request = StopPodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(stop_err) = self.stop_pod_sandbox(stop_request) {
                        error!(
                            "Failed to stop PodSandbox {} during rollback: {}",
                            pod_sandbox_id, stop_err
                        );
                    } else {
                        info!("PodSandbox stopped during rollback: {}", pod_sandbox_id);
                    }

                    let remove_request = RemovePodSandboxRequest {
                        pod_sandbox_id: pod_sandbox_id.clone(),
                    };
                    if let Err(remove_err) = self.remove_pod_sandbox(remove_request) {
                        error!(
                            "Failed to remove PodSandbox {} during rollback: {}",
                            pod_sandbox_id, remove_err
                        );
                    } else {
                        info!("PodSandbox deleted during rollback: {}", pod_sandbox_id);
                    }

                    return Err(anyhow!("Failed to start container {}: {}", container_id, e));
                }
            }
        }

        Ok((pod_sandbox_id, podip))
    }
}

// TODO: when bundle is not provided, then pull the default image from remote
#[allow(unused)]
fn get_pause_bundle() -> Result<String> {
    Err(anyhow!("local bundle path is not provided"))
}

// only support limit config now.
pub fn get_linux_container_config(
    res: Option<ContainerRes>,
) -> Result<Option<LinuxContainerConfig>, anyhow::Error> {
    if let Some(limits) = res.and_then(|r| r.limits) {
        Ok(Some(LinuxContainerConfig {
            resources: Some(parse_resource(limits.cpu, limits.memory)?),
            ..Default::default()
        }))
    } else {
        Ok(None)
    }
}

/// Convert CPU resource descriptions in the form of `1` or `1000m`,
/// and memory resource descriptions in the form of `1Mi`, `1Ki`, or `1Gi` to LinuxContainerResource.
fn parse_resource(
    cpu: Option<String>,
    memory: Option<String>,
) -> Result<LinuxContainerResources, anyhow::Error> {
    let mut res = LinuxContainerResources::default();

    if let Some(c) = cpu {
        let period: i64 = 1_000_000;
        let portion: i64 = if c.ends_with("m") {
            c[..c.len() - 1]
                .parse::<i64>()
                .map_err(|e| anyhow!("Failed to parse cpu resource config: {}", e))?
                * period
                / 1000
        } else {
            (c.parse::<f64>()
                .map_err(|e| anyhow!("Failed to parse cpu resource config: {}", e))?
                * period as f64) as i64
        };
        res.cpu_period = period;
        res.cpu_quota = portion;
    }

    if let Some(m) = memory {
        let mem_result: Result<i64, _> = if m.ends_with("Gi") {
            m[..m.len() - 2]
                .parse()
                .map(|x: i64| x * 1024 * 1024 * 1024)
        } else if m.ends_with("Mi") {
            m[..m.len() - 2].parse().map(|x: i64| x * 1024 * 1024)
        } else if m.ends_with("Ki") {
            m[..m.len() - 2].parse().map(|x: i64| x * 1024)
        } else {
            return Err(anyhow!("Failed to parse memory resource config: {}", m));
        };
        let mem =
            mem_result.map_err(|e| anyhow!("Failed to parse memory resource config: {}", e))?;
        res.memory_limit_in_bytes = mem;
    }

    Ok(res)
}

/// Convert type used to describe container config to oci_spec config.
impl From<&LinuxContainerResources> for LinuxResources {
    fn from(value: &LinuxContainerResources) -> Self {
        let mut res = LinuxResourcesBuilder::default();
        if value.cpu_period != 0 {
            res = res.cpu(
                LinuxCpuBuilder::default()
                    .period(value.cpu_period as u64)
                    .quota(value.cpu_quota)
                    .build()
                    .unwrap(),
            );
        }
        if value.memory_limit_in_bytes != 0 {
            res = res.memory(
                LinuxMemoryBuilder::default()
                    .limit(value.memory_limit_in_bytes)
                    .build()
                    .unwrap(),
            );
        }
        res.build().unwrap()
    }
}

pub fn get_cni() -> Result<Libcni, anyhow::Error> {
    let plugin_dirs = vec!["/opt/cni/bin".to_string()];
    let plugin_conf_dir = Path::new("/etc/cni/net.d");

    let cni = Libcni::new(
        Some(plugin_dirs),
        Some(plugin_conf_dir.to_string_lossy().to_string()),
        None,
    );
    Ok(cni)
}

pub fn add_cap_net_admin(capabilities: &mut LinuxCapabilities) {
    let mut bounding = capabilities.bounding().clone().unwrap();
    bounding.insert(Capability::NetAdmin);
    capabilities.set_bounding(Some(bounding));

    let mut effective = capabilities.effective().clone().unwrap();
    effective.insert(Capability::NetAdmin);
    capabilities.set_effective(Some(effective));

    let mut inheritable = capabilities.inheritable().clone().unwrap();
    inheritable.insert(Capability::NetAdmin);
    capabilities.set_inheritable(Some(inheritable));

    let mut permitted = capabilities.permitted().clone().unwrap();
    permitted.insert(Capability::NetAdmin);
    capabilities.set_permitted(Some(permitted));

    let mut ambient = capabilities.ambient().clone().unwrap();
    ambient.insert(Capability::NetAdmin);
    capabilities.set_ambient(Some(ambient));
}

pub fn add_cap_net_raw(capabilities: &mut LinuxCapabilities) {
    let mut bounding = capabilities.bounding().clone().unwrap();
    bounding.insert(Capability::NetRaw);
    capabilities.set_bounding(Some(bounding));

    let mut effective = capabilities.effective().clone().unwrap();
    effective.insert(Capability::NetRaw);
    capabilities.set_effective(Some(effective));

    let mut inheritable = capabilities.inheritable().clone().unwrap();
    inheritable.insert(Capability::NetRaw);
    capabilities.set_inheritable(Some(inheritable));

    let mut permitted = capabilities.permitted().clone().unwrap();
    permitted.insert(Capability::NetRaw);
    capabilities.set_permitted(Some(permitted));

    let mut ambient = capabilities.ambient().clone().unwrap();
    ambient.insert(Capability::NetRaw);
    capabilities.set_ambient(Some(ambient));
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_resource() {
        parse_resource(None, None).unwrap();
        let res = parse_resource(Some("100m".to_string()), None);
        assert_eq!(res.unwrap().cpu_quota, 100000);
        let res = parse_resource(Some("0.2".to_string()), None);
        assert_eq!(res.unwrap().cpu_quota, 200000);
        let res = parse_resource(None, Some("1Gi".to_string())).unwrap();
        assert_eq!(res.memory_limit_in_bytes, 1024_i64 * 1024_i64 * 1024_i64);
        let res = parse_resource(None, Some("200Ki".to_string())).unwrap();
        assert_eq!(res.memory_limit_in_bytes, 200 * 1024);
        let res = parse_resource(None, Some("30Mi".to_string())).unwrap();
        assert_eq!(res.memory_limit_in_bytes, 30 * 1024 * 1024);
    }
}
