use futures::{Stream, TryStreamExt};
use std::{fmt::Debug, pin::Pin};
use tonic::Status;

use crate::error::Result;

/// A transport-agnostic stream wrapper that exposes a tonic-like `message()`
/// interface without tying callers to `tonic::Streaming<T>`.
///
/// Internally, the stream item type is `Result<T, tonic::Status>` so that
/// existing code which converts a `tonic::Streaming` via the [`From`] impl
/// continues to work without changes.
pub struct MessageStream<T> {
    /// Pinned, type-erased inner stream.
    inner: Pin<Box<dyn Stream<Item = std::result::Result<T, Status>> + Send + 'static>>,
}

impl<T> MessageStream<T> {
    /// Create a new `MessageStream` from any compatible stream.
    #[inline]
    #[must_use]
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = std::result::Result<T, Status>> + Send + 'static,
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
    #[inline]
    fn from(stream: tonic::Streaming<T>) -> Self {
        Self::new(stream)
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
            Ok::<_, tonic::Status>(1_u64),
            Ok::<_, tonic::Status>(2_u64),
        ]);
        let mut stream = MessageStream::new(stream);

        assert_eq!(stream.message().await.expect("first").expect("some"), 1);
        assert_eq!(stream.message().await.expect("second").expect("some"), 2);
        assert!(stream.message().await.expect("eof").is_none());
    }

    #[tokio::test]
    async fn message_stream_returns_error_on_status_error() {
        let stream = stream::iter(vec![
            Err::<u64, _>(tonic::Status::internal("boom")),
        ]);
        let mut stream = MessageStream::new(stream);
        assert!(stream.message().await.is_err());
    }
}
