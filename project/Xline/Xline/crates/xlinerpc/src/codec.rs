//! Codec trait and implementations for RPC encoding/decoding

use prost::Message;

use crate::MetaData;

/// Errors that can occur during encoding
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// Protobuf encoding error
    #[error("Protobuf encode error: {0}")]
    ProtobufError(#[from] prost::EncodeError),
    /// Other encoding error
    #[error("Encode error: {0}")]
    Other(String),
}

/// Errors that can occur during decoding
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    /// Invalid binary format
    #[error("Invalid binary format")]
    InvalidFormat,
    /// Protobuf decoding error
    #[error("Protobuf decode error: {0}")]
    ProtobufError(#[from] prost::DecodeError),
    /// Other decoding error
    #[error("Decode error: {0}")]
    Other(String),
}

/// Codec trait for encoding/decoding RPC messages
///
/// Implementations define how to serialize/deserialize data and metadata
/// into binary format for transmission.
pub trait Codec: Send + Sync {
    /// Encode data and metadata into bytes
    ///
    /// # Errors
    /// Returns error if encoding fails
    fn encode<T: Message>(&self, data: &T, meta: &MetaData) -> Result<Vec<u8>, EncodeError>;

    /// Decode bytes into data and metadata
    ///
    /// # Errors
    /// Returns error if decoding fails
    fn decode<T: Message + Default>(&self, bytes: &[u8]) -> Result<(T, MetaData), DecodeError>;
}

/// Binary codec implementation
///
/// Binary format:
/// ```text
/// [meta_len: u32 (big-endian)][metadata][protobuf_data]
/// ```
///
/// Metadata format (length-prefixed key-value pairs):
/// ```text
/// [count: u8][key_len: u16][key_bytes][value_len: u16][value_bytes]...
/// ```
/// - count: number of key-value pairs (max 255)
/// - key_len/value_len: big-endian u16 (max 65535 bytes each)
/// - Fully supports binary data (no null terminator needed)
#[derive(Debug, Clone, Copy, Default)]
pub struct BinaryCodec;

impl BinaryCodec {
    /// Create a new binary codec
    #[must_use]
    #[inline]
    pub const fn new() -> Self {
        Self
    }

    /// Encode metadata to bytes
    ///
    /// Format: [count: u8][key_len: u16][key][value_len: u16][value]...
    fn encode_metadata(meta: &MetaData) -> Vec<u8> {
        let mut bytes = Vec::new();

        // `MetaData::insert` enforces len <= 255, so the cast is always exact.
        let count = meta.len() as u8;
        bytes.push(count);

        // Write each key-value pair.
        // `MetaData::insert` enforces key/value len <= u16::MAX, so casts are exact.
        for (key, value) in meta.iter() {
            // Write key length and key
            let key_len = key.len() as u16;
            bytes.extend_from_slice(&key_len.to_be_bytes());
            bytes.extend_from_slice(key);

            // Write value length and value
            let value_len = value.len() as u16;
            bytes.extend_from_slice(&value_len.to_be_bytes());
            bytes.extend_from_slice(value);
        }

        bytes
    }

    /// Decode metadata from bytes
    ///
    /// Format: [count: u8][key_len: u16][key][value_len: u16][value]...
    fn decode_metadata(bytes: &[u8]) -> Result<MetaData, DecodeError> {
        if bytes.is_empty() {
            return Ok(MetaData::new());
        }

        let mut meta = MetaData::new();
        let mut pos = 0;

        // Read count (bytes is guaranteed non-empty here)
        let count = bytes[pos];
        pos += 1;

        // Read each key-value pair
        for _ in 0..count {
            // Read key length
            if pos + 2 > bytes.len() {
                return Err(DecodeError::InvalidFormat);
            }
            let key_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
            pos += 2;

            // Read key
            if pos + key_len > bytes.len() {
                return Err(DecodeError::InvalidFormat);
            }
            let key = bytes[pos..pos + key_len].to_vec();
            pos += key_len;

            // Read value length
            if pos + 2 > bytes.len() {
                return Err(DecodeError::InvalidFormat);
            }
            let value_len = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]) as usize;
            pos += 2;

            // Read value
            if pos + value_len > bytes.len() {
                return Err(DecodeError::InvalidFormat);
            }
            let value = bytes[pos..pos + value_len].to_vec();
            pos += value_len;

            meta.insert(key, value);
        }

        Ok(meta)
    }
}

impl Codec for BinaryCodec {
    fn encode<T: Message>(&self, data: &T, meta: &MetaData) -> Result<Vec<u8>, EncodeError> {
        // Encode metadata
        let meta_bytes = Self::encode_metadata(meta);

        // Encode data using protobuf
        let data_bytes = data.encode_to_vec();

        // Combine: [meta_len: 4 bytes][meta][data]
        let meta_len = meta_bytes.len() as u32;
        let mut result = Vec::with_capacity(4 + meta_bytes.len() + data_bytes.len());
        result.extend_from_slice(&meta_len.to_be_bytes());
        result.extend_from_slice(&meta_bytes);
        result.extend_from_slice(&data_bytes);

        Ok(result)
    }

    fn decode<T: Message + Default>(&self, bytes: &[u8]) -> Result<(T, MetaData), DecodeError> {
        if bytes.len() < 4 {
            return Err(DecodeError::InvalidFormat);
        }

        // Read metadata length
        let meta_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

        if bytes.len() < 4 + meta_len {
            return Err(DecodeError::InvalidFormat);
        }

        // Decode metadata
        let meta_bytes = &bytes[4..4 + meta_len];
        let meta = Self::decode_metadata(meta_bytes)?;

        // Decode data
        let data_bytes = &bytes[4 + meta_len..];
        let data = T::decode(data_bytes)?;

        Ok((data, meta))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(int32, tag = "2")]
        value: i32,
    }

    #[test]
    fn test_binary_codec_encode_decode() {
        let codec = BinaryCodec::new();

        let msg = TestMessage {
            name: "test".to_string(),
            value: 42,
        };

        let mut meta = MetaData::new();
        meta.insert("trace-id", "trace-456");

        // Encode
        let encoded = codec.encode(&msg, &meta).expect("encode failed");
        assert!(!encoded.is_empty());

        // Decode
        let (decoded_msg, decoded_meta): (TestMessage, _) =
            codec.decode(&encoded).expect("decode failed");

        assert_eq!(decoded_msg.name, "test");
        assert_eq!(decoded_msg.value, 42);
        assert_eq!(decoded_meta.get("trace-id"), Some(b"trace-456".as_slice()));
    }

    #[test]
    fn test_binary_codec_empty_metadata() {
        let codec = BinaryCodec::new();

        let msg = TestMessage {
            name: "hello".to_string(),
            value: 999,
        };
        let meta = MetaData::new();

        let encoded = codec.encode(&msg, &meta).unwrap();
        let (decoded_msg, decoded_meta): (TestMessage, _) = codec.decode(&encoded).unwrap();

        assert_eq!(decoded_msg.name, "hello");
        assert_eq!(decoded_msg.value, 999);
        assert!(decoded_meta.is_empty());
    }

    #[test]
    fn test_binary_codec_large_metadata() {
        let codec = BinaryCodec::new();

        let msg = TestMessage {
            name: "test".to_string(),
            value: 1,
        };

        let mut meta = MetaData::new();
        for i in 0..50 {
            meta.insert(format!("key{}", i), format!("value{}", i));
        }

        let encoded = codec.encode(&msg, &meta).unwrap();
        let (decoded_msg, decoded_meta): (TestMessage, _) = codec.decode(&encoded).unwrap();

        assert_eq!(decoded_msg.name, "test");
        assert_eq!(decoded_meta.len(), 50);
        for i in 0..50 {
            let key = format!("key{}", i);
            let value = format!("value{}", i);
            assert_eq!(decoded_meta.get(&key), Some(value.as_bytes()));
        }
    }

    #[test]
    fn test_decode_invalid_format() {
        let codec = BinaryCodec::new();

        // Too short
        let result: Result<(TestMessage, MetaData), _> = codec.decode(&[1, 2]);
        assert!(matches!(result, Err(DecodeError::InvalidFormat)));

        // Invalid metadata length
        let result: Result<(TestMessage, MetaData), _> = codec.decode(&[0, 0, 1, 0, 99]); // meta_len=256 but only 1 byte
        assert!(matches!(result, Err(DecodeError::InvalidFormat)));
    }

    #[test]
    fn test_decode_corrupted_protobuf_payload() {
        let codec = BinaryCodec::new();

        // Valid metadata (0 entries), but corrupted protobuf data
        // Format: [meta_len: 4 bytes = 1][count: 1 byte = 0][garbage protobuf]
        let bad_data: Vec<u8> = vec![
            0, 0, 0, 1, // meta_len = 1 (u32 big-endian)
            0, // metadata: count = 0 (no entries)
            0xff, 0xfe, 0xfd, // garbage bytes (invalid protobuf)
        ];

        let result: Result<(TestMessage, MetaData), _> = codec.decode(&bad_data);
        assert!(matches!(result, Err(DecodeError::ProtobufError(_))));
    }

    #[test]
    fn test_metadata_encoding() {
        let mut meta = MetaData::new();
        meta.insert("key1", "value1");
        meta.insert("key2", "value2");

        let encoded = BinaryCodec::encode_metadata(&meta);
        let decoded = BinaryCodec::decode_metadata(&encoded).unwrap();

        assert_eq!(decoded.get("key1"), Some(b"value1".as_slice()));
        assert_eq!(decoded.get("key2"), Some(b"value2".as_slice()));
    }

    #[test]
    fn test_metadata_encoding_unicode() {
        let mut meta = MetaData::new();
        meta.insert("中文", "值");
        meta.insert("emoji", "🚀");

        let encoded = BinaryCodec::encode_metadata(&meta);
        let decoded = BinaryCodec::decode_metadata(&encoded).unwrap();

        assert_eq!(decoded.get("中文"), Some("值".as_bytes()));
        assert_eq!(decoded.get("emoji"), Some("🚀".as_bytes()));
    }

    #[test]
    fn test_metadata_encoding_binary_with_null_bytes() {
        let mut meta = MetaData::new();
        // Insert binary data containing null bytes
        meta.insert(vec![0x00, 0x01, 0x02], vec![0xff, 0x00, 0xfe]);
        meta.insert(b"normal-key".to_vec(), vec![0x00, 0x00, 0x00]);

        let encoded = BinaryCodec::encode_metadata(&meta);
        let decoded = BinaryCodec::decode_metadata(&encoded).unwrap();

        assert_eq!(
            decoded.get(&[0x00u8, 0x01, 0x02]),
            Some([0xffu8, 0x00, 0xfe].as_slice())
        );
        assert_eq!(
            decoded.get(b"normal-key"),
            Some([0x00u8, 0x00, 0x00].as_slice())
        );
    }

    #[test]
    fn test_metadata_max_entries() {
        let mut meta = MetaData::new();
        // Add exactly 255 entries (u8::MAX)
        for i in 0..255 {
            meta.insert(format!("k{}", i), format!("v{}", i));
        }

        let encoded = BinaryCodec::encode_metadata(&meta);
        let decoded = BinaryCodec::decode_metadata(&encoded).unwrap();

        assert_eq!(decoded.len(), 255);
    }

    #[test]
    #[should_panic(expected = "Metadata cannot exceed 255 entries")]
    fn test_metadata_exceeds_max_entries() {
        let mut meta = MetaData::new();
        // Try to add 256 entries - should panic
        for i in 0..256 {
            meta.insert(format!("k{}", i), format!("v{}", i));
        }
    }

    #[test]
    fn test_metadata_empty() {
        let meta = MetaData::new();
        let encoded = BinaryCodec::encode_metadata(&meta);
        assert_eq!(encoded, vec![0]); // Just count = 0

        let decoded = BinaryCodec::decode_metadata(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_metadata_decode_truncated() {
        // Incomplete metadata: says 2 entries but data is cut off
        let bad_data = vec![
            2, // count = 2
            0, 3, // key_len = 3
            b'k', b'e', b'y', // key
            0, 5, // value_len = 5
            b'v', b'a', b'l', b'u',
            b'e', // value
                  // Second entry is missing!
        ];

        let result = BinaryCodec::decode_metadata(&bad_data);
        assert!(result.is_err());
    }
}
