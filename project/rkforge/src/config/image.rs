use crate::config::auth::RkforgeConfig;
use crate::utils::cli::{original_user_config_path, original_user_home_dir};
use anyhow::{Context, Result, bail};
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::{fs, io::Write};

pub static CONFIG: Lazy<Config> =
    Lazy::new(|| Config::new().expect("Failed to initialize configuration"));

static REGISTRY: &str = "47.79.87.161:8968";
static ROOT_PATH: &str = "/var/lib/rkforge";

fn current_user_is_root() -> bool {
    nix::unistd::getuid().is_root()
}

pub(crate) fn default_storage_root(is_root: bool) -> Result<PathBuf> {
    if is_root {
        Ok(PathBuf::from(ROOT_PATH))
    } else {
        Ok(dirs::data_dir()
            .context("Failed to get user data directory")?
            .join("rk8s"))
    }
}

fn expand_home(path: &str) -> Result<PathBuf> {
    match path {
        "~" => original_user_home_dir().context("Failed to get home directory for `~`"),
        value => match value.strip_prefix("~/") {
            Some(suffix) => Ok(original_user_home_dir()
                .context("Failed to get home directory for `~/`")?
                .join(suffix)),
            None => Ok(PathBuf::from(value)),
        },
    }
}

fn resolve_storage_root_with_source(
    config: &RkforgeConfig,
    is_root: bool,
) -> Result<(PathBuf, bool)> {
    match config.storage_root() {
        Some(path) => {
            let expanded = expand_home(path)
                .with_context(|| format!("Failed to resolve image.storage path `{path}`"))?;
            if !expanded.is_absolute() {
                bail!(
                    "Configured image.storage `{path}` must be an absolute path or start with `~/`"
                );
            }
            Ok((expanded, true))
        }
        None => default_storage_root(is_root).map(|p| (p, false)),
    }
}

pub(crate) fn resolve_storage_root_from_config(
    config: &RkforgeConfig,
    is_root: bool,
) -> Result<PathBuf> {
    resolve_storage_root_with_source(config, is_root).map(|(path, _)| path)
}

pub(crate) fn resolve_storage_root_for_current_user(config: &RkforgeConfig) -> Result<PathBuf> {
    resolve_storage_root_from_config(config, current_user_is_root())
}

fn resolve_storage_root(is_root: bool) -> Result<(PathBuf, bool)> {
    resolve_storage_root_with_loader(is_root, || {
        let config_path = original_user_config_path("rk8s", Some("rkforge"))
            .with_context(|| "Failed to resolve rkforge config path")?;
        RkforgeConfig::load_from(&config_path).with_context(|| {
            format!(
                "Failed to load rkforge config from {}",
                config_path.display()
            )
        })
    })
}

fn resolve_storage_root_with_loader<F>(is_root: bool, load_config: F) -> Result<(PathBuf, bool)>
where
    F: FnOnce() -> Result<RkforgeConfig>,
{
    match load_config() {
        Ok(config) => resolve_storage_root_with_source(&config, is_root),
        Err(err) => {
            tracing::warn!(
                error = ?err,
                "Failed to read rkforge config for image.storage, falling back to default root"
            );
            default_storage_root(is_root).map(|path| (path, false))
        }
    }
}

/// Validates the storage root when the path already exists.
///
/// If `root_dir` or its parent does not exist yet, validation is deferred and
/// creation is handled later in `Config::new`.
fn validate_storage_root(root_dir: &Path) -> Result<()> {
    if root_dir.exists() {
        let metadata = fs::metadata(root_dir)
            .with_context(|| format!("Failed to stat storage root {}", root_dir.display()))?;
        if !metadata.is_dir() {
            bail!(
                "Configured image.storage `{}` is not a directory",
                root_dir.display()
            );
        }
    } else if let Some(parent) = root_dir.parent()
        && parent.exists()
    {
        let parent_meta = fs::metadata(parent)
            .with_context(|| format!("Failed to stat parent directory {}", parent.display()))?;
        if !parent_meta.is_dir() {
            bail!(
                "Parent path of image.storage `{}` is not a directory",
                root_dir.display()
            );
        }
    }
    Ok(())
}

fn ensure_storage_root_writable(root_dir: &Path) -> Result<()> {
    fs::create_dir_all(root_dir).with_context(|| {
        format!(
            "Failed to create configured image.storage directory {}",
            root_dir.display()
        )
    })?;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let probe = root_dir.join(format!(
        ".rkforge-write-probe-{}-{nanos}",
        std::process::id()
    ));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .with_context(|| {
            format!(
                "Configured image.storage `{}` is not writable",
                root_dir.display()
            )
        })?;
    file.write_all(b"")
        .with_context(|| format!("Failed to write probe file {}", probe.display()))?;
    drop(file);
    fs::remove_file(&probe)
        .with_context(|| format!("Failed to remove probe file {}", probe.display()))?;
    Ok(())
}

/// Configuration for rkforge build
#[derive(Debug)]
pub struct Config {
    pub layers_store_root: PathBuf,
    pub build_dir: PathBuf,
    pub metadata_dir: PathBuf,
    pub default_registry: String,
    pub is_root: bool,
    /// Container rootfs mount mode: true=persistent overlay mount, false=traditional cp mode
    pub use_overlay_rootfs: bool,
    /// Overlay backend: true=libfuse (unprivileged), false=Linux native (requires root)
    pub use_libfuse_overlay: bool,
}

impl Config {
    pub fn new() -> Result<Self> {
        let is_root = current_user_is_root();
        let (root_dir, from_config) = resolve_storage_root(is_root)?;
        validate_storage_root(&root_dir)?;
        if from_config {
            ensure_storage_root_writable(&root_dir)?;
        }
        let layers_store_root = root_dir.join("layers");
        let build_dir = root_dir.join("build");
        let metadata_dir = root_dir.join("metadata");

        fs::create_dir_all(&layers_store_root).with_context(|| {
            format!(
                "Failed to create layers directory at {:?}",
                layers_store_root
            )
        })?;
        fs::create_dir_all(&build_dir)
            .with_context(|| format!("Failed to create build directory at {:?}", build_dir))?;
        fs::create_dir_all(&metadata_dir).with_context(|| {
            format!("Failed to create metadata directory at {:?}", metadata_dir)
        })?;

        Ok(Self {
            layers_store_root,
            build_dir,
            metadata_dir,
            default_registry: String::from(REGISTRY),
            is_root,
            use_overlay_rootfs: std::env::var("RKFORGE_OVERLAY_ROOTFS")
                .map(|v| v != "0")
                .unwrap_or(true),
            use_libfuse_overlay: std::env::var("RKFORGE_USE_LIBFUSE")
                .map(|v| v == "1")
                .unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        default_storage_root, ensure_storage_root_writable, expand_home,
        resolve_storage_root_from_config, resolve_storage_root_with_loader, validate_storage_root,
    };
    use crate::config::auth::RkforgeConfig;
    use crate::utils::cli::original_user_home_dir;
    use anyhow::anyhow;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_expand_home_plain_path() {
        let path = expand_home("/tmp/rkforge").unwrap();
        assert_eq!(path.to_string_lossy(), "/tmp/rkforge");
    }

    #[test]
    fn test_expand_home_tilde_path() {
        match original_user_home_dir().ok() {
            Some(home) => {
                let path = expand_home("~/rkforge-storage").unwrap();
                assert_eq!(path, home.join("rkforge-storage"));
            }
            None => assert!(expand_home("~/rkforge-storage").is_err()),
        }
    }

    #[test]
    fn test_default_storage_root_for_root() {
        let path = default_storage_root(true).unwrap();
        assert_eq!(path.to_string_lossy(), "/var/lib/rkforge");
    }

    #[test]
    fn test_default_storage_root_for_non_root() {
        match dirs::data_dir() {
            Some(data_dir) => {
                let path = default_storage_root(false).unwrap();
                assert_eq!(path, data_dir.join("rk8s"));
            }
            None => assert!(default_storage_root(false).is_err()),
        }
    }

    #[test]
    fn test_validate_storage_root_rejects_file() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("not-a-dir");
        fs::write(&file, "x").unwrap();
        assert!(validate_storage_root(&file).is_err());
    }

    #[test]
    fn test_ensure_storage_root_writable_creates_probe_successfully() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("storage-root");
        ensure_storage_root_writable(&root).unwrap();
        assert!(root.exists());
        assert!(root.is_dir());
    }

    #[test]
    fn test_resolve_storage_root_from_config_file() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("rkforge.toml");
        fs::write(
            &config_path,
            r#"
[image]
storage = "/tmp/rkforge-storage-test"
"#,
        )
        .unwrap();

        let config = RkforgeConfig::load_from(&config_path).unwrap();
        let root = resolve_storage_root_from_config(&config, false).unwrap();
        assert_eq!(root, PathBuf::from("/tmp/rkforge-storage-test"));
    }

    #[test]
    fn test_validate_storage_root_allows_nonexistent_path() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("missing").join("storage");
        assert!(validate_storage_root(&root).is_ok());
    }

    #[test]
    fn test_resolve_storage_root_falls_back_when_config_load_fails() {
        let (root, from_config) =
            resolve_storage_root_with_loader(false, || Err(anyhow!("load failed"))).unwrap();
        assert!(!from_config);
        assert_eq!(root, default_storage_root(false).unwrap());
    }

    #[test]
    fn test_resolve_storage_root_from_config_rejects_relative_path() {
        let mut config = RkforgeConfig::default();
        config.image.storage = Some("relative/path".to_string());
        let err = resolve_storage_root_from_config(&config, false).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("must be an absolute path"),
            "unexpected error message: {msg}"
        );
    }
}
