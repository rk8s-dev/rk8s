pub mod args;
pub mod commands;
pub mod compressor;
pub mod config;
pub mod oci_spec;
pub mod overlayfs;
pub mod registry;
pub mod rt;
pub mod run;

use crate::args::{Cli, Commands};
use anyhow::Result;
use clap::Parser;
use tracing_subscriber::prelude::*;

macro_rules! match_commands {
    ($command:expr, {
        $($variant:ident),*
        $(,)?
    }) => {
        paste::paste! {
            match $command {
                $(
                    Commands::$variant(args) => {
                        // `[$variant:lower]` will turn `stringify!($variant)` into lowercase.
                        if let Err(e) = commands::[<$variant:lower>](args) {
                            tracing::debug!("{} failed: {e:?}", stringify!($variant));
                            return Err(e);
                        }
                    }
                ),*
            }
        }
    };
}

fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_thread_ids(true))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let cli = Cli::parse();

    match_commands!(
        cli.command,
        {
            Build,
            Mount,
            Exec,
            Cleanup,
            Login,
            Logout,
            Repo,
        }
    );
    Ok(())
}
