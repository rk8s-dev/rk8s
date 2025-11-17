use anyhow::{Result, anyhow};
use clap::Subcommand;
use std::env;

use crate::commands::pod::TLSConnectionArgs;

pub mod cluster;

#[derive(Subcommand)]
pub enum ReplicaSetCommand {
    #[command(about = "Create or update a ReplicaSet from a YAML file")]
    Apply {
        #[arg(value_name = "RS_YAML")]
        rs_yaml: String,

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

    #[command(about = "Create a ReplicaSet from a YAML file")]
    Create {
        #[arg(value_name = "RS_YAML")]
        rs_yaml: String,

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

    #[command(about = "Delete a ReplicaSet by name")]
    Delete {
        #[arg(value_name = "RS_NAME")]
        rs_name: String,

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

    #[command(about = "Get details of a specific ReplicaSet")]
    Get {
        #[arg(value_name = "RS_NAME")]
        rs_name: String,

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

    #[command(about = "List all ReplicaSets")]
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

pub fn replicaset_execute(cmd: ReplicaSetCommand) -> Result<()> {
    match cmd {
        ReplicaSetCommand::Apply {
            rs_yaml,
            cluster,
            tls_cfg,
        } => replicaset_apply(&rs_yaml, cluster, tls_cfg),
        ReplicaSetCommand::Create {
            rs_yaml,
            cluster,
            tls_cfg,
        } => replicaset_create(&rs_yaml, cluster, tls_cfg),
        ReplicaSetCommand::Delete {
            rs_name,
            cluster,
            tls_cfg,
        } => replicaset_delete(&rs_name, cluster, tls_cfg),
        ReplicaSetCommand::Get {
            rs_name,
            cluster,
            tls_cfg,
        } => replicaset_get(&rs_name, cluster, tls_cfg),
        ReplicaSetCommand::List { cluster, tls_cfg } => replicaset_list(cluster, tls_cfg),
    }
}

fn replicaset_apply(rs_yaml: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::apply_replicaset(rs_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn replicaset_create(
    rs_yaml: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::create_replicaset(rs_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn replicaset_delete(
    rs_name: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::delete_replicaset(rs_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn replicaset_get(rs_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::get_replicaset(rs_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn replicaset_list(addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::list_replicasets(&rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}
