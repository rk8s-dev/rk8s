use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rks", version, about = "RKS daemon CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the RKS daemon with config file
    Start {
        #[arg(short, long)]
        config: PathBuf,
    },
}
