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
    about = r#"Deletes the policy named NAME in the RustyVault server. Once the policy is deleted,
all tokens associated with the policy are affected immediately.

Delete the policy named "my-policy":

    $ rvault policy delete my-policy

Note that it is not possible to delete the "default" or "root" policies.
These are built-in policies.
"#
)]
pub struct Delete {
    #[clap(index = 1, value_name = "NAME", help = r#"The name of policy"#)]
    name: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for Delete {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let policy_name = self.name.trim().to_lowercase();

        match sys.delete_policy(&policy_name) {
            Ok(ret) => {
                if ret.response_status == 200 || ret.response_status == 204 {
                    println!("Success! Deleted policy: {policy_name}");
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

#[cfg(test)]
mod test {
    use crate::test_utils::TestHttpServer;

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_policy_delete() {
        let mut test_http_server = TestHttpServer::new("test_cli_policy_delete", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // delete default should be failed
        let ret = test_http_server.cli(&["policy", "delete"], &["default"]);
        assert!(ret.is_err());
        let err = ret.unwrap_err().to_string();
        assert!(err.contains("cannot delete default policy"));
        assert!(err.contains("Code: 400. Error"));

        // delete root should be failed
        let ret = test_http_server.cli(&["policy", "delete"], &["root"]);
        assert!(ret.is_err());
        let err = ret.unwrap_err().to_string();
        assert!(err.contains("cannot delete root policy"));
        assert!(err.contains("Code: 400. Error"));

        // write a test policy
        let test_policy = r#"path "secret/" {}"#;
        let client = test_http_server.client().unwrap();
        let sys = client.sys();
        assert!(sys.write_policy("my-policy", test_policy).is_ok());

        // delete default should be ok
        let ret = test_http_server.cli(&["policy", "delete"], &["my-policy"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), "Success! Deleted policy: my-policy\n");
    }
}
