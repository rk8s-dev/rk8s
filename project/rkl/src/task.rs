use anyhow::{Result, anyhow};
use common::{ContainerSpec, PodTask, RegistryCredential, VolumeSourceType};
use json::JsonValue;
use libcontainer::syscall::syscall::create_syscall;
use liboci_cli::{Create, Delete, Kill, Start};
use libruntime::cri::config::ContainerConfigBuilder;
use thiserror::Error;
// use libruntime::cri::config::get_linux_container_config;
use crate::daemon::tty::{broadcast_to_attach, register_tty_local};
use libruntime::cri::cri_api::{
    ContainerConfig, CreateContainerRequest, CreateContainerResponse, Mount, PodSandboxConfig,
    PodSandboxMetadata, PortMapping, Protocol, RemovePodSandboxRequest, RemovePodSandboxResponse,
    RunPodSandboxRequest, RunPodSandboxResponse, StartContainerRequest, StartContainerResponse,
    StopPodSandboxRequest, StopPodSandboxResponse,
};
use libruntime::cri::{create, create_with_log, delete, kill, load_container, start};
use libruntime::oci::{self, OCISpecGenerator};
use libruntime::rootpath;
use libruntime::utils::{
    ImagePuller, ImageType, determine_image, handle_image_typ, sync_handle_image_typ,
    sync_handle_oci_image_no_copy,
};

use crate::config::OVERLAY_CONFIG;
use oci_spec::runtime::RootBuilder;
use rkforge::commands::container::rootfs_mount::RootfsMount;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};

use crate::network::plugin_chain;

/// Error indicating pause container is dead, Pod needs to be rebuilt
#[derive(Debug, Error)]
#[error("Pause container (PID {pid}) is dead or zombie, Pod needs rebuild")]
pub struct PauseDeadError {
    pub pid: i32,
}

/// Check if the pause container process is alive and not a zombie
fn check_pause_alive(pid: i32) -> Result<(), PauseDeadError> {
    let status_path = format!("/proc/{pid}/status");

    // Check if process exists
    let status_content = match std::fs::read_to_string(&status_path) {
        Ok(content) => content,
        Err(_) => {
            return Err(PauseDeadError { pid });
        }
    };

    // Check if process is zombie
    for line in status_content.lines() {
        if line.starts_with("State:") {
            if line.contains("Z") || line.contains("zombie") {
                return Err(PauseDeadError { pid });
            }
            break;
        }
    }

    // Also verify namespace files are accessible
    let ns_path = format!("/proc/{pid}/ns/net");
    if std::fs::read_link(&ns_path).is_err() {
        return Err(PauseDeadError { pid });
    }

    Ok(())
}

struct RkforgeImagePuller {}

fn merge_auth_config_with_registry_credentials(
    mut auth_config: rkforge::config::auth::AuthConfig,
    creds: Vec<RegistryCredential>,
) -> rkforge::config::auth::AuthConfig {
    for cred in creds {
        let entry = rkforge::config::auth::AuthEntry::new(cred.pat, cred.registry);
        auth_config
            .entries
            .retain(|existing| existing.url != entry.url);
        auth_config.entries.push(entry);
    }
    auth_config
}

impl RkforgeImagePuller {
    /// Build an [rkforge::config::auth::AuthConfig] by overlaying rks-provided
    /// credentials on top of the local rkforge config.
    fn build_auth_config(&self) -> Option<rkforge::config::auth::AuthConfig> {
        let creds = crate::config::get_all_registry_credentials();
        if creds.is_empty() {
            return None;
        }
        let local_auth_config = match rkforge::config::auth::AuthConfig::load() {
            Ok(config) => config,
            Err(err) => {
                debug!(
                    "failed to load local rkforge auth config while merging rks credentials: {err}"
                );
                rkforge::config::auth::AuthConfig::default()
            }
        };
        Some(merge_auth_config_with_registry_credentials(
            local_auth_config,
            creds,
        ))
    }
}

#[async_trait::async_trait]
impl ImagePuller for RkforgeImagePuller {
    async fn pull_or_get_image(&self, image_ref: &str) -> Result<(PathBuf, Vec<PathBuf>)> {
        if let Some(auth_config) = self.build_auth_config() {
            return rkforge::pull::pull_or_get_image_with_config(
                image_ref,
                None::<&str>,
                &auth_config,
            )
            .await;
        }
        rkforge::pull::pull_or_get_image(image_ref, None::<&str>).await
    }

    fn sync_pull_or_get_image(&self, image_ref: &str) -> Result<(PathBuf, Vec<PathBuf>)> {
        if let Some(auth_config) = self.build_auth_config() {
            return rkforge::pull::sync_pull_or_get_image_with_config(
                image_ref,
                None::<&str>,
                &auth_config,
            );
        }
        rkforge::pull::sync_pull_or_get_image(image_ref, None::<&str>)
    }
}

pub struct TaskRunner {
    pub task: PodTask,
    pub pause_pid: Option<i32>, // pid of pause container
    pub sandbox_config: Option<PodSandboxConfig>,
    /// Per-container persistent overlay rootfs mounts (keyed by container name)
    rootfs_mounts: HashMap<String, RootfsMount>,
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
            rootfs_mounts: HashMap::new(),
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
    pub async fn run_pod_sandbox(
        &mut self,
        request: RunPodSandboxRequest,
    ) -> Result<(RunPodSandboxResponse, String), anyhow::Error> {
        let config = request.config.unwrap_or_default();
        let sandbox_id = config.metadata.unwrap_or_default().name.to_string();

        // 1. Get sandbox bundle path
        let sandbox_spec = ContainerSpec {
            name: "sandbox".to_string(),
            // FIXME: SHOULD define a const variable image name
            image: "pause:3.9".to_string(),
            ports: vec![],
            args: vec![],
            tty: false,
            resources: None,
            liveness_probe: None,
            readiness_probe: None,
            startup_probe: None,
            security_context: None,
            env: None,
            volume_mounts: None,
            command: None,
            working_dir: None,
        };

        let puller = RkforgeImagePuller {};
        let (config_builder, bundle_path) = handle_image_typ(&puller, &sandbox_spec)
            .await
            .map_err(|e| anyhow!("failed to get pause container's bundle_path: {e}"))?;

        // 2. build final oci specification config.json
        let mut config = ContainerConfig::default();
        if let Some(mut config_b) = config_builder {
            config = config_b
                .container_spec(sandbox_spec.clone())?
                .clone()
                .build();
        }
        let oci_spec = oci::OCISpecGenerator::new(&config, &sandbox_spec, None)
            .generate()
            .map_err(|e| anyhow!("failed to generate sandbox pause oci spec: {e}"))?;

        let config_path = format!("{bundle_path}/config.json");
        if !Path::new(&config_path).exists() {
            let file = File::create(&config_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, &oci_spec)?;
            writer.flush()?;
        }

        let bundle_dir = PathBuf::from(&bundle_path);
        if !bundle_dir.exists() {
            return Err(anyhow!("Bundle directory does not exist"));
        }

        info!("Get sandbox {sandbox_id}'s bundle path: {bundle_path}");

        // 3. Create container use cri
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

        // 4. Start container use cri
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

        let pod_json = Self::setup_pod_network(pid_i32).await.map_err(|e| {
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
        // let podip = runner.ip().unwrap().to_string();

        info!("podip:{podip}");
        let response = RunPodSandboxResponse {
            pod_sandbox_id: sandbox_id,
        };

        Ok((response, podip))
    }

    pub fn sync_run_pod_sandbox(
        &mut self,
        request: RunPodSandboxRequest,
    ) -> Result<(RunPodSandboxResponse, String), anyhow::Error> {
        let config = request.config.unwrap_or_default();
        let sandbox_id = config.metadata.unwrap_or_default().name.to_string();

        // 1. Get sandbox bundle path
        let sandbox_spec = ContainerSpec {
            name: "sandbox".to_string(),
            // FIXME: SHOULD define a const variable image name
            image: "pause:3.9".to_string(),
            // image: "/home/harry/Documents/rk8s/project/test/bundles/pause".to_string(),
            ports: vec![],
            args: vec![],
            tty: false,
            resources: None,
            liveness_probe: None,
            readiness_probe: None,
            startup_probe: None,
            security_context: None,
            env: None,
            volume_mounts: None,
            command: None,
            working_dir: None,
        };

        let puller = RkforgeImagePuller {};
        let (config_builder, bundle_path) = sync_handle_image_typ(&puller, &sandbox_spec)
            .map_err(|e| anyhow!("failed to get pause container's bundle_path: {e}"))?;

        // 2. build final oci specification config.json
        let mut config = ContainerConfig::default();
        if let Some(mut config_b) = config_builder {
            config = config_b
                .container_spec(sandbox_spec.clone())?
                .clone()
                .build();
        }
        let oci_spec = oci::OCISpecGenerator::new(&config, &sandbox_spec, None)
            .generate()
            .map_err(|e| anyhow!("failed to generate sandbox pause oci spec: {e}"))?;

        let config_path = format!("{bundle_path}/config.json");
        if !Path::new(&config_path).exists() {
            let file = File::create(&config_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, &oci_spec)?;
            writer.flush()?;
        }

        let bundle_dir = PathBuf::from(&bundle_path);
        if !bundle_dir.exists() {
            return Err(anyhow!("Bundle directory does not exist"));
        }

        info!("Get sandbox {sandbox_id}'s bundle path: {bundle_path}");

        // 3. Create container use cri
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

        // 4. Start container use cri
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

        let pod_json = Self::sync_setup_pod_network(pid_i32).map_err(|e| {
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
        // let podip = runner.ip().unwrap().to_string();

        info!("podip:{podip}");
        let response = RunPodSandboxResponse {
            pod_sandbox_id: sandbox_id,
        };

        Ok((response, podip))
    }

    pub async fn setup_pod_network(pid: i32) -> Result<JsonValue, anyhow::Error> {
        let netns_path = format!("/proc/{pid}/ns/net");
        let id = pid.to_string();

        plugin_chain::setup_network_async(&id, &netns_path)
            .await
            .map_err(|e| anyhow!("Failed to setup pod network via library chain: {e}"))
    }

    pub fn sync_setup_pod_network(pid: i32) -> Result<JsonValue, anyhow::Error> {
        let netns_path = format!("/proc/{pid}/ns/net");
        let id = pid.to_string();

        plugin_chain::setup_network(&id, &netns_path)
            .map_err(|e| anyhow!("Failed to setup pod network via library chain: {e}"))
    }

    /// Resolve container volumeMounts into CRI Mount entries using the pod's volume definitions.
    fn resolve_volume_mounts(&self, container: &ContainerSpec) -> Vec<Mount> {
        let volume_mounts = match &container.volume_mounts {
            Some(vms) if !vms.is_empty() => vms,
            _ => return vec![],
        };

        let pod_uid = &self.task.metadata.uid;
        let mut mounts = Vec::with_capacity(volume_mounts.len());

        for vm in volume_mounts {
            let vol = match self.task.spec.volumes.iter().find(|v| v.name == vm.name) {
                Some(v) => v,
                None => {
                    error!(
                        "volumeMount '{}' references unknown volume '{}', skipping",
                        vm.mount_path, vm.name
                    );
                    continue;
                }
            };

            let host_path = match &vol.source {
                VolumeSourceType::SlayerFs { .. } => {
                    format!("/var/lib/rkl/pods/{}/volumes/{}", pod_uid, vol.name)
                }
                VolumeSourceType::HostPath { path } => path.clone(),
                VolumeSourceType::EmptyDir => {
                    let path = format!("/var/lib/rkl/pods/{}/emptydir/{}", pod_uid, vol.name);
                    if let Err(e) = std::fs::create_dir_all(&path) {
                        error!("failed to create emptyDir path '{}': {}", path, e);
                    }
                    path
                }
            };

            mounts.push(Mount {
                container_path: vm.mount_path.clone(),
                host_path,
                readonly: vm.read_only.unwrap_or(false),
                selinux_relabel: false,
                propagation: 0,
                uid_mappings: vec![],
                gid_mappings: vec![],
                recursive_read_only: false,
                image: None,
                image_sub_path: String::new(),
            });
        }

        mounts
    }

    pub fn sync_build_create_container_request(
        &mut self,
        pod_sandbox_id: &str,
        container: &ContainerSpec,
    ) -> Result<CreateContainerRequest, anyhow::Error> {
        let puller = RkforgeImagePuller {};

        let (mut config_builder, bundle_path) = if OVERLAY_CONFIG.use_overlay_rootfs {
            if let ImageType::OCIImage = determine_image(&container.image)? {
                let (image_config, bundle_path, layers) =
                    sync_handle_oci_image_no_copy(&puller, &container.image)?;

                // Prepare overlay directories
                let (lower_dirs, upper_dir, work_dir, merged_dir) = tokio::runtime::Runtime::new()
                    .map_err(|e| anyhow!("Failed to create tokio runtime: {e}"))?
                    .block_on(libruntime::bundle::prepare_overlay_dirs(
                        &bundle_path,
                        &layers,
                    ))?;

                // Start background overlay daemon
                let rootfs_mount = RootfsMount::start(
                    &lower_dirs,
                    &upper_dir,
                    &work_dir,
                    &merged_dir,
                    &PathBuf::from(&bundle_path),
                    OVERLAY_CONFIG.use_libfuse_overlay,
                )?;

                self.rootfs_mounts
                    .insert(container.name.clone(), rootfs_mount);

                let mut builder = ContainerConfigBuilder::default();
                if let Some(config) = image_config.config() {
                    builder.args_from_image_config(config.entrypoint(), config.cmd());
                    builder.envs_from_image_config(config.env());
                    builder.work_dir(config.working_dir());
                }
                (Some(builder), bundle_path)
            } else {
                (None, "".to_string())
            }
        } else {
            sync_handle_image_typ(&puller, container)?
        };

        let volume_mounts = self.resolve_volume_mounts(container);

        let config = if let Some(ref mut builder) = config_builder {
            builder.container_spec(container.clone())?;
            builder.mounts(volume_mounts);
            builder.images(bundle_path);
            builder.clone().build()
        } else {
            let mut b = ContainerConfigBuilder::default();
            b.container_spec(container.clone())?;
            b.mounts(volume_mounts);
            b.build()
        };

        Ok(CreateContainerRequest {
            pod_sandbox_id: pod_sandbox_id.to_string(),
            config: Some(config),
            sandbox_config: self.sandbox_config.clone(),
        })
    }

    pub async fn build_create_container_request(
        &mut self,
        pod_sandbox_id: &str,
        container: &ContainerSpec,
    ) -> Result<CreateContainerRequest, anyhow::Error> {
        let puller = RkforgeImagePuller {};

        let (mut config_builder, bundle_path) = if OVERLAY_CONFIG.use_overlay_rootfs {
            if let ImageType::OCIImage = determine_image(&container.image)? {
                let (image_config, bundle_path, layers) =
                    sync_handle_oci_image_no_copy(&puller, &container.image)?;

                // Prepare overlay directories
                let (lower_dirs, upper_dir, work_dir, merged_dir) =
                    libruntime::bundle::prepare_overlay_dirs(&bundle_path, &layers).await?;

                // Start background overlay daemon
                let rootfs_mount = RootfsMount::start(
                    &lower_dirs,
                    &upper_dir,
                    &work_dir,
                    &merged_dir,
                    &PathBuf::from(&bundle_path),
                    OVERLAY_CONFIG.use_libfuse_overlay,
                )?;

                self.rootfs_mounts
                    .insert(container.name.clone(), rootfs_mount);

                let mut builder = ContainerConfigBuilder::default();
                if let Some(config) = image_config.config() {
                    builder.args_from_image_config(config.entrypoint(), config.cmd());
                    builder.envs_from_image_config(config.env());
                    builder.work_dir(config.working_dir());
                }
                (Some(builder), bundle_path)
            } else {
                (None, "".to_string())
            }
        } else {
            handle_image_typ(&puller, container).await?
        };

        let volume_mounts = self.resolve_volume_mounts(container);

        let config = if let Some(ref mut builder) = config_builder {
            builder.container_spec(container.clone())?;
            builder.mounts(volume_mounts);
            builder.images(bundle_path);
            builder.clone().build()
        } else {
            let mut b = ContainerConfigBuilder::default();
            b.container_spec(container.clone())?;
            b.mounts(volume_mounts);
            b.build()
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

        // Check if pause container is alive before creating work container
        check_pause_alive(pause_pid).map_err(|e| {
            error!(
                pause_pid,
                container_id = %container_id,
                "Pause container is dead, cannot create work container"
            );
            anyhow::Error::from(e)
        })?;

        let generator = OCISpecGenerator::new(config, container_spec, Some(pause_pid));
        let mut spec = generator.generate().map_err(|e| {
            anyhow!("failed to build OCI Specification for container {container_id}: {e}")
        })?;

        // If this container uses overlay rootfs, override Root path to "merged"
        if self.rootfs_mounts.contains_key(&container_id) {
            let root = RootBuilder::default()
                .path("merged")
                .readonly(false)
                .build()
                .map_err(|e| anyhow!("failed to build root spec: {e}"))?;
            spec.set_root(Some(root));
        }

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

        // FIXME: If there is a config.json in bundle (which is unexpected in production), keep it
        // Expected behavior: the container should own it's unique bundle path
        let config_path = format!("{bundle_path}/config.json");
        if !Path::new(&config_path).exists() {
            let file = File::create(&config_path)?;
            let mut writer = BufWriter::new(file);
            serde_json::to_writer_pretty(&mut writer, &spec)?;
            writer.flush()?;
        }

        // If container requests a TTY, pass a console_socket path to create_with_log
        // so it can receive the pty master fd from libcontainer via SCM_RIGHTS.
        // Otherwise (tty=false) the original pipe-based log capture is used.
        let console_sock_path = if container_spec.tty {
            Some(PathBuf::from(format!("/run/rkl/tty/{}.sock", container_id)))
        } else {
            None
        };

        let create_args = Create {
            bundle: bundle_path.clone().into(),
            console_socket: console_sock_path.clone(),
            pid_file: None,
            no_pivot: false,
            no_new_keyring: false,
            preserve_fds: 0,
            container_id: container_id.clone(),
        };

        let root_path = rootpath::determine(None, &*create_syscall())
            .map_err(|e| anyhow!("Failed to determine root path: {}", e))?;

        // Derive the original container name (strip pod_name prefix added in from_task)
        let original_container_name = container_id
            .strip_prefix(&format!("{}-", self.task.metadata.name))
            .unwrap_or(&container_id);

        let log_path = std::path::PathBuf::from(format!(
            "/var/log/pods/{}_{}_{}/{}/0.log",
            self.task.metadata.namespace,
            self.task.metadata.name,
            self.task.metadata.uid,
            original_container_name,
        ));

        let master_owned = create_with_log(create_args, root_path.clone(), log_path.clone())
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        // If PTY mode: start the tee task in daemon space and register the master
        // fd into TTY_STORE for future attach sessions.
        if let Some(owned_fd) = master_owned {
            spawn_attach_tee(&container_id, &log_path, &owned_fd);
            use std::os::fd::IntoRawFd;
            register_tty_local(&container_id, owned_fd.into_raw_fd())
                .map_err(|e| anyhow!("Failed to register PTY master for {container_id}: {e}"))?;
        }

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

    pub async fn run(&mut self) -> Result<(String, String), anyhow::Error> {
        // run PodSandbox（Pause container）
        let pod_request = self.build_run_pod_sandbox_request();
        let config = pod_request
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("PodSandbox config is required"))?;
        self.sandbox_config = Some(config.clone());
        let (pod_response, podip) = self
            .run_pod_sandbox(pod_request)
            .await
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

        // Clone containers list to avoid borrow conflict with &mut self
        let containers = self.task.spec.containers.clone();

        // create all container
        for container in &containers {
            let create_request = self
                .build_create_container_request(&pod_sandbox_id, container)
                .await?;
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

                    // Stop all overlay rootfs mounts during rollback
                    self.stop_all_rootfs_mounts();

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

                    // Stop all overlay rootfs mounts during rollback
                    self.stop_all_rootfs_mounts();

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

    pub fn sync_run(&mut self) -> Result<(String, String), anyhow::Error> {
        // run PodSandbox（Pause container）
        let pod_request = self.build_run_pod_sandbox_request();
        let config = pod_request
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("PodSandbox config is required"))?;
        self.sandbox_config = Some(config.clone());
        let (pod_response, podip) = self
            .sync_run_pod_sandbox(pod_request)
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

        // Clone containers list to avoid borrow conflict with &mut self
        let containers = self.task.spec.containers.clone();

        // create all container
        for container in &containers {
            let create_request =
                self.sync_build_create_container_request(&pod_sandbox_id, container)?;
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

                    // Stop all overlay rootfs mounts during rollback
                    self.stop_all_rootfs_mounts();

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

                    // Stop all overlay rootfs mounts during rollback
                    self.stop_all_rootfs_mounts();

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

    /// Stop all started overlay rootfs mounts (used for rollback cleanup).
    fn stop_all_rootfs_mounts(&mut self) {
        for (name, mount) in self.rootfs_mounts.drain() {
            if let Err(e) = mount.stop() {
                error!("Failed to stop rootfs overlay mount for {name}: {e}");
            }
        }
    }
}

fn spawn_attach_tee(container_id: &str, log_path: &Path, master_owned: &std::os::fd::OwnedFd) {
    let master_for_log = match nix::unistd::dup(master_owned.as_raw_fd()) {
        Ok(fd) => fd,
        Err(e) => {
            error!(
                container_id = %container_id,
                "Failed to dup PTY master for tee task: {e}"
            );
            return;
        }
    };

    let container_id = container_id.to_string();
    let log_path = log_path.to_path_buf();

    std::thread::spawn(move || {
        let mut reader = unsafe { std::fs::File::from_raw_fd(master_for_log) };
        if let Some(parent) = log_path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            error!(
                container_id = %container_id,
                "Failed to create log directory for {:?}: {e}",
                log_path
            );
            return;
        }

        let mut log_file = match fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            Ok(file) => file,
            Err(e) => {
                error!(
                    container_id = %container_id,
                    "Failed to open PTY log file {:?}: {e}",
                    log_path
                );
                return;
            }
        };

        let mut raw_buf = [0u8; 4096];
        let mut line_buf: Vec<u8> = Vec::with_capacity(256);

        loop {
            let n = match reader.read(&mut raw_buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => n,
            };
            let chunk = &raw_buf[..n];

            broadcast_to_attach(&container_id, chunk);

            for &byte in chunk {
                if byte == b'\n' {
                    let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
                    let line = String::from_utf8_lossy(&line_buf);
                    let _ = writeln!(log_file, "{ts} stdout F {line}");
                    line_buf.clear();
                } else if byte != b'\r' {
                    line_buf.push(byte);
                }
            }
        }

        if !line_buf.is_empty() {
            let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Nanos, true);
            let line = String::from_utf8_lossy(&line_buf);
            let _ = writeln!(log_file, "{ts} stdout F {line}");
        }
    });
}

// #[cfg(test)]
// mod test {
//     use super::*;

//     #[test]
//     fn test_parse_resource() {
//         parse_resource(None, None).unwrap();
//         let res = parse_resource(Some("100m".to_string()), None);
//         assert_eq!(res.unwrap().cpu_quota, 100000);
//         let res = parse_resource(Some("0.2".to_string()), None);
//         assert_eq!(res.unwrap().cpu_quota, 200000);
//         let res = parse_resource(None, Some("1Gi".to_string())).unwrap();
//         assert_eq!(res.memory_limit_in_bytes, 1024_i64 * 1024_i64 * 1024_i64);
//         let res = parse_resource(None, Some("200Ki".to_string())).unwrap();
//         assert_eq!(res.memory_limit_in_bytes, 200 * 1024);
//         let res = parse_resource(None, Some("30Mi".to_string())).unwrap();
//         assert_eq!(res.memory_limit_in_bytes, 30 * 1024 * 1024);
//     }
// }

#[cfg(test)]
mod tests {
    use super::merge_auth_config_with_registry_credentials;
    use common::RegistryCredential;
    use rkforge::config::auth::{AuthConfig, AuthEntry};

    #[test]
    fn merge_auth_config_preserves_unmanaged_local_entries() {
        let local = AuthConfig {
            entries: vec![
                AuthEntry::new("local-token", "local.registry"),
                AuthEntry::new("other-token", "other.registry"),
            ],
            insecure_registries: vec!["insecure.local:5000".to_string()],
        };

        let merged = merge_auth_config_with_registry_credentials(
            local,
            vec![RegistryCredential {
                registry: "central.registry".to_string(),
                pat: "central-token".to_string(),
            }],
        );

        assert_eq!(merged.entries.len(), 3);
        assert!(
            merged
                .entries
                .iter()
                .any(|entry| entry.url == "local.registry" && entry.pat == "local-token")
        );
        assert!(
            merged
                .entries
                .iter()
                .any(|entry| entry.url == "central.registry" && entry.pat == "central-token")
        );
        assert_eq!(merged.insecure_registries, vec!["insecure.local:5000"]);
    }

    #[test]
    fn merge_auth_config_prefers_central_entry_on_registry_conflict() {
        let local = AuthConfig {
            entries: vec![AuthEntry::new("local-token", "shared.registry")],
            insecure_registries: Vec::new(),
        };

        let merged = merge_auth_config_with_registry_credentials(
            local,
            vec![RegistryCredential {
                registry: "shared.registry".to_string(),
                pat: "central-token".to_string(),
            }],
        );

        assert_eq!(merged.entries.len(), 1);
        assert_eq!(merged.entries[0].url, "shared.registry");
        assert_eq!(merged.entries[0].pat, "central-token");
    }
}
