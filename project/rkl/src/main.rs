// src/main.rs

use clap::{Parser, Subcommand};
use rustls::crypto::CryptoProvider;
use std::{fs, path::PathBuf};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::{EnvFilter, Registry, layer::SubscriberExt};

mod commands;
mod config;
mod csi;
mod daemon;
mod network;
mod quic;
mod task;

use commands::{
    apply::ApplyCommand, attach::AttachCommand, container::ContainerCommand, delete::DeleteCommand,
    deployment::DeploymentCommand, exec::ExecCommand, get::GetCommand, logs::LogCommand,
    pod::PodCommand, replicaset::ReplicaSetCommand, run::RunCommand, service::ServiceCommand,
};
use commands::{
    apply::apply_execute, attach::attach_execute, container::container_execute,
    delete::delete_execute, deployment::deployment_execute, exec::exec_execute, get::get_execute,
    logs::logs_execute, pod::pod_execute, replicaset::replicaset_execute, run::run_execute,
    service::service_execute,
};
use tracing::error;

use rkforge::overlayfs::MountArgs;

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
            Workload::Apply(cmd) => apply_execute(cmd),
            Workload::Attach(cmd) => attach_execute(cmd),
            Workload::Run(cmd) => run_execute(cmd),
            Workload::Exec(cmd) => exec_execute(cmd),
            Workload::Get(cmd) => get_execute(cmd),
            Workload::Delete(cmd) => delete_execute(cmd),
            Workload::Pod(cmd) => pod_execute(cmd),
            Workload::Container(cmd) => container_execute(cmd),
            Workload::Replicaset(cmd) => replicaset_execute(cmd),
            Workload::Deployment(cmd) => deployment_execute(cmd),
            Workload::Service(cmd) => service_execute(cmd),
            Workload::Logs(cmd) => logs_execute(cmd),
            Workload::Mount(args) => rkforge::overlayfs::do_mount(args),
        }
    }
}

#[derive(Subcommand)]
enum Workload {
    #[command(about = "Apply a configuration to a resource whether it exists or not")]
    Apply(ApplyCommand),

    #[command(about = "Attach to a process that is already running inside an existing container.")]
    Attach(AttachCommand),

    #[command(about = "Create and run a particular image in a pod")]
    Run(RunCommand),

    #[command(about = "Execute a command in a container")]
    Exec(ExecCommand),

    #[command(about = "Display one or many resources")]
    Get(GetCommand),

    #[command(
        about = "Delete resources by file names, stdin, resources and names, or by resources and label selector."
    )]
    Delete(DeleteCommand),

    #[command(
        subcommand,
        about = "(Deprecated)Operations related to pods",
        alias = "p"
    )]
    Pod(PodCommand),

    #[command(
        subcommand,
        about = "(Deprecated)Manage standalone containers",
        alias = "c"
    )]
    Container(ContainerCommand),

    #[command(subcommand, about = "(Deprecated)Manage ReplicaSets", alias = "rs")]
    Replicaset(ReplicaSetCommand),

    #[command(subcommand, about = "(Deprecated)Manage Deployments", alias = "deploy")]
    Deployment(DeploymentCommand),

    #[command(subcommand, about = "(Deprecated)Manage Services", alias = "svc")]
    Service(ServiceCommand),

    #[command(about = "Get logs from a pod's container")]
    Logs(LogCommand),

    /// Internal: overlay mount daemon (hidden from help)
    #[command(hide = true)]
    Mount(MountArgs),
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
    tracing_log::LogTracer::init().map_err(|e| anyhow::anyhow!("log tracer init failed: {e}"))?;

    let console_filter = EnvFilter::from_default_env()
        .add_directive(
            "rfuse3=off"
                .parse()
                .expect("failed to filter [rfuse3]'s log"),
        )
        .add_directive(
            "netlink_packet_route=error"
                .parse()
                .expect("failed to filter [netlink_packet_route]'s log"),
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
