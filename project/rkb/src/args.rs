use crate::{build, exec, login, logout, mount, repo};
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "rkb", about = "A simple container image builder")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Build a container image from Dockerfile
    Build(build::BuildArgs),
    #[command(hide = true)]
    Mount(mount::MountArgs),
    #[command(hide = true)]
    Exec(exec::ExecArgs),
    #[command(hide = true)]
    Cleanup(exec::CleanupArgs),
    /// Login to distribution server
    Login(login::LoginArgs),
    /// Logout from distribution server
    Logout(logout::LogoutArgs),
    /// List and manage repositories
    Repo(repo::RepoArgs),
}
