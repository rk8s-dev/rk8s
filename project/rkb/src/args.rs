use crate::{exec_main, login_main, logout_main, mount_main, repo_main};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "rkb", about = "A simple container image builder")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    Build(BuildArgs),
    #[command(hide = true)]
    Mount(mount_main::MountArgs),
    #[command(hide = true)]
    Exec(exec_main::ExecArgs),
    #[command(hide = true)]
    Cleanup(exec_main::CleanupArgs),
    /// login to distribution server
    Login(login_main::LoginArgs),
    /// logout from distribution server
    Logout(logout_main::LogoutArgs),
    /// list information
    Repo(repo_main::RepoArgs),
}

#[derive(Parser, Debug)]
pub struct BuildArgs {
    /// Dockerfile or Containerfile
    #[arg(short, long, value_name = "FILE")]
    pub file: Option<PathBuf>,

    /// Name of the resulting image
    #[arg(short, long, value_name = "IMAGE NAME")]
    pub tag: Option<String>,

    /// Turn verbose logging on
    #[arg(short, long)]
    pub verbose: bool,

    /// Use libfuse-rs or linux mount
    #[arg(short, long)]
    pub libfuse: bool,

    /// Output directory for the image
    #[arg(short, long, value_name = "DIR")]
    pub output_dir: Option<String>,
    // TODO: Add registry info
}
