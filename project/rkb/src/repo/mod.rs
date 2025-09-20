mod types;

use crate::login::config::{LoginEntry, with_resolved_entry};
use crate::repo::types::{ListRepoResponse, Visibility};
use crate::rt::block_on;
use crate::utils::cli::assert_not_sudo;
use axum::http::{HeaderMap, StatusCode};
use clap::{Parser, Subcommand};
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use reqwest::{RequestBuilder, Response};
use serde_json::json;

#[derive(Parser, Debug)]
pub struct RepoArgs {
    /// URL of the distribution server (optional if only one entry exists)
    #[arg(long)]
    url: Option<String>,
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
    assert_not_sudo("repo")?;
    block_on(async move {
        with_resolved_entry(args.url, move |entry| {
            Box::pin(async move {
                match args.sub {
                    RepoSubArgs::List => handle_repo_list(entry).await,
                    RepoSubArgs::Vis { name, visibility } => {
                        handle_repo_visibility(entry, name, visibility).await
                    }
                }
            })
        })
        .await
    })?
}

async fn handle_repo_list(entry: &LoginEntry) -> anyhow::Result<()> {
    let client = client_with_authentication(&entry.pat).await?;
    let url = format!("https://{}/api/v1/repo", entry.url);

    let res = send_and_handle_unexpected(client.get(&url))
        .await?
        .json::<ListRepoResponse>()
        .await?;

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["repository", "visibility"]);

    res.data.into_iter().for_each(|view| {
        let visibility = if view.is_public { "public" } else { "private" };
        table.add_row(vec![
            format!("{}/{}", view.namespace, view.name),
            visibility.to_string(),
        ]);
    });

    println!("{table}");
    Ok(())
}

async fn handle_repo_visibility(
    entry: &LoginEntry,
    name: impl AsRef<str>,
    visibility: Visibility,
) -> anyhow::Result<()> {
    let client = client_with_authentication(&entry.pat).await?;
    let url = format!("https://{}/api/v1/{}/visibility", entry.url, name.as_ref());

    send_and_handle_unexpected(client.put(&url).json(&json!({
        "visibility": visibility.to_string(),
    })))
    .await?;
    Ok(())
}

pub async fn client_with_authentication(pat: impl AsRef<str>) -> anyhow::Result<reqwest::Client> {
    let mut headers = HeaderMap::new();
    headers.insert("Authorization", format!("Bearer {}", pat.as_ref()).parse()?);

    Ok(reqwest::Client::builder()
        .default_headers(headers)
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
