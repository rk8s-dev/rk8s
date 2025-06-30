use clap::{Args, Subcommand};

pub mod bundle;
pub mod cli_commands;
pub mod commands;
mod cri;
pub mod daemon;
mod rootpath;
pub mod task;

#[derive(Subcommand)]
pub enum ComposeCommand {
    // #[command(about = "Start a from a compose yaml")]
    Up(UpArgs),
    Down(DownArgs),
}

#[derive(Args)]
pub struct DownArgs {
    /// specify the compose application's name, default is the cwd
    #[arg(long = "project-name", value_name = "PROJECT_NAME")]
    project_name: Option<String>,
}

#[derive(Args)]
pub struct UpArgs {
    /// the compose.yaml's path, default under the cwd dir
    #[arg(value_name = "COMPOSE_YAML")]
    compose_yaml: Option<String>,

    /// specify the compose application's name, default is the cwd
    #[arg(long = "project-name", value_name = "PROJECT_NAME")]
    project_name: Option<String>,
}
