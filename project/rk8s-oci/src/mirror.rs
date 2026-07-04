//! Registry mirror configuration.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Error, ImageReference, Result};

const DOCKER_HUB: &str = "docker.io";

/// Registry mirror configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegistryMirrorConfig {
    mirrors: BTreeMap<String, String>,
}

impl RegistryMirrorConfig {
    /// Creates an empty config.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds or replaces the Docker Hub mirror.
    pub fn with_docker_hub_mirror(self, mirror: impl Into<String>) -> Result<Self> {
        self.with_registry_mirror(DOCKER_HUB, mirror)
    }

    /// Adds or replaces a mirror for a registry host.
    pub fn with_registry_mirror(
        mut self,
        registry: impl Into<String>,
        mirror: impl Into<String>,
    ) -> Result<Self> {
        let registry = registry.into();
        let mirror = mirror.into();
        validate_host("registry", &registry)?;
        validate_host("mirror", &mirror)?;
        self.mirrors.insert(registry, mirror);
        Ok(self)
    }

    /// Returns the configured mirror for a registry, if present.
    pub fn mirror_for(&self, registry: &str) -> Option<&str> {
        self.mirrors.get(registry).map(String::as_str)
    }

    /// Rewrites a reference to its mirror registry when configured.
    pub fn rewrite(&self, reference: &ImageReference) -> Result<ImageReference> {
        let Some(mirror) = self.mirror_for(reference.registry()) else {
            return Ok(reference.clone());
        };
        reference.with_registry(mirror)
    }
}

fn validate_host(label: &str, host: &str) -> Result<()> {
    if host.trim().is_empty() || host.contains('/') || host.contains("://") {
        return Err(Error::Invalid(format!("{label} host is invalid")));
    }
    Ok(())
}
