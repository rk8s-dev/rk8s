use crate::{
    commands::{
        Exec, ExecContainer,
        compose::network::{BRIDGE_CONF, CliNetworkConfig, STD_CONF_PATH},
        create, delete, exec, list, load_container, start,
        volume::parse_key_val,
    },
    pull,
};
use anyhow::{Ok, Result, anyhow};
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
use libruntime::utils::{ImageType, determine_image, sync_handle_oci_image};
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
use tracing::{debug, info, warn};

struct RkbImagePuller {}

#[async_trait::async_trait]
impl ImagePuller for RkbImagePuller {
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
}

impl ContainerRunner {
    pub fn ip(&self) -> Option<IpAddr> {
        self.ip
    }
    pub fn id(&self) -> String {
        self.container_id.clone()
    }

    // for now just for compose
    pub fn from_spec(mut spec: ContainerSpec, root_path: Option<PathBuf>) -> Result<Self> {
        let container_id = spec.name.clone();

        // image pull/push support
        let (builder, bundle_path) = handle_image_typ(&spec)?;
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
        })
    }

    pub fn from_file(spec_path: &str, volumes: Option<Vec<String>>) -> Result<Self> {
        // read the container_spec bytes
        let mut file = File::open(spec_path)
            .map_err(|e| anyhow!("open the container spec file failed: {e}"))?;
        let mut content = String::new();

        file.read_to_string(&mut content)?;

        let mut container_spec: ContainerSpec = serde_yaml::from_str(&content)?;

        // image pull/push support
        let (builder, bundle_path) = handle_image_typ(&container_spec)?;
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

        let root = RootBuilder::default()
            .readonly(false)
            .build()
            .unwrap_or_default();
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

        // TODO: Implement network setup without get_cni
        Ok(json::object! {
            "ips": json::array! [
                json::object! {
                    "address": "127.0.0.1/24"
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
    let root_path = rootpath::determine(None, &*create_syscall())?;
    is_container_exist(id, &root_path)?;
    let delete_args = Delete {
        container_id: id.to_string(),
        force: true,
    };
    // delete the network
    let container = load_container(&root_path, id)?;
    let pid = container
        .pid()
        .ok_or(anyhow!("invalid container {} can't find pid", id))?;
    remove_container_network(pid)?;

    delete(delete_args, root_path)?;

    Ok(())
}

pub fn remove_container(root_path: &Path, state: &State) -> Result<()> {
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
    let puller = RkbImagePuller {};
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
        let spec = ContainerSpec {
            name: "demo1".to_string(),
            image: "/tmp/demoimg".to_string(),
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
        let mut runner = ContainerRunner::from_spec(
            ContainerSpec {
                name: "demo2".to_string(),
                image: "/tmp/demoimg2".to_string(),
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
