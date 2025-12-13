use anyhow::{Result, anyhow};
use clap::Subcommand;
use std::env;

use crate::commands::pod::TLSConnectionArgs;

pub mod cluster;

#[derive(Subcommand)]
pub enum DeploymentCommand {
    #[command(about = "Create or update a Deployment from a YAML file")]
    Apply {
        #[arg(value_name = "DEPLOY_YAML")]
        deploy_yaml: String,

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

    #[command(about = "Create a Deployment from a YAML file")]
    Create {
        #[arg(value_name = "DEPLOY_YAML")]
        deploy_yaml: String,

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

    #[command(about = "Delete a Deployment by name")]
    Delete {
        #[arg(value_name = "DEPLOY_NAME")]
        deploy_name: String,

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

    #[command(about = "Get details of a specific Deployment")]
    Get {
        #[arg(value_name = "DEPLOY_NAME")]
        deploy_name: String,

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

    #[command(about = "List all Deployments")]
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

    #[command(about = "Rollback a Deployment to a previous revision")]
    Rollback {
        #[arg(value_name = "DEPLOY_NAME")]
        deploy_name: String,

        #[arg(long, short = 'r', value_name = "REVISION", default_value = "0")]
        to_revision: i64,

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

    #[command(about = "View Deployment revision history")]
    History {
        #[arg(value_name = "DEPLOY_NAME")]
        deploy_name: String,

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

pub fn deployment_execute(cmd: DeploymentCommand) -> Result<()> {
    match cmd {
        DeploymentCommand::Apply {
            deploy_yaml,
            cluster,
            tls_cfg,
        } => deployment_apply(&deploy_yaml, cluster, tls_cfg),
        DeploymentCommand::Create {
            deploy_yaml,
            cluster,
            tls_cfg,
        } => deployment_create(&deploy_yaml, cluster, tls_cfg),
        DeploymentCommand::Delete {
            deploy_name,
            cluster,
            tls_cfg,
        } => deployment_delete(&deploy_name, cluster, tls_cfg),
        DeploymentCommand::Get {
            deploy_name,
            cluster,
            tls_cfg,
        } => deployment_get(&deploy_name, cluster, tls_cfg),
        DeploymentCommand::List { cluster, tls_cfg } => deployment_list(cluster, tls_cfg),
        DeploymentCommand::Rollback {
            deploy_name,
            to_revision,
            cluster,
            tls_cfg,
        } => deployment_rollback(&deploy_name, to_revision, cluster, tls_cfg),
        DeploymentCommand::History {
            deploy_name,
            cluster,
            tls_cfg,
        } => deployment_history(&deploy_name, cluster, tls_cfg),
    }
}

fn deployment_apply(
    deploy_yaml: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::apply_deployment(deploy_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn deployment_create(
    deploy_yaml: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::create_deployment(deploy_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn deployment_delete(
    deploy_name: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::delete_deployment(deploy_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn deployment_get(
    deploy_name: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::get_deployment(deploy_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn deployment_list(addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::list_deployments(&rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn deployment_rollback(
    deploy_name: &str,
    revision: i64,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::rollback_deployment(
            deploy_name,
            revision,
            &rks_addr,
            tls_cfg,
        )),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

fn deployment_history(
    deploy_name: &str,
    addr: Option<String>,
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::get_deployment_history(
            deploy_name,
            &rks_addr,
            tls_cfg,
        )),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}
