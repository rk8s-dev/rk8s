use clap::Parser;
use derive_more::Deref;

use crate::{
    cli::{
        command::{self, CommandExecutor},
        util,
    },
    errors::RvError,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Moves an existing secrets engine to a new path. Any leases from the old secrets
engine are revoked, but all configuration associated with the engine is preserved.
It initiates the migration and intermittently polls its status, exiting if a final
state is reached.

This command works within or across namespaces, both source and destination paths
can be prefixed with a namespace heirarchy relative to the current namespace.

WARNING! Moving a secrets engine will revoke any leases from the old engine.

Move the secrets engine at secret/ to generic/:

    $ rvault secrets move secret/ generic/

Move the secrets engine at ns1/secret/ across namespaces to ns2/generic/,
where ns1 and ns2 are child namespaces of the current namespace:

    $ rvault secrets move ns1/secret/ ns2/generic/
"#
)]
pub struct Move {
    #[arg(index = 1, required = true, next_line_help = false, value_name = "SOURCE", help = r#"The path of source."#)]
    source: String,

    #[arg(
        index = 2,
        required = true,
        next_line_help = false,
        value_name = "DESTINATION",
        help = r#"The path of destination."#
    )]
    destination: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,
}

impl CommandExecutor for Move {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let from = util::ensure_trailing_slash(&self.source);
        let to = util::ensure_trailing_slash(&self.destination);

        match sys.remount(&from, &to) {
            Ok(ret) => match ret.response_status {
                200 | 204 => {
                    println!("Success! Finished moving auth method {from} to {to}.");
                }
                _ => ret.print_debug_info(),
            },
            Err(e) => eprintln!("Error moving auth method {from} to {to}: {e}"),
        }
        Ok(())
    }
}
