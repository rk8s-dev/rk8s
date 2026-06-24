use anyhow::{Result, anyhow};
use clap::Subcommand;
use log::warn;
use std::env;

use crate::commands::pod::TLSConnectionArgs;

pub mod cluster;

#[derive(Subcommand)]
pub enum JobCommand {
    #[command(about = "Create or update a Job from a YAML file")]
    Apply {
        #[arg(value_name = "JOB_YAML")]
        job_yaml: String,

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

    #[command(about = "Create a Job from a YAML file")]
    Create {
        #[arg(value_name = "JOB_YAML")]
        job_yaml: String,

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

    #[command(about = "Delete a Job by name")]
    Delete {
        #[arg(value_name = "JOB_NAME")]
        job_name: String,

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

    #[command(about = "Get details of a specific Job")]
    Get {
        #[arg(value_name = "JOB_NAME")]
        job_name: String,

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

    #[command(about = "List all Jobs")]
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

pub fn job_execute(cmd: JobCommand) -> Result<()> {
    match cmd {
        JobCommand::Apply {
            job_yaml,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl apply -f job.yaml' instead.");
            job_apply(&job_yaml, cluster, tls_cfg)
        }
        JobCommand::Create {
            job_yaml,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl apply -f job.yaml' instead.");
            job_create(&job_yaml, cluster, tls_cfg)
        }
        JobCommand::Delete {
            job_name,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl delete job JOB_NAME' instead.");
            job_delete(&job_name, cluster, tls_cfg)
        }
        JobCommand::Get {
            job_name,
            cluster,
            tls_cfg,
        } => {
            warn!("This command has been deprecated. Use 'rkl get job JOB_NAME' instead.");
            job_get(&job_name, cluster, tls_cfg)
        }
        JobCommand::List { cluster, tls_cfg } => {
            warn!("This command has been deprecated. Use 'rkl get jobs' instead.");
            job_list(cluster, tls_cfg)
        }
    }
}

pub fn job_apply(job_yaml: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::apply_job(job_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

pub fn job_create(job_yaml: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::create_job(job_yaml, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

pub fn job_delete(job_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::delete_job(job_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

pub fn job_get(job_name: &str, addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::get_job(job_name, &rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}

pub fn job_list(addr: Option<String>, tls_cfg: TLSConnectionArgs) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match addr.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::list_jobs(&rks_addr, tls_cfg)),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}
