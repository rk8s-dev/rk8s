use std::sync::Arc;

use clap::{Parser, Subcommand};
use dashmap::DashMap;
use lazy_static::lazy_static;
use sysexits::ExitCode;

use super::{auth_disable, auth_enable, auth_help, auth_list, auth_move};
use crate::{
    api::auth::LoginHandler,
    cli::command::CommandExecutor,
    modules::credential::{
        cert::cli::CertAuthCliHandler, token::cli::TokenCliHandler, userpass::cli::UsesPassCliHandler,
    },
    EXIT_CODE_INSUFFICIENT_PARAMS,
};

lazy_static! {
    pub static ref LoginHandlers: DashMap<String, Arc<dyn LoginHandler>> = {
        let map: DashMap<String, Arc<dyn LoginHandler>> = DashMap::new();

        map.insert("token".into(), Arc::new(TokenCliHandler));
        map.insert("userpass".into(), Arc::new(UsesPassCliHandler::default()));
        map.insert("cert".into(), Arc::new(CertAuthCliHandler::default()));

        map
    };
}

#[derive(Parser)]
#[command(
    author,
    version,
    about = "Perform operator-specific tasks",
    long_about = r#"This command groups subcommands for interacting with RustyVault's auth methods.
Users can list, enable, disable, and get help for different auth methods.

To authenticate to RustyVault as a user or machine, use the "rvault login" command instead.
This command is for interacting with the auth methods themselves, not authenticating to RustyVault.

List all enabled auth methods:

    $ rvault auth list

Enable a new auth method "userpass";

    $ rvault auth enable userpass

Get detailed help information about how to authenticate to a particular auth method:

    $ rvault auth help github"#
)]
pub struct Auth {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    List(auth_list::List),
    Enable(auth_enable::Enable),
    Disable(auth_disable::Disable),
    Move(auth_move::Move),
    Help(auth_help::Help),
}

impl Commands {
    pub fn execute(&mut self) -> ExitCode {
        match self {
            Commands::List(list) => list.execute(),
            Commands::Enable(enable) => enable.execute(),
            Commands::Disable(disable) => disable.execute(),
            Commands::Move(mv) => mv.execute(),
            Commands::Help(help) => help.execute(),
        }
    }
}

impl Auth {
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
    async fn test_cli_auth_help() {
        let mut test_http_server = TestHttpServer::new("test_cli_auth_help", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // auth help token/
        let ret = test_http_server.cli(&["auth", "help", "token/"], &[]);
        assert!(ret.is_ok());
        assert!(ret.unwrap().contains("Usage: rvault login TOKEN"));

        // auth enable
        let ret = test_http_server.cli(
            &["auth", "enable", "userpass"],
            &["--path=up1", "--description=userpass_test", "--options", "u1=p1", "--options", "u2=p2"],
        );
        assert!(ret.is_ok());
        assert_eq!(ret, Ok("Success! Enabled userpass auth method at: up1/\n".to_string()));

        // auth help up1/
        let ret = test_http_server.cli(&["auth", "help", "up1/"], &[]);
        assert!(ret.is_ok());
        assert!(ret.unwrap().contains("Usage: rvault login -method=userpass"));

        // auth help cert
        let ret = test_http_server.cli(&["auth", "help", "cert"], &[]);
        assert!(ret.is_ok());
        assert!(ret.unwrap().contains("Usage: rvault login -method=cert"));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_auth_list() {
        let mut test_http_server = TestHttpServer::new("test_cli_auth_list", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // test auth list
        let ret = test_http_server.cli(&["auth", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        println!("ret: {:?}", ret);
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list["token/"].is_object());
        assert_eq!(list["token/"]["type"], Value::String("token".into()));
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_auth_enable_disable() {
        let mut test_http_server = TestHttpServer::new("test_cli_auth_enable_disable", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // test auth enable
        let ret = test_http_server.cli(
            &["auth", "enable", "userpass"],
            &["--path=up1", "--description=userpass_test", "--options", "u1=p1", "--options", "u2=p2"],
        );
        assert!(ret.is_ok());
        assert_eq!(ret, Ok("Success! Enabled userpass auth method at: up1/\n".to_string()));

        // test auth list
        let ret = test_http_server.cli(&["auth", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list["token/"].is_object());
        assert_eq!(list["token/"]["type"], Value::String("token".into()));

        assert!(list["up1/"].is_object());
        assert_eq!(list["up1/"]["type"], Value::String("userpass".into()));
        assert_eq!(list["up1/"]["description"], Value::String("userpass_test".into()));

        // add user
        let username = "jinjiu";
        let password = "123123";
        let ret = test_http_server
            .cli(&["write"], &[&format!("auth/up1/users/{}", username), &format!("password={}", password), "ttl=600"]);
        println!("add user ret: {:?}", ret);
        assert!(ret.is_ok());

        // clear token
        test_http_server.token.clear();

        // test login
        let ret = test_http_server.cli(
            &["login"],
            &["--method=userpass", "--path=up1", &format!("username={}", username), &format!("password={}", password)],
        );
        assert!(ret.is_ok());

        // test auth disable
        test_http_server.token = test_http_server.root_token.clone();
        let ret = test_http_server.cli(&["auth", "disable"], &["up1"]);
        assert_eq!(ret, Ok("Success! Disabled the auth method (if it existed) at: up1/\n".to_string()));

        // clear token
        test_http_server.token.clear();

        // test login again
        let ret = test_http_server.cli(
            &["login"],
            &["--method=userpass", "--path=up1", &format!("username={}", username), &format!("password={}", password)],
        );
        assert!(ret.is_err());
    }

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_auth_move() {
        let mut test_http_server = TestHttpServer::new("test_cli_auth_move", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // test auth enable
        let ret = test_http_server.cli(
            &["auth", "enable", "userpass"],
            &["--path=up1", "--description=userpass_test", "--options", "u1=p1", "--options", "u2=p2"],
        );
        assert!(ret.is_ok());
        assert_eq!(ret, Ok("Success! Enabled userpass auth method at: up1/\n".to_string()));

        // add user
        let username = "jinjiu";
        let password = "123123";
        let ret = test_http_server
            .cli(&["write"], &[&format!("auth/up1/users/{}", username), &format!("password={}", password), "ttl=600"]);
        println!("add user ret: {:?}", ret);
        assert!(ret.is_ok());

        // test auth move
        test_http_server.token = test_http_server.root_token.clone();
        let ret = test_http_server.cli(&["auth", "move"], &["up1", "up2"]).unwrap();
        assert!(ret.starts_with("Success! Finished moving auth method"));

        // clear token
        test_http_server.token.clear();

        // test login on up1/ should fail
        let ret = test_http_server.cli(
            &["login"],
            &["--method=userpass", "--path=up1", &format!("username={}", username), &format!("password={}", password)],
        );
        assert!(ret.is_err());

        // test login on up2/ should ok
        let ret = test_http_server.cli(
            &["login"],
            &["--method=userpass", "--path=up2", &format!("username={}", username), &format!("password={}", password)],
        );
        assert!(ret.is_ok());

        // test auth list
        test_http_server.token = test_http_server.root_token.clone();
        let ret = test_http_server.cli(&["auth", "list"], &["--format=json"]);
        assert!(ret.is_ok());
        let ret = ret.unwrap();
        let list: Map<String, Value> = serde_json::from_str(&ret).unwrap();
        assert!(list["up2/"].is_object());
        assert_eq!(list["up2/"]["type"], Value::String("userpass".into()));
        assert_eq!(list["up2/"]["description"], Value::String("userpass_test".into()));
    }
}
