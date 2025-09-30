use clap::Parser;
use derive_more::Deref;

use crate::{
    api::sys::MountOutput,
    cli::{
        command::{self, auth::LoginHandlers, CommandExecutor},
        util,
    },
    errors::RvError,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Prints usage and help for an auth method.

- If given a TYPE, this command prints the default help for the
    auth method of that type.

- If given a PATH, this command prints the help output for the
    auth method enabled at that path. This path must already
    exist.

Get usage instructions for the userpass auth method:

    $ rvault auth help userpass

Print usage for the auth method enabled at my-method/:

    $ rvault auth help my-method/

Each auth method produces its own help output."#
)]
pub struct Help {
    #[arg(next_line_help = false, value_name = "TYPE | PATH", long_help = r#"Auth method type or path"#)]
    auth_type: String,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,
}

impl CommandExecutor for Help {
    #[inline]
    fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;
        let sys = client.sys();

        let mut login_handler = LoginHandlers.get(&self.auth_type);
        if login_handler.is_none() {
            let path = util::ensure_trailing_slash(&util::sanitize_path(&self.auth_type));
            let auth_list: crate::api::HttpResponse = sys.list_auth()?;
            if auth_list.response_status != 200 || auth_list.response_data.is_none() {
                println!("Error listing auth methods at: {path}");
                std::process::exit(2);
            }

            let auth_list_value = auth_list.response_data.unwrap();
            let auth_list_map = auth_list_value.as_object().unwrap();
            let auth = auth_list_map.get(&path);
            if auth.is_none() {
                println!("No auth method available on the server at: {path}");
                std::process::exit(1);
            }

            let mount_output: MountOutput = serde_json::from_value(auth.unwrap().clone())?;

            login_handler = LoginHandlers.get(&mount_output.logical_type);
            if login_handler.is_none() {
                println!("No method-specific CLI handler available for auth method {}", &self.auth_type);
                std::process::exit(2);
            }
        }

        println!("{}", login_handler.as_ref().unwrap().help());

        Ok(())
    }
}
