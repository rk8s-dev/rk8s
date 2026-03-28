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
    /// Login to a container registry via browser authentication
    Login {
        /// Auth server URL (e.g. libra.tools or http://localhost:7001)
        #[arg(default_value = "https://libra.tools")]
        server: String,
        /// RKS config file (needed to access vault)
        #[arg(short, long)]
        config: PathBuf,
        /// Skip TLS certificate verification
        #[arg(long)]
        skip_tls_verify: bool,
    },
    /// Remove stored registry credentials
    Logout {
        /// Registry host to remove (e.g. libra.tools)
        registry: String,
        /// RKS config file (needed to access vault)
        #[arg(short, long)]
        config: PathBuf,
    },
    /// Manage rks configuration
    Config {
        #[clap(subcommand)]
        sub: ConfigCommand,
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

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// List all stored configurations (registry credentials, etc.)
    List {
        /// RKS config file (needed to access vault)
        #[arg(short, long)]
        config: PathBuf,
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

impl ConfigCommand {
    pub async fn handle(&self) -> anyhow::Result<()> {
        match self {
            Self::List { config } => {
                load_config(config.to_str().unwrap())?;
                let vault = Vault::open().await?;
                let credentials = vault.list_registry_credentials().await?;
                if credentials.is_empty() {
                    println!("No configurations stored.");
                } else {
                    println!("Registry Credentials:");
                    println!("{:<40} TOKEN (masked)", "REGISTRY");
                    for cred in &credentials {
                        let masked = if cred.pat.len() > 8 {
                            format!("{}...{}", &cred.pat[..4], &cred.pat[cred.pat.len() - 4..])
                        } else {
                            "****".to_string()
                        };
                        println!("{:<40} {}", cred.registry, masked);
                    }
                }
                Ok(())
            }
        }
    }
}
