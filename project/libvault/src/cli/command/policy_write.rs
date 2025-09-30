use std::{
    fs::File,
    io::{self, Read},
};

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
    about = r#"Uploads a policy with name NAME from the contents of a local file PATH or
stdin. If PATH is "-", the policy is read from stdin. Otherwise, it is
loaded from the file at the given path on the local disk.

Upload a policy named "my-policy" from "/tmp/policy.hcl" on the local disk:

    $ rvault policy write my-policy /tmp/policy.hcl

Upload a policy from stdin:

    $ cat my-policy.hcl | rvault policy write my-policy -
"#
)]
pub struct Write {
    #[clap(index = 1, value_name = "NAME", help = r#"The name of policy"#)]
    name: String,

    #[arg(index = 2, next_line_help = false, value_name = "PATH", help = r#"The path of policy."#)]
    path: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,
}

impl CommandExecutor for Write {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let mut buffer = String::new();

        if self.path == "-" {
            io::stdin().read_to_string(&mut buffer)?;
        } else {
            let mut file = File::open(&self.path)?;
            file.read_to_string(&mut buffer)?;
        }

        let policy_name = self.name.trim().to_lowercase();

        match sys.write_policy(&policy_name, &buffer) {
            Ok(ret) => {
                if ret.response_status == 200 || ret.response_status == 204 {
                    println!("Success! Uploaded policy: {policy_name}");
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
    use std::{env, fs, io::prelude::*};

    use crate::test_utils::TestHttpServer;

    #[maybe_async::test(feature = "sync_handler", async(all(not(feature = "sync_handler")), tokio::test))]
    async fn test_cli_policy_write() {
        let mut test_http_server = TestHttpServer::new("test_cli_policy_write", true).await;
        test_http_server.token = test_http_server.root_token.clone();

        let test_policy = r#"path "secret/" {
  capabilities = ["read"]
}
        "#;
        let test_policy_path = env::temp_dir().join("my-policy.hcl").to_string_lossy().into_owned();
        let _ = fs::remove_file(&test_policy_path);
        let mut file = fs::File::create(&test_policy_path).unwrap();
        file.write_all(test_policy.as_bytes()).unwrap();
        file.flush().unwrap();

        // write policy with file path
        let ret = test_http_server.cli(&["policy", "write"], &["my-policy", test_policy_path.as_str()]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), "Success! Uploaded policy: my-policy\n");

        // list policy with table format
        let ret = test_http_server.cli(&["policy", "list"], &["--format=table"]);
        assert!(ret.is_ok());
        #[cfg(windows)]
        assert_eq!(ret.unwrap(), "default    \r\nmy-policy    \r\nroot    \r\n");
        #[cfg(not(windows))]
        assert_eq!(ret.unwrap(), "default    \nmy-policy    \nroot    \n");

        // read my-policy with table format
        let ret = test_http_server.cli(&["policy", "read"], &["my-policy", "--format=table"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), test_policy.to_string());

        // write policy with stdin input
        let ret = test_http_server.cli_with_input(&["policy", "write"], &["my-policy1", "-"], Some(test_policy));
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), "Success! Uploaded policy: my-policy1\n");

        // list policy with table format again
        let ret = test_http_server.cli(&["policy", "list"], &["--format=table"]);
        assert!(ret.is_ok());
        #[cfg(windows)]
        assert_eq!(ret.unwrap(), "default    \r\nmy-policy    \r\nmy-policy1    \r\nroot    \r\n");
        #[cfg(not(windows))]
        assert_eq!(ret.unwrap(), "default    \nmy-policy    \nmy-policy1    \nroot    \n");

        // read my-policy1 with table format
        let ret = test_http_server.cli(&["policy", "read"], &["my-policy1", "--format=table"]);
        assert!(ret.is_ok());
        assert_eq!(ret.unwrap(), test_policy.to_string());

        let _ = fs::remove_file(&test_policy_path);
    }
}
