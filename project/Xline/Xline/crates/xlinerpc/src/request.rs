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


#[cfg(feature = "tonic-compat")]
pub mod tonic_compat {
    use super::*;
    use crate::Response;

    /// Convert tonic::Request to xlinerpc::Request
    impl<T> From<tonic::Request<T>> for Request<T> {
        fn from(req: tonic::Request<T>) -> Self {
            let (data, metadata, _extensions) = req.into_parts();
            let mut xline_metadata = crate::MetaData::new();
            for (key, value) in metadata.iter() {
                if let Ok(v) = value.to_bytes() {
                    xline_metadata.insert(key.as_str(), String::from_utf8_lossy(&v).as_ref());
                }
            }
            Request::new(data, xline_metadata)
        }
    }

    /// Convert xlinerpc::Request to tonic::Request
    impl<T> From<Request<T>> for tonic::Request<T> {
        fn from(req: Request<T>) -> Self {
            let (data, metadata) = req.into_parts();
            let mut tonic_metadata = tonic::metadata::MetadataMap::new();
            for (key, value) in metadata.iter() {
                if let (Ok(key_str), Ok(val_str)) = (
                    std::str::from_utf8(key.as_bytes()),
                    std::str::from_utf8(value),
                ) {
                    let _ = tonic_metadata.insert(
                        tonic::metadata::MetadataKey::from_bytes(key.as_bytes()).unwrap(),
                        val_str.parse().unwrap_or_default(),
                    );
                }
            }
            let mut tonic_req = tonic::Request::new(data);
            *tonic_req.metadata_mut() = tonic_metadata;
            tonic_req
        }
    }

    /// Convert tonic::Response to xlinerpc::Response
    impl<T> From<tonic::Response<T>> for Response<T> {
        fn from(resp: tonic::Response<T>) -> Self {
            Response::from_data(resp.into_inner())
        }
    }

    /// Convert xlinerpc::Response to tonic::Response
    impl<T> From<Response<T>> for tonic::Response<T> {
        fn from(resp: Response<T>) -> Self {
            tonic::Response::new(resp.into_data())
        }
    }
}

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
