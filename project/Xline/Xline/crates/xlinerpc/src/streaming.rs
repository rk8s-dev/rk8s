//! Shared streaming response wrapper.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Stream, StreamExt};

use crate::Status;

/// Generic RPC response stream wrapper.
pub struct Streaming<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>,
}

impl<T> std::fmt::Debug for Streaming<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Streaming").finish_non_exhaustive()
    }
}

impl<T> Streaming<T> {
    /// Create a new stream wrapper.
    #[must_use]
    #[inline]
    pub fn new(inner: Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>) -> Self {
        Self { inner }
    }

    /// Receive the next message from the stream.
    pub async fn message(&mut self) -> Result<Option<T>, Status> {
        self.inner.next().await.transpose()
    }
}

impl<T> Stream for Streaming<T> {
    type Item = Result<T, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.as_mut().poll_next(cx)
    }
}
