use std::sync::Arc;

use async_trait::async_trait;

use crate::{EnvVar, InChannels, Node, NodeId, NodeName, NodeTable, OutChannels, Output};

/// # `Condition` trait
///
/// The [`Condition`] trait trait defines the condition logic for a [`ConditionalNode`].
/// The `run` method evaluates the condition based on input channels, output channels and environment variables.
/// Returns a boolean indicating whether the condition is met.
#[async_trait]
pub trait Condition: Send + Sync {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &OutChannels,
        env: Arc<EnvVar>,
    ) -> bool;
}

/// # Conditional node type
///
/// [`ConditionalNode`] is a node type that executes based on a condition.
/// It can evaluate the condition using input channels, output channels and environment variables.
/// The condition logic is defined by implementing the [`Condition`] trait.
///
/// When the condition is met during execution, Dagrs will continue running.
/// If the condition is not met, Dagrs will stop on this node.
pub struct ConditionalNode {
    id: NodeId,
    name: NodeName,
    condition: Box<dyn Condition>,
    in_channels: InChannels,
    out_channels: OutChannels,
}

impl ConditionalNode {
    /// Creates a new [`ConditionalNode`] with the given name and condition.
    /// The [`Condition`] determines whether execution should continue or stop at this node.
    /// The [`NodeTable`] is used to allocate a unique ID for this node.
    pub fn with_condition(
        name: NodeName,
        condition: impl Condition + 'static,
        node_table: &mut NodeTable,
    ) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            condition: Box::new(condition),
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
        }
    }
}

#[async_trait]
impl Node for ConditionalNode {
    /// Returns the unique identifier of this conditional node
    fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the name of this conditional node
    fn name(&self) -> NodeName {
        self.name.clone()
    }

    /// Returns a mutable reference to the input channels of this node
    fn input_channels(&mut self) -> &mut InChannels {
        &mut self.in_channels
    }

    /// Returns a mutable reference to the output channels of this node
    fn output_channels(&mut self) -> &mut OutChannels {
        &mut self.out_channels
    }

    /// Executes the condition logic of this node
    async fn run(&mut self, env: Arc<EnvVar>) -> Output {
        Output::ConditionResult(
            self.condition
                .run(&mut self.in_channels, &self.out_channels, env)
                .await,
        )
    }

    /// Returns true since this is a conditional node
    fn is_condition(&self) -> bool {
        true
    }
}
