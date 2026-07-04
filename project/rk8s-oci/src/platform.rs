//! OCI platform metadata.

use serde::{Deserialize, Serialize};

/// OCI platform selector.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    /// CPU architecture, for example `amd64` or `arm64`.
    pub architecture: String,
    /// Operating system, for example `linux`.
    pub os: String,
    /// Optional OS version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    /// Optional OS features.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub os_features: Vec<String>,
    /// Optional CPU variant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl Platform {
    /// Creates a platform value.
    pub fn new(os: impl Into<String>, architecture: impl Into<String>) -> Self {
        Self {
            architecture: architecture.into(),
            os: os.into(),
            os_version: None,
            os_features: Vec::new(),
            variant: None,
        }
    }

    /// Linux/amd64 platform.
    pub fn linux_amd64() -> Self {
        Self::new("linux", "amd64")
    }

    /// Linux/arm64 platform.
    pub fn linux_arm64() -> Self {
        Self::new("linux", "arm64")
    }

    /// Sets the CPU variant.
    pub fn with_variant(mut self, variant: impl Into<String>) -> Self {
        self.variant = Some(variant.into());
        self
    }
}
