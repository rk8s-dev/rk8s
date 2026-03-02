use futures::{Stream, TryStreamExt};
use std::{fmt::Debug, pin::Pin};
use xlinerpc::Status as XlineStatus;

use crate::error::Result;

/// A transport-agnostic stream wrapper that exposes a tonic-like `message()`
/// interface without tying callers to `tonic::Streaming<T>`.
///
/// The stream item error type is `xlinerpc::Status`, making this wrapper
/// usable over both tonic/HTTP-2 and QUIC transports.  The [`From`] impl for
/// `tonic::Streaming<T>` converts `tonic::Status` items to `xlinerpc::Status`
/// transparently so that existing callers need no changes.
pub struct MessageStream<T> {
    /// Pinned, type-erased inner stream.
    inner: Pin<Box<dyn Stream<Item = std::result::Result<T, XlineStatus>> + Send + 'static>>,
}

impl<T> MessageStream<T> {
    /// Create a new `MessageStream` from any compatible stream.
    #[inline]
    #[must_use]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = std::result::Result<T, XlineStatus>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }

    /// Receive the next message from the stream.
    ///
    /// Returns `Ok(None)` when the stream has been exhausted.
    ///
    /// # Errors
    ///
    /// Returns an error when the underlying transport stream fails.
    #[inline]
    pub async fn message(&mut self) -> Result<Option<T>> {
        Ok(self.inner.try_next().await?)
    }
}

impl<T> Debug for MessageStream<T> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MessageStream").finish_non_exhaustive()
    }
}

impl<T> From<tonic::Streaming<T>> for MessageStream<T>
where
    T: Send + 'static,
{
    /// Convert a `tonic::Streaming` into a `MessageStream`.
    ///
    /// Each `tonic::Status` error is mapped to an equivalent `xlinerpc::Status`
    /// using the same numeric code, preserving error semantics across
    /// transport boundaries.
    #[inline]
    fn from(stream: tonic::Streaming<T>) -> Self {
        use futures::StreamExt as _;
        Self::new(stream.map(|item| {
            item.map_err(|tonic_status| {
                XlineStatus::new(
                    xlinerpc::Code::from_i32(tonic_status.code() as i32),
                    tonic_status.message().to_owned(),
                )
            })
        }))
    }
}

/// Stream of [`xlineapi::LeaseKeepAliveResponse`] messages.
pub type LeaseKeepAliveStream = MessageStream<xlineapi::LeaseKeepAliveResponse>;
/// Stream of [`xlineapi::SnapshotResponse`] messages.
pub type SnapshotStream = MessageStream<xlineapi::SnapshotResponse>;
/// Stream of [`xlineapi::WatchResponse`] messages.
pub type WatchStream = MessageStream<xlineapi::WatchResponse>;

#[cfg(test)]
mod tests {
    use futures::stream;

    use super::MessageStream;

    #[tokio::test]
    async fn message_stream_receives_messages_in_order() {
        let stream = stream::iter(vec![
            Ok::<_, xlinerpc::Status>(1_u64),
            Ok::<_, xlinerpc::Status>(2_u64),
        ]);
        let mut stream = MessageStream::new(stream);

        assert_eq!(stream.message().await.expect("first").expect("some"), 1);
        assert_eq!(stream.message().await.expect("second").expect("some"), 2);
        assert!(stream.message().await.expect("eof").is_none());
    }

    #[tokio::test]
    async fn message_stream_returns_error_on_status_error() {
        let stream = stream::iter(vec![
            Err::<u64, _>(xlinerpc::Status::internal("boom")),
        ]);
        let mut stream = MessageStream::new(stream);
        assert!(stream.message().await.is_err());
    }

}
