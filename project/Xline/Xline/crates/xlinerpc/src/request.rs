//! Request type for RPC communication

use crate::envelope::Envelope;

/// Marker type that distinguishes a request envelope from a response envelope.
pub struct RequestKind;

/// Generic RPC request wrapper.
///
/// Carries a protobuf payload together with binary metadata (auth tokens, trace
/// IDs, …).  All shared logic lives in [`Envelope`]; this type alias exists
/// solely to give callers a meaningful name and type-level distinction from
/// [`crate::Response`].
pub type Request<T> = Envelope<T, RequestKind>;

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;
    use crate::{BinaryCodec, MetaData};

    #[derive(Clone, PartialEq, Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(int32, tag = "2")]
        value: i32,
    }

    #[test]
    fn test_request_from_data() {
        let msg = TestMessage {
            name: "test".to_string(),
            value: 42,
        };
        let req = Request::from_data(msg);

        assert_eq!(req.data().name, "test");
        assert_eq!(req.data().value, 42);
        assert!(req.meta().is_empty());
    }

    #[test]
    fn test_request_encode_decode() {
        let msg = TestMessage {
            name: "hello".to_string(),
            value: 999,
        };

        let mut meta = MetaData::new();
        meta.insert("trace-id", "trace-456");

        let request = Request::new(msg, meta);

        let encoded = request.encode_to_vec().expect("encode failed");
        assert!(!encoded.is_empty());

        let decoded = Request::<TestMessage>::decode_from_slice(&encoded).expect("decode failed");

        assert_eq!(decoded.data().name, "hello");
        assert_eq!(decoded.data().value, 999);
        assert_eq!(
            decoded.meta().get("trace-id"),
            Some(b"trace-456".as_slice())
        );
    }

    #[test]
    fn test_request_encode_decode_empty_meta() {
        let msg = TestMessage {
            name: "test".to_string(),
            value: 1,
        };
        let request = Request::from_data(msg);

        let encoded = request.encode_to_vec().unwrap();
        let decoded = Request::<TestMessage>::decode_from_slice(&encoded).unwrap();

        assert_eq!(decoded.data().name, "test");
        assert_eq!(decoded.data().value, 1);
        assert!(decoded.meta().is_empty());
    }

    #[test]
    fn test_request_encode_with_custom_codec() {
        let msg = TestMessage {
            name: "test".to_string(),
            value: 42,
        };
        let request = Request::from_data(msg);

        let codec = BinaryCodec::new();
        let encoded = request.encode_with(&codec).unwrap();
        let decoded = Request::<TestMessage>::decode_with(&encoded, &codec).unwrap();

        assert_eq!(decoded.data().name, "test");
        assert_eq!(decoded.data().value, 42);
    }

    #[test]
    fn test_request_into_parts() {
        let msg = TestMessage {
            name: "test".to_string(),
            value: 42,
        };
        let meta = MetaData::with_entry("key", "value");
        let request = Request::new(msg, meta);

        let (data, meta_out) = request.into_parts();
        assert_eq!(data.name, "test");
        assert_eq!(data.value, 42);
        assert_eq!(meta_out.get("key"), Some(b"value".as_slice()));
    }
}
