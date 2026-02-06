use crate::config::image::CONFIG;
use crate::utils::cli::original_user_config_path;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::pin::Pin;

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct AuthConfig {
    pub entries: Vec<AuthEntry>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct AuthEntry {
    pub pat: String,
    pub url: String,
}

impl AuthEntry {
    pub fn new(pat: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            pat: pat.into(),
            url: url.into(),
        }
    }
}

impl AuthConfig {
    const APP_NAME: &'static str = "rk8s";
    const CONFIG_NAME: &'static str = "rkforge";

    pub fn single_entry(&self) -> anyhow::Result<&AuthEntry> {
        match self.entries.len() {
            0 => anyhow::bail!("No entries. Maybe you need to set a url."),
            1 => Ok(self.entries.first().unwrap()),
            _ => anyhow::bail!("There are many entries. Maybe you need to select a url."),
        }
    }

    pub fn find_entry_by_url(&self, url: impl AsRef<str>) -> anyhow::Result<&AuthEntry> {
        let url = url.as_ref();
        self.entries
            .iter()
            .find(|entry| entry.url == url)
            .ok_or_else(|| anyhow::anyhow!("Failed to find entry with url {}", url))
    }

    pub fn resolve_entry(&self, url: Option<impl AsRef<str>>) -> anyhow::Result<&AuthEntry> {
        let entry = match url {
            Some(url) => self.find_entry_by_url(url.as_ref())?,
            None => self.single_entry()?,
        };
        Ok(entry)
    }

    /// Resolves the final registry URL based on a specific priority order.
    ///
    /// The resolution follows this priority order:
    /// 1. The URL provided in the `url` parameter, if specified.
    /// 2. The URL from the configuration file, if a single registry is configured.
    /// 3. The default registry URL as a final fallback.
    pub fn resolve_url(&self, url: Option<impl AsRef<str>>) -> String {
        if let Some(url) = url {
            return url.as_ref().to_string();
        }
        self.single_entry()
            .map(|entry| entry.url.to_string())
            .unwrap_or(CONFIG.default_registry.to_string())
    }

    pub fn with_single_entry<F, R>(&self, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&AuthEntry) -> anyhow::Result<R>,
    {
        f(self.single_entry()?)
    }

    pub fn with_resolved_entry<F, R>(&self, url: Option<impl AsRef<str>>, f: F) -> anyhow::Result<R>
    where
        F: FnOnce(&AuthEntry) -> anyhow::Result<R>,
    {
        f(self.resolve_entry(url)?)
    }

    /// Loads the config from pre-defined config path.
    pub fn load() -> anyhow::Result<Self> {
        let path = original_user_config_path(Self::APP_NAME, Some(Self::CONFIG_NAME))?;
        Self::load_from(path)
    }

    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        confy::load_path(path)
            .with_context(|| format!("failed to load config file `{}`", path.display()))
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

    pub fn is_anonymous(&self, url: impl AsRef<str>) -> bool {
        let url = url.as_ref();
        self.entries.iter().all(|entry| entry.url != url)
    }

    pub fn login(pat: impl Into<String>, url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = Self::load()?;

        let url = url.into();
        let entry = AuthEntry::new(pat, &url);
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
    F: for<'a> FnOnce(&'a AuthEntry) -> Pin<Box<dyn Future<Output = anyhow::Result<R>> + 'a>>,
{
    let config = AuthConfig::load()?;

    let entry = match url {
        Some(url) => config.find_entry_by_url(url.as_ref())?,
        None => config.single_entry()?,
    };

    f(entry).await
}
