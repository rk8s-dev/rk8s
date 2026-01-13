// src/main.rs

use clap::{Parser, Subcommand};
use rustls::crypto::CryptoProvider;

mod bundle;
mod commands;
mod cri;
mod daemon;
mod dns;
mod network;
mod oci;
mod quic;
mod rootpath;
mod task;

use commands::{
    compose::ComposeCommand, container::ContainerCommand, deployment::DeploymentCommand,
    pod::PodCommand, replicaset::ReplicaSetCommand,
};
use commands::{
    compose::compose_execute, container::container_execute, deployment::deployment_execute,
    pod::pod_execute, replicaset::replicaset_execute,
};
use tracing::error;

use crate::commands::volume::{VolumeCommand, volume_execute};

#[derive(Parser)]
#[command(name = "rkl")]
#[command(
    about = "A simple container runtime",
    long_about = None,
    override_usage = "rkl <workload> <command> [OPTIONS]",
)]
struct Cli {
    #[command(subcommand)]
    workload: Workload,
}

impl Cli {
    fn run(self) -> Result<(), anyhow::Error> {
        match self.workload {
            Workload::Pod(cmd) => pod_execute(cmd),
            Workload::Container(cmd) => container_execute(cmd),
            Workload::Compose(cmd) => compose_execute(cmd),
            Workload::Volume(cmd) => volume_execute(cmd),
            Workload::Replicaset(cmd) => replicaset_execute(cmd),
            Workload::Deployment(cmd) => deployment_execute(cmd),
        }
    }
}

#[derive(Subcommand)]
enum Workload {
    #[command(subcommand, about = "Operations related to pods", alias = "p")]
    Pod(PodCommand),

    #[command(subcommand, about = "Manage standalone containers", alias = "c")]
    Container(ContainerCommand),

    #[command(
        subcommand,
        about = "Manage multi-container apps using compose",
        alias = "C"
    )]
    Compose(ComposeCommand),

    #[command(subcommand, about = "Manage the volume type", alias = "v")]
    Volume(VolumeCommand),
    #[command(subcommand, about = "Manage ReplicaSets", alias = "rs")]
    Replicaset(ReplicaSetCommand),

    #[command(subcommand, about = "Manage Deployments", alias = "deploy")]
    Deployment(DeploymentCommand),
}

fn main() -> Result<(), anyhow::Error> {
    CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("failed to install default CryptoProvider");

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(
                "rfuse3=off"
                    .parse()
                    .expect("failed to filter [rfuse3]'s log"),
            ),
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cli = Cli::parse();
    cli.run().inspect_err(|err| error!("Failed to run: {err}"))
}
