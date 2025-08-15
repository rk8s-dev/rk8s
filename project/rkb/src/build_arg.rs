use clap::Parser;
use std::path::PathBuf;

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
