//! Helpers for interacting with blobs and their verification
use std::task::Poll;

use futures_util::stream::{BoxStream, Stream};
use futures_util::TryStreamExt;

use crate::digest::Digester;
use crate::errors::DigestError;

/// Stream response of a blob with optional content length if available
pub struct SizedStream {
    /// The length of the stream if the upstream registry sent a `Content-Length` header
    pub content_length: Option<u64>,
    /// The digest header value if the upstream registry sent a `Digest` header. This should be used
    /// (in addition to the layer digest) for validation when using partial requests as the library
    /// can't validate against the full response.
    pub digest_header_value: Option<String>,
    /// The stream of bytes
    pub stream: BoxStream<'static, Result<bytes::Bytes, std::io::Error>>,
}

impl Stream for SizedStream {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.stream.try_poll_next_unpin(cx)
    }
}

/// The response of a partial blob request
pub enum BlobResponse {
    /// The response is a full blob (for example when partial requests aren't supported)
    Full(SizedStream),
    /// The response is a partial blob as requested
    Partial(SizedStream),
}

pub(crate) struct VerifyingStream {
    stream: BoxStream<'static, Result<bytes::Bytes, std::io::Error>>,
    layer_digester: Digester,
    expected_layer_digest: String,
    header_digester: Option<(Digester, String)>,
}

impl VerifyingStream {
    pub fn new(
        stream: BoxStream<'static, Result<bytes::Bytes, std::io::Error>>,
        layer_digester: Digester,
        expected_layer_digest: String,
        header_digester_and_digest: Option<(Digester, String)>,
    ) -> Self {
        Self {
            stream,
            layer_digester,
            expected_layer_digest,
            header_digester: header_digester_and_digest,
        }
    }
}

impl Stream for VerifyingStream {
    type Item = Result<bytes::Bytes, std::io::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match futures_util::ready!(this.stream.as_mut().poll_next(cx)) {
            Some(Ok(bytes)) => {
                this.layer_digester.update(&bytes);
                if let Some((digester, _)) = this.header_digester.as_mut() {
                    digester.update(&bytes);
                }
                Poll::Ready(Some(Ok(bytes)))
            }
            Some(Err(e)) => Poll::Ready(Some(Err(e))),
            None => {
                // Now that we've reached the end of the stream, verify the digest(s)
                match this.header_digester.as_mut() {
                    Some((digester, expected)) => {
                        // Check the header digester and then the layer digester before returning
                        let digest = digester.finalize();
                        if digest != *expected {
                            return Poll::Ready(Some(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                DigestError::VerificationError {
                                    expected: expected.clone(),
                                    actual: digest,
                                },
                            ))));
                        }
                        let digest = this.layer_digester.finalize();
                        if digest == this.expected_layer_digest {
                            Poll::Ready(None)
                        } else {
                            Poll::Ready(Some(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                DigestError::VerificationError {
                                    expected: expected.clone(),
                                    actual: digest,
                                },
                            ))))
                        }
                    }
                    None => {
                        let digest = this.layer_digester.finalize();
                        if digest == this.expected_layer_digest {
                            Poll::Ready(None)
                        } else {
                            Poll::Ready(Some(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                DigestError::VerificationError {
                                    expected: this.expected_layer_digest.clone(),
                                    actual: digest,
                                },
                            ))))
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Bytes;
    use futures_util::TryStreamExt;
    use sha2::Digest as _;

    #[tokio::test]
    async fn test_verifying_stream() {
        // Test with correct SHA
        let data = b"Hello, world!";
        let correct_sha = format!("sha256:{:x}", sha2::Sha256::digest(data));
        let stream = VerifyingStream::new(
            Box::pin(futures_util::stream::iter(vec![Ok(Bytes::from_static(
                data,
            ))])),
            Digester::new(&correct_sha).unwrap(),
            correct_sha.clone(),
            None,
        );
        stream
            .try_collect::<Vec<_>>()
            .await
            .expect("Should not error with valid data");

        // Test with incorrect SHA
        let incorrect_sha = "sha256:incorrect_hash";
        let stream = VerifyingStream::new(
            Box::pin(futures_util::stream::iter(vec![Ok(Bytes::from_static(
                data,
            ))])),
            Digester::new(incorrect_sha).unwrap(),
            incorrect_sha.to_string(),
            None,
        );

        let err = stream
            .try_collect::<Vec<_>>()
            .await
            .expect_err("Should error with invalid sha");

        let err = err
            .into_inner()
            .expect("Should have inner error")
            .downcast::<DigestError>()
            .expect("Should be a DigestError");
        assert!(
            matches!(*err, DigestError::VerificationError { .. }),
            "Error should be a verification error"
        );

        // Test with correct SHA and header
        let correct_header_sha = format!("sha512:{:x}", sha2::Sha512::digest(data));
        let stream = VerifyingStream::new(
            Box::pin(futures_util::stream::iter(vec![Ok(Bytes::from_static(
                data,
            ))])),
            Digester::new(&correct_sha).unwrap(),
            correct_sha.clone(),
            Some((
                Digester::new(&correct_header_sha).unwrap(),
                correct_header_sha.clone(),
            )),
        );
        stream
            .try_collect::<Vec<_>>()
            .await
            .expect("Should not error with valid data");

        // Test with correct layer sha and wrong header sha
        let incorrect_header_sha = "sha512:incorrect_hash";
        let stream = VerifyingStream::new(
            Box::pin(futures_util::stream::iter(vec![Ok(Bytes::from_static(
                data,
            ))])),
            Digester::new(&correct_sha).unwrap(),
            correct_sha.clone(),
            Some((
                Digester::new(incorrect_header_sha).unwrap(),
                incorrect_header_sha.to_string(),
            )),
        );

        let err = stream
            .try_collect::<Vec<_>>()
            .await
            .expect_err("Should error with invalid sha");

        let err = err
            .into_inner()
            .expect("Should have inner error")
            .downcast::<DigestError>()
            .expect("Should be a DigestError");
        assert!(
            matches!(*err, DigestError::VerificationError { .. }),
            "Error should be a verification error"
        );
    }
}
