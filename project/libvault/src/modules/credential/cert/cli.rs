use better_default::Default;
use serde_json::{Map, Value};

use crate::{
    api::{auth::LoginHandler, client::Client, HttpResponse},
    errors::RvError,
    rv_error_response,
};

#[derive(Default)]
pub struct CertAuthCliHandler {
    #[default("cert".to_string())]
    pub default_mount: String,
}

impl LoginHandler for CertAuthCliHandler {
    fn auth(&self, client: &Client, data: &Map<String, Value>) -> Result<HttpResponse, RvError> {
        if data["name"].as_str().is_none() {
            return Err(rv_error_response!("'name' must be specified"));
        }

        let payload = serde_json::json!({
            "name": data["name"].clone(),
        });

        let mount = data["mount"]
            .as_str()
            .map_or(self.default_mount.as_str(), |s| if s.is_empty() { self.default_mount.as_str() } else { s })
            .trim();
        let path = format!("auth/{mount}/login");

        let logical = client.logical();

        logical.write(&path, payload.as_object().cloned())
    }

    fn help(&self) -> String {
        let help = r#"
Usage: rvault login -method=cert [CONFIG K=V...]

The certificate auth method allows users to authenticate with a client certificate passed with the request. 
The -client-cert and -client-key flags are included with the "rvault login" command, NOT as configuration to the auth method.

Authenticate using a local client certificate:

    $ rvault login -method=cert -client-cert=cert.pem -client-key=key.pem

Configuration:

name=<string>
    Certificate role to authenticate against.
        "#;
        help.trim().to_string()
    }
}
