use crate::login_main::{LoginEntry, with_resolved_entry};
use axum::http::StatusCode;
use clap::{Parser, Subcommand};
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use reqwest::header::HeaderMap;
use reqwest::{RequestBuilder, Response};
use serde::Deserialize;
use serde_json::json;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Parser, Debug)]
pub struct RepoArgs {
    /// Url of the distribution server.
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

#[derive(Debug, Clone)]
enum Visibility {
    Public,
    Private,
}

impl FromStr for Visibility {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "public" => Ok(Visibility::Public),
            "private" => Ok(Visibility::Private),
            _ => Err("visibility must be `public` or `private`".to_string()),
        }
    }
}

impl Display for Visibility {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Private => write!(f, "private"),
        }
    }
}

#[derive(Deserialize)]
pub struct ListRepoResponse {
    data: Vec<RepoView>,
}

#[derive(Deserialize)]
struct RepoView {
    namespace: String,
    name: String,
    is_public: bool,
}

pub async fn main(args: RepoArgs) -> anyhow::Result<()> {
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
}

async fn handle_repo_list(entry: &LoginEntry) -> anyhow::Result<()> {
    let client = client_with_authentication(&entry.pat).await?;
    let url = format!("http://{}/api/v1/repo", entry.url);

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

    println!("{}", table);
    Ok(())
}

async fn handle_repo_visibility(
    entry: &LoginEntry,
    name: impl AsRef<str>,
    visibility: Visibility,
) -> anyhow::Result<()> {
    let client = client_with_authentication(&entry.pat).await?;
    let url = format!("http://{}/api/v1/{}/visibility", entry.url, name.as_ref());

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
        StatusCode::INTERNAL_SERVER_ERROR => Err(anyhow::anyhow!("a internal error occurred")),
        StatusCode::NOT_FOUND => Err(anyhow::anyhow!("request url {} not found", res.url())),
        StatusCode::UNAUTHORIZED => anyhow::bail!("Please log in again."),
        _ => Err(anyhow::anyhow!(
            "request failed with error: {}",
            res.text().await?
        )),
    }
}
