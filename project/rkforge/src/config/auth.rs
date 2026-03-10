use crate::config::image::CONFIG;
use crate::registry::{RegistryScheme, parse_registry_host, scheme_for_registry};
use crate::utils::cli::original_user_config_path;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use std::pin::Pin;

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct ImageConfig {
    pub storage: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct RegistryConfig {
    #[serde(default, rename = "insecure-registries", alias = "insecure_registries")]
    pub insecure_registries: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
pub struct RkforgeConfig {
    #[serde(default)]
    pub entries: Vec<AuthEntry>,
    #[serde(default)]
    pub registry: RegistryConfig,
    #[serde(
        default,
        rename = "insecure-registries",
        alias = "insecure_registries",
        skip_serializing
    )]
    legacy_insecure_registries: Vec<String>,
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
    #[serde(default)]
    pub insecure_registries: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default, Ord, PartialOrd, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
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
        let url = parse_registry_host(url.as_ref())?;
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
    pub fn resolve_url(&self, url: Option<impl AsRef<str>>) -> anyhow::Result<String> {
        if let Some(url) = url {
            return parse_registry_host(url.as_ref());
        }
        if let Ok(entry) = self.single_entry() {
            return Ok(entry.url.to_string());
        }
        parse_registry_host(&CONFIG.default_registry)
            .with_context(|| "failed to normalize default registry")
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
        Self::from_rkforge_config(config)
    }

    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let config = RkforgeConfig::load_from(path)?;
        Self::from_rkforge_config(config)
    }

    pub fn insecure_registries(&self) -> &[String] {
        &self.insecure_registries
    }

    pub fn registry_scheme(&self, registry: impl AsRef<str>) -> RegistryScheme {
        scheme_for_registry(registry, &self.insecure_registries)
    }

    fn from_rkforge_config(config: RkforgeConfig) -> anyhow::Result<Self> {
        let mut insecure_registries = config.registry.insecure_registries;
        insecure_registries.extend(config.legacy_insecure_registries);
        Ok(Self {
            entries: normalize_entries(config.entries)?,
            insecure_registries: normalize_registry_list(
                insecure_registries,
                "insecure_registries",
            )?,
        })
    }

    pub fn is_anonymous(&self, url: impl AsRef<str>) -> bool {
        let Ok(url) = parse_registry_host(url.as_ref()) else {
            return true;
        };
        self.entries.iter().all(|entry| entry.url != url)
    }

    pub fn login(pat: impl Into<String>, url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = RkforgeConfig::load()?;
        let url = parse_registry_host(url.into())?;
        let entry = AuthEntry::new(pat, &url);
        if let Some((idx, _)) = config
            .entries
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.url == url)
        {
            config.entries.remove(idx);
        }

        config.entries.iter_mut().try_for_each(|entry| {
            entry.url = parse_registry_host(&entry.url)?;
            Ok::<(), anyhow::Error>(())
        })?;
        config.registry.insecure_registries =
            normalize_registry_list(config.registry.insecure_registries, "insecure_registries")?;
        config.entries.push(entry);
        config.store()
    }

    pub fn logout(url: impl Into<String>) -> anyhow::Result<()> {
        let mut config = RkforgeConfig::load()?;
        let url = parse_registry_host(url.into())?;
        config.registry.insecure_registries =
            normalize_registry_list(config.registry.insecure_registries, "insecure_registries")?;
        config.entries.retain(|entry| entry.url != url);
        config.store()
    }
}

fn normalize_entries(entries: Vec<AuthEntry>) -> anyhow::Result<Vec<AuthEntry>> {
    entries
        .into_iter()
        .map(|mut entry| {
            let raw_url = entry.url.clone();
            entry.url = parse_registry_host(&entry.url)
                .with_context(|| format!("invalid registry in entries.url: {raw_url}"))?;
            Ok(entry)
        })
        .collect()
}

fn normalize_registry_list(values: Vec<String>, field: &str) -> anyhow::Result<Vec<String>> {
    let mut seen = HashSet::new();
    let mut normalized = Vec::new();
    for value in values {
        let item = parse_registry_host(&value)
            .with_context(|| format!("invalid registry in {field}: {value}"))?;
        if seen.insert(item.clone()) {
            normalized.push(item);
        }
    }
    Ok(normalized)
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
    use super::{AuthConfig, ImageConfig, RegistryConfig, RkforgeConfig};
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

    #[test]
    fn test_insecure_registries_deduped_and_normalized() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
[registry]
insecure-registries = ["Example.com", "example.com", "[::1]", "::1"]
"#,
        )
        .unwrap();

        let auth = AuthConfig::load_from(&config_path).unwrap();
        assert_eq!(
            auth.insecure_registries,
            vec!["example.com".to_string(), "[::1]".to_string()]
        );
    }

    #[test]
    fn test_legacy_top_level_insecure_registries_is_still_loaded() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
insecure_registries = ["legacy.example.com:5000"]
"#,
        )
        .unwrap();

        let auth = AuthConfig::load_from(&config_path).unwrap();
        assert_eq!(
            auth.insecure_registries,
            vec!["legacy.example.com:5000".to_string()]
        );
    }

    #[test]
    fn test_registry_table_can_appear_after_entries() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
[[entries]]
pat = "token"
url = "127.0.0.1:8968"

[image]
storage = "/data/rkforge"

[registry]
insecure-registries = ["47.79.87.161:8968"]
"#,
        )
        .unwrap();

        let config = RkforgeConfig::load_from(&config_path).unwrap();
        assert_eq!(
            config.registry,
            RegistryConfig {
                insecure_registries: vec!["47.79.87.161:8968".to_string()]
            }
        );
    }

    #[test]
    fn test_insecure_registries_inside_entry_is_rejected() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
[[entries]]
pat = "token"
url = "127.0.0.1:8968"
insecure-registries = ["47.79.87.161:8968"]
"#,
        )
        .unwrap();

        assert!(AuthConfig::load_from(&config_path).is_err());
    }
}
