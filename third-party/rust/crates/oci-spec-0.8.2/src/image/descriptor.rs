use super::{Arch, Digest, MediaType, Os};
use crate::error::OciSpecError;
use derive_builder::Builder;
use getset::{CopyGetters, Getters, Setters};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(
    Builder, Clone, CopyGetters, Debug, Deserialize, Eq, Getters, Setters, PartialEq, Serialize,
)]
#[serde(rename_all = "camelCase")]
#[builder(
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
/// A Content Descriptor (or simply Descriptor) describes the disposition of
/// the targeted content. It includes the type of the content, a content
/// identifier (digest), and the byte-size of the raw content.
/// Descriptors SHOULD be embedded in other formats to securely reference
/// external content.
pub struct Descriptor {
    /// This REQUIRED property contains the media type of the referenced
    /// content. Values MUST comply with RFC 6838, including the naming
    /// requirements in its section 4.2.
    #[getset(get = "pub", set = "pub")]
    media_type: MediaType,
    /// This REQUIRED property is the digest of the targeted content,
    /// conforming to the requirements outlined in Digests. Retrieved
    /// content SHOULD be verified against this digest when consumed via
    /// untrusted sources.
    #[getset(get = "pub", set = "pub")]
    digest: Digest,
    /// This REQUIRED property specifies the size, in bytes, of the raw
    /// content. This property exists so that a client will have an
    /// expected size for the content before processing. If the
    /// length of the retrieved content does not match the specified
    /// length, the content SHOULD NOT be trusted.
    #[getset(get_copy = "pub", set = "pub")]
    size: u64,
    /// This OPTIONAL property specifies a list of URIs from which this
    /// object MAY be downloaded. Each entry MUST conform to [RFC 3986](https://tools.ietf.org/html/rfc3986).
    /// Entries SHOULD use the http and https schemes, as defined
    /// in [RFC 7230](https://tools.ietf.org/html/rfc7230#section-2.7).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    #[builder(default)]
    urls: Option<Vec<String>>,
    /// This OPTIONAL property contains arbitrary metadata for this
    /// descriptor. This OPTIONAL property MUST use the annotation
    /// rules.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    #[builder(default)]
    annotations: Option<HashMap<String, String>>,
    /// This OPTIONAL property describes the minimum runtime requirements of
    /// the image. This property SHOULD be present if its target is
    /// platform-specific.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    #[builder(default)]
    platform: Option<Platform>,
    /// This OPTIONAL property contains the type of an artifact when the descriptor points to an
    /// artifact. This is the value of the config descriptor mediaType when the descriptor
    /// references an image manifest. If defined, the value MUST comply with RFC 6838, including
    /// the naming requirements in its section 4.2, and MAY be registered with IANA.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    #[builder(default)]
    artifact_type: Option<MediaType>,
    /// This OPTIONAL property contains an embedded representation of the referenced content.
    /// Values MUST conform to the Base 64 encoding, as defined in RFC 4648. The decoded data MUST
    /// be identical to the referenced content and SHOULD be verified against the digest and size
    /// fields by content consumers. See Embedded Content for when this is appropriate.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[getset(get = "pub", set = "pub")]
    #[builder(default)]
    data: Option<String>,
}

#[derive(
    Builder, Clone, Debug, Default, Deserialize, Eq, Getters, Setters, PartialEq, Serialize,
)]
#[builder(
    pattern = "owned",
    setter(into, strip_option),
    build_fn(error = "OciSpecError")
)]
#[getset(get = "pub", set = "pub")]
/// Describes the minimum runtime requirements of the image.
pub struct Platform {
    /// This REQUIRED property specifies the CPU architecture.
    /// Image indexes SHOULD use, and implementations SHOULD understand,
    /// values listed in the Go Language document for GOARCH.
    architecture: Arch,
    /// This REQUIRED property specifies the operating system.
    /// Image indexes SHOULD use, and implementations SHOULD understand,
    /// values listed in the Go Language document for GOOS.
    os: Os,
    /// This OPTIONAL property specifies the version of the operating system
    /// targeted by the referenced blob. Implementations MAY refuse to use
    /// manifests where os.version is not known to work with the host OS
    /// version. Valid values are implementation-defined. e.g.
    /// 10.0.14393.1066 on windows.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    os_version: Option<String>,
    /// This OPTIONAL property specifies an array of strings, each
    /// specifying a mandatory OS feature. When os is windows, image
    /// indexes SHOULD use, and implementations SHOULD understand
    /// the following values:
    /// - win32k: image requires win32k.sys on the host (Note: win32k.sys is
    ///   missing on Nano Server)
    ///
    /// When os is not windows, values are implementation-defined and SHOULD
    /// be submitted to this specification for standardization.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    os_features: Option<Vec<String>>,
    /// This OPTIONAL property specifies the variant of the CPU.
    /// Image indexes SHOULD use, and implementations SHOULD understand,
    /// variant values listed in the [Platform Variants]
    /// (<https://github.com/opencontainers/image-spec/blob/main/image-index.md#platform-variants>)
    /// table.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    variant: Option<String>,
    /// This property is RESERVED for future versions of the specification.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    features: Option<Vec<String>>,
}

impl Descriptor {
    /// Construct a new descriptor with the required fields.
    pub fn new(media_type: MediaType, size: u64, digest: impl Into<Digest>) -> Self {
        Self {
            media_type,
            size,
            digest: digest.into(),
            urls: Default::default(),
            annotations: Default::default(),
            platform: Default::default(),
            artifact_type: Default::default(),
            data: Default::default(),
        }
    }

    /// Return a view of [`Self::digest()`] that has been parsed as a valid SHA-256.
    pub fn as_digest_sha256(&self) -> Option<&str> {
        match self.digest.algorithm() {
            super::DigestAlgorithm::Sha256 => Some(self.digest.digest()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn test_deserialize() {
        let descriptor_str = r#"{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest":"sha256:c2b8beca588702777e5f35dafdbeae9ec16c2bab802331f81cacd2a92f1d5356",
            "size":769,
            "annotations":{"org.opencontainers.image.created": "2023-10-11T22:37:26Z"},
            "artifactType":"application/spdx+json"}"#;
        let descriptor: Descriptor = serde_json::from_str(descriptor_str).unwrap();
        assert_eq!(descriptor.media_type, MediaType::ImageManifest);
        assert_eq!(
            descriptor.digest,
            Digest::from_str(
                "sha256:c2b8beca588702777e5f35dafdbeae9ec16c2bab802331f81cacd2a92f1d5356"
            )
            .unwrap()
        );
        assert_eq!(descriptor.size, 769);
        assert_eq!(
            descriptor
                .annotations
                .unwrap()
                .get("org.opencontainers.image.created"),
            Some(&"2023-10-11T22:37:26Z".to_string())
        );
        assert_eq!(
            descriptor.artifact_type.unwrap(),
            MediaType::Other("application/spdx+json".to_string())
        );
    }

    #[test]
    fn test_malformed_digest() {
        let descriptor_str = r#"{
            "mediaType": "application/vnd.oci.image.manifest.v1+json",
            "digest":"../blah:this-is-an-attack",
            "size":769,
            "annotations":{"org.opencontainers.image.created": "2023-10-11T22:37:26Z"},
            "artifactType":"application/spdx+json"}"#;
        assert!(serde_json::from_str::<Descriptor>(descriptor_str).is_err());
    }
}
