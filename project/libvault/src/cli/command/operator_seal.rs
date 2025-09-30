use clap::Parser;
use derive_more::Deref;
use sysexits::ExitCode;

use crate::{
    cli::command::{self, CommandExecutor},
    errors::RvError,
    EXIT_CODE_INSUFFICIENT_PARAMS, EXIT_CODE_OK,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Seals the RustyVault server. Sealing tells the RustyVault server to stop responding
to any operations until it is unsealed. When sealed, the RustyVault server discards
its in-memory root key to unlock the data, so it is physically blocked from responding
to operations unsealed.

If an unseal is in progress, sealing the Vault will reset the unsealing process. Users
will have to re-enter their portions of the root key again.

This command does nothing if the RustyVault server is already sealed.

Seal the RustyVault server:

  $ rvault operator seal"#
)]
pub struct Seal {
    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,
}

impl CommandExecutor for Seal {
    #[inline]
    fn execute(&mut self) -> ExitCode {
        match self.main() {
            Ok(_) => EXIT_CODE_OK,
            Err(e) => {
                eprintln!("Error: {e}");
                EXIT_CODE_INSUFFICIENT_PARAMS
            }
        }
    }

    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        match sys.seal() {
            Ok(_) => {
                println!("Success! RustyVault is sealed.");
            }
            Err(e) => eprintln!("Error sealing: {e}"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use serde_json::Value;

    use crate::test_utils::TestHttpServer;

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_operator_seal() {
        let test_http_server = TestHttpServer::new("test_cli_operator_seal", true).await;

        // rvault status
        let ret = test_http_server.cli(&["status"], &["--format=raw"]);
        let ret = Value::from_str(ret.unwrap().as_str()).unwrap();
        let status_result = ret.as_object().unwrap();
        assert_eq!(status_result["sealed"], false);

        // rvault operator seal
        let ret = test_http_server.cli(&["operator", "seal"], &[]);
        assert_eq!(ret, Ok("Success! RustyVault is sealed.\n".into()));

        // rvault status
        let ret = test_http_server.cli(&["status"], &["--format=raw"]);
        let ret = Value::from_str(ret.unwrap().as_str()).unwrap();
        let status_result = ret.as_object().unwrap();
        assert_eq!(status_result["sealed"], true);
    }
}
