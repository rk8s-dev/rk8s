use anyhow::{Context, Result};
use oci_spec::image::{Config, ConfigBuilder};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

pub static DEFAULT_ENV: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Normalize a path by resolving `.` and `..` components.
/// This does not access the filesystem, just manipulates the path string.
pub(crate) fn normalize_path(path: &str) -> String {
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
        let mut result = PathBuf::new();
        for c in &components {
            result.push(c);
        }
        result.to_string_lossy().to_string()
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
    pub working_dir: Option<String>,
    pub user: Option<String>,
    pub volumes: Option<Vec<String>>,
    pub stop_signal: Option<String>,
    pub exposed_ports: Option<Vec<String>>,
    pub shell: Option<Vec<String>>,
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

    pub fn get_working_dir(&self) -> &str {
        self.working_dir.as_deref().unwrap_or("/")
    }

    pub fn set_user(&mut self, user: String) {
        self.user = Some(user);
    }

    pub fn get_user(&self) -> Option<&str> {
        self.user.as_deref()
    }

    /// Add a volume mount point. This is set by the VOLUME instruction.
    /// Each call adds a new volume path to the list.
    pub fn add_volume(&mut self, volume: String) {
        if self.volumes.is_none() {
            self.volumes = Some(Vec::new());
        }
        if let Some(ref mut volumes) = self.volumes {
            // Avoid duplicates
            if !volumes.contains(&volume) {
                volumes.push(volume);
            }
        }
    }

    pub fn get_volumes(&self) -> Option<&Vec<String>> {
        self.volumes.as_ref()
    }

    pub fn set_stop_signal(&mut self, signal: String) {
        self.stop_signal = Some(signal);
    }

    pub fn get_stop_signal(&self) -> Option<&str> {
        self.stop_signal.as_deref()
    }

    pub fn add_exposed_port(&mut self, port: String) {
        if self.exposed_ports.is_none() {
            self.exposed_ports = Some(Vec::new());
        }
        // Normalize port format: if no protocol specified, add /tcp
        let normalized_port = if port.contains('/') {
            port
        } else {
            format!("{}/tcp", port)
        };
        if let Some(ref mut ports) = self.exposed_ports {
            // Avoid duplicates
            if !ports.contains(&normalized_port) {
                ports.push(normalized_port);
            }
        }
    }

    pub fn get_exposed_ports(&self) -> Option<&Vec<String>> {
        self.exposed_ports.as_ref()
    }

    pub fn set_shell(&mut self, shell: Vec<String>) {
        self.shell = Some(shell);
    }

    pub fn get_shell(&self) -> Vec<String> {
        self.shell
            .clone()
            .unwrap_or_else(|| vec!["/bin/sh".to_string(), "-c".to_string()])
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

        if let Some(volumes) = &self.volumes {
            config = config.volumes(volumes.clone());
        }

        if let Some(stop_signal) = &self.stop_signal {
            config = config.stop_signal(stop_signal.clone());
        }

        if let Some(exposed_ports) = &self.exposed_ports {
            config = config.exposed_ports(exposed_ports.clone());
        }

        // Note: SHELL is not part of OCI image config spec, it only affects build-time behavior

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
            volumes: None,
            stop_signal: None,
            exposed_ports: None,
            shell: None,
        }
    }
}
