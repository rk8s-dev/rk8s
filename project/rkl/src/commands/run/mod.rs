use anyhow::Result;
use clap::Args;

use super::container::run_container_in_cluster;
use super::pod::TLSConnectionArgs;

#[derive(Args, Debug, Clone)]
pub struct RunCommand {
    #[arg(value_name = "CONTAINER_YAML")]
    pub container_yaml: String,

    #[arg(long, short = 'v')]
    pub volumes: Option<Vec<String>>,

    /// RKS control-plane address.
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

/// Create a single-container Pod through the rks control plane.
pub fn run_execute(cmd: RunCommand) -> Result<(), anyhow::Error> {
    run_container_in_cluster(&cmd.container_yaml, cmd.volumes, cmd.cluster, cmd.tls_cfg)
}
