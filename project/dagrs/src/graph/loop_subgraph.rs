use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{EnvVar, InChannels, Node, NodeId, NodeName, NodeTable, OutChannels, Output};

/// A special node type that represents a subgraph of nodes in a loop structure.
///
/// The LoopSubgraph is included in the main graph as a single node, but internally contains
/// multiple nodes that will be executed repeatedly. The connection and execution of the loop is controlled
/// by the parent graph rather than the LoopSubgraph itself.
pub struct LoopSubgraph {
    id: NodeId,
    name: NodeName,
    in_channels: InChannels,
    out_channels: OutChannels,
    // Inner nodes, contains the nodes that need to be executed in a loop
    inner_nodes: Vec<Arc<Mutex<dyn Node>>>,
}

impl LoopSubgraph {
    pub fn new(name: NodeName, node_table: &mut NodeTable) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
            inner_nodes: Vec::new(),
        }
    }

    /// Add a node to the subgraph
    pub fn add_node(&mut self, node: impl Node + 'static) {
        self.inner_nodes.push(Arc::new(Mutex::new(node)));
    }
}

#[async_trait]
impl Node for LoopSubgraph {
    fn id(&self) -> NodeId {
        self.id
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }

    fn input_channels(&mut self) -> &mut InChannels {
        &mut self.in_channels
    }

    fn output_channels(&mut self) -> &mut OutChannels {
        &mut self.out_channels
    }

    fn loop_structure(&self) -> Option<Vec<Arc<Mutex<dyn Node>>>> {
        Some(self.inner_nodes.clone())
    }

    async fn run(&mut self, _: Arc<EnvVar>) -> Output {
        panic!("Loop subgraph is not executed directly, it will be executed by the parent graph.");
    }
}
