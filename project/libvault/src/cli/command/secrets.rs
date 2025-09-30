use clap::{Parser, Subcommand};
use sysexits::ExitCode;

use super::{secrets_disable, secrets_enable, secrets_list, secrets_move};
use crate::{cli::command::CommandExecutor, EXIT_CODE_INSUFFICIENT_PARAMS};

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Perform operator-specific tasks",
    long_about = r#"This command groups subcommands for interacting with RustyVault's secrets engines.
Each secret engine behaves differently. Please see the documentation for more information.

List all enabled secrets engines:

    $ rvault secrets list

Enable a new secrets engine:

    $ rvault secrets enable database

Please see the individual subcommand help for detailed usage information."#
)]
pub struct Secrets {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    Disable(secrets_disable::Disable),
    Enable(secrets_enable::Enable),
    List(secrets_list::List),
    Move(secrets_move::Move),
}

impl Commands {
    pub fn execute(&mut self) -> ExitCode {
        match self {
            Commands::Disable(disable) => disable.execute(),
            Commands::Enable(enable) => enable.execute(),
            Commands::List(list) => list.execute(),
            Commands::Move(mv) => mv.execute(),
        }
    }
}

impl Secrets {
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
    use serde_json::{Map, Value};

    use crate::test_utils::TestHttpServer;

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_secrets_list() {
        let mut test_http_server = TestHttpServer::new("test_cli_secrets_list", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        let ret = test_http_server.cli(&["secrets", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list["secret/"].is_object());
        assert_eq!(list["secret/"]["type"], Value::String("kv".into()));
        assert!(list["sys/"].is_object());
        assert_eq!(list["sys/"]["type"], Value::String("system".into()));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_secret_enable_disable() {
        let mut test_http_server = TestHttpServer::new("test_cli_secret_enable_disable", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // test secrets enable
        let ret = test_http_server.cli(&["secrets", "enable", "kv"], &["--path=kv1", "--description=kv_test"]);
        assert!(ret.is_ok());
        assert_eq!(ret, Ok("Success! Enabled the secrets engine at: kv1/\n".to_string()));

        // test secrets list
        let ret = test_http_server.cli(&["secrets", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list["secret/"].is_object());
        assert_eq!(list["secret/"]["type"], Value::String("kv".into()));

        assert!(list["kv1/"].is_object());
        assert_eq!(list["kv1/"]["type"], Value::String("kv".into()));
        assert_eq!(list["kv1/"]["description"], Value::String("kv_test".into()));

        // test write data to kv1/
        let ret = test_http_server.cli(&["write"], &["kv1/foo", "aa=bb", "cc=dd"]);
        assert_eq!(ret, Ok("Success! Data written to: kv1/foo\n".into()));

        // Reading data should ok
        let ret = test_http_server.cli(&["read"], &["kv1/foo"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Key    Value    \r\n---    -----    \r\naa     bb    \r\ncc     dd    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Key    Value    \n---    -----    \naa     bb    \ncc     dd    \n".into()));

        // test secrets disable
        test_http_server.token = test_http_server.root_token.clone();
        let ret = test_http_server.cli(&["secrets", "disable"], &["kv1/"]);
        assert_eq!(ret, Ok("Success! Disabled the secrets engine (if it existed) at: kv1/\n".to_string()));

        // test secrets list again
        let ret = test_http_server.cli(&["secrets", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list.get("kv1").is_none());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_secret_move() {
        let mut test_http_server = TestHttpServer::new("test_cli_secret_move", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // test secrets enable
        let ret = test_http_server.cli(&["secrets", "enable", "kv"], &["--path=kv1", "--description=kv_test"]);
        assert!(ret.is_ok());
        assert_eq!(ret, Ok("Success! Enabled the secrets engine at: kv1/\n".to_string()));

        // test write data to kv1/
        let ret = test_http_server.cli(&["write"], &["kv1/foo", "aa=bb", "cc=dd"]);
        assert_eq!(ret, Ok("Success! Data written to: kv1/foo\n".into()));

        // test secrets move
        let ret = test_http_server.cli(&["secrets", "move", "kv1/", "kv2/"], &[]);
        assert!(ret.is_ok());
        assert_eq!(ret, Ok("Success! Finished moving auth method kv1/ to kv2/.\n".to_string()));

        // Reading data should ok
        let ret = test_http_server.cli(&["read"], &["kv2/foo"]);
        #[cfg(windows)]
        assert_eq!(ret, Ok("Key    Value    \r\n---    -----    \r\naa     bb    \r\ncc     dd    \r\n".into()));
        #[cfg(not(windows))]
        assert_eq!(ret, Ok("Key    Value    \n---    -----    \naa     bb    \ncc     dd    \n".into()));

        // test secrets list
        test_http_server.token = test_http_server.root_token.clone();
        let ret = test_http_server.cli(&["secrets", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list.get("kv1").is_none());
        assert!(list["kv2/"].is_object());
        assert_eq!(list["kv2/"]["type"], Value::String("kv".into()));
        assert_eq!(list["kv2/"]["description"], Value::String("kv_test".into()));
    }
}
