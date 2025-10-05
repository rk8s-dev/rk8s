use clap::{Args, Parser};
use derive_more::Deref;
use serde_json::Value;

use crate::{
    api::secret::Secret,
    cli::{
        command::{self, CommandExecutor, auth::LoginHandlers},
        util,
    },
    errors::RvError,
    utils::kv_builder::KvPairParse,
};

#[derive(Parser, Deref)]
#[command(
    author,
    version,
    about = r#"Authenticates users or machines to RustyVault using the provided arguments. A
successful authentication results in a RustyVault token - conceptually similar to
a session token on a website. By default, this token is cached on the local
machine for future requests.

The default auth method is "token". If not supplied via the CLI,
RustyVault will prompt for input. If the argument is "-", the values are read
from stdin.

The -method flag allows using other auth methods, such as userpass, github, or
cert. For these, additional "K=V" pairs may be required. For example, to
authenticate to the userpass auth method:

    $ rvault login -method=userpass username=my-username

For more information about the list of configuration parameters available for
a given auth method, use the "rvault auth help TYPE" command. You can also use
"rvault auth list" to see the list of enabled auth methods.

If an auth method is enabled at a non-standard path, the -method flag still
refers to the canonical type, but the -path flag refers to the enabled path.
If a github auth method was enabled at "github-prod", authenticate like this:

    $ rvault login -method=github -path=github-prod

If the authentication is requested with response wrapping (via -wrap-ttl),
the returned token is automatically unwrapped unless:

- The -token-only flag is used, in which case this command will output
    the wrapping token.

- The -no-store flag is used, in which case this command will output the
    details of the wrapping token."#
)]
pub struct Login {
    #[clap(
        index = 1,
        value_name = "AUTH K=V...",
        help = r#"AUTH is specified as "key=value" pairs."#
    )]
    data: Vec<String>,

    #[deref]
    #[command(flatten, next_help_heading = "HTTP Options")]
    http_options: command::HttpOptions,

    #[command(flatten, next_help_heading = "Output Options")]
    output: command::OutputOptions,

    #[command(flatten, next_help_heading = "Command Options")]
    options: LoginOptions,
}

#[derive(Args)]
#[group(required = false, multiple = true)]
pub struct LoginOptions {
    #[arg(
        long,
        next_line_help = false,
        default_value = "token",
        value_name = "string",
        long_help = r#"Type of authentication to use such as "userpass" or "cert". Note this
corresponds to the TYPE, not the enabled path. Use -path to specify the
path where the authentication is enabled. The default is token."#
    )]
    method: String,

    #[arg(
        long,
        next_line_help = false,
        default_value = "",
        value_name = "string",
        long_help = r#"Remote path in RustyVault where the auth method is enabled. This defaults to
the TYPE of method (e.g. userpass -> userpass/)."#
    )]
    path: String,

    #[arg(
        long,
        next_line_help = true,
        default_value_t = false,
        long_help = r#"Do not display the token. The token will be still be stored to the
configured token helper. The default is false"#
    )]
    no_print: bool,
}

#[async_trait::async_trait]
impl CommandExecutor for Login {
    #[inline]
    async fn main(&self) -> Result<(), RvError> {
        let client = self.client()?;

        let mut auth_method = util::sanitize_path(&self.options.method);

        if auth_method.is_empty() {
            auth_method = "token".into();
        }

        let mut auth_path = self.options.path.clone();
        if auth_path.is_empty() {
            auth_path = util::ensure_trailing_slash(&auth_method);
        }

        let login_handler = LoginHandlers.get(&auth_method);
        if login_handler.is_none() {
            println!("Unknown auth method: {auth_method}.");
            println!(r#"Use "rvault auth list" to see the complete list of auth methods."#);
            println!("Additionally, some auth methods are only available via the HTTP API.");
            std::process::exit(1);
        }

        let mut auth_data =
            if auth_method == "token" && !self.data.is_empty() && !self.data[0].contains('=') {
                let mut data = self.data.clone();
                data[0] = format!("token={}", self.data[0]);
                data.to_map()
            } else {
                self.data.to_map()
            };

        if !auth_path.is_empty() {
            auth_data.insert("mount".into(), Value::String(auth_path));
        }

        let ret = login_handler
            .as_ref()
            .unwrap()
            .auth(&client, &auth_data)
            .await?;
        if ret.response_status != 200 {
            println!("Error authenticating: ");
            ret.print_debug_info();
            std::process::exit(2);
        }

        let response_value = ret.response_data.ok_or(RvError::ErrResponseDataInvalid)?;

        let secret: Secret = serde_json::from_value(response_value)?;
        if secret.auth.is_none() {
            println!(
                "RustyVault returned a secret, but the secret has no authentication information attached. "
            );
            println!("This should never happen and is likely a bug.");
            std::process::exit(2);
        }

        if self.options.no_print {
            return Ok(());
        }

        self.output.print_secret(&secret, None)
    }
}
