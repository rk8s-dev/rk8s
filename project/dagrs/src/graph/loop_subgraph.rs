use async_trait::async_trait;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::{
    DagrsError, DagrsResult, EnvVar, ErrorCode, InChannels, Node, NodeId, NodeName, NodeTable,
    OutChannels, Output,
};

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
    inner_node_ids: HashSet<NodeId>,
}

impl LoopSubgraph {
    pub fn new(name: NodeName, node_table: &mut NodeTable) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
            inner_nodes: Vec::new(),
            inner_node_ids: HashSet::new(),
        }
    }

    /// Fallible constructor kept for API symmetry with the main graph build path.
    ///
    /// Construction is currently infallible, so this simply returns `Ok(Self::new(...))`.
    /// Callers that prefer a uniform `Result`-based build style can use this variant.
    pub fn try_new(name: NodeName, node_table: &mut NodeTable) -> DagrsResult<Self> {
        Ok(Self::new(name, node_table))
    }

    /// Add a node to the subgraph
    pub fn add_node(&mut self, node: impl Node + 'static) -> DagrsResult<NodeId> {
        let node_id = node.id();
        if self.inner_node_ids.contains(&node_id) {
            return Err(DagrsError::new(
                ErrorCode::DgBld0003DuplicateNodeId,
                "duplicate node id detected while building loop subgraph",
            )
            .with_node_id(node_id.as_usize()));
        }

        self.inner_node_ids.insert(node_id);
        self.inner_nodes.push(Arc::new(Mutex::new(node)));
        Ok(node_id)
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
        Output::error(DagrsError::new(
            ErrorCode::DgRun0006NodeExecutionFailed,
            "loop subgraph should not be executed directly",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::LoopSubgraph;
    use crate::node::{NodeId, NodeName};
    use crate::{EnvVar, InChannels, Node, NodeTable, OutChannels, Output};
    use async_trait::async_trait;
    use std::sync::Arc;

    struct FixedIdNode {
        id: NodeId,
        name: NodeName,
        in_channels: InChannels,
        out_channels: OutChannels,
    }

    impl FixedIdNode {
        fn new(id: NodeId, name: impl Into<NodeName>) -> Self {
            Self {
                id,
                name: name.into(),
                in_channels: InChannels::default(),
                out_channels: OutChannels::default(),
            }
        }
    }

    #[async_trait]
    impl Node for FixedIdNode {
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

        async fn run(&mut self, _: Arc<EnvVar>) -> Output {
            Output::empty()
        }
    }

    #[test]
    fn add_node_rejects_duplicate_ids() {
        let mut table = NodeTable::new();
        let mut loop_subgraph = LoopSubgraph::new("loop".to_string(), &mut table);
        let duplicate_id = NodeId(42);

        loop_subgraph
            .add_node(FixedIdNode::new(duplicate_id, "first"))
            .unwrap();
        let err = loop_subgraph
            .add_node(FixedIdNode::new(duplicate_id, "second"))
            .expect_err("duplicate loop node id should fail");

        assert_eq!(err.code, crate::ErrorCode::DgBld0003DuplicateNodeId);
    }
}
