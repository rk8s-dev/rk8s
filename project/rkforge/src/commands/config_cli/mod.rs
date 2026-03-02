use crate::config::auth::RkforgeConfig;
use crate::config::image::resolve_storage_root_for_current_user;
use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::path::Path;

const IMAGE_STORAGE_KEY: &str = "image.storage";

#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub sub: ConfigSubCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigSubCommand {
    /// Set a config key to a value
    Set {
        /// Config key, currently only supports image.storage
        key: String,
        /// Config value
        value: String,
    },
    /// Get effective value of a config key
    Get {
        /// Config key, currently only supports image.storage
        key: String,
    },
}

pub fn config(args: ConfigArgs) -> Result<()> {
    match args.sub {
        ConfigSubCommand::Set { key, value } => {
            let mut cfg = RkforgeConfig::load()?;
            set_value(&mut cfg, &key, &value)?;
            cfg.store()?;
        }
        ConfigSubCommand::Get { key } => {
            let cfg = RkforgeConfig::load()?;
            let value = get_value(&cfg, &key)?;
            println!("{value}");
        }
    }
    Ok(())
}

fn set_value(cfg: &mut RkforgeConfig, key: &str, value: &str) -> Result<()> {
    match key {
        IMAGE_STORAGE_KEY => {
            let value = value.trim();
            if value.is_empty() {
                bail!("config value for `{IMAGE_STORAGE_KEY}` must not be empty");
            }
            validate_storage_value(value)?;
            cfg.image.storage = Some(value.to_string());
            Ok(())
        }
        _ => bail!("unsupported config key `{key}`. supported keys: {IMAGE_STORAGE_KEY}"),
    }
}

fn validate_storage_value(value: &str) -> Result<()> {
    if value == "~" || value.starts_with("~/") {
        return Ok(());
    }

    if value.starts_with('~') {
        bail!(
            "unsupported home path `{value}`: only `~` and `~/...` are supported for {IMAGE_STORAGE_KEY}"
        );
    }

    if !Path::new(value).is_absolute() {
        bail!("config value for `{IMAGE_STORAGE_KEY}` must be an absolute path or start with `~/`");
    }
    Ok(())
}

fn get_value(cfg: &RkforgeConfig, key: &str) -> Result<String> {
    match key {
        IMAGE_STORAGE_KEY => Ok(resolve_storage_root_for_current_user(cfg)?
            .to_string_lossy()
            .to_string()),
        _ => bail!("unsupported config key `{key}`. supported keys: {IMAGE_STORAGE_KEY}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{IMAGE_STORAGE_KEY, get_value, set_value};
    use crate::config::auth::RkforgeConfig;

    #[test]
    fn test_set_image_storage_key() {
        let mut cfg = RkforgeConfig::default();
        set_value(&mut cfg, IMAGE_STORAGE_KEY, "/data/rkforge").unwrap();
        assert_eq!(cfg.image.storage.as_deref(), Some("/data/rkforge"));
    }

    #[test]
    fn test_get_image_storage_key_from_config() {
        let mut cfg = RkforgeConfig::default();
        cfg.image.storage = Some("/data/rkforge".to_string());
        let value = get_value(&cfg, IMAGE_STORAGE_KEY).unwrap();
        assert_eq!(value, "/data/rkforge");
    }

    #[test]
    fn test_get_image_storage_key_fallback_to_default() {
        let cfg = RkforgeConfig::default();
        let value = get_value(&cfg, IMAGE_STORAGE_KEY).unwrap();
        assert!(!value.trim().is_empty());
    }

    #[test]
    fn test_set_rejects_empty_value() {
        let mut cfg = RkforgeConfig::default();
        assert!(set_value(&mut cfg, IMAGE_STORAGE_KEY, "   ").is_err());
    }

    #[test]
    fn test_set_rejects_relative_path() {
        let mut cfg = RkforgeConfig::default();
        assert!(set_value(&mut cfg, IMAGE_STORAGE_KEY, "relative/path").is_err());
    }

    #[test]
    fn test_set_rejects_tilde_username_path() {
        let mut cfg = RkforgeConfig::default();
        assert!(set_value(&mut cfg, IMAGE_STORAGE_KEY, "~foo/data").is_err());
    }

    #[test]
    fn test_set_unknown_key_fails() {
        let mut cfg = RkforgeConfig::default();
        assert!(set_value(&mut cfg, "unknown.key", "/data/rkforge").is_err());
    }

    #[test]
    fn test_get_unknown_key_fails() {
        let cfg = RkforgeConfig::default();
        assert!(get_value(&cfg, "unknown.key").is_err());
    }
}
