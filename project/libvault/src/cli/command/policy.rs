use clap::{Parser, Subcommand};
use sysexits::ExitCode;

use super::{policy_delete, policy_list, policy_read, policy_write};
use crate::{cli::command::CommandExecutor, EXIT_CODE_INSUFFICIENT_PARAMS};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Interact with policies",
    long_about = r#"This command groups subcommands for interacting with policies.
Users can write, read, and list policies in RustyVault.

List all enabled policies:

    $ rvault policy list

Create a policy named "my-policy" from contents on local disk:

    $ rvault policy write my-policy ./my-policy.hcl

Delete the policy named my-policy:

    $ rvault policy delete my-policy

Please see the individual subcommand help for detailed usage information."#
)]
pub struct Policy {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    List(policy_list::List),
    Write(policy_write::Write),
    Delete(policy_delete::Delete),
    Read(policy_read::Read),
}

impl Commands {
    pub fn execute(&mut self) -> ExitCode {
        match self {
            Commands::List(list) => list.execute(),
            Commands::Write(write) => write.execute(),
            Commands::Read(read) => read.execute(),
            Commands::Delete(delete) => delete.execute(),
        }
    }
}

impl Policy {
    #[inline]
    pub fn execute(&mut self) -> ExitCode {
        if let Some(ref mut cmd) = &mut self.command {
            return cmd.execute();
        }

        EXIT_CODE_INSUFFICIENT_PARAMS
    }
}

#[cfg(test)]
mod test {
    use crate::{errors::RvError, rv_error_string, test_utils::TestHttpServer};

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_policy() {
        let mut test_http_server = TestHttpServer::new("test_cli_policy", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // There is no data by default, and reading should fail.
        let ret = test_http_server.cli(&["read"], &["kv/foo"]);
        assert!(ret.is_err());
        assert_eq!(ret.unwrap_err(), rv_error_string!("No value found at kv/foo\n"));
    }
}
