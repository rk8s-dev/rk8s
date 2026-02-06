//! Serialization abstraction for metadata structures.
//!
//! Provides dual-format support:
//! - **rkyv binary format** (when `rkyv-serialization` feature is enabled): Zero-copy deserialization
//! - **JSON fallback** (always available): Human-readable, backward compatible
//!
//! Wire format (rkyv mode):
//! ```text
//! +--------+--------+--------+--------+--------+--------+...
//! | Magic  | Schema | Payload (rkyv archived bytes)     |
//! | 4 bytes| 4 bytes| variable length                    |
//! +--------+--------+--------+--------+--------+--------+...
//!
//! Magic: b"RKYV" (0x524B5956) - big endian
//! Schema: u32 little-endian (currently 1)
//! Payload: rkyv::to_bytes() output
//! ```
//!
//! Format detection:
//! - If bytes.len() >= 8 && bytes[0..4] == b"RKYV" → rkyv path
//! - Else → JSON fallback

use crate::meta::store::MetaError;
#[cfg_attr(feature = "rkyv-serialization", allow(unused_imports))]
use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};

/// Current schema version for rkyv serialization format.
#[cfg(feature = "rkyv-serialization")]
pub const SCHEMA_VERSION: u32 = 1;

/// Magic bytes identifying rkyv-serialized data.
#[cfg(feature = "rkyv-serialization")]
pub const RKYV_MAGIC: &[u8; 4] = b"RKYV";

/// Header size: 4 bytes magic + 4 bytes schema version.
#[cfg(feature = "rkyv-serialization")]
pub const HEADER_SIZE: usize = 8;

/// Serialize metadata to bytes using the configured format.
///
/// # Behavior
/// - **With `rkyv-serialization` feature**: Uses rkyv binary format with header
/// - **Without feature**: Uses JSON fallback
///
/// # Errors
/// Returns `MetaError::Serialization` if encoding fails.
#[cfg(feature = "rkyv-serialization")]
pub fn serialize_meta<T>(value: &T) -> Result<Vec<u8>, MetaError>
where
    T: rkyv::Archive,
    for<'a> T: rkyv::Serialize<
            rkyv::rancor::Strategy<
                rkyv::ser::Serializer<
                    rkyv::util::AlignedVec,
                    rkyv::ser::allocator::ArenaHandle<'a>,
                    rkyv::ser::sharing::Share,
                >,
                rkyv::rancor::Error,
            >,
        >,
{
    // Serialize with rkyv
    let payload = rkyv::to_bytes::<rkyv::rancor::Error>(value)
        .map_err(|e| MetaError::Serialization(format!("rkyv serialization failed: {}", e)))?;

    // Construct header + payload
    let mut buf = Vec::with_capacity(HEADER_SIZE + payload.len());
    buf.extend_from_slice(RKYV_MAGIC);
    buf.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
    buf.extend_from_slice(&payload);

    Ok(buf)
}

#[cfg(not(feature = "rkyv-serialization"))]
pub fn serialize_meta<T>(value: &T) -> Result<Vec<u8>, MetaError>
where
    T: SerdeSerialize,
{
    serde_json::to_vec(value)
        .map_err(|e| MetaError::Serialization(format!("JSON serialization failed: {}", e)))
}

/// Deserialize metadata from bytes with automatic format detection.
///
/// # Behavior
/// - **With `rkyv-serialization` feature**: Detects rkyv header and deserializes accordingly
/// - **Without feature**: Always uses JSON
///
/// Format detection (when feature enabled):
/// - If `bytes[0..4] == b"RKYV"` → rkyv deserialization
/// - Else → JSON fallback (backward compatibility)
///
/// # Errors
/// Returns `MetaError::Serialization` if:
/// - Header is malformed
/// - Schema version is unsupported
/// - Decoding fails
#[cfg(feature = "rkyv-serialization")]
pub fn deserialize_meta<T>(bytes: &[u8]) -> Result<T, MetaError>
where
    T: rkyv::Archive,
    T::Archived: rkyv::Deserialize<T, rkyv::rancor::Strategy<rkyv::de::Pool, rkyv::rancor::Error>>,
    for<'de> T: SerdeDeserialize<'de>,
{
    // Format detection
    if bytes.len() >= HEADER_SIZE && bytes[..4] == *RKYV_MAGIC {
        // rkyv path: validate header and deserialize
        let schema_version = u32::from_le_bytes(
            bytes[4..8]
                .try_into()
                .map_err(|_| MetaError::Serialization("invalid schema version bytes".into()))?,
        );

        if schema_version != SCHEMA_VERSION {
            return Err(MetaError::Serialization(format!(
                "unsupported schema version: {} (expected {})",
                schema_version, SCHEMA_VERSION
            )));
        }

        let payload = &bytes[HEADER_SIZE..];

        // SAFETY: Trusted data source - all rkyv-encoded data is produced by this system.
        // We use access_unchecked for zero-copy deserialization without validation overhead.
        let archived = unsafe { rkyv::access_unchecked::<T::Archived>(payload) };

        rkyv::deserialize::<T, rkyv::rancor::Error>(archived)
            .map_err(|e| MetaError::Serialization(format!("rkyv deserialization failed: {}", e)))
    } else {
        // JSON fallback for backward compatibility
        serde_json::from_slice(bytes)
            .map_err(|e| MetaError::Serialization(format!("JSON deserialization failed: {}", e)))
    }
}

#[cfg(not(feature = "rkyv-serialization"))]
pub fn deserialize_meta<T>(bytes: &[u8]) -> Result<T, MetaError>
where
    for<'de> T: SerdeDeserialize<'de>,
{
    serde_json::from_slice(bytes)
        .map_err(|e| MetaError::Serialization(format!("JSON deserialization failed: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    #[cfg_attr(
        feature = "rkyv-serialization",
        derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
    )]
    #[derive(SerdeSerialize, SerdeDeserialize)]
    struct TestStruct {
        id: u64,
        name: String,
    }

    #[test]
    fn test_roundtrip() {
        let original = TestStruct {
            id: 42,
            name: "test".into(),
        };

        let bytes = serialize_meta(&original).expect("serialization should succeed");
        let deserialized: TestStruct =
            deserialize_meta(&bytes).expect("deserialization should succeed");

        assert_eq!(original, deserialized);
    }

    #[test]
    #[cfg(feature = "rkyv-serialization")]
    fn test_format_detection() {
        let original = TestStruct {
            id: 99,
            name: "format_test".into(),
        };

        // Serialize with rkyv (should have header)
        let rkyv_bytes = serialize_meta(&original).expect("rkyv serialization should succeed");
        assert!(rkyv_bytes.len() >= HEADER_SIZE);
        assert_eq!(&rkyv_bytes[..4], RKYV_MAGIC);

        // Deserialize should auto-detect rkyv format
        let deserialized: TestStruct =
            deserialize_meta(&rkyv_bytes).expect("rkyv deserialization should succeed");
        assert_eq!(original, deserialized);
    }

    #[test]
    #[cfg(feature = "rkyv-serialization")]
    fn test_json_backward_compat() {
        let original = TestStruct {
            id: 123,
            name: "json_compat".into(),
        };

        // Manually create JSON bytes (simulate legacy data)
        let json_bytes = serde_json::to_vec(&original).expect("JSON serialization should succeed");

        // Deserialize should fall back to JSON
        let deserialized: TestStruct =
            deserialize_meta(&json_bytes).expect("JSON fallback should succeed");
        assert_eq!(original, deserialized);
    }
}
