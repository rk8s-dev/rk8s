use clap::{Parser, Subcommand};
use sysexits::ExitCode;

use super::{operator_init, operator_seal, operator_unseal};
use crate::{cli::command::CommandExecutor, EXIT_CODE_INSUFFICIENT_PARAMS};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Perform operator-specific tasks",
    long_about = r#"This command groups subcommands for operators interacting with RustyVault.
Most users will not need to interact with these commands.

Initialize a new RustyVault server:

  $ rvault operator init

Unseals the RustyVault server:

  $ rvault operator unseal

Seals the RustyVault server:

  $ rvault operator seal"#
)]
pub struct Operator {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Init(operator_init::Init),
    Seal(operator_seal::Seal),
    Unseal(operator_unseal::Unseal),
}

impl Commands {
    pub fn execute(&mut self) -> ExitCode {
        match self {
            Commands::Init(init) => init.execute(),
            Commands::Seal(seal) => seal.execute(),
            Commands::Unseal(unseal) => unseal.execute(),
        }
    }
}

impl Operator {
    #[inline]
    pub fn execute(&mut self) -> ExitCode {
        if let Some(ref mut cmd) = &mut self.command {
            return cmd.execute();
        }

        EXIT_CODE_INSUFFICIENT_PARAMS
    }
}
