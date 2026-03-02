pub mod rootfs_mount;

use self::rootfs_mount::RootfsMount;
use crate::{
    commands::{
        Exec, ExecContainer,
        compose::network::{BRIDGE_CONF, CliNetworkConfig, STD_CONF_PATH},
        create, delete, exec, list, load_container, start,
        volume::parse_key_val,
    },
    config::image::CONFIG,
    pull,
};
use anyhow::{Context, Ok, Result, anyhow};
use chrono::{DateTime, Local};
use clap::Subcommand;
use common::ContainerSpec;
use json::JsonValue;
use libcontainer::syscall::syscall::create_syscall;
use libcontainer::{
    container::{Container, ContainerStatus, State, state},
    error::LibcontainerError,
};
use liboci_cli::{Create, Delete, List, Start};
use libruntime::cri::config::ContainerConfigBuilder;
use libruntime::oci;
use libruntime::rootpath;
use libruntime::utils::{
    ImageType, determine_image, sync_handle_oci_image, sync_handle_oci_image_no_copy,
};
use libruntime::volume::{VolumeManager, VolumePattern, string_to_pattern};
use libruntime::{
    cri::cri_api::{ContainerConfig, CreateContainerResponse, Mount},
    utils::ImagePuller,
};
use nix::unistd::Pid;
use oci_spec::runtime::{LinuxBuilder, ProcessBuilder, RootBuilder, Spec, get_default_namespaces};
use oci_spec::runtime::{Mount as OciMount, MountBuilder};
use std::{
    env,
    io::{self, BufWriter},
};
use std::{fmt::Write as fmtWrite, net::IpAddr};
use std::{
    fs::{self, File},
    io::Read,
    io::Write,
    path::{Path, PathBuf},
};
use tabwriter::TabWriter;
use tracing::{debug, error, info, warn};

struct RkforgeImagePuller {}

#[async_trait::async_trait]
impl ImagePuller for RkforgeImagePuller {
    async fn pull_or_get_image(&self, image_ref: &str) -> Result<(PathBuf, Vec<PathBuf>)> {
        pull::pull_or_get_image(image_ref, None::<&str>).await
    }
    fn sync_pull_or_get_image(&self, image_ref: &str) -> Result<(PathBuf, Vec<PathBuf>)> {
        pull::sync_pull_or_get_image(image_ref, None::<&str>)
    }
}

#[derive(Subcommand, Debug)]
pub enum ContainerCommand {
    #[command(about = "Run a single container from a YAML file using rkl run container.yaml")]
    Run {
        #[arg(value_name = "CONTAINER_YAML")]
        container_yaml: String,

        #[arg(long, short = 'v')]
        volumes: Option<Vec<String>>,
    },
    #[command(about = "Create a Container from a YAML file using rkl create container.yaml")]
    Create {
        #[arg(value_name = "CONTAINER_YAML")]
        container_yaml: String,

        #[arg(long, short = 'v', value_parser=parse_key_val)]
        volumes: Option<Vec<String>>,
    },
    #[command(about = "Start a Container with a Container-name using rkl start container-name")]
    Start {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },

    #[command(about = "Delete a Container with a Container-name using rkl delete container-name")]
    Delete {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },
    #[command(about = "Get the state of a container using rkl state container-name")]
    State {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },

    #[command(about = "List the current running container")]
    List {
        /// Only display container IDs default is false
        #[arg(long, short)]
        quiet: Option<bool>,

        /// Specify the format (default or table)
        #[arg(long, short)]
        format: Option<String>,
    },

    Exec(Box<ExecContainer>),
}

pub struct ContainerRunner {
    spec: ContainerSpec,
    config: Option<ContainerConfig>,
    config_builder: ContainerConfigBuilder,
    root_path: PathBuf,
    container_id: String,
    volumes: Option<Vec<String>>,
    ip: Option<IpAddr>,

    compose_assigned_ip: Option<IpAddr>,
    /// Persistent overlay rootfs mount (None when disabled)
    rootfs_mount: Option<RootfsMount>,
}

impl ContainerRunner {
    pub fn ip(&self) -> Option<IpAddr> {
        self.ip
    }
    pub fn id(&self) -> String {
        self.container_id.clone()
    }

    pub fn set_compose_assigned_ip(&mut self, ip: IpAddr) {
        self.compose_assigned_ip = Some(ip);
    }

    // for now just for compose
    pub fn from_spec(mut spec: ContainerSpec, root_path: Option<PathBuf>) -> Result<Self> {
        let container_id = spec.name.clone();

        // Choose overlay or traditional cp path based on configuration switch
        let (builder, bundle_path, rootfs_mount) = if CONFIG.use_overlay_rootfs {
            handle_image_with_overlay(&spec)?
        } else {
            let (b, bp) = handle_image_typ(&spec)?;
            (b, bp, None)
        };
        if builder.is_some() {
            spec.image = bundle_path;
        }

        Ok(ContainerRunner {
            spec,
            config: None,
            config_builder: builder.unwrap_or_default(),
            container_id,
            root_path: match root_path {
                Some(p) => p,
                None => rootpath::determine(None, &*create_syscall())?,
            },
            volumes: None,
            ip: None,
            compose_assigned_ip: None,
            rootfs_mount,
        })
    }

    pub fn from_file(spec_path: &str, volumes: Option<Vec<String>>) -> Result<Self> {
        // read the container_spec bytes
        let mut file = File::open(spec_path)
            .map_err(|e| anyhow!("open the container spec file failed: {e}"))?;
        let mut content = String::new();

        file.read_to_string(&mut content)?;

        let mut container_spec: ContainerSpec = serde_yaml::from_str(&content)?;

        // Choose overlay or traditional cp path based on configuration switch
        let (builder, bundle_path, rootfs_mount) = if CONFIG.use_overlay_rootfs {
            handle_image_with_overlay(&container_spec)?
        } else {
            let (b, bp) = handle_image_typ(&container_spec)?;
            (b, bp, None)
        };
        if builder.is_some() {
            container_spec.image = bundle_path;
        }

        let container_id = container_spec.name.clone();
        let root_path = rootpath::determine(None, &*create_syscall())?;
        Ok(ContainerRunner {
            spec: container_spec,
            config_builder: builder.unwrap_or_default(),
            root_path,
            config: None,
            container_id,
            volumes,
            ip: None,
            compose_assigned_ip: None,
            rootfs_mount,
        })
    }

    pub fn from_container_id(container_id: &str, root_path: Option<PathBuf>) -> Result<Self> {
        Ok(ContainerRunner {
            config_builder: ContainerConfigBuilder::default(),
            spec: ContainerSpec {
                name: container_id.to_string(),
                image: "".to_string(),
                ports: vec![],
                args: vec![],
                resources: None,
                liveness_probe: None,
                readiness_probe: None,
                startup_probe: None,
                security_context: None,
                env: None,
                volume_mounts: None,
                command: None,
                working_dir: None,
            },
            config: None,
            container_id: container_id.to_string(),
            root_path: match root_path {
                Some(path) => path,
                None => rootpath::determine(None, &*create_syscall())?,
            },
            volumes: None,
            ip: None,
            compose_assigned_ip: None,
            rootfs_mount: None,
        })
    }

    /// This function do following things:
    /// 1. use VolumeManager to parse volumes
    /// 2. add mount to container config
    pub fn handle_volumes(&mut self) -> Result<Vec<String>> {
        if let Some(volumes) = &self.volumes {
            let mut manager = VolumeManager::new()?;

            let parsed_pattern: Result<Vec<VolumePattern>> = volumes
                .iter()
                .map(|v| v.as_str())
                .map(string_to_pattern)
                .collect();

            let parsed_pattern = parsed_pattern?;

            let (volume_names, mounts) = manager.handle_container_volume(parsed_pattern, false)?;
            self.add_mounts(mounts);

            // set back the volume_name to runner
            self.volumes = Some(volume_names);

            Ok(vec![])
        } else {
            Ok(vec![])
        }
    }

    pub fn add_mounts(&mut self, mounts: Vec<Mount>) {
        self.config_builder.mounts(mounts);
    }

    pub fn create(&mut self) -> Result<()> {
        // create container_config
        self.build_config()?;

        let _ = self.create_container()?;
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        // create container_config
        self.build_config()?;

        let _ = self.create_container()?;

        let id = self.container_id.clone();
        // See if the container exists
        match is_container_exist(id.as_str(), &self.root_path).is_ok() {
            // exist
            true => {
                self.start_container(None)?;
                info!("Container: {id} runs successfully!");
                Ok(())
            }
            // not exist
            false => {
                // create container
                let CreateContainerResponse { container_id } = self.create_container()?;
                self.start_container(None)?;
                info!("Container: {container_id} runs successfully!");
                Ok(())
            }
        }
    }

    pub fn get_container_state(&self) -> Result<State> {
        let container = load_container(&self.root_path, &self.container_id)?;
        Ok(container.state)
    }

    pub fn get_container_id(&self) -> Result<String> {
        let container_id = self
            .config
            .as_ref()
            .and_then(|c| c.metadata.as_ref().map(|m| m.name.clone()))
            .ok_or_else(|| anyhow!("Failed to get the container id"))?;
        Ok(container_id)
    }

    pub fn build_config(&mut self) -> Result<()> {
        let _ = self.handle_volumes()?;

        let config = self
            .config_builder
            .container_spec(self.spec.clone())?
            .clone()
            .build();

        debug!("After building config: {:#?}", config);
        self.config = Some(config);
        Ok(())
    }

    pub fn create_oci_spec(&self) -> Result<Spec> {
        // validate the config
        let config = self
            .config
            .as_ref()
            .ok_or_else(|| anyhow!("Container's Config is required"))?;

        debug!("Get container config while create oci_spec: {:#?}", config);

        let mut spec = Spec::default();

        let root = if self.rootfs_mount.is_some() {
            // Overlay persistent mount mode: use merged directory
            RootBuilder::default()
                .path("merged")
                .readonly(false)
                .build()
                .unwrap_or_default()
        } else {
            // Traditional cp mode: use default rootfs
            RootBuilder::default()
                .readonly(false)
                .build()
                .unwrap_or_default()
        };
        spec.set_root(Some(root));

        // use the default namespace configuration
        let namespaces = get_default_namespaces();

        let mut linux: LinuxBuilder = LinuxBuilder::default().namespaces(namespaces);
        if let Some(x) = &config.linux
            && let Some(r) = &x.resources
        {
            linux = linux.resources(r);
        }
        let linux = linux.build()?;
        spec.set_linux(Some(linux));

        // build the process path
        let mut process = ProcessBuilder::default().cwd(&config.working_dir).build()?;
        let capabilities = oci::new_linux_capabilities_with_defaults();
        process.set_capabilities(Some(capabilities));
        process.set_terminal(Some(false));
        process.set_args(Some(config.args.clone()));
        // TODO: env
        // process.set_env(Some(config.envs));

        spec.set_process(Some(process));

        let mut mounts = convert_cri_to_oci_mounts(&config.mounts)?;
        let existing_mounts = spec.mounts().clone().unwrap_or_default();
        mounts.extend(existing_mounts);
        spec.set_mounts(Some(mounts));

        Ok(spec)
    }

    pub fn create_container(&mut self) -> Result<CreateContainerResponse> {
        let container_id = self.get_container_id()?;
        // determine if it's in the single mode

        //  create oci spec
        let oci_spec: Spec = self.create_oci_spec()?;

        debug!(
            "[container {}] created oci_spec {:?}",
            self.spec.name, oci_spec
        );

        // create a config.path at the bundle path
        let bundle_path = self.spec.image.clone();
        if bundle_path.is_empty() {
            return Err(anyhow!(
                "[container {}] Bundle path is empty",
                self.spec.name
            ));
        }
        let bundle_dir = Path::new(&bundle_path);
        if !bundle_dir.exists() {
            let current_root = env::current_dir()?;
            debug!("current root: {:?}", current_root);
            return Err(anyhow!("Bundle directory does not exist: {:?}", bundle_dir));
        }

        let config_path = format!("{bundle_path}/config.json");
        if Path::new(&config_path).exists() {
            std::fs::remove_file(&config_path).map_err(|e| {
                anyhow!(
                    "Failed to remove existing config.json in bundle path: {}",
                    e
                )
            })?;
        }
        let file = File::create(&config_path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &oci_spec)?;
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

        create(create_args, self.root_path.clone(), false)
            .map_err(|e| anyhow!("Failed to create container: {}", e))?;

        // Single container: After creating container, persistent the volume usage into container state.json
        // Compose: self.volumes parameter is passed by the cli parameter, so when this container is from compose
        // self.volumes is None, this persist logic will not execute.
        // However, to persist the compose-project's volume usage, /run/youki/<compose>/<compose-project>/metadata.json will be used.
        // TODO: wrapper this persist logic to one util function (Can not use container's origin state)

        Ok(CreateContainerResponse { container_id })
    }

    pub fn setup_container_network(&self) -> Result<JsonValue> {
        // If this container is not from compose, then setup the config file again
        if self.determine_single_status() {
            setup_network_conf()?;
        }

        // Compose: use allocated IP from subnet pool instead of 127.0.0.1
        let address = self
            .compose_assigned_ip
            .map(|ip| format!("{}/24", ip))
            .unwrap_or_else(|| "127.0.0.1/24".to_string());

        Ok(json::object! {
            "ips": json::array! [
                json::object! {
                    "address": address.clone()
                }
            ]
        })
    }

    #[allow(clippy::result_large_err)]
    pub fn load_container(&self) -> Result<Container, LibcontainerError> {
        Container::load(self.root_path.clone().join(&self.container_id))
    }

    pub fn start_container(&mut self, id: Option<String>) -> Result<()> {
        let root_path = self.root_path.clone();

        // check the current container's status, if it's stopped then delete it and create a new one
        if self.get_container_state()?.status == ContainerStatus::Stopped {
            delete_container(&self.container_id)?;
            return self.create();
        }
        // setup the network
        let setup_result = self.setup_container_network()?;
        self.retrieve_container_ip(setup_result)?;

        match id {
            None => {
                let id = self.get_container_id()?;
                start(
                    Start {
                        container_id: id.clone(),
                    },
                    root_path,
                )?;

                Ok(())
            }
            Some(id) => {
                start(
                    Start {
                        container_id: id.clone(),
                    },
                    root_path,
                )?;

                Ok(())
            }
        }
    }

    pub fn retrieve_container_ip(&mut self, setup_result: JsonValue) -> Result<()> {
        // Currently save container's ip as Ipv4 and collect the first IP(A container can have multiple IP addrs)
        let ips = setup_result["ips"].clone();
        if !ips.is_array() {
            return Err(anyhow!("CNI result missing 'ips' array"));
        };
        if ips.is_empty() {
            return Err(anyhow!(
                "CNI returned no IP addresses for container {}",
                self.container_id
            ));
        }
        let binding = ips[0]["address"].clone();
        let ip_with_cidr = binding.as_str().ok_or_else(|| anyhow!(
                "[container {}] CNI result missing valid IP address string at ['ips'][0]['address']: {binding:?}",
                self.container_id
            ))?;
        debug!(
            "[container {}] Get container's ip_with_cidr: {ip_with_cidr}",
            self.container_id
        );
        let ip_str = ip_with_cidr.split('/').next().unwrap_or(ip_with_cidr);
        let ip_addr: IpAddr = ip_str
            .parse()
            .map_err(|_| anyhow!("invalid IP address: {ip_with_cidr}"))?;
        self.ip = match ip_addr {
            IpAddr::V4(ipv4) => Some(IpAddr::V4(ipv4)),
            _ => {
                return Err(anyhow!(
                    "[container {}] only IPv4 addresses are supported, got: {ip_with_cidr}",
                    self.container_id
                ));
            }
        };
        Ok(())
    }

    // due to the compose manager reusing the container manager to run container
    // so we can determine the mode by find "compose" in the root_path
    pub fn determine_single_status(&self) -> bool {
        !self.root_path.parent().unwrap().ends_with("compose")
    }
}

fn convert_cri_to_oci_mounts(mounts: &Vec<Mount>) -> Result<Vec<OciMount>> {
    let mut oci_mounts: Vec<OciMount> = vec![];
    for mount in mounts {
        let oci_mount = MountBuilder::default()
            .typ(determine_mount_type(&mount.host_path))
            .destination(&mount.container_path)
            .source(&mount.host_path)
            .options(build_mount_options(mount))
            .build()?;
        oci_mounts.push(oci_mount);
    }
    Ok(oci_mounts)
}

fn build_mount_options(mount: &Mount) -> Vec<String> {
    let mut options = vec![];

    if mount.readonly {
        options.push("ro".to_string());
    } else {
        options.push("rw".to_string());
    }

    options.push("rbind".to_string());

    // TODO: more options
    options
}

fn determine_mount_type(host_path: &str) -> String {
    match host_path {
        "proc" => "proc".to_string(),
        "tmpfs" => "tmpfs".to_string(),
        _ => "bind".to_string(), // default is the bind
    }
}

pub fn run_container(path: &str, volumes: Option<Vec<String>>) -> Result<(), anyhow::Error> {
    // read the container_spec bytes to container_spec struct
    let mut runner = ContainerRunner::from_file(path, volumes)?;

    // create container_config
    runner.build_config()?;

    let id = runner.container_id.clone();
    // See if the container exists
    match is_container_exist(id.as_str(), &runner.root_path).is_ok() {
        // exist
        true => {
            // determine if the container is running
            if runner.load_container()?.can_start() {
                runner.start_container(None)?;
                info!("Container: {id} runs successfully!");
                return Ok(());
            }
            warn!(
                "Container: {id} can not start, status: {}! Creating a new one...",
                runner.load_container()?.status()
            );
            delete_container(&id)?;
            let CreateContainerResponse { container_id } = runner.create_container()?;
            runner.start_container(None)?;
            info!("Container: {container_id} runs successfully!");
            Ok(())
        }
        // not exist
        false => {
            // create container
            let CreateContainerResponse { container_id } = runner.create_container()?;
            runner.start_container(None)?;
            info!("Container: {container_id} runs successfully!");
            Ok(())
        }
    }
}

pub fn is_container_exist(id: &str, root_path: &PathBuf) -> Result<()> {
    let _ = load_container(root_path, id)?;
    Ok(())
}

/// command state
pub fn state_container(id: &str) -> Result<()> {
    let root_path = rootpath::determine(None, &*create_syscall())?;
    debug!("ROOT PATH: {}", root_path.to_str().unwrap_or_default());
    print_status(id.to_owned(), root_path)
}

/// command delete
pub fn delete_container(id: &str) -> Result<()> {
    let delete_start = std::time::Instant::now();
    let root_path = rootpath::determine(None, &*create_syscall())?;
    is_container_exist(id, &root_path)?;

    // Get bundle_path before delete (container state will be cleaned up after delete)
    let container = load_container(&root_path, id)?;
    let bundle_path = container.bundle().to_path_buf();
    let pid = container
        .pid()
        .ok_or(anyhow!("invalid container {} can't find pid", id))?;
    remove_container_network(pid)?;

    let delete_args = Delete {
        container_id: id.to_string(),
        force: true,
    };
    let t0 = std::time::Instant::now();
    delete(delete_args, root_path)?;
    debug!("youki delete took {:?}", t0.elapsed());

    // Stop overlay rootfs mount
    if let Some(rootfs_mount) = RootfsMount::load(&bundle_path)?
        && let Err(e) = rootfs_mount.stop()
    {
        error!("Failed to stop rootfs overlay mount for {id}: {e}");
    }

    info!(
        "delete_container({id}) total took {:?}",
        delete_start.elapsed()
    );
    Ok(())
}

pub fn remove_container(root_path: &Path, state: &State) -> Result<()> {
    // Get bundle_path before delete
    let container = load_container(root_path, &state.id)?;
    let bundle_path = container.bundle().to_path_buf();

    let delete_args = Delete {
        container_id: state.id.clone(),
        force: true,
    };
    let pid = state
        .pid
        .ok_or(anyhow!("failed to get pid of container {}", &state.id))?;
    // delete the network
    remove_container_network(Pid::from_raw(pid))?;
    delete(delete_args, root_path.to_path_buf())?;

    // Stop overlay rootfs mount
    if let Some(rootfs_mount) = RootfsMount::load(&bundle_path)?
        && let Err(e) = rootfs_mount.stop()
    {
        error!("Failed to stop rootfs overlay mount for {}: {e}", &state.id);
    }

    Ok(())
}

pub fn remove_container_network(_pid: Pid) -> Result<()> {
    // TODO: Implement network removal without get_cni
    Ok(())
}
pub fn start_container(container_id: &str) -> Result<()> {
    let mut runner = ContainerRunner::from_container_id(container_id, None)?;
    runner.start_container(Some(container_id.to_string()))?;
    info!("container {container_id} start successfully");
    Ok(())
}

pub fn list_container(quiet: Option<bool>, format: Option<String>) -> Result<()> {
    list(
        List {
            format: format.unwrap_or("default".to_owned()),
            quiet: quiet.unwrap_or(false),
        },
        rootpath::determine(None, &*create_syscall())?,
    )?;
    Ok(())
}

pub fn exec_container(args: ExecContainer, root_path: Option<PathBuf>) -> Result<i32> {
    let args = Exec::from(args);

    let exit_code = exec(
        args,
        match root_path {
            Some(path) => path,
            None => rootpath::determine(None, &*create_syscall())?,
        },
    )?;
    Ok(exit_code)
}

pub fn create_container(path: &str, volumes: Option<Vec<String>>) -> Result<()> {
    let mut runner = ContainerRunner::from_file(path, volumes)?;
    runner.create()
}

pub fn print_status(container_id: String, root_path: PathBuf) -> Result<()> {
    let root_path = fs::canonicalize(root_path)?;
    let container_root = root_path.join(container_id);
    let mut content = String::new();

    let state_file = state::State::file_path(&container_root);
    if !state_file.exists() {
        return Err(anyhow!(
            "Broken container_file: no container state find in {}",
            container_root.to_str().unwrap()
        ));
    }
    let container = Container::load(container_root)?;
    let pid = if let Some(pid) = container.pid() {
        pid.to_string()
    } else {
        "".to_owned()
    };

    let creator = container.creator().unwrap_or_default();
    let created = if let Some(utc) = container.created() {
        let local: DateTime<Local> = DateTime::from(utc);
        local.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
    } else {
        "".to_owned()
    };
    let _ = writeln!(
        content,
        "{}\t{}\t{}\t{}\t{}\t{}",
        container.id(),
        pid,
        container.status(),
        container.bundle().display(),
        created,
        creator.to_string_lossy()
    );

    let mut tab_writer = TabWriter::new(io::stdout());
    writeln!(&mut tab_writer, "ID\tPID\tSTATUS\tBUNDLE\tCREATED\tCREATOR")?;
    write!(&mut tab_writer, "{content}")?;
    tab_writer.flush()?;

    Ok(())
}

pub fn setup_network_conf() -> Result<()> {
    let conf = CliNetworkConfig::from_name_bridge("single-net", "single0");
    let conf_value = serde_json::to_value(conf).expect("Failed to parse network config");

    let mut conf_path = PathBuf::from(STD_CONF_PATH);
    conf_path.push(BRIDGE_CONF);
    if let Some(parent) = conf_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent)?;
    }

    fs::write(conf_path, serde_json::to_string_pretty(&conf_value)?)?;

    Ok(())
}

// This function will return 2 args:
// If using image ref, return (ConfigBuilder, BundlePath)
// If This container is using local bundle path, return (None, "")
pub fn handle_image_typ(
    container_spec: &ContainerSpec,
) -> Result<(Option<ContainerConfigBuilder>, String)> {
    let puller = RkforgeImagePuller {};
    if let ImageType::OCIImage = determine_image(&container_spec.image)? {
        let (image_config, bundle_path) =
            sync_handle_oci_image(&puller, &container_spec.image, container_spec.name.clone())?;
        // handle image_config
        let mut builder = ContainerConfigBuilder::default();
        if let Some(config) = image_config.config() {
            // add cmd to config
            builder.args_from_image_config(config.entrypoint(), config.cmd());
            // extend env
            builder.envs_from_image_config(config.env());
            // set work_dir
            builder.work_dir(config.working_dir());
            // builder.users(config.user());
        }
        return Ok((Some(builder), bundle_path));
    }
    Ok((None, "".to_string()))
}

/// Handle image using persistent overlay mount, without executing cp -a.
///
/// Returns `(config_builder, bundle_path, rootfs_mount)`.
fn handle_image_with_overlay(
    container_spec: &ContainerSpec,
) -> Result<(Option<ContainerConfigBuilder>, String, Option<RootfsMount>)> {
    let puller = RkforgeImagePuller {};
    if let ImageType::OCIImage = determine_image(&container_spec.image)? {
        // Pull image, get config + layers (without executing mount+cp)
        let (image_config, bundle_path, layers) =
            sync_handle_oci_image_no_copy(&puller, &container_spec.image)?;

        // Prepare overlay directory structure
        let (lower_dirs, upper_dir, work_dir, merged_dir) = tokio::runtime::Runtime::new()
            .context("Failed to create tokio runtime")?
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
            CONFIG.use_libfuse_overlay,
        )?;

        // Build config builder
        let mut builder = ContainerConfigBuilder::default();
        if let Some(config) = image_config.config() {
            builder.args_from_image_config(config.entrypoint(), config.cmd());
            builder.envs_from_image_config(config.env());
            builder.work_dir(config.working_dir());
        }

        return Ok((Some(builder), bundle_path, Some(rootfs_mount)));
    }
    Ok((None, "".to_string(), None))
}

pub fn container_execute(cmd: ContainerCommand) -> Result<()> {
    match cmd {
        ContainerCommand::Run {
            container_yaml,
            volumes,
        } => run_container(&container_yaml, volumes),
        ContainerCommand::Start { container_name } => start_container(&container_name),
        ContainerCommand::State { container_name } => state_container(&container_name),
        ContainerCommand::Delete { container_name } => delete_container(&container_name),
        ContainerCommand::Create {
            container_yaml,
            volumes,
        } => create_container(&container_yaml, volumes),
        ContainerCommand::List { quiet, format } => list_container(quiet, format),
        ContainerCommand::Exec(exec) => {
            // root_path => default directory
            // to support enter the container created by compose-style
            let exit_code =
                exec_container((*exec).clone(), exec.root_path.as_ref().map(PathBuf::from))?;
            std::process::exit(exit_code)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_container_runner_from_spec_and_file() {
        let bundle_dir = tempdir().unwrap();
        let bundle_path = bundle_dir.path().to_string_lossy().to_string();
        let spec = ContainerSpec {
            name: "demo1".to_string(),
            image: bundle_path,
            ports: vec![],
            args: vec!["/bin/echo".to_string(), "hi".to_string()],
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
        let runner = ContainerRunner::from_spec(spec.clone(), None).unwrap();
        assert_eq!(runner.container_id, "demo1");
        assert_eq!(runner.spec.name, "demo1");

        // test from_file
        let dir = tempdir().unwrap();
        let yaml_path = dir.path().join("spec.yaml");
        let yaml = serde_yaml::to_string(&spec).unwrap();
        fs::write(&yaml_path, yaml).unwrap();
        let runner2 = ContainerRunner::from_file(yaml_path.to_str().unwrap(), None).unwrap();
        assert_eq!(runner2.spec.name, "demo1");
    }

    #[test]
    fn test_build_config() {
        let bundle_dir = tempdir().unwrap();
        let bundle_path = bundle_dir.path().to_string_lossy().to_string();
        let mut runner = ContainerRunner::from_spec(
            ContainerSpec {
                name: "demo2".to_string(),
                image: bundle_path,
                ports: vec![],
                args: vec![],
                resources: None,
                liveness_probe: None,
                readiness_probe: None,
                startup_probe: None,
                security_context: None,
                env: None,
                volume_mounts: None,
                command: None,
                working_dir: None,
            },
            None,
        )
        .unwrap();
        assert!(runner.build_config().is_ok());
        assert!(runner.config.is_some());
        assert_eq!(runner.get_container_id().unwrap(), "demo2");
    }

    #[test]
    fn test_is_container_exist_fail() {
        let res = is_container_exist("not_exist", &PathBuf::from("/tmp/none"));
        assert!(res.is_err());
    }

    #[test]
    fn test_list_container_default() {
        // Should not panic, even if no containers exist
        let res = list_container(None, None);
        assert!(res.is_ok() || res.is_err());
    }
}

/// Only bundle directories under this prefix are deleted on rm.
/// Bundles outside this path are left untouched with a warning.
const MANAGED_BUNDLE_ROOT: &str = "/var/lib/rkl/bundle";

fn is_managed_bundle(path: &std::path::Path) -> bool {
    is_managed_bundle_with_root(path, std::path::Path::new(MANAGED_BUNDLE_ROOT))
}

/// Testable helper that accepts a custom managed root.
fn is_managed_bundle_with_root(path: &std::path::Path, managed_root: &std::path::Path) -> bool {
    match std::fs::canonicalize(path) {
        std::result::Result::Ok(canonical) => canonical.starts_with(managed_root),
        // If canonicalization fails, treat as unmanaged for safety.
        std::result::Result::Err(_) => false,
    }
}

fn normalize_signal(signal: &str) -> String {
    let upper = signal.to_uppercase();
    if upper.starts_with("SIG") {
        upper
    } else {
        format!("SIG{}", upper)
    }
}

/// Send a signal to a container's init process (default: SIGKILL).
pub fn kill_container(id: &str, signal: &str) -> Result<()> {
    use nix::sys::signal::{self, Signal};
    use std::str::FromStr;

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let container = load_container(&root_path, id)?;

    if container.status() != ContainerStatus::Running {
        return Err(anyhow!("container {} is not running", id));
    }

    let pid = container
        .pid()
        .ok_or_else(|| anyhow!("container {} has no pid", id))?;

    let sig_str = normalize_signal(signal);
    let sig = Signal::from_str(&sig_str).map_err(|_| {
        anyhow!(
            "unknown signal '{}'; valid signals are standard POSIX names like KILL, TERM, HUP, INT (with or without 'SIG' prefix, case-insensitive)",
            signal
        )
    })?;

    signal::kill(pid, sig)?;
    println!("Sent {} to container {}", sig_str, id);
    Ok(())
}

/// Gracefully stop a container: SIGTERM first, then SIGKILL after timeout.
/// timeout_secs = 0 sends SIGKILL immediately after the first 200 ms poll;
/// use wait_container for indefinite waiting semantics.
pub fn stop_container(id: &str, timeout_secs: u64) -> Result<()> {
    use nix::sys::signal::{self, Signal};
    use std::time::{Duration, Instant};

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let container = load_container(&root_path, id)?;

    if container.status() == ContainerStatus::Stopped {
        println!("Container {} is already stopped", id);
        return Ok(());
    }

    if container.status() != ContainerStatus::Running {
        return Err(anyhow!(
            "container {} is not running (status: {})",
            id,
            container.status()
        ));
    }

    let pid = container
        .pid()
        .ok_or_else(|| anyhow!("container {} has no pid", id))?;

    signal::kill(pid, Signal::SIGTERM)?;
    println!("Sent SIGTERM to container {}", id);

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        std::thread::sleep(Duration::from_millis(200));

        let c = load_container(&root_path, id)?;
        if c.status() == ContainerStatus::Stopped {
            println!("Container {} stopped gracefully", id);
            return Ok(());
        }

        if Instant::now() >= deadline {
            signal::kill(pid, Signal::SIGKILL)?;
            println!("Timeout reached, sent SIGKILL to container {}", id);

            // Poll /proc/<pid> until the process disappears, since waitpid only works
            // for direct children and container runtimes typically use double-fork.
            let proc_path = std::path::PathBuf::from(format!("/proc/{}", pid.as_raw()));
            for _ in 0..50 {
                if !proc_path.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }

            if proc_path.exists() {
                warn!(
                    "Process {} for container {} still exists after SIGKILL",
                    pid.as_raw(),
                    id
                );
            } else {
                println!("Container {} force-stopped", id);
            }

            return Ok(());
        }
    }
}

/// Wait for a container to stop and print its exit code.
/// Tries waitpid (non-blocking) for a real exit code if the container is a direct
/// child of rkforge; falls back to polling otherwise (libcontainer uses double-fork).
/// In the fallback case, exit code is always 0 because libcontainer does not expose it.
/// If timeout is 0, wait indefinitely; otherwise return error on timeout.
#[allow(clippy::collapsible_if)]
pub fn wait_container(id: &str, timeout_secs: u64) -> Result<()> {
    use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
    use std::time::{Duration, Instant};

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let container = load_container(&root_path, id)?;

    if container.status() == ContainerStatus::Stopped {
        println!("0");
        return Ok(());
    }

    let pid = container
        .pid()
        .ok_or_else(|| anyhow!("container {} has no pid", id))?;

    let deadline = if timeout_secs > 0 {
        Some(Instant::now() + Duration::from_secs(timeout_secs))
    } else {
        None
    };

    loop {
        // Non-blocking waitpid: get real exit code if container is a direct child,
        // without blocking so the timeout is always respected.
        match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
            std::result::Result::Ok(WaitStatus::Exited(_, code)) => {
                println!("{}", code);
                return Ok(());
            }
            std::result::Result::Ok(WaitStatus::Signaled(_, sig, _)) => {
                println!("{}", 128 + sig as i32);
                return Ok(());
            }
            _ => {}
        }

        let c = load_container(&root_path, id)?;
        if c.status() == ContainerStatus::Stopped {
            // libcontainer does not store exit code; print 0 as convention
            println!("0");
            return Ok(());
        }

        if let Some(dl) = deadline {
            if Instant::now() >= dl {
                return Err(anyhow!("timeout waiting for container {} to stop", id));
            }
        }

        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Remove a container and its metadata.
/// -f: force remove running container; -a: remove all stopped containers (with -f, also removes running).
/// Bundle directories are only deleted if they reside under MANAGED_BUNDLE_ROOT.
pub fn rm_container(id: Option<&str>, force: bool, all: bool) -> Result<()> {
    use nix::sys::signal::{self, Signal};
    use std::time::Duration;

    let force_kill = |pid: nix::unistd::Pid| {
        let _ = signal::kill(pid, Signal::SIGKILL);
        let proc_path = std::path::PathBuf::from(format!("/proc/{}", pid.as_raw()));
        for _ in 0..50 {
            if !proc_path.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if proc_path.exists() {
            warn!("Process {} still exists after SIGKILL", pid.as_raw());
        }
    };

    let remove_bundle = |bundle_path: &std::path::Path| {
        if bundle_path.exists() {
            if is_managed_bundle(bundle_path) {
                if let Err(e) = fs::remove_dir_all(bundle_path) {
                    warn!("Failed to remove bundle {}: {}", bundle_path.display(), e);
                }
            } else {
                warn!(
                    "Skipping bundle deletion for {} (not under managed root {})",
                    bundle_path.display(),
                    MANAGED_BUNDLE_ROOT
                );
            }
        }
    };

    let root_path = rootpath::determine(None, &*create_syscall())?;

    if all {
        let entries = fs::read_dir(&root_path)?;
        for entry in entries.flatten() {
            let container_id = entry.file_name().to_string_lossy().to_string();
            let container = match load_container(&root_path, &container_id) {
                std::result::Result::Ok(c) => c,
                std::result::Result::Err(e) => {
                    warn!("Skipping {}: not a valid container ({})", container_id, e);
                    continue;
                }
            };
            if container.status() == ContainerStatus::Stopped {
                let bundle_path = container.bundle().to_path_buf();
                delete_container(&container_id)?;
                remove_bundle(&bundle_path);
                println!("Removed container {}", container_id);
            } else if force {
                let bundle_path = container.bundle().to_path_buf();
                if let Some(pid) = container.pid() {
                    force_kill(pid);
                }
                delete_container(&container_id)?;
                remove_bundle(&bundle_path);
                println!("Force removed container {}", container_id);
            } else {
                println!(
                    "Skipping running container {} (use -f to force remove)",
                    container_id
                );
            }
        }
        return Ok(());
    }

    let id = id.ok_or_else(|| anyhow!("container name is required unless -a is specified"))?;
    let container = load_container(&root_path, id)?;
    let bundle_path = container.bundle().to_path_buf();

    if container.status() != ContainerStatus::Stopped {
        if force {
            if let Some(pid) = container.pid() {
                force_kill(pid);
            }
        } else {
            return Err(anyhow!(
                "container {} is running, use -f to force remove",
                id
            ));
        }
    }

    delete_container(id)?;
    remove_bundle(&bundle_path);
    println!("Removed container {}", id);
    Ok(())
}

/// RAII guard that restores terminal settings on drop, ensuring recovery even on panic.
struct TermGuard {
    orig: nix::sys::termios::Termios,
}

impl Drop for TermGuard {
    fn drop(&mut self) {
        use nix::sys::termios::{self, SetArg};
        use std::os::fd::BorrowedFd;
        // SAFETY: STDIN_FILENO (0) is a valid file descriptor that always exists for the process.
        let fd = unsafe { BorrowedFd::borrow_raw(nix::libc::STDIN_FILENO) };
        let _ = termios::tcsetattr(fd, SetArg::TCSANOW, &self.orig);
    }
}

/// Attach to a running container's stdio via /proc/<pid>/fd.
/// Forwards stdout and stderr concurrently via background threads.
/// Press Ctrl+P then Ctrl+Q to detach without stopping the container.
/// Requires an interactive TTY; returns an error in non-interactive contexts.
pub fn attach_container(id: &str) -> Result<()> {
    use anyhow::Context;
    use nix::sys::termios::{self, SetArg};
    use std::io::{Read, Write};
    use std::os::fd::BorrowedFd;

    let root_path = rootpath::determine(None, &*create_syscall())?;
    let container = load_container(&root_path, id)?;

    if container.status() != ContainerStatus::Running {
        return Err(anyhow!("container {} is not running", id));
    }

    let pid = container
        .pid()
        .ok_or_else(|| anyhow!("container {} has no pid", id))?;

    let pid_raw = pid.as_raw();

    let container_stdout = std::fs::OpenOptions::new()
        .read(true)
        .open(format!("/proc/{}/fd/1", pid_raw))
        .with_context(|| format!("Failed to open container stdout: /proc/{}/fd/1", pid_raw))?;
    let container_stderr = std::fs::OpenOptions::new()
        .read(true)
        .open(format!("/proc/{}/fd/2", pid_raw))
        .with_context(|| format!("Failed to open container stderr: /proc/{}/fd/2", pid_raw))?;
    let mut container_stdin = std::fs::OpenOptions::new()
        .write(true)
        .open(format!("/proc/{}/fd/0", pid_raw))
        .with_context(|| format!("Failed to open container stdin: /proc/{}/fd/0", pid_raw))?;

    // SAFETY: STDIN_FILENO (0) is a valid file descriptor that always exists for the process.
    let stdin_fd = unsafe { BorrowedFd::borrow_raw(nix::libc::STDIN_FILENO) };

    if unsafe { nix::libc::isatty(nix::libc::STDIN_FILENO) } == 0 {
        return Err(anyhow!(
            "attach requires an interactive TTY; stdin is not a terminal"
        ));
    }

    let original_termios = termios::tcgetattr(stdin_fd)?;
    let mut raw = original_termios.clone();
    termios::cfmakeraw(&mut raw);
    termios::tcsetattr(stdin_fd, SetArg::TCSANOW, &raw)?;

    let _guard = TermGuard {
        orig: original_termios,
    };

    print!("Attached to {}. Use Ctrl+P, Ctrl+Q to detach.\r\n", id);

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut host_stdout = std::io::stdout();
        let mut container_stdout = container_stdout;
        loop {
            match container_stdout.read(&mut buf) {
                std::result::Result::Ok(0) => break,
                std::result::Result::Err(e) => {
                    tracing::debug!("Error reading container stdout: {}", e);
                    break;
                }
                std::result::Result::Ok(n) => {
                    let _ = host_stdout.write_all(&buf[..n]);
                    let _ = host_stdout.flush();
                }
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        let mut buf = [0u8; 1024];
        let mut host_stderr = std::io::stderr();
        let mut container_stderr = container_stderr;
        loop {
            match container_stderr.read(&mut buf) {
                std::result::Result::Ok(0) => break,
                std::result::Result::Err(e) => {
                    tracing::debug!("Error reading container stderr: {}", e);
                    break;
                }
                std::result::Result::Ok(n) => {
                    let _ = host_stderr.write_all(&buf[..n]);
                    let _ = host_stderr.flush();
                }
            }
        }
    });

    // Ctrl+P (0x10) followed by Ctrl+Q (0x11) detaches.
    // Only this exact sequence is consumed; all other input is forwarded normally.
    let mut buf = [0u8; 1];
    let mut last_was_ctrl_p = false;
    let mut detached = false;
    let mut host_stdin = std::io::stdin();

    loop {
        match host_stdin.read(&mut buf) {
            std::result::Result::Ok(0) | std::result::Result::Err(_) => break,
            std::result::Result::Ok(_) => {
                let byte = buf[0];
                if last_was_ctrl_p {
                    last_was_ctrl_p = false;
                    if byte == 0x11 {
                        drop(container_stdin);
                        detached = true;
                        break;
                    }
                    // Not a detach sequence: forward the held Ctrl+P and current byte
                    if container_stdin
                        .write_all(&[0x10])
                        .and_then(|_| container_stdin.write_all(&buf))
                        .is_err()
                    {
                        break;
                    }
                    continue;
                }
                if byte == 0x10 {
                    last_was_ctrl_p = true;
                    continue;
                }
                if container_stdin.write_all(&buf).is_err() {
                    break;
                }
            }
        }
    }

    drop(_guard);

    if detached {
        println!("\nDetached from container {}.", id);
        drop(stdout_thread);
        drop(stderr_thread);
    } else {
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();
    }
    Ok(())
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn test_normalize_signal() {
        assert_eq!(normalize_signal("kill"), "SIGKILL");
        assert_eq!(normalize_signal("KILL"), "SIGKILL");
        assert_eq!(normalize_signal("sigkill"), "SIGKILL");
        assert_eq!(normalize_signal("SIGKILL"), "SIGKILL");
        assert_eq!(normalize_signal("term"), "SIGTERM");
        assert_eq!(normalize_signal("SIGTERM"), "SIGTERM");
    }

    #[test]
    fn test_rm_container_no_id_no_all() {
        let result = rm_container(None, false, false);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("container name is required")
        );
    }

    #[test]
    fn test_is_managed_bundle() {
        use std::fs;

        let tmp = tempfile::tempdir().unwrap();

        // Create a fake managed root
        let managed_root = tmp.path().join("bundle");
        fs::create_dir(&managed_root).unwrap();

        // Create a container directory under managed root
        let container_dir = managed_root.join("abc123");
        fs::create_dir(&container_dir).unwrap();

        assert!(is_managed_bundle_with_root(&container_dir, &managed_root));

        // Outside path
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();

        assert!(!is_managed_bundle_with_root(&outside, &managed_root));

        // Non-existent path
        let nonexistent = managed_root.join("does_not_exist");
        assert!(!is_managed_bundle_with_root(&nonexistent, &managed_root));
    }
}
