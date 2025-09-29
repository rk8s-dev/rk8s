use crate::config::registry::CONFIG;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The `repositories.json`, but there, it is `repositories.toml`.
///
/// It records the maps from image reference with tags to its manifest path.
///
/// `(e,g.)
/// "library/ubuntu.latest" = "sha256:4c07c..."`
#[derive(Serialize, Deserialize, Default)]
pub struct Repositories {
    repositories: HashMap<String, String>,
}

// todo: Implement repositories cache.
impl Repositories {
    pub fn load() -> anyhow::Result<Self> {
        let path = CONFIG.metadata_dir.join("repositories.toml");
        confy::load_path(path).with_context(|| "Failed to read repositories metadata")
    }

    pub fn store(&self) -> anyhow::Result<()> {
        let path = CONFIG.metadata_dir.join("repositories.toml");
        confy::store_path(path, self).with_context(|| "Failed to store repositories metadata")
    }

    pub fn add(&mut self, image_ref: impl Into<String>, digest: impl Into<String>) {
        self.repositories.insert(image_ref.into(), digest.into());
    }

    pub fn add_store(entry: Vec<(impl Into<String>, impl Into<String>)>) -> anyhow::Result<Self> {
        let mut repositories = Self::load()?;
        entry
            .into_iter()
            .for_each(|(image_ref, digest)| repositories.add(image_ref, digest));
        repositories.store()?;
        Ok(repositories)
    }

    /// Obtain the path corresponding to an image reference.
    ///
    /// The image reference must be `full`, that means, it cannot be missing any of the following:
    /// namespace, repository name, or tag.
    ///
    /// You can get the full image reference with [`full_image_ref`][`crate::storage::full_image_ref`].
    pub fn get(&self, image_ref: impl AsRef<str>) -> anyhow::Result<Option<&str>> {
        match self.repositories.get(image_ref.as_ref()) {
            Some(digest) => Ok(Some(digest)),
            None => Ok(None),
        }
    }
}
