mod types;

use crate::config::auth::AuthConfig;
use crate::config::auth::AuthEntry;
use crate::registry::{
    RegistryScheme, api_url, effective_skip_tls_verify, parse_registry_host_arg,
};
use crate::repo::types::{ListRepoResponse, Visibility};
use crate::rt::block_on;
use crate::utils::cli::format_size;
use axum::http::{HeaderMap, StatusCode};
use chrono::{DateTime, Local, Utc};
use clap::{Parser, Subcommand};
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use reqwest::{RequestBuilder, Response};
use serde_json::json;

#[derive(Parser, Debug)]
pub struct RepoArgs {
    /// Registry host in `host[:port]` format (optional if only one server is configured)
    #[arg(long, value_parser = parse_registry_host_arg)]
    url: Option<String>,
    /// Skip TLS certificate verification for HTTPS registry.
    #[arg(long)]
    skip_tls_verify: bool,
    #[clap(subcommand)]
    sub: RepoSubArgs,
}

#[derive(Subcommand, Debug)]
enum RepoSubArgs {
    /// List all repositories, including others and mine.
    List,
    /// Change the visibility of a repository.
    Vis {
        name: String,
        visibility: Visibility,
    },
}

pub fn repo(args: RepoArgs) -> anyhow::Result<()> {
    let auth_config = AuthConfig::load()?;
    let entry = auth_config.resolve_entry(args.url.as_ref())?.clone();
    let scheme = auth_config.registry_scheme(&entry.url);
    let skip_tls_verify = effective_skip_tls_verify(args.skip_tls_verify, scheme, &entry.url);
    block_on(async move {
        match args.sub {
            RepoSubArgs::List => handle_repo_list(&entry, scheme, skip_tls_verify).await,
            RepoSubArgs::Vis { name, visibility } => {
                handle_repo_visibility(&entry, scheme, skip_tls_verify, name, visibility).await
            }
        }
    })?
}

async fn handle_repo_list(
    entry: &AuthEntry,
    scheme: RegistryScheme,
    skip_tls_verify: bool,
) -> anyhow::Result<()> {
    let client = client_with_authentication(&entry.pat, skip_tls_verify).await?;
    let url = api_url(scheme, &entry.url, "api/v1/repo");

    let res = send_and_handle_unexpected(client.get(&url))
        .await?
        .json::<ListRepoResponse>()
        .await?;

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["repository", "visibility", "tags", "size", "updated"]);

    res.data.into_iter().for_each(|view| {
        let visibility = if view.is_public { "public" } else { "private" };
        let tags = if view.tags.is_empty() {
            "-".to_string()
        } else {
            view.tags.join("\n")
        };
        let size = match (view.size_tag.as_deref(), view.size_bytes) {
            (Some(tag), Some(size_bytes)) => format!("{tag}  {}", format_size(size_bytes)),
            _ => "-".to_string(),
        };
        let updated = format_local_timestamp(view.last_pushed_at);
        table.add_row(vec![
            format!("{}/{}", view.namespace, view.name),
            visibility.to_string(),
            tags,
            size,
            updated,
        ]);
    });

    println!("{table}");
    Ok(())
}

fn format_local_timestamp(value: Option<DateTime<Utc>>) -> String {
    match value {
        Some(utc) => {
            let local: DateTime<Local> = DateTime::from(utc);
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        None => "-".to_string(),
    }
}

async fn handle_repo_visibility(
    entry: &AuthEntry,
    scheme: RegistryScheme,
    skip_tls_verify: bool,
    name: impl AsRef<str>,
    visibility: Visibility,
) -> anyhow::Result<()> {
    let client = client_with_authentication(&entry.pat, skip_tls_verify).await?;
    let url = api_url(
        scheme,
        &entry.url,
        format!("api/v1/{}/visibility", name.as_ref()),
    );

    send_and_handle_unexpected(client.put(&url).json(&json!({
        "visibility": visibility.to_string(),
    })))
    .await?;
    Ok(())
}

pub async fn client_with_authentication(
    pat: impl AsRef<str>,
    skip_tls_verify: bool,
) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {}", pat.as_ref()).parse()?);

    Ok(reqwest::Client::builder()
        .default_headers(headers)
        .danger_accept_invalid_certs(skip_tls_verify)
        .build()?)
}

pub async fn send_and_handle_unexpected(builder: RequestBuilder) -> anyhow::Result<Response> {
    let res = builder.send().await?;
    match res.status() {
        StatusCode::OK => Ok(res),
        StatusCode::INTERNAL_SERVER_ERROR => anyhow::bail!("a internal error occurred"),
        StatusCode::NOT_FOUND => anyhow::bail!("request url {} not found", res.url()),
        StatusCode::UNAUTHORIZED => anyhow::bail!("Please log in again."),
        _ => anyhow::bail!("request failed with error: {}", res.text().await?),
    }
}
