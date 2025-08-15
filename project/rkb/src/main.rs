pub mod build_arg;
pub mod chroot;
pub mod compression;
pub mod image_build;
pub mod oci_spec;
pub mod overlayfs;
pub mod registry;

use crate::build_arg::BuildArgs;
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::prelude::*;

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_thread_ids(true))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let build_args = BuildArgs::parse();
    image_build::build_image(&build_args)?;
    println!("Successfully built image");

    Ok(())
}
