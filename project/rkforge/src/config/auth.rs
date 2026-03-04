use crate::config::image::CONFIG;
use crate::utils::cli::original_user_config_path;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::pin::Pin;

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct ImageConfig {
    pub storage: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct RkforgeConfig {
    #[serde(default)]
    pub entries: Vec<AuthEntry>,
    #[serde(default)]
    pub image: ImageConfig,
}

impl RkforgeConfig {
    const APP_NAME: &'static str = "rk8s";
    const CONFIG_NAME: &'static str = "rkforge";

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

    pub fn store(&self) -> anyhow::Result<()> {
        let path = original_user_config_path(Self::APP_NAME, Some(Self::CONFIG_NAME))?;
        confy::store_path(&path, self)
            .with_context(|| format!("failed to store config file `{}`", path.display()))
    }

    pub fn storage_root(&self) -> Option<&str> {
        self.image
            .storage
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct AuthConfig {
    #[serde(default)]
    pub entries: Vec<AuthEntry>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
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

    pub fn load() -> anyhow::Result<Self> {
        let config = RkforgeConfig::load()?;
        Ok(Self {
            entries: config.entries,
        })
    }

    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config = RkforgeConfig::load_from(path)?;
        Ok(Self {
            entries: config.entries,
        })
    }

    pub fn is_anonymous(&self, url: impl AsRef<str>) -> bool {
        let url = url.as_ref();
        self.entries.iter().all(|entry| entry.url != url)
    }

    pub fn login(pat: impl Into<String>, url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = RkforgeConfig::load()?;

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
        let mut config = RkforgeConfig::load()?;
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

#[cfg(test)]
mod tests {
    use super::{AuthConfig, ImageConfig, RkforgeConfig};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_storage_root_empty_is_none() {
        let config = RkforgeConfig {
            image: ImageConfig {
                storage: Some("   ".to_string()),
            },
            ..Default::default()
        };
        assert_eq!(config.storage_root(), None);
    }

    #[test]
    fn test_storage_root_trimmed_value() {
        let config = RkforgeConfig {
            image: ImageConfig {
                storage: Some("  /data/rkforge  ".to_string()),
            },
            ..Default::default()
        };
        assert_eq!(config.storage_root(), Some("/data/rkforge"));
    }

    #[test]
    fn test_load_config_with_storage_and_entries() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
[[entries]]
pat = "token"
url = "example.com"

[image]
storage = "/data/rkforge"
"#,
        )
        .unwrap();

        let config = RkforgeConfig::load_from(&config_path).unwrap();
        assert_eq!(config.entries.len(), 1);
        assert_eq!(config.storage_root(), Some("/data/rkforge"));
    }

    #[test]
    fn test_auth_view_keeps_entries_compatibility() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
[[entries]]
pat = "token"
url = "example.com"

[image]
storage = "/data/rkforge"
"#,
        )
        .unwrap();

        let auth = AuthConfig::load_from(&config_path).unwrap();
        assert_eq!(auth.entries.len(), 1);
        assert_eq!(auth.entries[0].url, "example.com");
    }
}
