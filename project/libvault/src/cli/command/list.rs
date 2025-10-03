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
    about = r#"Lists data from RustyVault at the given path. This can be used to list keys in a,
given secret engine.

List values under the "my-app" folder of the generic secret engine:

  $ rvault list secret/my-app/

For a full list of examples and paths, please see the documentation that corresponds
to the secret engine in use. Not all engines support listing."#
)]
pub struct List {
    #[arg(next_line_help = false, value_name = "PATH")]
    path: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for List {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let logical = client.logical();

        match logical.list(&self.path) {
            Ok(ret) => {
                if ret.response_status == 200 {
                    let value = ret.response_data.as_ref().unwrap();
                    let keys = &value["keys"];
                    if *keys == serde_json::from_str::<serde_json::Value>("[]").unwrap() {
                        println!("No value found at {}", self.path);
                        return Err(RvError::ErrRequestNoData);
                    }
                    self.output.print_keys(keys)?;
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
