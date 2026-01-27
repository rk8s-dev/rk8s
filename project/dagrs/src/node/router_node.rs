use crate::connection::{in_channel::InChannels, out_channel::OutChannels};
use crate::node::{Node, NodeId, NodeName, NodeTable};
use crate::utils::{env::EnvVar, output::FlowControl, output::Output};
use async_trait::async_trait;
use std::sync::Arc;

/// Router trait for routing logic.
///
/// This trait allows users to implement dynamic branching in the graph.
/// A `Router` decides which downstream path(s) execution should follow.
#[async_trait]
pub trait Router: Send + Sync {
    /// Determine the next nodes to execute.
    ///
    /// # Arguments
    /// * `input` - Input channels to read data from upstream nodes.
    /// * `output` - Output channels to send data to downstream nodes.
    /// * `env` - The execution environment variables.
    ///
    /// # Returns
    /// A vector of `usize` representing the `NodeId`s of the nodes that should be activated.
    /// Any downstream nodes connected to this router that are NOT in this list will be skipped (pruned).
    ///
    /// # Data Transmission vs. Route Selection
    /// The `route` method serves two purposes:
    /// 1. **Data Transmission**: It can optionally send data to downstream nodes via `output`.
    ///    This is useful if the router acts as a dispatcher or load balancer.
    /// 2. **Route Selection**: It MUST return the IDs of the nodes that should run.
    ///    Even if data is sent to a node, if its ID is not in the returned vector, it will NOT run.
    async fn route(
        &self,
        input: &mut InChannels,
        output: &OutChannels,
        env: Arc<EnvVar>,
    ) -> Vec<usize>;

    /// Reset the router state.
    /// This is called when the graph is reset.
    fn reset(&mut self) {}
}

/// Router node implementation.
///
/// Wraps a user-provided `Router` implementation and integrates it into the graph execution.
/// When executed, it delegates to `Router::route` and converts the result into a `FlowControl::Branch` instruction.
pub struct RouterNode {
    id: NodeId,
    name: NodeName,
    in_channels: InChannels,
    out_channels: OutChannels,
    router: Box<dyn Router>,
}

impl RouterNode {
    /// Create a new RouterNode
    pub fn new(name: NodeName, router: impl Router + 'static, node_table: &mut NodeTable) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
            router: Box::new(router),
        }
    }
}

#[async_trait]
impl Node for RouterNode {
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
    async fn run(&mut self, env: Arc<EnvVar>) -> Output {
        let routes = self
            .router
            .route(&mut self.in_channels, &self.out_channels, env)
            .await;
        Output::Flow(FlowControl::Branch(routes))
    }
    fn is_condition(&self) -> bool {
        true
    }

    fn reset(&mut self) {
        self.router.reset();
    }
}
