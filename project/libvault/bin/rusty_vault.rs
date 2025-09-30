//! This is the 'application' part of RustyVault, a Rust replica of Hashicorp Vault.
//! The code here will be built into a binary (with a main function which utilize the
//! `rusty_vault::cli` module to run the application).
//!
//! This document is generated for the application part of RustyVault. But we don't organize the
//! real doc here, please go to RustyVault's [documentation site]
//!
//! [documentation site]: https://www.tongsuo.net

use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use rusty_vault::cli::Cli;

fn main() -> ExitCode {
    let mut cli = Cli::parse();

    let ret = cli.run();
    if !ret.is_success() {
        Cli::command().print_long_help().unwrap();
    }

    ret.into()
}
