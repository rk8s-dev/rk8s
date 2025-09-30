use clap::{Args, Parser};
use derive_more::Deref;

use crate::{
    api::sys::{MountConfigInput, MountInput},
    cli::{
        command::{self, CommandExecutor},
        util::ensure_trailing_slash,
    },
    errors::RvError,
    utils::{
        kv_builder::KvPairParse,
        token_util::{DEFAULT_LEASE_TTL, MAX_LEASE_TTL},
    },
};

#[derive(Parser, Deref)]
#[command(
    author,
    disable_version_flag = false,
    about = r#"Enables a secrets engine. By default, secrets engines are enabled at the path
corresponding to their TYPE, but users can customize the path using the
-path option.

Once enabled, RustyVault will route all requests which begin with the path to the
secrets engine.

Enable the AWS secrets engine at aws/:

    $ rvault secrets enable aws

Enable the SSH secrets engine at ssh-prod/:

    $ rvault secrets enable -path=ssh-prod ssh

Enable the database secrets engine with an explicit maximum TTL of 30m:

    $ rvault secrets enable -max-lease-ttl=30m database

For a full list of secrets engines and examples, please see the documentation."#
)]
pub struct Enable {
    #[arg(required = true, next_line_help = false, value_name = "TYPE", long_help = r#"engine type"#)]
    engine_type: String,

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
        value_name = "string",
        default_value = "",
        long_help = r#"A place that a secrets engine is enabled at, thus the engine can be accessed via that path.
This must be unique across all secrets engines. This defaults to the "type" of the secrets
engine."#
    )]
    path: String,

    #[arg(
        long,
        next_line_help = false,
        value_name = "string",
        default_value = "",
        long_help = r#"Human-friendly description for the purpose of this engine."#
    )]
    description: String,

    #[arg(
        long,
        next_line_help = false,
        value_name = "duration",
        long_help = r#"The default lease TTL for this secrets engine. If unspecified,
this defaults to the RustyVault server's globally configured default lease TTL."#
    )]
    default_lease_ttl: Option<humantime::Duration>,

    #[arg(
        long,
        next_line_help = false,
        value_name = "duration",
        long_help = r#"The maximum lease TTL for this secrets engine. If unspecified,
this defaults to the RustyVault server's globally configured maximum lease TTL."#
    )]
    max_lease_ttl: Option<humantime::Duration>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "int",
        default_value = "0",
        long_help = r#"Select the version of the engine to run. Not supported by all engines."#
    )]
    version: u8,

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

        let path = if self.options.path.is_empty() { &self.engine_type } else { &self.options.path };

        let mount_path = ensure_trailing_slash(path);
        let default_lease_ttl =
            if let Some(dttl) = self.options.default_lease_ttl { dttl.into() } else { DEFAULT_LEASE_TTL };
        let max_lease_ttl = if let Some(mttl) = self.options.max_lease_ttl { mttl.into() } else { MAX_LEASE_TTL };

        let mount_input = MountInput {
            logical_type: self.engine_type.clone(),
            description: self.options.description.clone(),
            config: MountConfigInput { default_lease_ttl, max_lease_ttl, ..Default::default() },
            options: self.options.options.to_map(),
        };

        match sys.mount(&mount_path, &mount_input) {
            Ok(ret) => {
                if ret.response_status == 200 || ret.response_status == 204 {
                    println!("Success! Enabled the secrets engine at: {mount_path}");
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
