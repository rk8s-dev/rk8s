pub mod args;
pub mod build;
pub mod compressor;
pub mod config;
pub mod exec;
pub mod login;
pub mod logout;
pub mod mount;
pub mod oci_spec;
pub mod overlayfs;
pub mod registry;
pub mod repo;
pub mod rt;
pub mod run;
pub mod utils;

use crate::args::{Cli, Commands};
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::prelude::*;

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_thread_ids(true))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Build(args) => build::build_image(&args),
        Commands::Exec(args) => exec::exec(args),
        Commands::Cleanup(args) => exec::cleanup(args),
        Commands::Mount(args) => mount::main(args),
        Commands::Login(args) => login::login(args),
        Commands::Logout(args) => logout::logout(args),
        Commands::Repo(args) => repo::repo(args),
    }
}
