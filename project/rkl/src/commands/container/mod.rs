use crate::commands::{
    ExecContainer,
    pod::{TLSConnectionArgs, cluster as pod_cluster, resolve_rks_addr},
};
use libruntime::volume::parse_key_val;

use anyhow::{Result, anyhow};
use clap::Subcommand;
use common::{ContainerSpec, ObjectMeta, PodSpec, PodStatus, PodTask, RestartPolicy};
use serde_yaml::Value;
use std::{collections::HashMap, fs::File, io::Read};
use tracing::warn;

#[derive(Subcommand)]
pub enum ContainerCommand {
    #[command(about = "Run a single container from a YAML file using rkl run container.yaml")]
    Run {
        #[arg(value_name = "CONTAINER_YAML")]
        container_yaml: String,

        #[arg(long, short = 'v')]
        volumes: Option<Vec<String>>,

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
    #[command(about = "Create a Container from a YAML file using rkl create container.yaml")]
    Create {
        #[arg(value_name = "CONTAINER_YAML")]
        container_yaml: String,

        #[arg(long, short = 'v', value_parser=parse_key_val)]
        volumes: Option<Vec<String>>,

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
    #[command(about = "Start a Container with a Container-name using rkl start container-name")]
    Start {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,
    },

    #[command(about = "Delete a Container with a Container-name using rkl delete container-name")]
    Delete {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,

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
    #[command(about = "Get the state of a container using rkl state container-name")]
    State {
        #[arg(value_name = "CONTAINER_NAME")]
        container_name: String,

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

    #[command(about = "List the current running container")]
    List {
        /// Only display container IDs default is false
        #[arg(long, short)]
        quiet: Option<bool>,

        /// Specify the format (default or table)
        #[arg(long, short)]
        format: Option<String>,

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

    Exec(Box<ExecContainer>),
}

fn yaml_mapping_get<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Mapping(map) => map.get(Value::String(key.to_string())),
        _ => None,
    }
}

fn yaml_string_field(value: &Value, key: &str) -> Option<String> {
    match yaml_mapping_get(value, key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn parse_container_spec_value(
    value: &Value,
    metadata_name: Option<String>,
) -> Result<ContainerSpec> {
    let mut value = value.clone();
    if let Value::Mapping(map) = &mut value {
        let name_key = Value::String("name".to_string());
        if !map.contains_key(&name_key)
            && let Some(name) = metadata_name
        {
            map.insert(name_key, Value::String(name));
        }
    }
    serde_yaml::from_value(value).map_err(|e| anyhow!("invalid container spec: {e}"))
}

fn container_spec_from_path(path: &str) -> Result<ContainerSpec> {
    let mut file =
        File::open(path).map_err(|e| anyhow!("open the container spec file failed: {e}"))?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    let value: Value =
        serde_yaml::from_str(&content).map_err(|e| anyhow!("invalid container yaml: {e}"))?;
    let metadata_name = yaml_mapping_get(&value, "metadata")
        .and_then(|metadata| yaml_string_field(metadata, "name"));

    if yaml_string_field(&value, "kind").is_some_and(|kind| kind.eq_ignore_ascii_case("Container"))
    {
        let spec_value = yaml_mapping_get(&value, "spec").unwrap_or(&value);
        return parse_container_spec_value(spec_value, metadata_name);
    }

    parse_container_spec_value(&value, metadata_name)
}

fn container_spec_to_pod_task(container: ContainerSpec) -> PodTask {
    let pod_name = container.name.clone();
    let mut labels = HashMap::new();
    labels.insert("app".to_string(), pod_name.clone());
    labels.insert("rk8s.io/source".to_string(), "rkl-container".to_string());

    let mut annotations = HashMap::new();
    annotations.insert("rk8s.io/source-kind".to_string(), "Container".to_string());

    let mut metadata = ObjectMeta {
        name: pod_name,
        labels,
        ..ObjectMeta::default()
    };
    metadata.annotations = annotations;

    PodTask {
        api_version: "v1".to_string(),
        kind: "Pod".to_string(),
        metadata,
        spec: PodSpec {
            containers: vec![container],
            restart_policy: RestartPolicy::Always,
            ..PodSpec::default()
        },
        status: PodStatus::default(),
    }
}

fn ensure_cluster_container_args(volumes: &Option<Vec<String>>) -> Result<()> {
    if volumes.as_ref().is_some_and(|volumes| !volumes.is_empty()) {
        return Err(anyhow!(
            "container volume flags are standalone-runtime options and are not supported in cluster mode; use Pod volumes in YAML"
        ));
    }
    Ok(())
}

pub fn create_container_in_cluster(
    path: &str,
    volumes: Option<Vec<String>>,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    ensure_cluster_container_args(&volumes)?;
    let container = container_spec_from_path(path)?;
    let pod_task = container_spec_to_pod_task(container);
    let rks_addr = resolve_rks_addr(addr)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(pod_cluster::create_pod_task(
        Box::new(pod_task),
        rks_addr.as_str(),
        tls_cfg,
    ))
}

pub fn run_container_in_cluster(
    path: &str,
    volumes: Option<Vec<String>>,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    create_container_in_cluster(path, volumes, addr, tls_cfg)
}

pub fn delete_container_in_cluster(
    container_name: &str,
    _addr: Option<String>,
    _tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    Err(anyhow!(
        "container '{}' is not a top-level cluster resource and cannot be deleted directly; use 'rkl delete pod POD_NAME' to delete the owning Pod",
        container_name
    ))
}

pub fn state_container_in_cluster(
    container_name: &str,
    _addr: Option<String>,
    _tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    Err(anyhow!(
        "container '{}' is not a top-level cluster resource; use 'rkl get pod POD_NAME' to inspect pod.status.containerStatuses",
        container_name
    ))
}

pub fn list_container_in_cluster(
    _quiet: Option<bool>,
    _format: Option<String>,
    _addr: Option<String>,
    _tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    Err(anyhow!(
        "containers are not listed as top-level cluster resources; use 'rkl get pods' to list Pods and inspect their container statuses"
    ))
}

pub fn container_execute(cmd: ContainerCommand) -> Result<()> {
    match cmd {
        ContainerCommand::Run {
            container_yaml,
            volumes,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl run' instead.");
            run_container_in_cluster(&container_yaml, volumes, cluster, tls_cfg)
        }
        ContainerCommand::Start { container_name } => Err(anyhow!(
            "rkl container start '{}' is not supported in cluster mode; create the workload through rks with 'rkl run' or 'rkl apply -f'",
            container_name
        )),
        ContainerCommand::State {
            container_name,
            cluster,
            tls_cfg,
        } => {
            warn!("This command is not implemented. Use 'rkl get pod POD_NAME' instead.");
            state_container_in_cluster(&container_name, cluster, tls_cfg)
        }
        ContainerCommand::Delete {
            container_name,
            cluster,
            tls_cfg,
        } => {
            warn!("This command is not implemented. Use 'rkl delete pod POD_NAME' instead.");
            delete_container_in_cluster(&container_name, cluster, tls_cfg)
        }
        ContainerCommand::Create {
            container_yaml,
            volumes,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl apply -f container.yaml' instead.");
            create_container_in_cluster(&container_yaml, volumes, cluster, tls_cfg)
        }
        ContainerCommand::List {
            quiet,
            format,
            cluster,
            tls_cfg,
        } => {
            warn!("This command is not implemented. Use 'rkl get pods' instead.");
            list_container_in_cluster(quiet, format, cluster, tls_cfg)
        }
        ContainerCommand::Exec(exec) => {
            warn!("This command has been deprecated. Use 'rkl exec' instead.");
            Err(anyhow!(
                "rkl container exec '{}' is not a cluster operation yet; use 'rkl attach' for the current rks-mediated interactive path",
                exec.container_id
            ))
        }
    }
}
