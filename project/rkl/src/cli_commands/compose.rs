use std::{
    collections, env,
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::{Ok, Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::{
    ComposeCommand, DownArgs, UpArgs,
    cli_commands::ContainerRunner,
    rootpath,
    task::{ContainerSpec, Port},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ComposeSpec {
    #[serde(default)]
    pub services: collections::HashMap<String, ServiceSpec>,

    #[serde(default)]
    pub volumes: Vec<VolumeSpec>,

    #[serde(default)]
    pub configs: Vec<ConfigSpec>,

    #[serde(default)]
    pub networks: Vec<NetworkSpec>,

    #[serde(default)]
    pub secrets: Vec<SecretSpec>,

    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServiceSpec {
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub networks: Vec<String>,

    #[serde(default)]
    pub command: Vec<String>,

    #[serde(default)]
    pub configs: Option<Vec<String>>,

    #[serde(default)]
    pub secrets: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkSpec {}

#[derive(Debug, Serialize, Deserialize)]
pub struct VolumeSpec {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigSpec {}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecretSpec {}

pub struct ComposeManager {
    /// the path to store the basic info of compose application
    root_path: PathBuf,
    project_name: String,
}

impl ComposeManager {
    fn new(project_name: String) -> Result<Self> {
        let root_path = rootpath::determine(None)?;

        // /root_path/compose/compose_id to store the state of current compose application
        let root_path = Path::new(&root_path).join("compose").join(&project_name);
        Ok(Self {
            root_path,
            project_name,
        })
    }

    fn down(&self, _: DownArgs) -> Result<()> {
        Ok(())
    }

    fn up(&self, args: UpArgs) -> Result<()> {
        let compose_yaml = args.compose_yaml;
        // check the project_id exists?
        if self.root_path.exists() {
            return Err(anyhow!("The project {} already exists", self.project_name));
        }

        let target_path = get_yml_path(compose_yaml)?;

        // read the yaml
        let spec = self.read_spec(target_path)?;

        // store the spec info into a .yml file

        // start the whole containers
        self.run(spec)
    }

    pub fn read_spec(&self, path: PathBuf) -> Result<ComposeSpec> {
        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("compose.yml file is None"))?;
        let reader = File::open(path)?;
        let spec: ComposeSpec = serde_yaml::from_reader(reader)
            .map_err(|err| anyhow!("Read the compose specification failed: {}", err))?;
        Ok(spec)
    }

    fn run(&self, spec: ComposeSpec) -> Result<()> {
        for (srv_name, srv) in spec.services {
            let container_ports = map_port_style(srv.ports.clone())?;

            let container_spec = ContainerSpec {
                name: srv_name,
                image: srv.image,
                ports: container_ports,
                // TODO: Here just pass the command directly not support ENTRYPOINT yet
                args: srv.command,
                resources: None,
            };

            let mut runner = ContainerRunner::from_spec(container_spec)?;
            runner.run()?;
        }
        Ok(())
    }
}

// map the compose-style port to k8s-container-style ports
// compose-style: "(host-ip) 80: (container-ip) 3000"
// k8s-container-style:
// - containerPort: 80
//   protocol: ""
//   hostPort: 0
//   hostIP: "" default is ""
fn map_port_style(ports: Vec<String>) -> Result<Vec<Port>> {
    ports
        .into_iter()
        .map(|port| {
            let parts: Vec<&str> = port.split(":").collect();
            let (host_ip, host_port, container_port) = match parts.len() {
                2 => ("", parts[0], parts[1]),
                3 => (parts[0], parts[1], parts[2]),
                _ => return Err(anyhow!("Invalid port mapping syntax in compose file")),
            };

            let host_port = host_port
                .parse::<i32>()
                .map_err(|_| anyhow!("Invalid port mapping syntax in compose file"))?;

            let container_port = container_port
                .parse::<i32>()
                .map_err(|_| anyhow!("Invalid port mapping syntax in compose file"))?;

            let host_ip = host_ip.to_string();

            Ok(Port {
                container_port,
                protocol: "".to_string(),
                host_port,
                host_ip,
            })
        })
        .collect()
}

pub fn get_yml_path(compose_yaml: Option<String>) -> Result<PathBuf> {
    let target_path = if let Some(path) = compose_yaml {
        PathBuf::from(path)
    } else {
        let cwd = env::current_dir()?;
        let yml = cwd.join("compose.yml");
        let yaml = cwd.join("compose.yaml");
        if yml.exists() {
            yml
        } else if yaml.exists() {
            yaml
        } else {
            return Err(anyhow!(
                "No compose.yml or compose.yaml file in current directory: {}",
                cwd.display()
            ));
        }
    };
    Ok(target_path)
}

pub fn get_manager_from_name(project_name: Option<String>) -> Result<ComposeManager> {
    match project_name {
        Some(name) => ComposeManager::new(name),
        None => {
            let cwd = env::current_dir()?;
            let project_name = cwd
                .file_name()
                .and_then(|os_str| os_str.to_str())
                .ok_or_else(|| anyhow!("Failed to get current directory'name"))?
                .to_string();
            return ComposeManager::new(project_name);
        }
    }
}

pub fn execute(command: ComposeCommand) -> Result<()> {
    match command {
        ComposeCommand::Up(up_args) => {
            let manager = get_manager_from_name(up_args.project_name.clone())?;
            manager.up(up_args)
        }
        ComposeCommand::Down(down_args) => {
            get_manager_from_name(down_args.project_name.clone())?.down(down_args)
        }
    }
}
