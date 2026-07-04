//! OCI descriptors and digests.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{Error, MediaType, Platform, Result};

/// Content digest in `<algorithm>:<hex>` form.
#[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Digest(String);

impl Digest {
    /// Parses and validates a digest.
    pub fn parse(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        let (algorithm, encoded) = value
            .split_once(':')
            .ok_or_else(|| Error::Invalid("digest must contain an algorithm".to_owned()))?;
        if algorithm.is_empty() || encoded.is_empty() {
            return Err(Error::Invalid(
                "digest algorithm and encoded value cannot be empty".to_owned(),
            ));
        }
        if !algorithm
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '+' | '.'))
        {
            return Err(Error::Invalid("digest algorithm is invalid".to_owned()));
        }
        if !encoded.chars().all(|ch| ch.is_ascii_hexdigit()) {
            return Err(Error::Invalid("digest value must be hex".to_owned()));
        }
        Ok(Self(value.to_owned()))
    }

    /// Returns the digest as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for Digest {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl std::fmt::Display for Digest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// OCI descriptor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    /// Descriptor media type.
    pub media_type: MediaType,
    /// Content digest.
    pub digest: Digest,
    /// Content size in bytes.
    pub size: u64,
    /// Optional URLs from which the content may be downloaded.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub urls: Vec<String>,
    /// Optional platform information for image indexes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<Platform>,
    /// Optional descriptor annotations.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub annotations: BTreeMap<String, String>,
}

impl Descriptor {
    /// Creates a descriptor from required OCI fields.
    pub fn new(media_type: MediaType, digest: Digest, size: u64) -> Self {
        Self {
            media_type,
            digest,
            size,
            urls: Vec::new(),
            platform: None,
            annotations: BTreeMap::new(),
        }
    }

    /// Sets the platform.
    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    /// Adds a URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.urls.push(url.into());
        self
    }

    /// Adds an annotation.
    pub fn with_annotation(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.annotations.insert(key.into(), value.into());
        self
    }
}
