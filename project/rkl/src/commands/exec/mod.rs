use anyhow::{Result, anyhow};
use clap::Args;

use super::ExecBase;

#[derive(Args, Debug, Clone)]
#[command(override_usage = "\
rkl exec [OPTIONS] <TARGET> [-c <CONTAINER_NAME>] -- [COMMAND...]

TARGET can be a pod name or a container ID (auto-detected)")]
pub struct ExecCommand {
    /// Pod name or container ID
    #[arg(value_name = "TARGET")]
    pub target: String,

    #[arg(long, short = 'c', value_name = "CONTAINER_NAME")]
    pub container: Option<String>,

    #[clap(long)]
    pub root_path: Option<String>,

    #[clap(required = false)]
    pub command: Vec<String>,

    #[clap(flatten)]
    pub base: ExecBase,
}

pub fn exec_execute(cmd: ExecCommand) -> Result<(), anyhow::Error> {
    Err(anyhow!(
        "rkl exec '{}' is not a cluster operation yet; use 'rkl attach' for the current rks-mediated interactive path",
        cmd.target
    ))
}
