pub mod action;
pub mod conditional_node;
pub mod default_node;
pub mod id_allocate;

pub mod typed_action;
use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::{
    connection::{in_channel::InChannels, out_channel::OutChannels},
    utils::{env::EnvVar, output::Output},
};

use id_allocate::alloc_id;

///# The [`Node`] trait
///
/// Nodes are the basic scheduling units of Graph. They can be identified by
/// a globally assigned [`NodeId`] and a user-provided name.
///
/// Nodes can communicate with others asynchronously through [`InChannels`] and [`OutChannels`].
///
/// In addition to the above properties, users can also customize some other attributes.
#[async_trait]
pub trait Node: Send + Sync {
    /// id is the unique identifier of each node, it will be assigned by the [`NodeTable`]
    /// when creating a new node, you can find this node through this identifier.
    fn id(&self) -> NodeId;
    /// The node's name.
    fn name(&self) -> NodeName;
    /// Input Channels of this node.
    fn input_channels(&mut self) -> &mut InChannels;
    /// Output Channels of this node.
    fn output_channels(&mut self) -> &mut OutChannels;
    /// Execute a run of this node.
    async fn run(&mut self, env: Arc<EnvVar>) -> Output;
    /// Return true if this node is conditional node. By default, it returns false.
    fn is_condition(&self) -> bool {
        false
    }
    /// Returns the list of nodes that are part of this node's loop structure, if any.
    ///
    /// This method is used to identify nodes that are part of a loop-like structure, such as a loop subgraph.
    /// When this method returns Some(nodes), the loop detection check will skip checking these nodes for cycles.
    ///
    /// Returns None by default, indicating this is not a loop-containing node.
    fn loop_structure(&self) -> Option<Vec<Arc<Mutex<dyn Node>>>> {
        None
    }

    /// Returns true if this node has TypedContent input.
    /// By default, it returns false.
    fn has_typed_input(&self) -> bool {
        false
    }

    /// Returns true if this node has TypedContent output.
    /// By default, it returns false.
    fn has_typed_output(&self) -> bool {
        false
    }
}

#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy, Ord, PartialOrd)]
pub struct NodeId(pub(crate) usize);

impl NodeId {
    /// Return the numeric identifier wrapped by this `NodeId`.
    pub fn as_usize(&self) -> usize {
        self.0
    }
}

impl From<NodeId> for usize {
    fn from(value: NodeId) -> Self {
        value.0
    }
}

pub type NodeName = String;

/// [NodeTable]: a mapping from [Node]'s name to [NodeId].
#[derive(Default)]
pub struct NodeTable(pub(crate) HashMap<NodeName, NodeId>);

/// [NodeTable]'s name in [`EnvVar`].
pub const NODE_TABLE_STR: &str = "node_table";

impl NodeTable {
    /// Alloc a new [NodeId] for a [Node].
    ///
    /// If there is a Node requesting for an ID with a duplicate name,
    /// the older one's info will be overwritten.
    pub fn alloc_id_for(&mut self, name: &str) -> NodeId {
        let id = alloc_id();
        log::debug!("alloc id {:?} for {:?}", id, name);

        if let Some(v) = self.0.insert(name.to_string(), id) {
            log::warn!("Node {} is already allocated with id {:?}.", name, v);
        };
        id
    }

    /// Get the [`NodeId`] of the node corresponding to its name.
    pub fn get(&self, name: &str) -> Option<&NodeId> {
        self.0.get(name)
    }

    /// Create an empty [`NodeTable`].
    pub fn new() -> Self {
        Self::default()
    }
}

impl EnvVar {
    /// Get a [`Node`]'s [`NodeId`] by providing its name.
    pub fn get_node_id(&self, node_name: &str) -> Option<&NodeId> {
        let node_table: &NodeTable = self.get_ref(NODE_TABLE_STR).unwrap();
        node_table.get(node_name)
    }
}
