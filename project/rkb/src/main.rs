#![allow(dead_code)]

// For Buck2 build compatibility,
// all modules need to be declared in `main.rs` as well (refer to `lib.rs`).
mod args;
mod compressor;
mod config;
mod copy;
mod image;
mod login;
mod logout;
mod oci_spec;
mod overlayfs;
mod pull;
mod push;
mod repo;
mod rt;
mod run;
mod storage;
mod task;
mod utils;

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
        Commands::Build(args) => image::build_image(&args),
        Commands::Cleanup(args) => overlayfs::cleanup(args),
        Commands::Copy(args) => copy::copy(args),
        Commands::Login(args) => login::login(args),
        Commands::Logout(args) => logout::logout(args),
        Commands::Mount(args) => overlayfs::do_mount(args),
        Commands::Pull(args) => pull::pull(args),
        Commands::Push(args) => push::push(args),
        Commands::Repo(args) => repo::repo(args),
        Commands::Run(args) => run::run(args),
    }
}
