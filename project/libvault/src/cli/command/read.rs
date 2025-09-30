use clap::Parser;
use derive_more::Deref;

use crate::{
    cli::command::{self, CommandExecutor},
    errors::RvError,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Reads data from RustyVault at the given path. This can be used to read secrets,
generate dynamic credentials, get configuration details, and more.

Read a secret from the static secrets engine:

  $ rvault read secret/my-secret

For a full list of examples and paths, please see the documentation that
corresponds to the secrets engine in use."#
)]
pub struct Read {
    #[arg(next_line_help = false, value_name = "PATH", help = r#"The path of secret."#)]
    path: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::LogicalOutputOptions,
}

impl CommandExecutor for Read {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let logical = client.logical();

        match logical.read(&self.path) {
            Ok(ret) => {
                if ret.response_status == 200 {
                    self.output.print_data(ret.response_data.as_ref().unwrap(), self.output.field.as_deref())?;
                } else if ret.response_status == 204 {
                    println!("ok");
                } else if ret.response_status == 404 {
                    println!("No value found at {}", self.path);
                    return Err(RvError::ErrRequestNoData);
                } else {
                    ret.print_debug_info();
                }
            }
            Err(e) => eprintln!("{e}"),
        }
        Ok(())
    }
}
