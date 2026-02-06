#![allow(dead_code)]

// For Buck2 build compatibility,
// all modules need to be declared in `main.rs` as well (refer to `lib.rs`).
mod args;
mod commands;
mod compressor;
mod config;
mod copy;
mod image;
mod login;
mod logout;
mod oci_spec;
mod overlayfs;
mod pod_task;
mod pull;
mod push;
mod repo;
mod rt;
mod run;
mod storage;
mod task;
mod utils;
use crate::args::{Cli, Commands};
use crate::commands::{compose, container, pod, volume};
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::prelude::*;

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_thread_ids(true))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Build(args) => image::build_image(&args),
        Commands::Cleanup(args) => overlayfs::cleanup(args),
        Commands::Compose(cmd) => compose::compose_execute(cmd),
        Commands::Copy(args) => copy::copy(args),
        Commands::Create(args) => container::create_container(&args.container_yaml, args.volumes),
        Commands::Delete(args) => container::delete_container(&args.container_name),
        Commands::Exec(args) => {
            let args = *args; // Unbox
            let root_path = args.root_path.clone();
            let exit_code = container::exec_container(
                crate::commands::ExecContainer {
                    container_id: args.container_id,
                    command: args.command,
                    root_path: args.root_path,
                    base: crate::commands::ExecBase {
                        console_socket: args.console_socket,
                        cwd: args.cwd,
                        env: args.env,
                        tty: args.tty,
                        user: args.user,
                        additional_gids: args.additional_gids,
                        process: args.process,
                        detach: args.detach,
                        pid_file: args.pid_file,
                        process_label: args.process_label,
                        apparmor: args.apparmor,
                        no_new_privs: args.no_new_privs,
                        cap: args.cap,
                        preserve_fds: args.preserve_fds,
                        ignore_paused: args.ignore_paused,
                        cgroup: args.cgroup,
                    },
                },
                root_path.as_ref().map(|p| p.into()),
            )?;
            std::process::exit(exit_code)
        }
        Commands::Login(args) => login::login(args),
        Commands::Logout(args) => logout::logout(args),
        Commands::Mount(args) => overlayfs::do_mount(args),
        Commands::Pod(cmd) => pod::pod_execute(cmd),
        Commands::Ps(args) => container::list_container(args.quiet, args.format),
        Commands::Pull(args) => pull::pull(args),
        Commands::Push(args) => push::push(args),
        Commands::Repo(args) => repo::repo(args),
        Commands::Run(args) => container::run_container(&args.container_yaml, args.volumes),
        Commands::Start(args) => container::start_container(&args.container_name),
        Commands::State(args) => container::state_container(&args.container_name),
        Commands::Volume(cmd) => volume::volume_execute(cmd),
        Commands::ExecInternal(args) => run::exec_internal(args),
    }
}
