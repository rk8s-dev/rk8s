//! The `rusty_vault::cli` module is used to serve the RustyVault application.
//! This module basically accepts options from command-line and starts a server up.

use clap::{Parser, Subcommand};
use sysexits::ExitCode;

use crate::{cli::command::CommandExecutor, EXIT_CODE_INSUFFICIENT_PARAMS, VERSION};

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
    Read(command::read::Read),
    Write(command::write::Write),
    Delete(command::delete::Delete),
    List(command::list::List),
    Login(command::login::Login),
    Auth(command::auth::Auth),
    Policy(command::policy::Policy),
    Secrets(command::secrets::Secrets),
}

impl Commands {
    pub fn execute(&mut self) -> ExitCode {
        match self {
            Commands::Server(server) => server.execute(),
            Commands::Status(status) => status.execute(),
            Commands::Operator(operator) => operator.execute(),
            Commands::Read(read) => read.execute(),
            Commands::Write(write) => write.execute(),
            Commands::Delete(delete) => delete.execute(),
            Commands::List(list) => list.execute(),
            Commands::Login(login) => login.execute(),
            Commands::Auth(auth) => auth.execute(),
            Commands::Policy(policy) => policy.execute(),
            Commands::Secrets(secrets) => secrets.execute(),
        }
    }
}

impl Cli {
    /// Do real jobs.
    #[inline]
    pub fn run(&mut self) -> ExitCode {
        if let Some(ref mut cmd) = &mut self.command {
            return cmd.execute();
        }

        EXIT_CODE_INSUFFICIENT_PARAMS
    }
}
