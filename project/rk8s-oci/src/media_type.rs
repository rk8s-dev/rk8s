//! OCI and Docker image media type constants.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Known OCI and Docker image media types.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum MediaType {
    /// OCI image index.
    OciImageIndex,
    /// OCI image manifest.
    OciImageManifest,
    /// OCI image config.
    OciImageConfig,
    /// OCI gzip-compressed layer.
    OciLayerGzip,
    /// Docker manifest list.
    DockerManifestList,
    /// Docker schema 2 manifest.
    DockerManifest,
    /// Docker image config.
    DockerConfig,
    /// Docker gzip-compressed rootfs diff layer.
    DockerLayerGzip,
    /// Unknown or extension media type.
    Other(String),
}

impl MediaType {
    /// Creates a custom media type.
    pub fn custom(value: impl Into<String>) -> Self {
        Self::Other(value.into())
    }

    /// Returns the media type string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::OciImageIndex => "application/vnd.oci.image.index.v1+json",
            Self::OciImageManifest => "application/vnd.oci.image.manifest.v1+json",
            Self::OciImageConfig => "application/vnd.oci.image.config.v1+json",
            Self::OciLayerGzip => "application/vnd.oci.image.layer.v1.tar+gzip",
            Self::DockerManifestList => "application/vnd.docker.distribution.manifest.list.v2+json",
            Self::DockerManifest => "application/vnd.docker.distribution.manifest.v2+json",
            Self::DockerConfig => "application/vnd.docker.container.image.v1+json",
            Self::DockerLayerGzip => "application/vnd.docker.image.rootfs.diff.tar.gzip",
            Self::Other(value) => value,
        }
    }
}

impl std::fmt::Display for MediaType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for MediaType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for MediaType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "application/vnd.oci.image.index.v1+json" => Self::OciImageIndex,
            "application/vnd.oci.image.manifest.v1+json" => Self::OciImageManifest,
            "application/vnd.oci.image.config.v1+json" => Self::OciImageConfig,
            "application/vnd.oci.image.layer.v1.tar+gzip" => Self::OciLayerGzip,
            "application/vnd.docker.distribution.manifest.list.v2+json" => Self::DockerManifestList,
            "application/vnd.docker.distribution.manifest.v2+json" => Self::DockerManifest,
            "application/vnd.docker.container.image.v1+json" => Self::DockerConfig,
            "application/vnd.docker.image.rootfs.diff.tar.gzip" => Self::DockerLayerGzip,
            _ => Self::Other(value),
        })
    }
}
