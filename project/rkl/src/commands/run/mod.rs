use anyhow::Result;
use clap::Args;

use super::container::run_container;

#[derive(Args, Debug, Clone)]
pub struct RunCommand {
    #[arg(value_name = "CONTAINER_YAML")]
    pub container_yaml: String,

    #[arg(long, short = 'v')]
    pub volumes: Option<Vec<String>>,
}

/// TODO: In RunCommand, remove usage of yaml, name a container by options directly.
pub fn run_execute(cmd: RunCommand) -> Result<(), anyhow::Error> {
    run_container(&cmd.container_yaml, cmd.volumes)
}
