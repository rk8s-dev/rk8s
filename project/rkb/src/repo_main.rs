use crate::login_main::{LoginEntry, with_resolved_entry};
use axum::http::StatusCode;
use clap::{Parser, Subcommand};
use comfy_table::Table;
use comfy_table::presets::UTF8_FULL;
use reqwest::header::HeaderMap;
use reqwest::{RequestBuilder, Response};
use serde::Deserialize;
use serde_json::Value;

#[derive(Parser, Debug)]
pub struct RepoArgs {
    url: Option<String>,
    /// list all repositories, including others and mine.
    #[clap(subcommand)]
    sub: RepoSubArgs,
}

#[derive(Subcommand, Debug)]
pub enum RepoSubArgs {
    
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

pub async fn main(args: ListArgs) -> anyhow::Result<()> {
    match args {
        ListArgs::Repo { url } => {
            with_resolved_entry(url, move |entry: &LoginEntry| {
                Box::pin(async move {
                    let pat = &entry.pat;
                    let client = client_with_authentication(pat).await?;
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
                })
            })
            .await
        }
    }
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
        _ => Err(anyhow::anyhow!("request failed with error: {}", res.text().await?)),
    }
}
