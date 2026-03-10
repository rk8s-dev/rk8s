//! Response type for RPC communication

use crate::envelope::Envelope;

/// Marker type that distinguishes a response envelope from a request envelope.
pub struct ResponseKind;

/// Generic RPC response wrapper.
pub type Response<T> = Envelope<T, ResponseKind>;

impl<T> Response<T> {
    /// Get inner data (consuming the response)
    pub fn into_inner(self) -> T {
        let (data, _metadata) = self.into_parts();
        data
    }
}

#[cfg(feature = "tonic-compat")]
pub mod tonic_compat {
    use super::*;

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
        result: String,
        #[prost(int32, tag = "2")]
        code: i32,
    }

    #[test]
    fn test_response_from_data() {
        let msg = TestMessage {
            result: "success".to_string(),
            code: 200,
        };
        let resp = Response::from_data(msg);

        assert_eq!(resp.data().result, "success");
        assert_eq!(resp.data().code, 200);
        assert!(resp.meta().is_empty());
    }

    #[test]
    fn test_response_into_inner() {
        let msg = TestMessage {
            result: "success".to_string(),
            code: 200,
        };
        let resp = Response::from_data(msg);
        let data = resp.into_inner();
        assert_eq!(data.result, "success");
        assert_eq!(data.code, 200);
    }

    #[test]
    fn test_response_encode_decode() {
        let msg = TestMessage {
            result: "hello".to_string(),
            code: 100,
        };

        let mut meta = MetaData::new();
        meta.insert("request-id", "req-789");

        let response = Response::new(msg, meta);

        let encoded = response.encode_to_vec().expect("encode failed");
        assert!(!encoded.is_empty());

        let decoded = Response::<TestMessage>::decode_from_slice(&encoded).expect("decode failed");

        assert_eq!(decoded.data().result, "hello");
        assert_eq!(decoded.data().code, 100);
        assert_eq!(
            decoded.meta().get("request-id"),
            Some(b"req-789".as_slice())
        );
    }

    #[test]
    fn test_response_encode_decode_empty_meta() {
        let msg = TestMessage {
            result: "test".to_string(),
            code: 1,
        };
        let response = Response::from_data(msg);

        let encoded = response.encode_to_vec().unwrap();
        let decoded = Response::<TestMessage>::decode_from_slice(&encoded).unwrap();

        assert_eq!(decoded.data().result, "test");
        assert_eq!(decoded.data().code, 1);
        assert!(decoded.meta().is_empty());
    }

    #[test]
    fn test_response_encode_with_custom_codec() {
        let msg = TestMessage {
            result: "test".to_string(),
            code: 42,
        };
        let response = Response::from_data(msg);

        let codec = BinaryCodec::new();
        let encoded = response.encode_with(&codec).unwrap();
        let decoded = Response::<TestMessage>::decode_with(&encoded, &codec).unwrap();

        assert_eq!(decoded.data().result, "test");
        assert_eq!(decoded.data().code, 42);
    }

    #[test]
    fn test_response_into_parts() {
        let msg = TestMessage {
            result: "test".to_string(),
            code: 42,
        };
        let meta = MetaData::with_entry("status", "OK");
        let response = Response::new(msg, meta);

        let (data, meta_out) = response.into_parts();
        assert_eq!(data.result, "test");
        assert_eq!(data.code, 42);
        assert_eq!(meta_out.get("status"), Some(b"OK".as_slice()));
    }

    #[cfg(feature = "tonic-compat")]
    #[test]
    fn test_response_tonic_conversion() {
        let msg = TestMessage {
            result: "success".to_string(),
            code: 200,
        };
        let xline_resp = Response::from_data(msg.clone());

        // xlinerpc::Response → tonic::Response
        let tonic_resp: tonic::Response<TestMessage> = xline_resp.into();
        assert_eq!(tonic_resp.get_ref().result, "success");
        assert_eq!(tonic_resp.get_ref().code, 200);

        // tonic::Response → xlinerpc::Response
        let xline_resp2: Response<TestMessage> = tonic_resp.into();
        assert_eq!(xline_resp2.data().result, "success");
        assert_eq!(xline_resp2.data().code, 200);
    }
}