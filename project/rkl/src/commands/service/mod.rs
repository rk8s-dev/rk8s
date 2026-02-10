use anyhow::{Result, anyhow};
use clap::Subcommand;
use std::env;

use crate::commands::pod::TLSConnectionArgs;

pub mod cluster;

#[derive(Subcommand)]
pub enum ServiceCommand {
    #[command(about = "Create or update a Service from a YAML file")]
    Apply {
        #[arg(value_name = "SVC_YAML")]
        svc_yaml: String,

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

    #[command(about = "Create a Service from a YAML file")]
    Create {
        #[arg(value_name = "SVC_YAML")]
        svc_yaml: String,

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

    #[command(about = "Delete a Service by name")]
    Delete {
        #[arg(value_name = "SVC_NAME")]
        svc_name: String,

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

    #[command(about = "Get details of a specific Service")]
    Get {
        #[arg(value_name = "SVC_NAME")]
        svc_name: String,

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

    #[command(about = "List all Services")]
    List {
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
}

pub fn service_execute(cmd: ServiceCommand) -> Result<()> {
    match cmd {
        ServiceCommand::Apply {
            svc_yaml,
            cluster,
            tls_cfg,
        } => service_apply(&svc_yaml, cluster, tls_cfg),
        ServiceCommand::Create {
            svc_yaml,
            cluster,
            tls_cfg,
        } => service_create(&svc_yaml, cluster, tls_cfg),
        ServiceCommand::Delete {
            svc_name,
            cluster,
            tls_cfg,
        } => service_delete(&svc_name, cluster, tls_cfg),
        ServiceCommand::Get {
            svc_name,
            cluster,
            tls_cfg,
        } => service_get(&svc_name, cluster, tls_cfg),
        ServiceCommand::List { cluster, tls_cfg } => service_list(cluster, tls_cfg),
    }
}

fn service_apply(svc_yaml: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::apply_service(svc_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn service_create(svc_yaml: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::create_service(svc_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn service_delete(svc_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::delete_service(svc_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn service_get(svc_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::get_service(svc_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn service_list(addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::list_services(&rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}
