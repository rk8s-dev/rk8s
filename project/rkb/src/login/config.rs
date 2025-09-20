use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct LoginConfig {
    pub entries: Vec<LoginEntry>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
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

    pub fn login(pat: impl Into<String>, url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = Self::load()?;

        let url = url.into();
        let entry = LoginEntry::new(pat, &url);
        if let Some((idx, _)) = config
            .entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.url == url)
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
