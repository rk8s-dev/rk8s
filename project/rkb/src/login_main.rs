use anyhow::Context;
use axum::Router;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use clap::Parser;
use directories::ProjectDirs;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tokio::fs;
use tokio::sync::oneshot;
use tokio::sync::oneshot::Sender;

#[derive(Debug, Parser)]
pub struct LoginArgs {
    url: String,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct LoginConfig {
    pub entries: Vec<LoginEntry>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq, Clone)]
pub struct LoginEntry {
    pub pat: String,
    pub url: String,
}

impl LoginEntry {
    pub fn new(pat: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            pat: pat.into(),
            url: url.into(),
        }
    }
}

impl LoginConfig {
    const APP_NAME: &'static str = "rk8s";
    const CONFIG_NAME: &'static str = "rkb";

    pub fn single_entry(&self) -> anyhow::Result<&LoginEntry> {
        match self.entries.len() {
            0 => anyhow::bail!("No entries, please log in first."),
            1 => Ok(self.entries.first().unwrap()),
            _ => anyhow::bail!("There are many entries, please select a url."),
        }
    }

    pub fn find_entry_by_url(&self, url: &str) -> anyhow::Result<&LoginEntry> {
        self.entries
            .iter()
            .find(|entry| entry.url == url)
            .ok_or_else(|| anyhow::anyhow!("Failed to find entry with url {}", url))
    }

    pub fn with_single_entry<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&LoginEntry) -> anyhow::Result<R>,
    {
        f(self.single_entry()?)
    }

    pub fn load() -> anyhow::Result<Self> {
        confy::load::<Self>(Self::APP_NAME, Self::CONFIG_NAME).with_context(|| {
            format!(
                "failed to load config file `{}.{}`",
                Self::APP_NAME,
                Self::CONFIG_NAME
            )
        })
    }

    fn store(&self) -> anyhow::Result<()> {
        confy::store(Self::APP_NAME, Self::CONFIG_NAME, self).with_context(|| {
            format!(
                "failed to store config file `{}.{}`",
                Self::APP_NAME,
                Self::CONFIG_NAME
            )
        })
    }

    pub fn login(pat: impl Into<String>, url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = Self::load()?;

        let url = url.into();
        let entry = LoginEntry::new(pat, &url);
        if let Some((idx, _)) = config
            .entries
            .iter()
            .enumerate()
            .find(|(idx, entry)| entry.url == url)
        {
            config.entries.remove(idx);
        }

        config.entries.push(entry);
        config.store()
    }

    pub fn logout(url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = Self::load()?;
        let url = url.into();
        config.entries.retain(|entry| entry.url != url);
        config.store()
    }
}

pub async fn login(args: LoginArgs) -> anyhow::Result<()> {
    let (tx, rx) = oneshot::channel();
    let state = AppState {
        oneshot: Mutex::new(Some(tx)),
        url: args.url.clone(),
    };

    tokio::spawn(async move {
        let router = local_callback_server(state);
        let listener = tokio::net::TcpListener::bind("0.0.0.0:8969")
            .await
            .with_context(|| "Failed to listen local callback server")?;

        axum::serve(listener, router)
            .await
            .with_context(|| "Failed to start local callback server")?;
        Ok::<_, anyhow::Error>(())
    });

    let auth_url = "https://github.com/login/oauth/authorize?client_id=Ov23liWfNNbVkVKkxOGr&scope=read:user&redirect_uri=http://localhost:8969/";
    match opener::open(auth_url) {
        Ok(_) => {
            println!("Please complete authorization in the opened browser");
        }
        x @ Err(_) => return x.with_context(|| "Could not open url"),
    }

    let res = rx.await??;
    LoginConfig::login(&res.pat, args.url)?;
    println!("Logged in successfully!");
    Ok(())
}

struct AppState {
    oneshot: Mutex<Option<Sender<anyhow::Result<LoginResponse>>>>,
    url: String,
}

fn local_callback_server(state: AppState) -> Router {
    Router::new()
        .route("/", get(request_token))
        .with_state(Arc::new(state))
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pat: String,
}

async fn request_token(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let code = &params["code"];
    match reqwest::get(format!(
        "http://{}/api/v1/auth/github/callback?code={code}",
        state.url
    ))
    .await
    {
        Ok(res) => {
            let oneshot = state.oneshot.lock().unwrap().take().unwrap();
            let res = res
                .json()
                .await
                .with_context(|| "Failed to parse json from response");
            oneshot.send(res).unwrap();
        }
        Err(e) => {
            let oneshot = state.oneshot.lock().unwrap().take().unwrap();
            oneshot
                .send(Err(anyhow::anyhow!("Failed to request token: {e}")))
                .unwrap();
        }
    }
    StatusCode::OK
}

pub async fn with_resolved_entry<F, R>(url: Option<impl AsRef<str>>, f: F) -> anyhow::Result<R>
where
    F: for<'a> FnOnce(&'a LoginEntry) -> Pin<Box<dyn Future<Output = anyhow::Result<R>> + 'a>>,
{
    let config = LoginConfig::load()?;

    let entry = match url {
        Some(url) => config.find_entry_by_url(url.as_ref())?,
        None => config.single_entry()?,
    };

    f(entry).await
}
