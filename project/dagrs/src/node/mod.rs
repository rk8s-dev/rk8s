pub mod action;
pub mod conditional_node;
pub mod default_node;
pub mod id_allocate;
pub mod loop_node;
pub mod router_node;

pub mod typed_action;

pub use action::{Action, EmptyAction};
pub use conditional_node::ConditionalNode;
pub use default_node::DefaultNode;
pub use loop_node::{LoopCondition, LoopNode};
pub use router_node::{Router, RouterNode};

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

    /// Returns the maximum number of retry attempts for this node.
    ///
    /// When a node fails (returns `Output::Err`), the graph executor will
    /// retry the node up to this many times before marking it as failed.
    ///
    /// # Default
    /// Returns 0 by default, meaning no retries (fail immediately).
    ///
    /// # Example
    /// Override this method to enable retries:
    /// ```ignore
    /// fn max_retries(&self) -> u32 {
    ///     3 // Retry up to 3 times
    /// }
    /// ```
    fn max_retries(&self) -> u32 {
        0
    }

    /// Returns the delay between retry attempts in milliseconds.
    ///
    /// This can be used to implement backoff strategies.
    /// The default implementation returns a fixed 100ms delay.
    ///
    /// # Arguments
    /// * `attempt` - The current retry attempt number (1-indexed)
    ///
    /// # Returns
    /// Delay in milliseconds before the next retry attempt.
    fn retry_delay_ms(&self, _attempt: u32) -> u64 {
        100
    }

    /// Reset the node state to its initial state.
    ///
    /// # Behavior
    /// - This method is **optional**. The default implementation does nothing.
    /// - It is **ONLY** called when `Graph::reset()` is invoked. The node should not call this itself.
    /// - Nodes with internal state (e.g., counters, buffers) **MUST** implement this method
    ///   to ensure correct behavior when the graph is re-executed.
    fn reset(&mut self) {}
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
