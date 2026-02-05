use anyhow::{Context, Result};
use oci_spec::image::{Config, ConfigBuilder};
use std::collections::HashMap;
use std::path::{Component, Path};

pub static DEFAULT_ENV: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Normalize a path by resolving `.` and `..` components.
/// This does not access the filesystem, just manipulates the path string.
fn normalize_path(path: &str) -> String {
    let path = Path::new(path);
    let mut components = Vec::new();

    for component in path.components() {
        match component {
            Component::RootDir => {
                components.clear();
                components.push(Component::RootDir);
            }
            Component::CurDir => {
                // Skip `.` components
            }
            Component::ParentDir => {
                // Pop the last component if possible (but don't go above root)
                if components.len() > 1 {
                    components.pop();
                }
            }
            Component::Normal(c) => {
                components.push(Component::Normal(c));
            }
            Component::Prefix(_) => {
                // Windows prefix, not relevant for container paths
            }
        }
    }

    if components.is_empty() {
        "/".to_string()
    } else {
        components
            .iter()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
            .replace("//", "/")
    }
}

/// Image config is used in OCI image's `config.json`.
///
/// Currently not exhaustive, only some simple fields.
///
/// Struct fields should be used to construct `OciImageConfig`.
#[derive(Debug, Clone)]
pub struct ImageConfig {
    pub labels: HashMap<String, String>,
    pub envp: HashMap<String, String>,
    pub entrypoint: Option<Vec<String>>,
    pub cmd: Option<Vec<String>>,
    /// Working directory for the container. This is set by the WORKDIR instruction.
    /// If None, defaults to "/" (root directory).
    pub working_dir: Option<String>,
    /// User to run as. This is set by the USER instruction.
    /// Format can be: "user", "uid", "user:group", "uid:gid", "uid:group", "user:gid"
    pub user: Option<String>,
}

impl ImageConfig {
    pub fn add_label(&mut self, key: String, value: String) {
        self.labels.insert(key, value);
    }

    pub fn add_envp(&mut self, key: String, value: String) {
        self.envp.insert(key, value);
    }

    pub fn set_entrypoint(&mut self, entrypoint: Vec<String>) {
        self.entrypoint = Some(entrypoint);
    }

    pub fn set_cmd(&mut self, cmd: Vec<String>) {
        self.cmd = Some(cmd);
    }

    /// Set the working directory for the container.
    /// If the path is relative, it will be resolved relative to the current working directory.
    /// Paths are normalized to resolve `.` and `..` components.
    pub fn set_working_dir(&mut self, dir: String) {
        let new_dir = if dir.starts_with('/') {
            // Normalize absolute path
            normalize_path(&dir)
        } else {
            // Resolve relative path based on current working directory
            let current = self.working_dir.as_deref().unwrap_or("/");
            let combined = if current == "/" {
                format!("/{}", dir)
            } else {
                format!("{}/{}", current, dir)
            };
            normalize_path(&combined)
        };
        self.working_dir = Some(new_dir);
    }

    /// Get the current working directory, defaulting to "/" if not set.
    pub fn get_working_dir(&self) -> &str {
        self.working_dir.as_deref().unwrap_or("/")
    }

    /// Set the user to run as.
    pub fn set_user(&mut self, user: String) {
        self.user = Some(user);
    }

    /// Get the current user, if set.
    pub fn get_user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    pub fn get_oci_image_config(&self) -> Result<Config> {
        let mut config = ConfigBuilder::default();

        if !self.labels.is_empty() {
            config = config.labels(self.labels.clone());
        }

        let env_vars = self
            .envp
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<String>>();

        config = config.env(env_vars);

        if let Some(entrypoint) = &self.entrypoint {
            config = config.entrypoint(entrypoint.clone());
        }

        if let Some(cmd) = &self.cmd {
            config = config.cmd(cmd.clone());
        }

        if let Some(working_dir) = &self.working_dir {
            config = config.working_dir(working_dir.clone());
        }

        if let Some(user) = &self.user {
            config = config.user(user.clone());
        }

        config.build().context("Failed to build OCI image config")
    }
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            labels: HashMap::new(),
            envp: HashMap::from([
                ("PATH".to_string(), DEFAULT_ENV.to_string()),
                ("DEBIAN_FRONTEND".to_string(), "noninteractive".to_string()),
            ]),
            entrypoint: None,
            cmd: None,
            working_dir: None,
            user: None,
        }
    }
}
