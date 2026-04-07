use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::Arc,
};

use futures::future::{join_all, select_ok};
use tokio::sync::{Mutex, broadcast, mpsc};

use crate::NodeId;
use crate::graph::error::{DagrsError, DagrsResult, ErrorCode};

use super::information_packet::Content;

#[derive(Default)]
pub struct InChannels(
    pub(crate) HashMap<NodeId, Arc<Mutex<InChannel>>>,
    pub(crate) HashSet<NodeId>,
);

impl InChannels {
    pub async fn recv_from(&mut self, id: &NodeId) -> DagrsResult<Content> {
        if self.is_disabled(id) {
            return Err(disabled_channel(*id));
        }
        match self.get(id) {
            Some(channel) => channel.lock().await.recv(*id).await,
            None => Err(no_such_channel(*id)),
        }
    }

    pub async fn recv_any(&mut self) -> DagrsResult<(NodeId, Content)> {
        let mut futures = Vec::new();
        let ids = self.keys();

        for id in ids {
            let channel = self.get(&id).ok_or_else(|| no_such_channel(id))?;
            let fut = Box::pin(async move {
                let content = channel.lock().await.recv(id).await?;
                Ok::<_, DagrsError>((id, content))
            });
            futures.push(fut);
        }

        if futures.is_empty() {
            return Err(no_such_channel(NodeId(0)).with_detail("scope", "recv_any"));
        }

        match select_ok(futures).await {
            Ok((result, _)) => Ok(result),
            Err(err) => Err(err),
        }
    }

    pub async fn map<F, T>(&mut self, f: F) -> Vec<T>
    where
        F: FnMut(DagrsResult<Content>) -> T,
    {
        let disabled = self.1.clone();
        let futures = self
            .0
            .iter_mut()
            .filter(|(id, _)| !disabled.contains(id))
            .map(|(id, c)| async move { c.lock().await.recv(*id).await });
        join_all(futures).await.into_iter().map(f).collect()
    }

    pub async fn close(&mut self, id: &NodeId) {
        if let Some(c) = self.get(id) {
            c.lock().await.close();
            self.0.remove(id);
            self.1.remove(id);
        }
    }

    pub(crate) fn insert(&mut self, node_id: NodeId, channel: Arc<Mutex<InChannel>>) {
        self.0.insert(node_id, channel);
        self.1.remove(&node_id);
    }

    pub(crate) async fn close_all(&mut self) {
        let channels: Vec<_> = self.0.values().cloned().collect();
        for channel in channels {
            channel.lock().await.close();
        }
        self.1.clear();
    }

    pub(crate) fn disable_sender(&mut self, node_id: NodeId) {
        if self.0.contains_key(&node_id) {
            self.1.insert(node_id);
        }
    }

    pub(crate) fn enable_sender(&mut self, node_id: NodeId) {
        self.1.remove(&node_id);
    }

    pub(crate) fn clear_disabled(&mut self) {
        self.1.clear();
    }

    pub fn get_sender_ids(&self) -> Vec<NodeId> {
        self.keys()
    }

    fn get(&self, id: &NodeId) -> Option<Arc<Mutex<InChannel>>> {
        self.0.get(id).cloned()
    }

    fn keys(&self) -> Vec<NodeId> {
        self.0
            .keys()
            .filter(|id| !self.is_disabled(id))
            .copied()
            .collect()
    }

    fn is_disabled(&self, id: &NodeId) -> bool {
        self.1.contains(id)
    }
}

pub enum InChannel {
    Mpsc(mpsc::Receiver<Content>),
    Bcst(broadcast::Receiver<Content>),
}

impl InChannel {
    async fn recv(&mut self, channel_id: NodeId) -> DagrsResult<Content> {
        match self {
            InChannel::Mpsc(receiver) => receiver.recv().await.ok_or_else(|| {
                DagrsError::new(ErrorCode::DgChn0002Closed, "channel is closed")
                    .with_channel(channel_id.as_usize())
            }),
            InChannel::Bcst(receiver) => match receiver.recv().await {
                Ok(v) => Ok(v),
                Err(broadcast::error::RecvError::Closed) => Err(DagrsError::new(
                    ErrorCode::DgChn0002Closed,
                    "channel is closed",
                )
                .with_channel(channel_id.as_usize())),
                Err(broadcast::error::RecvError::Lagged(x)) => Err(DagrsError::new(
                    ErrorCode::DgChn0003Lagged,
                    "channel receiver lagged behind broadcast sender",
                )
                .with_channel(channel_id.as_usize())
                .with_detail("lagged", x.to_string())),
            },
        }
    }

    fn close(&mut self) {
        match self {
            InChannel::Mpsc(receiver) => receiver.close(),
            InChannel::Bcst(_) => (),
        }
    }
}

#[derive(Default)]
pub struct TypedInChannels<T: Send + Sync + 'static>(
    pub(crate) HashMap<NodeId, Arc<Mutex<InChannel>>>,
    pub(crate) HashSet<NodeId>,
    pub(crate) PhantomData<T>,
);

impl<T: Send + Sync + 'static> TypedInChannels<T> {
    pub async fn recv_from(&mut self, id: &NodeId) -> DagrsResult<Option<Arc<T>>> {
        if self.is_disabled(id) {
            return Err(disabled_channel(*id));
        }
        match self.get(id) {
            Some(channel) => {
                let content: Content = channel.lock().await.recv(*id).await?;
                Ok(content.into_inner())
            }
            None => Err(no_such_channel(*id)),
        }
    }

    pub async fn recv_any(&mut self) -> DagrsResult<(NodeId, Option<Arc<T>>)> {
        let mut futures = Vec::new();
        let ids = self.keys();

        for id in ids {
            let channel = self.get(&id).ok_or_else(|| no_such_channel(id))?;
            let fut = Box::pin(async move {
                let content: Content = channel.lock().await.recv(id).await?;
                Ok::<_, DagrsError>((id, content.into_inner()))
            });
            futures.push(fut);
        }

        if futures.is_empty() {
            return Err(no_such_channel(NodeId(0)).with_detail("scope", "typed_recv_any"));
        }

        match select_ok(futures).await {
            Ok((result, _)) => Ok(result),
            Err(err) => Err(err),
        }
    }

    pub async fn map<F, U>(&mut self, f: F) -> Vec<U>
    where
        F: FnMut(DagrsResult<Option<Arc<T>>>) -> U,
    {
        let disabled = self.1.clone();
        let futures = self
            .0
            .iter_mut()
            .filter(|(id, _)| !disabled.contains(id))
            .map(|(id, c)| async move {
                let content: Content = c.lock().await.recv(*id).await?;
                Ok(content.into_inner())
            });
        join_all(futures).await.into_iter().map(f).collect()
    }

    pub async fn close(&mut self, id: &NodeId) {
        if let Some(c) = self.get(id) {
            c.lock().await.close();
            self.0.remove(id);
            self.1.remove(id);
        }
    }

    fn get(&self, id: &NodeId) -> Option<Arc<Mutex<InChannel>>> {
        self.0.get(id).cloned()
    }

    fn keys(&self) -> Vec<NodeId> {
        self.0
            .keys()
            .filter(|id| !self.is_disabled(id))
            .copied()
            .collect()
    }

    fn is_disabled(&self, id: &NodeId) -> bool {
        self.1.contains(id)
    }
}

fn no_such_channel(id: NodeId) -> DagrsError {
    DagrsError::new(ErrorCode::DgChn0001NoSuchChannel, "channel not found")
        .with_channel(id.as_usize())
}

fn disabled_channel(id: NodeId) -> DagrsError {
    DagrsError::new(
        ErrorCode::DgChn0002Closed,
        "channel is disabled by restored control flow state",
    )
    .with_channel(id.as_usize())
    .with_detail("state", "disabled")
}
