use std::{collections::HashMap, marker::PhantomData, sync::Arc};

use futures::future::join_all;
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::node::NodeId;

use super::information_packet::Content;

/// # Output Channels
/// A hash-table mapping `NodeId` to `OutChannel`. In **Dagrs**, each `Node` stores output
/// channels in this map, enabling `Node` to send information packets to other `Node`s.
#[derive(Default)]
pub struct OutChannels(pub(crate) HashMap<NodeId, Arc<Mutex<OutChannel>>>);

impl OutChannels {
    /// Perform a blocking send on the outcoming channel from `NodeId`.
    pub fn blocking_send_to(&self, id: &NodeId, content: Content) -> Result<(), SendErr> {
        match self.get(id) {
            Some(channel) => channel.blocking_lock().blocking_send(content),
            None => Err(SendErr::NoSuchChannel),
        }
    }

    /// Perform a asynchronous send on the outcoming channel from `NodeId`.
    pub async fn send_to(&self, id: &NodeId, content: Content) -> Result<(), SendErr> {
        match self.get(id) {
            Some(channel) => channel.lock().await.send(content).await,
            None => Err(SendErr::NoSuchChannel),
        }
    }

    /// Broadcasts the `content` to all the [`OutChannel`]s asynchronously.
    pub async fn broadcast(&self, content: Content) -> Vec<Result<(), SendErr>> {
        let futures = self
            .0
            .values()
            .map(|c| async { c.lock().await.send(content.clone()).await });

        join_all(futures).await
    }

    /// Blocking broadcasts the `content` to all the [`OutChannel`]s.
    pub fn blocking_broadcast(&self, content: Content) -> Vec<Result<(), SendErr>> {
        self.0
            .values()
            .map(|c| c.blocking_lock().blocking_send(content.clone()))
            .collect()
    }

    /// Close the channel by the given `NodeId`, and remove the channel in this map.
    pub fn close(&mut self, id: &NodeId) {
        if self.get(id).is_some() {
            self.0.remove(id);
        }
    }

    pub(crate) fn close_all(&mut self) {
        self.0.clear();
    }

    fn get(&self, id: &NodeId) -> Option<Arc<Mutex<OutChannel>>> {
        self.0.get(id).cloned()
    }

    pub(crate) fn insert(&mut self, node_id: NodeId, channel: Arc<Mutex<OutChannel>>) {
        self.0.insert(node_id, channel);
    }

    /// Returns a list of all available receiver node IDs.
    pub fn get_receiver_ids(&self) -> Vec<NodeId> {
        self.0.keys().copied().collect()
    }
}

/// # Output Channel
/// Wrapper of senderrs of `tokio::sync::mpsc` and `tokio::sync::broadcast`. **Dagrs** will
/// decide the inner type of channel when building the graph.
/// Learn more about [Tokio Channels](https://tokio.rs/tokio/tutorial/channels).
pub enum OutChannel {
    /// Sender of a `tokio::sync::mpsc` channel.
    Mpsc(mpsc::Sender<Content>),
    /// Sender of a `tokio::sync::broadcast` channel.
    Bcst(broadcast::Sender<Content>),
}

impl OutChannel {
    /// Perform a blocking send on this channel.
    fn blocking_send(&self, value: Content) -> Result<(), SendErr> {
        match self {
            OutChannel::Mpsc(sender) => match sender.blocking_send(value) {
                Ok(_) => Ok(()),
                Err(e) => Err(SendErr::ClosedChannel(e.0)),
            },
            OutChannel::Bcst(sender) => match sender.send(value) {
                Ok(_) => Ok(()),
                Err(e) => Err(SendErr::ClosedChannel(e.0)),
            },
        }
    }

    /// Perform a asynchronous send on this channel.
    async fn send(&self, value: Content) -> Result<(), SendErr> {
        match self {
            OutChannel::Mpsc(sender) => match sender.send(value).await {
                Ok(_) => Ok(()),
                Err(e) => Err(SendErr::ClosedChannel(e.0)),
            },
            OutChannel::Bcst(sender) => match sender.send(value) {
                Ok(_) => Ok(()),
                Err(e) => Err(SendErr::ClosedChannel(e.0)),
            },
        }
    }
}

/// # Output Channel Error Types
/// - NoSuchChannel: try to get a channel with an invalid `NodeId`.
/// - ClosedChannel: the channel is closed alredy.
///
/// In cases of getting errs of type `MpscError` and `BcstError`, the sender
/// will find there are no active receivers left, so try to send messages is
/// meaningless for now.
#[derive(Debug)]
pub enum SendErr {
    NoSuchChannel,
    ClosedChannel(Content),
}

/// # Typed Output Channels
/// A hash-table mapping [`NodeId`] to [`OutChannel`]. This provides type-safe channel communication
/// between nodes.
#[derive(Default)]
pub struct TypedOutChannels<T: Send + Sync + 'static>(
    pub(crate) HashMap<NodeId, Arc<Mutex<OutChannel>>>,
    // maker for type T
    pub(crate) PhantomData<T>,
);

impl<T: Send + Sync + 'static> TypedOutChannels<T> {
    /// Perform a blocking send on the outcoming channel from `NodeId`.
    pub fn blocking_send_to(&self, id: &NodeId, content: T) -> Result<(), SendErr> {
        match self.get(id) {
            Some(channel) => channel.blocking_lock().blocking_send(Content::new(content)),
            None => Err(SendErr::NoSuchChannel),
        }
    }

    /// Perform a asynchronous send on the outcoming channel from `NodeId`.
    pub async fn send_to(&self, id: &NodeId, content: T) -> Result<(), SendErr> {
        match self.get(id) {
            Some(channel) => channel.lock().await.send(Content::new(content)).await,
            None => Err(SendErr::NoSuchChannel),
        }
    }

    /// Broadcasts the `content` to all the [`TypedOutChannel`]s asynchronously.
    pub async fn broadcast(&self, content: T) -> Vec<Result<(), SendErr>> {
        let content = Content::new(content);
        let futures = self
            .0
            .values()
            .map(|c| async { c.lock().await.send(content.clone()).await });

        join_all(futures).await
    }

    /// Blocking broadcasts the `content` to all the [`TypedOutChannel`]s.
    pub fn blocking_broadcast(&self, content: T) -> Vec<Result<(), SendErr>> {
        let content = Content::new(content);
        self.0
            .values()
            .map(|c| c.blocking_lock().blocking_send(content.clone()))
            .collect()
    }

    /// Close the channel by the given `NodeId`, and remove the channel in this map.
    pub fn close(&mut self, id: &NodeId) {
        if self.get(id).is_some() {
            self.0.remove(id);
        }
    }

    fn get(&self, id: &NodeId) -> Option<Arc<Mutex<OutChannel>>> {
        self.0.get(id).cloned()
    }

    /// Returns a list of all available receiver node IDs.
    pub fn get_receiver_ids(&self) -> Vec<NodeId> {
        self.0.keys().copied().collect()
    }
}
