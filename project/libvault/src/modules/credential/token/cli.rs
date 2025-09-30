use std::io::{self, Write};

use better_default::Default;
use rpassword::read_password;
use serde_json::{Map, Value};

use crate::{
    api::{
        auth::LoginHandler,
        client::Client,
        secret::{Secret, SecretAuth},
        HttpResponse,
    },
    errors::RvError,
    logical::field::FieldTrait,
    rv_error_string,
};

#[derive(Default)]
pub struct TokenCliHandler;

impl LoginHandler for TokenCliHandler {
    fn auth(&self, client: &Client, data: &Map<String, Value>) -> Result<HttpResponse, RvError> {
        let mut token = data["token"].as_str().unwrap_or("").to_string();
        if token.is_empty() {
            let mut writer = io::stdout();
            write!(writer, "Token (will be hidden): ")?;
            writer.flush()?;
            let value = read_password().expect("Failed to read token");
            writeln!(writer)?;
            token = value;
        }

        token = token.trim().to_string();
        if token.is_empty() {
            return Err(rv_error_string!("a token must be passed to auth, please view the help for more information"));
        }

        let lookup = if let Some(lookup_value) = data.get("lookup") {
            lookup_value.as_bool_ex().ok_or(rv_error_string!("Failed to parse \"lookup\" as boolean"))?
        } else {
            true
        };

        if !lookup {
            let auth = SecretAuth { client_token: token.clone(), ..Default::default() };

            let resp = Secret { auth: Some(auth), ..Default::default() };
            let ret = HttpResponse {
                response_status: 200,
                response_data: Some(serde_json::to_value(resp)?),
                ..Default::default()
            };
            return Ok(ret);
        }

        let mut client = client.clone();
        client.token = token;

        let ret = client.token().lookup_self()?;
        let response_value = ret.response_data.ok_or(RvError::ErrResponseDataInvalid)?;
        let secret: Secret = serde_json::from_value(response_value)?;

        let auth = SecretAuth {
            client_token: secret.token_id()?,
            accessor: secret.token_accessor()?,
            policies: secret.token_policies()?,
            token_policies: secret.token_policies()?,
            metadata: secret.token_metadata()?,
            lease_duration: secret.token_ttl()?,
            renewable: secret.token_is_renewable()?,
            ..Default::default()
        };

        let resp = Secret { auth: Some(auth), ..Default::default() };
        let ret = HttpResponse {
            response_status: 200,
            response_data: Some(serde_json::to_value(resp)?),
            ..Default::default()
        };

        Ok(ret)
    }

    fn help(&self) -> String {
        let help = r#"
Usage: rvault login TOKEN [CONFIG K=V...]

The token auth method allows logging in directly with a token. This
can be a token from the "token-create" command or API. There are no
configuration options for this auth method.

Authenticate using a token:

    $ rvault login 96ddf4bc-d217-f3ba-f9bd-017055595017

Authenticate but do not lookup information about the token:

    $ rvault login token=96ddf4bc-d217-f3ba-f9bd-017055595017 lookup=false

This token usually comes from a different source such as the API or via the
built-in "rvault token create" command.

Configuration:

token=<string>
    The token to use for authentication. This is usually provided directly
    via the "rvault login" command.

lookup=<bool>
    If true, it performs a lookup of the token's metadata and policies."#;
        help.trim().to_string()
    }
}
