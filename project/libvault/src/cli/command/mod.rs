//! This module provides different commands for the RustyVault application.
//! For instance, we have a 'server' command to indicate the application running in the server mode
//! and starts to accept HTTP request to do real RustyVault functionality.

use std::path::PathBuf;

use clap::{ArgAction, Args, ValueEnum, ValueHint};
use sysexits::ExitCode;

use crate::{
    api::{client::TLSConfigBuilder, Client},
    errors::RvError,
    EXIT_CODE_OK,
};

pub mod auth;
pub mod auth_disable;
pub mod auth_enable;
pub mod auth_help;
pub mod auth_list;
pub mod auth_move;
pub mod delete;
pub mod format;
pub mod list;
pub mod login;
pub mod operator;
pub mod operator_init;
pub mod operator_seal;
pub mod operator_unseal;
pub mod policy;
pub mod policy_delete;
pub mod policy_list;
pub mod policy_read;
pub mod policy_write;
pub mod read;
pub mod secrets;
pub mod secrets_disable;
pub mod secrets_enable;
pub mod secrets_list;
pub mod secrets_move;
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

    #[clap(long, hide = true, required = false, env = "VAULT_TOKEN", default_value = "")]
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
        let mut client = Client::new().with_addr(&self.address).with_token(&self.token);

        if self.address.starts_with("https://") {
            let mut tls_config_builder = TLSConfigBuilder::new().with_insecure(self.tls_skip_verify);

            if let Some(ca_cert) = &self.ca_cert {
                tls_config_builder = tls_config_builder.with_server_ca_path(ca_cert)?;
            }

            if let (Some(client_cert), Some(client_key)) = (&self.client_cert, &self.client_key) {
                tls_config_builder = tls_config_builder.with_client_cert_path(client_cert, client_key)?;
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

#[cfg(test)]
mod test {
    use crate::{errors::RvError, rv_error_string, test_utils::TestHttpServer};

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_logical() {
        let mut test_http_server = TestHttpServer::new("test_cli_read", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // There is no data by default, and reading should fail.
        let ret = test_http_server.cli(&["read"], &["kv/foo"]);
        assert!(ret.is_err());
        assert_eq!(ret.unwrap_err(), rv_error_string!("No value found at kv/foo\n"));

        // Without the mount kv engine, writing data should fail.
        let ret = test_http_server.cli(&["write"], &["kv/foo", "aa=bb", "cc=dd"]);
        assert!(ret.is_err());

        // Mount kv engine to path: kv/
        let ret = test_http_server.mount("kv", "kv");
        assert!(ret.is_ok());

        // Writing data should ok
        let ret = test_http_server.cli(&["write"], &["kv/foo", "aa=bb", "cc=dd"]);
        assert_eq!(ret, Ok("Success! Data written to: kv/foo\n".into()));

        // Reading data should ok
        let ret = test_http_server.cli(&["read"], &["kv/foo"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Key    Value    \r\n---    -----    \r\naa     bb    \r\ncc     dd    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Key    Value    \n---    -----    \naa     bb    \ncc     dd    \n".into()));

        let ret = test_http_server.cli(&["read"], &["--format=table", "kv/foo"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Key    Value    \r\n---    -----    \r\naa     bb    \r\ncc     dd    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Key    Value    \n---    -----    \naa     bb    \ncc     dd    \n".into()));

        let ret = test_http_server.cli(&["read"], &["--format=json", "kv/foo"]);
        assert_eq!(ret, Ok("{\n  \"aa\": \"bb\",\n  \"cc\": \"dd\"\n}\n".into()));

        let ret = test_http_server.cli(&["read"], &["--format=yaml", "kv/foo"]);
        assert_eq!(ret, Ok("aa: bb\ncc: dd\n".into()));

        let ret = test_http_server.cli(&["read"], &["--format=yml", "kv/foo"]);
        assert_eq!(ret, Ok("aa: bb\ncc: dd\n".into()));

        let ret = test_http_server.cli(&["read"], &["--format=raw", "kv/foo"]);
        assert_eq!(ret, Ok("{\"aa\":\"bb\",\"cc\":\"dd\"}\n".into()));

        let ret = test_http_server.cli(&["read"], &["--field=aa", "kv/foo"]);
        assert_eq!(ret, Ok("bb\n".into()));

        let ret = test_http_server.cli(&["read"], &["--field=gg", "kv/foo"]);
        assert_eq!(ret, Err(rv_error_string!("Error: Field \"gg\" not present in secret\n")));

        // list /
        let ret = test_http_server.cli(&["list"], &[]);
        assert!(ret.is_err());

        // list kv/
        let ret = test_http_server.cli(&["list"], &["kv/"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Keys    \r\n----    \r\nfoo    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Keys    \n----    \nfoo    \n".into()));

        // list kvv/
        let ret = test_http_server.cli(&["list"], &["kvv/"]);
        assert_eq!(ret, Err(rv_error_string!("No value found at kvv/\n")));

        // write kv/goo
        let ret = test_http_server.cli(&["write"], &["kv/goo", "aaa=bbb", "ccc=ddd"]);
        assert!(ret.is_ok());

        // list kv/ again
        let ret = test_http_server.cli(&["list"], &["kv/"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Keys    \r\n----    \r\nfoo    \r\ngoo    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Keys    \n----    \nfoo    \ngoo    \n".into()));

        // delete kv/goo
        let ret = test_http_server.cli(&["delete"], &["kv/goo"]);
        assert_eq!(ret, Ok("Success! Data deleted (if it existed) at: kv/goo\n".into()));

        // list kv/goo, again
        let ret = test_http_server.cli(&["list"], &["kv/goo"]);
        assert_eq!(ret, Err(rv_error_string!("No value found at kv/goo\n")));

        // delete kv/koo
        let ret = test_http_server.cli(&["delete"], &["kv/koo"]);
        assert_eq!(ret, Ok("Success! Data deleted (if it existed) at: kv/koo\n".into()));

        // delete kv/
        let ret = test_http_server.cli(&["delete"], &["kv/"]);
        assert_eq!(ret, Ok("Success! Data deleted (if it existed) at: kv/\n".into()));

        // list kv/ again
        let ret = test_http_server.cli(&["list"], &["kv/"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Keys    \r\n----    \r\nfoo    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Keys    \n----    \nfoo    \n".into()));
    }
}
