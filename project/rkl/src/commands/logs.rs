use anyhow::{Result, anyhow};
use clap::Args;
use std::env;

use crate::commands::pod::TLSConnectionArgs;
use crate::commands::pod::cluster;

#[derive(Args, Debug, Clone)]
pub struct LogCommand {
    #[arg(value_name = "POD_NAME")]
    pub pod_name: String,

    #[arg(short = 'c', long, value_name = "CONTAINER")]
    pub container: Option<String>,

    #[arg(short = 'f', long)]
    pub follow: bool,

    #[arg(long, value_name = "LINES", default_value = "-1")]
    pub tail: i64,

    #[arg(long, value_name = "TIMESTAMP")]
    pub since: Option<String>,

    #[arg(long)]
    pub timestamps: bool,

    #[arg(short = 'p', long)]
    pub previous: bool,

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

pub fn logs_execute(cmd: LogCommand) -> Result<()> {
    let env_addr = env::var("RKS_ADDRESS").ok();
    let rt = tokio::runtime::Runtime::new()?;
    match cmd.cluster.or(env_addr) {
        Some(rks_addr) => rt.block_on(cluster::get_pod_logs(
            &cmd.pod_name,
            cmd.container.as_deref(),
            cmd.follow,
            cmd.tail,
            cmd.since.as_deref(),
            cmd.timestamps,
            cmd.previous,
            &rks_addr,
            cmd.tls_cfg,
        )),
        None => Err(anyhow!(
            "No RKS address provided. Set RKS_ADDRESS or use --cluster"
        )),
    }
}
