use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "rks", version, about = "RKS daemon CLI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Args, Debug, Clone)]
pub struct TLSConnectionArgs {
    #[arg(long, env = "ENABLE_TLS", required = false)]
    pub enable_tls: bool,
    #[arg(long, env = "VAULT_URL", required = true)]
    pub vault_url: String,
    #[arg(long, env = "BOOTSTRAP_TOKEN", required = true)]
    pub bootstrap_token: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the RKS daemon with config file
    Start {
        #[arg(short, long)]
        config: PathBuf,

        #[clap(flatten)]
        tls_cfg: TLSConnectionArgs,
    },
}
