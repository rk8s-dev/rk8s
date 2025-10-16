use crate::{copy, image, login, logout, overlayfs, pull, push, repo, run};
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
    Build(image::BuildArgs),
    #[command(hide = true)]
    Cleanup(overlayfs::CleanupArgs),
    #[command(hide = true)]
    Copy(copy::CopyArgs),
    /// Login to distribution server
    Login(login::LoginArgs),
    /// Logout from distribution server
    Logout(logout::LogoutArgs),
    #[command(hide = true)]
    Mount(overlayfs::MountArgs),
    /// Pull an image from specific distribution server.
    Pull(pull::PullArgs),
    /// Push an image to specific distribution server.
    Push(push::PushArgs),
    /// List and manage repositories
    Repo(repo::RepoArgs),
    #[command(hide = true)]
    Run(run::RunArgs),
}
