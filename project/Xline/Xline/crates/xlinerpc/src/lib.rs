//! RPC layer for Xline
//!
//! Provides Request/Response wrappers with metadata support and binary encoding.

use std::{collections::BTreeMap, path::Path, path::PathBuf};

pub mod codec;
pub mod envelope;
pub mod grpc;
pub mod h3_client;
pub mod method;
pub mod request;
pub mod response;
pub mod server;
pub mod status;
pub mod streaming;

// Re-export commonly used types
pub use codec::{BinaryCodec, Codec, DecodeError, EncodeError};
pub use envelope::Envelope;
pub use h3_client::H3Channel;
pub use method::{ALL_METHOD_IDS, MethodId};
pub use request::Request;
pub use response::Response;
pub use status::{Code, Status};
pub use streaming::Streaming;

/// Trait for types that can be converted into metadata bytes (keys or values)
///
/// This trait consolidates the conversion of various types into binary metadata.
pub trait IntoMetadataBytes {
    /// Convert into metadata bytes
    fn into_metadata_bytes(self) -> Vec<u8>;
}

// Implement for common types
impl IntoMetadataBytes for &str {
    #[inline]
    fn into_metadata_bytes(self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl IntoMetadataBytes for String {
    #[inline]
    fn into_metadata_bytes(self) -> Vec<u8> {
        self.into_bytes()
    }
}

impl IntoMetadataBytes for &[u8] {
    #[inline]
    fn into_metadata_bytes(self) -> Vec<u8> {
        self.to_vec()
    }
}

impl IntoMetadataBytes for Vec<u8> {
    #[inline]
    fn into_metadata_bytes(self) -> Vec<u8> {
        self
    }
}

/// Metadata for RPC requests and responses
/// Metadata map for xlinerpc requests and responses, backed by binary data
/// In fact, the entry number is usually less than ten, and the key/value size is usually less than 128 bytes.
/// Uses `BTreeMap` internally to guarantee deterministic iteration order during encoding.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetaData {
    /// Key-value pairs for metadata (both binary)
    headers: BTreeMap<Vec<u8>, Vec<u8>>,
}

/// QUIC TLS configuration shared across transport layers.
///
/// This is intentionally a plain data object so higher-level crates can map it
/// to concrete QUIC client builders without depending on transport-specific types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuicTlsConfig {
    /// Optional CA certificate PEM bytes used to verify peer certificates.
    peer_ca_cert_pem: Option<Vec<u8>>,
    /// Optional client certificate path for mTLS.
    client_cert_path: Option<PathBuf>,
    /// Optional client private key path for mTLS.
    client_key_path: Option<PathBuf>,
}

impl QuicTlsConfig {
    /// Create a new QUIC TLS config.
    #[must_use]
    #[inline]
    pub fn new(
        peer_ca_cert_pem: Option<Vec<u8>>,
        client_cert_path: Option<PathBuf>,
        client_key_path: Option<PathBuf>,
    ) -> Self {
        Self {
            peer_ca_cert_pem,
            client_cert_path,
            client_key_path,
        }
    }

    /// Get peer CA certificate PEM bytes.
    #[must_use]
    #[inline]
    pub fn peer_ca_cert_pem(&self) -> Option<&[u8]> {
        self.peer_ca_cert_pem.as_deref()
    }

    /// Get client certificate path.
    #[must_use]
    #[inline]
    pub fn client_cert_path(&self) -> Option<&Path> {
        self.client_cert_path.as_deref()
    }

    /// Get client private key path.
    #[must_use]
    #[inline]
    pub fn client_key_path(&self) -> Option<&Path> {
        self.client_key_path.as_deref()
    }

    /// Set peer CA certificate PEM bytes.
    #[must_use]
    #[inline]
    pub fn with_peer_ca_cert_pem(self, peer_ca_cert_pem: Vec<u8>) -> Self {
        Self {
            peer_ca_cert_pem: Some(peer_ca_cert_pem),
            ..self
        }
    }

    /// Set client certificate and key paths used for mTLS.
    #[must_use]
    #[inline]
    pub fn with_client_identity_paths(self, cert_path: PathBuf, key_path: PathBuf) -> Self {
        Self {
            client_cert_path: Some(cert_path),
            client_key_path: Some(key_path),
            ..self
        }
    }
}

impl MetaData {
    /// Create a new empty metadata
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self {
            headers: BTreeMap::new(),
        }
    }

    /// Create metadata with a single key-value pair
    ///
    /// # Panics
    /// Panics if key or value exceeds 65535 bytes (codec limit)
    #[must_use]
    #[inline]
    pub fn with_entry<K, V>(key: K, value: V) -> Self
    where
        K: IntoMetadataBytes,
        V: IntoMetadataBytes,
    {
        let mut meta = Self::new();
        meta.insert(key, value);
        meta
    }

    /// Insert a key-value pair.
    ///
    /// # Panics
    ///
    /// Panics if any of the following preconditions are violated:
    /// - Key exceeds 65535 bytes (`u16::MAX`)
    /// - Value exceeds 65535 bytes (`u16::MAX`)
    /// - Total entries would exceed 255 (`u8::MAX`)
    ///
    /// In practice these limits are never reached: metadata carries only a
    /// handful of short RPC header fields.
    #[inline]
    pub fn insert<K, V>(&mut self, key: K, value: V)
    where
        K: IntoMetadataBytes,
        V: IntoMetadataBytes,
    {
        let key_bytes = key.into_metadata_bytes();
        let value_bytes = value.into_metadata_bytes();

        assert!(
            key_bytes.len() <= u16::MAX as usize,
            "Metadata key exceeds 65535 bytes (got {})",
            key_bytes.len()
        );
        assert!(
            value_bytes.len() <= u16::MAX as usize,
            "Metadata value exceeds 65535 bytes (got {})",
            value_bytes.len()
        );

        self.headers.insert(key_bytes, value_bytes);

        assert!(
            self.headers.len() <= 255,
            "Metadata cannot exceed 255 entries (got {})",
            self.headers.len()
        );
    }

    /// Get a value by key (returns binary data)
    #[must_use]
    #[inline]
    pub fn get<K: AsRef<[u8]>>(&self, key: K) -> Option<&[u8]> {
        self.headers.get(key.as_ref()).map(Vec::as_slice)
    }

    /// Get a value by key as UTF-8 string
    ///
    /// # Errors
    /// Returns error if the value is not valid UTF-8
    #[inline]
    pub fn get_str<K: AsRef<[u8]>>(&self, key: K) -> Option<Result<&str, std::str::Utf8Error>> {
        self.get(key).map(std::str::from_utf8)
    }

    /// Remove a key-value pair
    #[inline]
    pub fn remove<K: AsRef<[u8]>>(&mut self, key: K) -> Option<Vec<u8>> {
        self.headers.remove(key.as_ref())
    }

    /// Check if metadata is empty
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.headers.is_empty()
    }

    /// Get the number of entries
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.headers.len()
    }

    /// Iterate over all entries as byte slices
    ///
    /// Iteration order is deterministic (sorted by key) due to `BTreeMap` storage.
    pub fn iter(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.headers
            .iter()
            .map(|(k, v)| (k.as_slice(), v.as_slice()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_new() {
        let meta = MetaData::new();
        assert!(meta.is_empty());
        assert_eq!(meta.len(), 0);
    }

    #[test]
    fn test_metadata_insert_get() {
        let mut meta = MetaData::new();
        meta.insert("key1", "value1");
        meta.insert("key2", "value2");

        assert_eq!(meta.len(), 2);
        assert_eq!(meta.get("key1"), Some(b"value1".as_slice()));
        assert_eq!(meta.get_str("key1"), Some(Ok("value1")));
        assert_eq!(meta.get("key2"), Some(b"value2".as_slice()));
        assert_eq!(meta.get("key3"), None);
    }

    #[test]
    fn test_metadata_with_entry() {
        let meta = MetaData::with_entry("key", "value");
        assert_eq!(meta.get("key"), Some(b"value".as_slice()));
        assert_eq!(meta.get_str("key"), Some(Ok("value")));
        assert_eq!(meta.len(), 1);
    }

    #[test]
    fn test_metadata_remove() {
        let mut meta = MetaData::new();
        meta.insert("key", "value");
        assert_eq!(meta.len(), 1);

        let removed = meta.remove("key");
        assert_eq!(removed, Some(b"value".to_vec()));
        assert_eq!(meta.len(), 0);
        assert!(meta.is_empty());
    }

    #[test]
    fn test_metadata_binary_data() {
        let mut meta = MetaData::new();
        // Insert binary data (not UTF-8)
        let key = vec![0u8, 1, 2, 3];
        let value = vec![255u8, 254, 253];
        meta.insert(key.clone(), value.clone());

        assert_eq!(meta.get(&key), Some(value.as_slice()));
        // get_str should return Err for invalid UTF-8
        assert!(meta.get_str(&key).unwrap().is_err());
    }

    #[test]
    fn test_metadata_string_and_binary_mix() {
        let mut meta = MetaData::new();
        // Insert string
        meta.insert("string-key", "string-value");
        // Insert binary
        meta.insert(vec![0xffu8, 0xfe], vec![0x01, 0x02]);

        assert_eq!(meta.get("string-key"), Some(b"string-value".as_slice()));
        assert_eq!(meta.get(&[0xffu8, 0xfe]), Some([0x01u8, 0x02].as_slice()));
        assert_eq!(meta.len(), 2);
    }
}
