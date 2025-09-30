use std::io::{self, Write};

use better_default::Default;
use rpassword::read_password;
use serde_json::{Map, Value};

use crate::{
    api::{auth::LoginHandler, client::Client, HttpResponse},
    errors::RvError,
    rv_error_response,
};

#[derive(Default)]
pub struct UsesPassCliHandler {
    #[default("usepass".to_string())]
    pub default_mount: String,
}

impl LoginHandler for UsesPassCliHandler {
    fn auth(&self, client: &Client, data: &Map<String, Value>) -> Result<HttpResponse, RvError> {
        if data["username"].as_str().is_none() {
            return Err(rv_error_response!("'username' must be specified"));
        }
        let username = data["username"].as_str().unwrap();

        let mut password = data["password"].as_str().unwrap_or("").to_string();
        if password.is_empty() {
            let mut writer = io::stdout();
            write!(writer, "Password (will be hidden): ")?;
            writer.flush()?;
            let value = read_password().expect("Failed to read password");
            writeln!(writer)?;
            password = value;
        }

        let payload = serde_json::json!({
            "password": password,
        });

        let mut mount = data["mount"].as_str().unwrap_or("");
        if mount.is_empty() {
            mount = &self.default_mount;
        }
        let path = format!("auth/{mount}/login/{username}");

        let logical = client.logical();

        logical.write(&path, payload.as_object().cloned())
    }

    fn help(&self) -> String {
        let help = r#"
Usage: rvault login -method=userpass [CONFIG K=V...]

The userpass auth method allows users to authenticate using RustyVault's internal user database.

Authenticate as "sally":

    $ rvault login -method=userpass username=sally
    Password (will be hidden):

Authenticate as "bob":

    $ rvault login -method=userpass username=bob password=password

Configuration:

password=<string>
    Password to use for authentication. If not provided, the CLI will prompt for this on stdin.

username=<string>
    Username to use for authentication."#;
        help.trim().to_string()
    }
}
