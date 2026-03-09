use crate::config::auth::AuthConfig;
use crate::login::oauth::OAuthFlow;
use crate::login::types::{CallbackResponse, RequestClientIdResponse};
use crate::registry::{
    RegistryScheme, api_url, effective_skip_tls_verify, parse_registry_host_arg,
};
use crate::rt::block_on;
use crate::utils::cli::RequestBuilderExt;
use axum::http::HeaderMap;
use clap::Parser;
use reqwest::Client;

mod oauth;
mod types;

fn client(skip_tls_verify: bool) -> anyhow::Result<Client> {
    let mut headers = HeaderMap::new();
    headers.insert("Accept", "application/json".parse().unwrap());

    Client::builder()
        .default_headers(headers)
        .danger_accept_invalid_certs(skip_tls_verify)
        .build()
        .map_err(Into::into)
}

#[derive(Debug, Parser)]
pub struct LoginArgs {
    /// Registry host in `host[:port]` format (optional if only one server is configured).
    #[arg(value_parser = parse_registry_host_arg)]
    url: Option<String>,
    /// Skip TLS certificate verification for HTTPS registry.
    #[arg(long)]
    skip_tls_verify: bool,
}

pub fn login(args: LoginArgs) -> anyhow::Result<()> {
    let config = AuthConfig::load()?;
    let registry = match args.url {
        Some(url) => url,
        None => config.single_entry()?.url.to_string(),
    };
    let scheme = config.registry_scheme(&registry);
    let skip_tls_verify = effective_skip_tls_verify(args.skip_tls_verify, scheme, &registry);
    let client = client(skip_tls_verify)?;

    block_on(async move {
        let res = request_client_id(&client, &registry, scheme).await?;
        let client_id = &res.client_id;

        let oauth = OAuthFlow::new(client_id);
        let res = oauth.request_token().await?;

        let req_url = api_url(scheme, &registry, "api/v1/auth/github/callback");
        let res = client
            .post(req_url)
            .json(&res)
            .send_and_json::<CallbackResponse>()
            .await?;

        AuthConfig::login(res.pat, &registry)?;
        println!("Logged in successfully!");
        Ok(())
    })?
}

async fn request_client_id(
    client: &Client,
    registry: impl AsRef<str>,
    scheme: RegistryScheme,
) -> anyhow::Result<RequestClientIdResponse> {
    let url = api_url(scheme, registry, "api/v1/auth/github/client_id");
    client.get(url).send_and_json().await
}
