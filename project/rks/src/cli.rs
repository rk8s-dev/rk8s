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
    JoinToken {
        #[arg(long, default_value = "6789", required = false)]
        port: String,
    },
}

impl GenCommand {
    pub async fn handle(&self) -> anyhow::Result<()> {
        match self {
            Self::Certs { config } => {
                load_config(config.to_str().unwrap())?;

                let mut vault = Vault::with_file_backend()?;
                vault.generate_certs().await
            }
            Self::JoinToken { port } => {
                let resp = reqwest::get(format!("http://127.0.0.1:{port}/join_token")).await?;
                let body = resp.text().await?;
                println!("{body}");
                Ok(())
            }
        }
    }
}
