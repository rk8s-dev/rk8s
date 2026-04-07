use std::{collections::HashMap, marker::PhantomData, sync::Arc};

use futures::future::join_all;
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::graph::error::{DagrsError, DagrsResult, ErrorCode};
use crate::node::NodeId;

use super::information_packet::Content;

#[derive(Default)]
pub struct OutChannels(pub(crate) HashMap<NodeId, Arc<Mutex<OutChannel>>>);

impl OutChannels {
    pub async fn send_to(&self, id: &NodeId, content: Content) -> DagrsResult<()> {
        match self.get(id) {
            Some(channel) => channel.lock().await.send(*id, content).await,
            None => Err(no_such_channel(*id)),
        }
    }

    pub async fn broadcast(&self, content: Content) -> Vec<DagrsResult<()>> {
        let futures = self
            .0
            .iter()
            .map(|(id, c)| async { c.lock().await.send(*id, content.clone()).await });

        join_all(futures).await
    }

    pub async fn close(&mut self, id: &NodeId) {
        if self.get(id).is_some() {
            self.0.remove(id);
        }
    }

    pub(crate) async fn close_all(&mut self) {
        self.0.clear();
    }

    fn get(&self, id: &NodeId) -> Option<Arc<Mutex<OutChannel>>> {
        self.0.get(id).cloned()
    }

    pub(crate) fn insert(&mut self, node_id: NodeId, channel: Arc<Mutex<OutChannel>>) {
        self.0.insert(node_id, channel);
    }

    pub fn get_receiver_ids(&self) -> Vec<NodeId> {
        self.0.keys().copied().collect()
    }
}

pub enum OutChannel {
    Mpsc(mpsc::Sender<Content>),
    Bcst(broadcast::Sender<Content>),
}

impl OutChannel {
    async fn send(&self, channel_id: NodeId, value: Content) -> DagrsResult<()> {
        match self {
            OutChannel::Mpsc(sender) => match sender.send(value).await {
                Ok(_) => Ok(()),
                Err(_) => Err(closed_channel(channel_id)),
            },
            OutChannel::Bcst(sender) => match sender.send(value) {
                Ok(_) => Ok(()),
                Err(_) => Err(closed_channel(channel_id)),
            },
        }
    }
}

#[derive(Default)]
pub struct TypedOutChannels<T: Send + Sync + 'static>(
    pub(crate) HashMap<NodeId, Arc<Mutex<OutChannel>>>,
    pub(crate) PhantomData<T>,
);

impl<T: Send + Sync + 'static> TypedOutChannels<T> {
    pub async fn send_to(&self, id: &NodeId, content: T) -> DagrsResult<()> {
        match self.get(id) {
            Some(channel) => channel.lock().await.send(*id, Content::new(content)).await,
            None => Err(no_such_channel(*id)),
        }
    }

    pub async fn broadcast(&self, content: T) -> Vec<DagrsResult<()>> {
        let content = Content::new(content);
        let futures = self
            .0
            .iter()
            .map(|(id, c)| async { c.lock().await.send(*id, content.clone()).await });

        join_all(futures).await
    }

    pub async fn close(&mut self, id: &NodeId) {
        if self.get(id).is_some() {
            self.0.remove(id);
        }
    }

    fn get(&self, id: &NodeId) -> Option<Arc<Mutex<OutChannel>>> {
        self.0.get(id).cloned()
    }

    pub fn get_receiver_ids(&self) -> Vec<NodeId> {
        self.0.keys().copied().collect()
    }
}

fn no_such_channel(id: NodeId) -> DagrsError {
    DagrsError::new(ErrorCode::DgChn0001NoSuchChannel, "channel not found")
        .with_channel(id.as_usize())
}

fn closed_channel(id: NodeId) -> DagrsError {
    DagrsError::new(ErrorCode::DgChn0002Closed, "channel is closed").with_channel(id.as_usize())
}
