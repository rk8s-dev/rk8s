use clap::Parser;
use derive_more::Deref;
use ureq::json;

use crate::{
    cli::command::{self, CommandExecutor},
    errors::RvError,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Prints the contents and metadata of the RustyVault policy named NAME. If the policy
does not exist, an error is returned.

Read the policy named "my-policy":

    $ rvault policy read my-policy
"#
)]
pub struct Read {
    #[clap(index = 1, value_name = "NAME", help = r#"The name of policy"#)]
    name: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for Read {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let policy_name = self.name.trim().to_lowercase();

        match sys.read_policy(&policy_name) {
            Ok(ret) => {
                if ret.response_status == 200 {
                    let value = ret.response_data.as_ref().unwrap();
                    let policy = &value["policy"];
                    if self.output.is_format_table() {
                        print!("{}", policy.as_str().unwrap_or(""));
                    } else {
                        let value = json!({
                            "policy": policy.clone(),
                        });
                        self.output.print_value(&value, false)?;
                    }
                } else if ret.response_status == 404 {
                    println!("No policy named: {policy_name}");
                    return Err(RvError::ErrRequestNoData);
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
    use crate::{
        errors::RvError, modules::policy::policy_store::DEFAULT_POLICY, rv_error_string, test_utils::TestHttpServer,
    };

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_policy_read() {
        let mut test_http_server = TestHttpServer::new("test_cli_policy_read", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        // read a not exist policy should be failed
        let ret = test_http_server.cli(&["policy", "read"], &["not-a-real-policy"]);
        assert!(ret.is_err());
        assert_eq!(ret.unwrap_err(), rv_error_string!("No policy named: not-a-real-policy\n"));

        // read default policy
        let ret = test_http_server.cli(&["policy", "read"], &["default"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), DEFAULT_POLICY.to_string());

        // write a test policy
        let test_policy = r#"path "secret/" {}"#;
        let client = test_http_server.client().unwrap();
        let sys = client.sys();
        assert!(sys.write_policy("my-policy", test_policy).is_ok());

        // read my-policy
        let ret = test_http_server.cli(&["policy", "read"], &["my-policy"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), test_policy.to_string());

        // read my-policy with table format
        let ret = test_http_server.cli(&["policy", "read"], &["my-policy", "--format=table"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), test_policy.to_string());

        // read my-policy with json format
        let ret = test_http_server.cli(&["policy", "read"], &["my-policy", "--format=json"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), format!("{{\n  \"policy\": \"{}\"\n}}\n", test_policy.replace("\"", r#"\""#)));
    }
}
