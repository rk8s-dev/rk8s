use std::{
    collections, env,
    fs::{self, File},
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Ok, Result, anyhow};
use libcontainer::container::State;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    ComposeCommand, DownArgs, UpArgs,
    cli_commands::ContainerRunner,
    rootpath,
    task::{ContainerSpec, Port},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct NetworkSpec {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeSpec {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigSpec {}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
        let path = PathBuf::from_str("/run/youki/123")?;
        if let Some(pat) = rootpath::determine(Some(path))?.to_str() {
            println!("{}", (pat));
        }
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

        // start the whole containers
        let states = self.run(spec)?;

        // store the spec info into a .json file
        self.persist_compose_state(states)?;

        println!("Project {} starts successfully", self.project_name);
        Ok(())
    }

    // persist the compose application's status to a json file
    ///{
    /// "project_name": "",
    /// "containers": [ {} {},],
    /// ""
    ///}
    fn persist_compose_state(&self, states: Vec<State>) -> Result<()> {
        let obj = json!({
            "project_name": self.project_name,
            "containers": states
        });
        let json_str = serde_json::to_string_pretty(&obj)?;

        let file_path = self.root_path.join("state.json");
        fs::create_dir_all(&self.root_path)?;
        fs::write(file_path, json_str)?;
        Ok(())
    }

    pub fn read_spec(&self, path: PathBuf) -> Result<ComposeSpec> {
        let path = path
            .to_str()
            .ok_or_else(|| anyhow!("compose.yml file is None"))?;
        let reader = File::open(path)?;
        let spec: ComposeSpec = serde_yaml::from_reader(reader).map_err(|_| {
            anyhow!("Read the compose specification failed, make sure the file is valid")
        })?;
        Ok(spec)
    }

    fn run(&self, spec: ComposeSpec) -> Result<Vec<State>> {
        let mut states: Vec<State> = vec![];
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
            states.push(runner.get_container_state()?);
        }
        // return the compose application's state
        Ok(states)
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

pub fn compose_execute(command: ComposeCommand) -> Result<()> {
    let (project_name, action): (
        Option<String>,
        Box<dyn FnOnce(ComposeManager) -> Result<()>>,
    ) = match command {
        ComposeCommand::Up(up_args) => {
            let name = up_args.project_name.clone();
            (name, Box::new(move |manager| manager.up(up_args)))
        }
        ComposeCommand::Down(down_args) => {
            let name = down_args.project_name.clone();
            (name, Box::new(move |manager| manager.down(down_args)))
        }
    };

    let manager = get_manager_from_name(project_name)?;
    action(manager)
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;
    use anyhow::Result;

    use crate::cli_commands::ComposeManager;

    #[test]
    pub fn test_parse_compose_spec() -> Result<()> {
        let manager = ComposeManager::new("test_project".to_string()).unwrap();

        let path = PathBuf::from_str("/home/erasernoob/srv.yaml").unwrap();
        let spec = manager.read_spec(path).unwrap();
        let yaml_str = serde_yaml::to_string(&spec)?;
        println!("{}", yaml_str);
        Ok(())
    }
}
