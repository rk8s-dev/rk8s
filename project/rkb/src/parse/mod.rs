use anyhow::{Context, Result};
use build_args::BuildArgs;

pub mod build_args;

pub fn execute(build_args: &BuildArgs) -> Result<()> {
    build_args::execute(build_args)
        .with_context(|| "Failed to build image")?;
    println!("Successfully built image");
    Ok(())
}