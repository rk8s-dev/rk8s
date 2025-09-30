use clap::Parser;
use derive_more::Deref;
use sysexits::ExitCode;

use crate::{
    cli::command::{self, CommandExecutor},
    errors::RvError,
    http::sys::InitRequest,
    rv_error_string, EXIT_CODE_OK,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Initializes a RustyVault server. Initialization is the process by which RustyVault's storage
backend is prepared to receive data. Since RustyVault servers share the same storage backend
in HA mode, you only need to initialize one RustyVault to initialize the storage backend.

During initialization, RustyVault generates an in-memory root key and applies Shamir's secret
sharing algorithm to disassemble that root key into a configuration number of key shares such
that a configurable subset of those key shares must come together to regenerate the root key.
These keys are often called "unseal keys" in RustyVault's documentation.

This command cannot be run against an already-initialized RustyVault cluster.

Start initialization with the default options:

  $ rvault operator init

Initialize, but specify key-shares and key-threshold:

  $ rvault operator init \
      -key-shares=3 \
      -key-threshold=2"#
)]
pub struct Init {
    #[arg(
        long,
        next_line_help = true,
        value_name = "int",
        long_help = r#"Number of key shares to split the generated root key into. This is the
number of "unseal keys" to generate."#
    )]
    key_shares: u8,

    #[arg(
        long,
        next_line_help = true,
        value_name = "int",
        long_help = r#"Number of key shares required to reconstruct the root key. This must be
less than or equal to -key-shares."#
    )]
    key_threshold: u8,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for Init {
    #[inline]
    fn execute(&mut self) -> ExitCode {
        match self.main() {
            Ok(_) => EXIT_CODE_OK,
            Err(e) => {
                eprintln!("Error: {e}");
                // TODO
                std::process::exit(2);
            }
        }
    }

    fn main(&self) -> Result<(), RvError> {
        if self.key_threshold > self.key_shares {
            return Err(rv_error_string!("invalid seal configuration: threshold cannot be larger than shares"));
        }

        let client = self.client()?;
        let sys = client.sys();

        let init_req = InitRequest { secret_shares: self.key_shares, secret_threshold: self.key_threshold };

        match sys.init(&init_req) {
            Ok(ret) => {
                if ret.response_status == 200 {
                    self.output.print_value(ret.response_data.as_ref().unwrap(), true)?;
                } else if ret.response_status == 204 {
                    println!("ok");
                } else {
                    ret.print_debug_info();
                }
            }
            Err(e) => eprintln!("{e}"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use serde_json::Value;

    use crate::test_utils::TestHttpServer;

    #[test]
    fn test_cli_operator_init() {
        let test_http_server = TestHttpServer::new_without_init("test_cli_operator_init", true);

        // rvault operator init
        let ret = test_http_server.cli(&["operator", "init"], &["--format=raw", "--key-shares=5", "--key-threshold=3"]);
        assert!(ret.is_ok());
        let ret = Value::from_str(ret.unwrap().as_str()).unwrap();
        let init_result = ret.as_object().unwrap();

        // rvault status
        let ret = test_http_server.cli(&["status"], &["--format=json"]);
        let ret = Value::from_str(ret.unwrap().as_str()).unwrap();
        let status_result = ret.as_object().unwrap();
        assert_eq!(init_result["keys"].as_array().unwrap().len(), status_result["threshold"]);
    }
}
