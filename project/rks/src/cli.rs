use crate::protocol::config::load_config;
use crate::vault::Vault;
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
    /// Generate something
    Gen {
        #[clap(subcommand)]
        sub: GenCommand,
    },
}

#[derive(Subcommand)]
pub enum GenCommand {
    /// Generate certificates
    Certs { config: PathBuf },
}

impl GenCommand {
    pub async fn handle(&self) -> anyhow::Result<()> {
        match self {
            Self::Certs { config } => {
                load_config(config.to_str().unwrap())?;

                let mut vault = Vault::with_file_backend()?;
                vault.generate_certs().await
            }
        }
    }
}
