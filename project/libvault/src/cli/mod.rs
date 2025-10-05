//! The `libvault::cli` module is used to serve the RustyVault application.
//! This module basically accepts options from command-line and starts a server up.

use clap::{Parser, Subcommand};
use sysexits::ExitCode;

use crate::{EXIT_CODE_INSUFFICIENT_PARAMS, VERSION, cli::command::CommandExecutor};

pub mod command;
pub mod config;
pub mod kv_util;
pub mod util;

#[derive(Parser)]
#[command(
    version = VERSION,
    disable_help_subcommand = true,
    about = "A secure and high performance secret management software that is compatible with Hashicorp Vault."
)]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Server(command::server::Server),
    Status(command::status::Status),
    Operator(command::operator::Operator),
    Auth(command::auth::Auth),
    Policy(command::policy::Policy),
    Secrets(command::secrets::Secrets),
    List(command::list::List),
    Read(command::read::Read),
    Login(command::login::Login),
    Write(command::write::Write),
    Delete(command::delete::Delete),
}

impl Commands {
    pub fn execute(&mut self) -> ExitCode {
        match self {
            Commands::Server(server) => server.execute(),
            Commands::Status(status) => status.execute(),
            Commands::Operator(operator) => operator.execute(),
            Commands::Auth(auth) => auth.execute(),
            Commands::Policy(policy) => policy.execute(),
            Commands::Secrets(secrets) => secrets.execute(),
            Commands::List(list) => list.execute(),
            Commands::Read(read) => read.execute(),
            Commands::Login(login) => login.execute(),
            Commands::Write(write) => write.execute(),
            Commands::Delete(delete) => delete.execute(),
        }
    }
}

impl Cli {
    /// Do real jobs.
    #[inline]
    pub fn run(&mut self) -> ExitCode {
        if let Some(cmd) = &mut self.command {
            return cmd.execute();
        }

        EXIT_CODE_INSUFFICIENT_PARAMS
    }
}
