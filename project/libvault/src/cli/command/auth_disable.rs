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
    about = r#"Disables an existing auth method at the given PATH. The argument corresponds
to the PATH of the mount, not the TYPE!. Once the auth method is disabled its
path can no longer be used to authenticate.

All access tokens generated via the disabled auth method are immediately
revoked. This command will block until all tokens are revoked.

Disable the auth method at userpass/:

    $ rvault auth disable userpass/"#
)]
pub struct Disable {
    #[arg(
        next_line_help = false,
        value_name = "PATH",
        long_help = r#"Disables an existing auth method at the given PATH"#
    )]
    path: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,
}

impl CommandExecutor for Disable {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let path = util::ensure_trailing_slash(&util::sanitize_path(&self.path));

        match sys.disable_auth(&path) {
            Ok(ret) => match ret.response_status {
                200 | 204 | 404 => {
                    println!("Success! Disabled the auth method (if it existed) at: {path}");
                }
                _ => ret.print_debug_info(),
            },
            Err(e) => eprintln!("Error disabling auth method at {path}: {e}"),
        }
        Ok(())
    }
}
