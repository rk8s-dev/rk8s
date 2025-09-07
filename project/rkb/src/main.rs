pub mod args;
pub mod compressor;
pub mod config;
pub mod exec_main;
pub mod image_build;
pub mod mount_main;
pub mod oci_spec;
pub mod overlayfs;
pub mod registry;
pub mod run;

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
        Commands::Build(build_args) => {
            image_build::build_image(&build_args)?;
            tracing::info!("Successfully built image");
        }
        Commands::Mount(mount_args) => {
            if let Err(e) = mount_main::main(mount_args) {
                tracing::debug!("Mount failed: {e:?}");
                return Err(e);
            }
        }
        Commands::Exec(exec_args) => {
            if let Err(e) = exec_main::exec(exec_args) {
                tracing::debug!("Exec failed: {e:?}");
                return Err(e);
            }
        }
        Commands::Cleanup(cleanup_args) => {
            if let Err(e) = exec_main::cleanup(cleanup_args) {
                tracing::debug!("Cleanup failed: {e:?}");
                return Err(e);
            }
        }
    }
    Ok(())
}
