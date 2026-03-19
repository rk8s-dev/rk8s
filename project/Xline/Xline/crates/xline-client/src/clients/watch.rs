use std::fmt::Debug;

use futures::channel::mpsc::channel;
use xlineapi::{RequestUnion, WatchResponse};

use crate::{
    Client, ClientOptions,
    error::{Result, XlineClientBuildError, XlineClientError},
    transport::{Channel, MethodId, Streaming},
    types::watch::{WatchOptions, WatchStreaming, Watcher},
};

/// Channel size for watch request stream
const CHANNEL_SIZE: usize = 128;

/// Client for Watch operations.
#[derive(Clone, Debug)]
pub struct WatchClient {
    /// The watch transport
    inner: Channel,
}

impl WatchClient {
    /// Creates a watch client by connecting to the cluster.
    ///
    /// This is a public construction path for users that want to work with
    /// `WatchClient` directly rather than going through `Client::watch_client()`.
    #[inline]
    pub async fn connect<E, S>(
        all_members: S,
        options: ClientOptions,
    ) -> std::result::Result<Self, XlineClientBuildError>
    where
        E: AsRef<str>,
        S: IntoIterator<Item = E>,
    {
        Ok(Client::connect(all_members, options).await?.watch_client())
    }

    /// Creates a new maintenance client
    #[inline]
    #[must_use]
    pub(crate) fn new(channel: Channel) -> Self {
        Self { inner: channel }
    }

    /// Watches for events happening or that have happened. Both input and output
    /// are streams; the input stream is for creating and canceling watcher and the output
    /// stream sends events. The entire event history can be watched starting from the
    /// last compaction revision.
    ///
    /// # Errors
    ///
    /// This function will return an error if the RPC client fails to send request
    ///
    /// # Panics
    ///
    /// This function will panic if the RPC server doesn't return a create watch response
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xline_client::{Client, ClientOptions};
    /// use anyhow::Result;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let curp_members = ["10.0.0.1:2379", "10.0.0.2:2379", "10.0.0.3:2379"];
    ///
    ///     let client = Client::connect(curp_members, ClientOptions::default()).await?;
    ///     let mut watch_client = client.watch_client();
    ///     let mut kv_client = client.kv_client();
    ///
    ///     let (mut watcher, mut stream) = watch_client.watch("key1", None).await?;
    ///     kv_client.put("key1", "value1", None).await?;
    ///
    ///     let resp = stream.message().await?.unwrap();
    ///     let kv = resp.events[0].kv.as_ref().unwrap();
    ///
    ///     println!(
    ///         "got key: {}, value: {}",
    ///         String::from_utf8_lossy(&kv.key),
    ///         String::from_utf8_lossy(&kv.value)
    ///     );
    ///
    ///     // cancel the watch
    ///     watcher.cancel()?;
    ///
    ///     Ok(())
    /// }
    /// ```
    #[inline]
    pub async fn watch<K: Into<Vec<u8>>>(
        &mut self,
        key: K,
        options: Option<WatchOptions>,
    ) -> Result<(Watcher, WatchStreaming)> {
        let (mut request_sender, request_receiver) =
            channel::<xlineapi::WatchRequest>(CHANNEL_SIZE);

        let request = xlineapi::WatchRequest {
            request_union: Some(RequestUnion::CreateRequest(
                options.unwrap_or_default().with_key(key.into()).into(),
            )),
        };

        request_sender
            .try_send(request)
            .map_err(|e| XlineClientError::WatchError(e.to_string()))?;

        let mut response_stream: Streaming<WatchResponse> = self
            .inner
            .client_streaming(MethodId::XlineWatch, request_receiver)
            .await?;

        let watch_id = match response_stream.message().await? {
            Some(resp) => {
                assert!(resp.created, "not a create watch response");
                resp.watch_id
            }
            None => {
                return Err(XlineClientError::WatchError(String::from(
                    "failed to create watch",
                )));
            }
        };

        Ok((
            Watcher::new(watch_id, request_sender.clone()),
            WatchStreaming::new(response_stream, request_sender),
        ))
    }
}
