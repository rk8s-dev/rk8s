use clap::{Args, Parser};
use derive_more::Deref;

use crate::{
    api::sys::AuthInput,
    cli::{
        command::{self, CommandExecutor},
        util,
    },
    errors::RvError,
    utils::kv_builder::KvPairParse,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Enables a new auth method. An auth method is responsible for authenticating users
or machines and assigning them policies with which they can access RustyVault.

Enable the userpass auth method at userpass/:

    $ rvault auth enable userpass

Enable the cert auth method at cert-prod/:

    $ rvault auth enable -path=cert-prod cert"#
)]
pub struct Enable {
    #[arg(next_line_help = false, value_name = "TYPE", long_help = r#"Auth method type"#)]
    method: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Command Options")]
    options: EnableOptions,
}

#[derive(Args)]
#[group(required = false, multiple = true)]
pub struct EnableOptions {
    #[arg(
        long,
        next_line_help = false,
        default_value = "",
        value_name = "string",
        long_help = r#"Place where the auth method will be accessible. This must be unique
across all auth methods. This defaults to the "type" of the auth method.
The auth method will be accessible at "/auth/<path>""#
    )]
    path: String,

    #[arg(
        long,
        next_line_help = false,
        default_value = "",
        value_name = "string",
        long_help = r#"Human-friendly description for the purpose of this auth method."#
    )]
    description: String,

    #[clap(
        long,
        next_line_help = false,
        value_name = "key=value",
        help = r#"Key-value pair provided as key=value for the mount options. This can be
specified multiple times."#
    )]
    options: Vec<String>,
}

impl CommandExecutor for Enable {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let path = util::ensure_trailing_slash(&self.options.path);
        let auth_input = AuthInput {
            logical_type: self.method.clone(),
            description: self.options.description.clone(),
            options: self.options.options.to_map(),
            ..Default::default()
        };

        match sys.enable_auth(&path, &auth_input) {
            Ok(ret) => match ret.response_status {
                200 | 204 => {
                    println!("Success! Enabled {} auth method at: {}", self.method, path);
                }
                _ => ret.print_debug_info(),
            },
            Err(e) => eprintln!("Error: {e}"),
        }
        Ok(())
    }
}
