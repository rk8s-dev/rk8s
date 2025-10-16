use anyhow::{Context, Result};
use oci_spec::image::{Config, ConfigBuilder};
use std::collections::HashMap;

pub static DEFAULT_ENV: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

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
        }
    }
}
