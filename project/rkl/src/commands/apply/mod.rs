use anyhow::{Result, anyhow};
use clap::Args;
use libruntime::volume::parse_key_val;
use serde::Deserialize;
use std::fs::File;

use crate::commands::container::create_container;
use crate::commands::deployment::deployment_apply;
use crate::commands::pod::{TLSConnectionArgs, pod_create};
use crate::commands::replicaset::replicaset_apply;
use crate::commands::service::service_apply;

/// Minimal struct to detect the `kind` field of any resource YAML.
#[derive(Debug, Deserialize)]
struct ResourceKindHint {
    kind: String,
}

#[derive(Args, Debug, Clone)]
pub struct ApplyCommand {
    /// File(s) containing the resource configuration to apply.
    #[arg(long, short = 'f', required = true)]
    pub filename: Vec<String>,

    /// RKS control-plane address (required for Deployment, ReplicaSet, Service and cluster-mode Pod).
    #[arg(
        long,
        value_name = "RKS_ADDRESS",
        env = "RKS_ADDRESS",
        required = false
    )]
    pub cluster: Option<String>,

    #[clap(flatten)]
    pub tls_cfg: TLSConnectionArgs,

    #[arg(long, short = 'v', value_parser=parse_key_val)]
    volumes: Option<Vec<String>>,
}

pub fn apply_execute(cmd: ApplyCommand) -> Result<(), anyhow::Error> {
    for filename in &cmd.filename {
        apply_single_file(
            filename,
            cmd.cluster.clone(),
            cmd.tls_cfg.clone(),
            cmd.volumes.clone(),
        )?;
    }
    Ok(())
}

fn apply_single_file(
    filename: &str,
    cluster: Option<String>,
    tls_cfg: TLSConnectionArgs,
    volumes: Option<Vec<String>>,
) -> Result<()> {
    // Detect the resource kind
    let hint: ResourceKindHint = {
        let f =
            File::open(filename).map_err(|e| anyhow!("Failed to open '{}': {}", filename, e))?;
        serde_yaml::from_reader(f)
            .map_err(|e| anyhow!("Failed to parse YAML '{}': {}", filename, e))?
    };

    match hint.kind.as_str() {
        // for pod and container, apply means create.
        "Pod" => pod_create(filename, cluster, tls_cfg),
        "Container" => create_container(filename, volumes),
        // for deployment/replicaset/service Apply command is already idempotent
        "Deployment" => deployment_apply(filename, cluster, tls_cfg),
        "ReplicaSet" => replicaset_apply(filename, cluster, tls_cfg),
        "Service" => service_apply(filename, cluster, tls_cfg),
        other => Err(anyhow!(
            "Unsupported resource kind '{}' in '{}'. \
            Supported kinds: Pod, Container, Deployment, ReplicaSet, Service",
            other,
            filename
        )),
    }
}
