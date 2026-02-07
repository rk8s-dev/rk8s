// src/main.rs

use clap::{Parser, Subcommand};
use rustls::crypto::CryptoProvider;
use std::{fs, path::PathBuf};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt};

mod commands;
mod daemon;
mod network;
mod quic;
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

const DEFAULT_RKL_LOG_DIR: &str = "/var/log/rk8s/rkl";
const LOG_PREFIX: &str = "rkl.log";
const DAEMON_LOG_PREFIX: &str = "rkl-daemon.log";

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

    let cli = Cli::parse();
    init_tracing(matches!(
        &cli.workload,
        Workload::Pod(PodCommand::Daemon { .. })
    ))?;
    cli.run().inspect_err(|err| error!("Failed to run: {err}"))
}

fn init_tracing(is_daemon: bool) -> Result<(), anyhow::Error> {
    let console_filter = EnvFilter::from_default_env().add_directive(
        "rfuse3=off"
            .parse()
            .expect("failed to filter [rfuse3]'s log"),
    );
    let console_layer = tracing_subscriber::fmt::layer()
        .with_ansi(true)
        .with_filter(console_filter);

    let log_dir = std::env::var("RKL_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_RKL_LOG_DIR));
    fs::create_dir_all(&log_dir)?;

    let file_layer = if is_daemon {
        let file_appender = tracing_appender::rolling::daily(log_dir, DAEMON_LOG_PREFIX);
        tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_writer(file_appender)
            .with_filter(LevelFilter::DEBUG)
            .boxed()
    } else {
        let file_appender = tracing_appender::rolling::daily(log_dir, LOG_PREFIX);
        tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_writer(file_appender)
            .boxed()
    };

    tracing::subscriber::set_global_default(
        Registry::default().with(console_layer).with(file_layer),
    )
    .map_err(|e| anyhow::anyhow!("setting default subscriber failed: {e}"))?;
    Ok(())
}
