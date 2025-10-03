//! This module provides different commands for the RustyVault application.
//! For instance, we have a 'server' command to indicate the application running in the server mode
//! and starts to accept HTTP request to do real RustyVault functionality.

use std::path::PathBuf;

use clap::{ArgAction, Args, ValueEnum, ValueHint};
use sysexits::ExitCode;

use crate::{
    EXIT_CODE_OK,
    api::{Client, client::TLSConfigBuilder},
    errors::RvError,
};

pub mod delete;
pub mod format;
pub mod operator;
pub mod operator_init;
pub mod operator_seal;
pub mod operator_unseal;
pub mod server;
pub mod status;
pub mod write;

pub use format::{LogicalOutputOptions, OutputOptions};

#[derive(Args, Default)]
#[group(required = false, multiple = true)]
pub struct HttpOptions {
    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        default_value = "https://127.0.0.1:8200",
        long_help = r#"Address of the RustyVault server. This can also be specified via the
VAULT_ADDR or RUSTY_VAULT_ADDR environment variable."#
    )]
    address: String,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        value_hint = ValueHint::FilePath,
        env = "VAULT_CACERT",
        long_help = r#"Path on the local disk to a single PEM-encoded CA certificate to verify
the RustyVault server's SSL certificate. This takes precedence over -ca-path.
This can also be specified via the VAULT_CACERT environment variable."#
    )]
    ca_cert: Option<PathBuf>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        value_hint = ValueHint::DirPath,
        env = "VAULT_CAPATH",
        long_help = r#"Path on the local disk to a directory of PEM-encoded CA certificates to
verify the RustyVault server's SSL certificate. This can also be specified
via the VAULT_CAPATH environment variable."#
    )]
    ca_path: Option<PathBuf>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        value_hint = ValueHint::FilePath,
        env = "VAULT_CLIENT_CERT",
        long_help = r#"Path on the local disk to a single PEM-encoded CA certificate to use
for TLS authentication to the Vault server. If this flag is specified,
-client-key is also required. This can also be specified via the VAULT_CLIENT_CERT
environment variable."#
    )]
    client_cert: Option<PathBuf>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        value_hint = ValueHint::FilePath,
        env = "VAULT_CLIENT_KEY",
        long_help = r#"Path on the local disk to a single PEM-encoded private key matching the
client certificate from -client-cert. This can also be specified via the
VAULT_CLIENT_KEY environment variable."#
    )]
    client_key: Option<PathBuf>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        long_help = r#"Name to use as the SNI host when connecting to the Vault server via TLS.
This can also be specified via the VAULT_TLS_SERVER_NAME environment variable."#
    )]
    tls_server_name: Option<String>,

    #[arg(
        long,
        next_line_help = true,
        env = "VAULT_SKIP_VERIFY",
        long_help = r#"Disable verification of TLS certificates. Using this option is highly
discouraged as it decreases the security of data transmissions to and
from the RustyVault server. The default is false. This can also be specified
via the VAULT_SKIP_VERIFY environment variable."#
    )]
    tls_skip_verify: bool,

    #[clap(
        long,
        value_name = "key=value",
        action = ArgAction::Append,
        long_help = r#"Key-value pair provided as key=value to provide http header added to any
request done by the CLI. Trying to add headers starting with 'X-Vault-'
is forbidden and will make the command fail. This can be specified multiple times.
        "#
    )]
    header: Vec<String>,

    #[clap(
        long,
        hide = true,
        required = false,
        env = "VAULT_TOKEN",
        default_value = ""
    )]
    token: String,
}

#[derive(Args)]
#[group(required = false, multiple = true)]
pub struct CommandOptions {
    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        long_help = r#"Path to a configuration file or directory of configuration files. This
flag can be specified multiple times to load multiple configurations.
If the path is a directory, all files which end in .hcl or .json are loaded."#
    )]
    config: Option<PathBuf>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        default_value = "false",
        long_help = "Path to the log file that Vault should use for logging"
    )]
    log_file: Option<PathBuf>,

    #[arg(
        long,
        next_line_help = true,
        value_name = "string",
        num_args = 0..=1,
        env = "VAULT_LOG_LEVEL",
        default_value_t = LogLevel::Warn,
        default_missing_value = "error",
        long_help = r#"Log verbosity level. This can also be specified via the VAULT_LOG_LEVEL
or RUSTY_VAULT_LOG_LEVEL environment variable.
"#,
        value_enum
    )]
    log_level: LogLevel,
}

#[derive(ValueEnum, Copy, Clone, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl HttpOptions {
    pub fn init(&mut self) -> Result<(), RvError> {
        Ok(())
    }

    pub fn client(&self) -> Result<Client, RvError> {
        let mut client = Client::new()
            .with_addr(&self.address)
            .with_token(&self.token);

        if self.address.starts_with("https://") {
            let mut tls_config_builder =
                TLSConfigBuilder::new().with_insecure(self.tls_skip_verify);

            if let Some(ca_cert) = &self.ca_cert {
                tls_config_builder = tls_config_builder.with_server_ca_path(ca_cert)?;
            }

            if let (Some(client_cert), Some(client_key)) = (&self.client_cert, &self.client_key) {
                tls_config_builder =
                    tls_config_builder.with_client_cert_path(client_cert, client_key)?;
            }

            let tls_config = tls_config_builder.build()?;

            client = client.with_tls_config(tls_config);
        }

        Ok(client.build())
    }
}

pub trait CommandExecutor {
    #[inline]
    fn execute(&mut self) -> ExitCode {
        match self.main() {
            Ok(_) => EXIT_CODE_OK,
            Err(RvError::ErrRequestNoData) => {
                std::process::exit(2);
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
    }

    fn main(&self) -> Result<(), RvError>;
}
