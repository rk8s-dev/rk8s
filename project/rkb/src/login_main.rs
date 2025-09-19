use crate::rt::block_on;
use anyhow::Context;
use clap::Parser;
use qrcode::QrCode;
use qrcode::render::unicode;
use reqwest::Client;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug, Parser)]
pub struct LoginArgs {
    /// URL of the distribution server (optional if only one server is configured)
    url: Option<String>,
    /// Github OAuth app client id (required for first login to this server)
    client_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct LoginConfig {
    pub entries: Vec<LoginEntry>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct LoginEntry {
    pub pat: String,
    pub url: String,
    pub client_id: String,
}

impl LoginEntry {
    pub fn new(
        pat: impl Into<String>,
        url: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        Self {
            pat: pat.into(),
            url: url.into(),
            client_id: client_id.into(),
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

    /// Note: if load the config with sudo, it will load from `/root/.config/rk8s/rkb.toml`, which may not be expected.
    pub fn load() -> anyhow::Result<Self> {
        confy::load::<Self>(Self::APP_NAME, Self::CONFIG_NAME).with_context(|| {
            format!(
                "failed to load config file `{}.{}`",
                Self::APP_NAME,
                Self::CONFIG_NAME,
            )
        })
    }

    fn store(&self) -> anyhow::Result<()> {
        confy::store(Self::APP_NAME, Self::CONFIG_NAME, self).with_context(|| {
            format!(
                "failed to store config file `{}.{}`",
                Self::APP_NAME,
                Self::CONFIG_NAME,
            )
        })
    }

    pub fn login(
        pat: impl Into<String>,
        url: impl Into<String>,
        client_id: impl Into<String>,
    ) -> anyhow::Result<()> {
        let mut config = Self::load()?;

        let url = url.into();
        let entry = LoginEntry::new(pat, &url, client_id);
        if let Some((idx, _)) = config
            .entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.url == url)
        {
            config.entries.remove(idx);
        }

        config.entries.push(entry);
        println!("{:#?}", config.entries);
        config.store()
    }

    pub fn logout(url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = Self::load()?;
        let url = url.into();
        config.entries.retain(|entry| entry.url != url);
        config.store()
    }
}

pub fn login(args: LoginArgs) -> anyhow::Result<()> {
    assert_not_sudo("login")?;
    let config = LoginConfig::load()?;

    let url = match args.url {
        Some(ref url) => url,
        None => &config.single_entry()?.url,
    };

    let client_id = match args.client_id {
        Some(ref id) => id,
        None => {
            &config
                .find_entry_by_url(url)
                .with_context(|| "Please set the github oauth client id")?
                .client_id
        }
    };

    block_on(async move {
        let oauth = OAuthFlow::new(client_id)?;
        let res = oauth.request_token().await?;

        let req_url = format!("http://{url}/api/v1/auth/github/callback");
        let res = Client::new()
            .post(req_url)
            .json(&res)
            .send()
            .await?
            .json::<CallbackResponse>()
            .await
            .with_context(|| "Failed to deserialize")?;

        LoginConfig::login(res.pat, url, client_id)?;
        println!("Logged in successfully!");
        Ok(())
    })?
}

#[derive(Deserialize)]
struct CallbackResponse {
    pat: String,
}

#[derive(Default)]
struct OAuthFlow {
    client: Option<Client>,
    client_id: String,
}

#[derive(Deserialize)]
struct RequestCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    interval: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PollTokenResponse {
    Ok(PollTokenOk),
    Err {
        error: PollTokenErrorKind,
        error_description: String,
        interval: Option<u64>,
    },
}

#[derive(Serialize, Deserialize)]
struct PollTokenOk {
    access_token: String,
    token_type: String,
    scope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum PollTokenErrorKind {
    AuthorizationPending,
    SlowDown,
    ExpiredToken,
    UnsupportedGrantType,
    IncorrectClientCredentials,
    IncorrectDeviceCode,
    AccessDenied,
    DeviceFlowDisabled,
}

impl OAuthFlow {
    pub fn new(client_id: impl Into<String>) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert("Accept", "application/json".parse()?);

        Ok(Self {
            client: Some(Client::builder().default_headers(headers).build()?),
            client_id: client_id.into(),
        })
    }

    fn client_ref(&self) -> &Client {
        self.client.as_ref().unwrap()
    }

    fn display_qr_and_code(uri: impl AsRef<str>, user_code: impl AsRef<str>) {
        let code = QrCode::new(uri.as_ref().as_bytes()).unwrap();
        let image = code.render::<unicode::Dense1x2>().build();

        println!("Scan the QR code below with your phone to open the authentication page:");
        println!("{image}");
        println!("Or visit this URL in your browser: {}", uri.as_ref());
        println!("Your one-time code is: {}", user_code.as_ref());
    }

    async fn request_code(&self) -> anyhow::Result<RequestCodeResponse> {
        let url = "https://github.com/login/device/code";
        let scope = "read:user";

        self.client_ref()
            .post(url)
            .form(&json!({
                "client_id": self.client_id,
                "scope": scope,
            }))
            .send()
            .await?
            .json()
            .await
            .with_context(|| "Failed to deserialize response, maybe you set a invalid client id?")
    }

    async fn poll_token(
        &self,
        interval: u64,
        device_code: impl AsRef<str>,
    ) -> anyhow::Result<PollTokenOk> {
        let url = "https://github.com/login/oauth/access_token";

        let mut sleep_secs = interval;
        loop {
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;

            let res = self
                .client_ref()
                .post(url)
                .form(&json!({
                    "client_id": self.client_id,
                    "device_code": device_code.as_ref(),
                    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                }))
                .send()
                .await?
                .json::<PollTokenResponse>()
                .await
                .with_context(|| "Failed to deserialize response")?;

            match res {
                PollTokenResponse::Ok(res) => return Ok(res),
                PollTokenResponse::Err {
                    error,
                    error_description,
                    interval,
                    ..
                } => match error {
                    PollTokenErrorKind::AuthorizationPending => {}
                    PollTokenErrorKind::SlowDown => {
                        let interval = interval.unwrap();
                        sleep_secs = interval;
                    }
                    _ => anyhow::bail!("{error_description}"),
                },
            }
        }
    }

    pub async fn request_token(&self) -> anyhow::Result<PollTokenOk> {
        let res = self.request_code().await?;
        Self::display_qr_and_code(res.verification_uri, res.user_code);
        self.poll_token(res.interval, res.device_code).await
    }
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

pub fn assert_not_sudo(name: impl AsRef<str>) -> anyhow::Result<()> {
    if nix::unistd::getuid().is_root() {
        anyhow::bail!("`rkb {}` should not be run with sudo", name.as_ref())
    }
    Ok(())
}
