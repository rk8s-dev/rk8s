use anyhow::Result;
use clap::Args;

use crate::commands::get::{ResourceArg, ResourceType, parse_resource_type};
use crate::commands::pod::TLSConnectionArgs;
use crate::commands::{
    container::delete_container, deployment::deployment_delete, pod::pod_delete,
    replicaset::replicaset_delete, service::service_delete,
};
use tracing::warn;

#[derive(Args, Debug, Clone)]
pub struct DeleteCommand {
    /// resources type
    /// pod po pods rs,service pod/[pod-name] name
    pub resource: Vec<String>,

    #[arg(long)]
    pub all: bool,

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
}

pub fn delete_execute(cmd: DeleteCommand) -> Result<()> {
    let resource_args: Vec<ResourceArg> = parse_resource_type(&cmd.resource);

    for resource_arg in resource_args {
        if !resource_arg.resource_name.is_empty() {
            // there is specified resource name, do delete
            for name in resource_arg.resource_name {
                match resource_arg.resource_type {
                    ResourceType::Container => delete_container(&name)?,
                    ResourceType::Pod => {
                        pod_delete(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                    ResourceType::Deployment => {
                        deployment_delete(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                    ResourceType::ReplicaSet => {
                        replicaset_delete(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                    ResourceType::Service => {
                        service_delete(&name, cmd.cluster.clone(), cmd.tls_cfg.clone())?
                    }
                }
            }
        } else {
            // no specified resource name, check --all flag, do delete all
            if cmd.all {
                warn!("delete --all is not yet supported, please specify a resource name");
            } else {
                warn!("resource name or --all flag is required for delete operation");
            }
        }
    }

    Ok(())
}
