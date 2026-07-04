//! OCI image reference parsing and formatting.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{Digest, Error, Result};

const DEFAULT_REGISTRY: &str = "docker.io";
const DEFAULT_TAG: &str = "latest";

/// The terminal selector used by an image reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ReferenceKind {
    /// Reference is selected by tag.
    Tag,
    /// Reference is selected by digest.
    Digest,
}

/// Normalized OCI/Docker image reference.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ImageReference {
    registry: String,
    repository: String,
    tag: Option<String>,
    digest: Option<Digest>,
}

impl ImageReference {
    /// Creates a normalized image reference.
    pub fn new(
        registry: impl Into<String>,
        repository: impl Into<String>,
        tag: Option<String>,
        digest: Option<Digest>,
    ) -> Result<Self> {
        let registry = registry.into();
        let repository = repository.into();
        validate_registry(&registry)?;
        validate_repository(&repository)?;
        if let Some(tag) = &tag {
            validate_tag(tag)?;
        }
        if tag.is_some() && digest.is_some() {
            return Err(Error::Invalid(
                "image reference cannot contain both tag and digest".to_owned(),
            ));
        }
        Ok(Self {
            registry,
            repository,
            tag,
            digest,
        })
    }

    /// Registry host.
    pub fn registry(&self) -> &str {
        &self.registry
    }

    /// Repository path.
    pub fn repository(&self) -> &str {
        &self.repository
    }

    /// Optional tag.
    pub fn tag(&self) -> Option<&str> {
        self.tag.as_deref()
    }

    /// Optional digest.
    pub fn digest(&self) -> Option<&Digest> {
        self.digest.as_ref()
    }

    /// Returns whether this reference is tag or digest selected.
    pub fn kind(&self) -> ReferenceKind {
        if self.digest.is_some() {
            ReferenceKind::Digest
        } else {
            ReferenceKind::Tag
        }
    }

    /// Returns a copy with a different registry.
    pub fn with_registry(&self, registry: impl Into<String>) -> Result<Self> {
        Self::new(
            registry,
            self.repository.clone(),
            self.tag.clone(),
            self.digest.clone(),
        )
    }
}

impl FromStr for ImageReference {
    type Err = Error;

    fn from_str(input: &str) -> Result<Self> {
        let input = input.trim();
        if input.is_empty() {
            return Err(Error::Invalid("image reference cannot be empty".to_owned()));
        }

        let (name, digest) = match input.split_once('@') {
            Some((name, digest)) if !name.is_empty() && !digest.is_empty() => {
                (name, Some(Digest::parse(digest)?))
            }
            Some(_) => {
                return Err(Error::Invalid(
                    "image digest reference is incomplete".to_owned(),
                ));
            }
            None => (input, None),
        };

        let (name, tag) = split_tag(name)?;
        if digest.is_some() && tag.is_some() {
            return Err(Error::Invalid(
                "image reference cannot contain both tag and digest".to_owned(),
            ));
        }

        let (registry, repository) = split_registry_and_repository(name)?;
        let repository = normalize_docker_hub_repository(registry, repository);
        let tag = if digest.is_none() {
            Some(tag.unwrap_or(DEFAULT_TAG).to_owned())
        } else {
            None
        };

        Self::new(registry.to_owned(), repository, tag, digest)
    }
}

impl std::fmt::Display for ImageReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.registry, self.repository)?;
        if let Some(digest) = &self.digest {
            write!(f, "@{digest}")?;
        } else if let Some(tag) = &self.tag {
            write!(f, ":{tag}")?;
        }
        Ok(())
    }
}

fn split_registry_and_repository(name: &str) -> Result<(&str, &str)> {
    let (first, rest) = name.split_once('/').unwrap_or((name, ""));
    if first.is_empty() {
        return Err(Error::Invalid("image registry cannot be empty".to_owned()));
    }
    let has_registry = first == "localhost" || first.contains('.') || first.contains(':');
    if has_registry {
        if rest.is_empty() {
            return Err(Error::Invalid(
                "image repository cannot be empty".to_owned(),
            ));
        }
        Ok((first, rest))
    } else {
        Ok((DEFAULT_REGISTRY, name))
    }
}

fn split_tag(name: &str) -> Result<(&str, Option<&str>)> {
    let slash = name.rfind('/').map_or(0, |index| index + 1);
    match name[slash..].rfind(':') {
        Some(relative_index) => {
            let tag_index = slash + relative_index;
            let tag = &name[tag_index + 1..];
            if tag.is_empty() {
                return Err(Error::Invalid("image tag cannot be empty".to_owned()));
            }
            Ok((&name[..tag_index], Some(tag)))
        }
        None => Ok((name, None)),
    }
}

fn normalize_docker_hub_repository(registry: &str, repository: &str) -> String {
    if registry == DEFAULT_REGISTRY && !repository.contains('/') {
        format!("library/{repository}")
    } else {
        repository.to_owned()
    }
}

fn validate_registry(registry: &str) -> Result<()> {
    if registry.is_empty() || registry.contains('/') || registry.contains("://") {
        return Err(Error::Invalid("image registry is invalid".to_owned()));
    }
    Ok(())
}

fn validate_repository(repository: &str) -> Result<()> {
    if repository.is_empty()
        || repository.starts_with('/')
        || repository.ends_with('/')
        || repository.split('/').any(str::is_empty)
    {
        return Err(Error::Invalid("image repository is invalid".to_owned()));
    }
    Ok(())
}

fn validate_tag(tag: &str) -> Result<()> {
    if tag.is_empty() || tag.contains('/') || tag.contains(':') {
        return Err(Error::Invalid("image tag is invalid".to_owned()));
    }
    Ok(())
}
