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
    about = r#"Disables a secrets engine at the given PATH. The argument corresponds to
the enabled PATH of the engine, not the TYPE! All secrets created by this
engine are revoked and its RustyVault data is removed.

Disable the secrets engine enabled at aws/:

    $ rvault secrets disable aws/"#
)]
pub struct Disable {
    #[arg(index = 1, next_line_help = false, value_name = "PATH", help = r#"The path of secrets."#)]
    path: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for Disable {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let path = util::ensure_trailing_slash(&util::sanitize_path(&self.path));

        match sys.unmount(&path) {
            Ok(ret) => {
                if ret.response_status == 200 || ret.response_status == 204 {
                    println!("Success! Disabled the secrets engine (if it existed) at: {}", self.path);
                } else {
                    ret.print_debug_info();
                    std::process::exit(2);
                }
            }
            Err(e) => eprintln!("{e}"),
        }

        Ok(())
    }
}
