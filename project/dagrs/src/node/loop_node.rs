use crate::connection::{in_channel::InChannels, out_channel::OutChannels};
use crate::node::{Node, NodeId, NodeName, NodeTable};
use crate::utils::{env::EnvVar, output::FlowControl, output::Output};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};

/// A trait defining the condition for a loop to continue.
///
/// This trait allows users to define custom logic for controlling loop execution.
/// The condition is evaluated *after* the execution of the nodes within the loop body.
///
/// # Evaluation Semantics
/// - **Post-check**: The loop body executes at least once before `should_continue` is called.
/// - **Frequency**: Called once per iteration, after all nodes in the loop body have finished.
///
/// # Example
///
/// ```rust
/// use dagrs::{LoopCondition, InChannels, OutChannels, EnvVar};
/// use std::sync::Arc;
///
/// struct MyLoopCondition {
///     count: usize,
/// }
///
/// impl LoopCondition for MyLoopCondition {
///     fn should_continue(&mut self, _input: &InChannels, _out: &OutChannels, _env: Arc<EnvVar>) -> bool {
///         self.count += 1;
///         self.count < 5
///     }
///     
///     fn reset(&mut self) {
///         self.count = 0;
///     }
/// }
/// ```
pub trait LoopCondition: Send + Sync {
    /// Determines whether the loop should continue for another iteration.
    ///
    /// This method is called by the `LoopNode` during execution.
    ///
    /// # Arguments
    ///
    /// * `input` - Input channels to the loop node.
    /// * `out` - Output channels from the loop node.
    /// * `env` - The environment variables.
    ///
    /// # Returns
    ///
    /// `true` if the loop should continue (jump back to target), `false` otherwise (proceed to next node).
    fn should_continue(&mut self, input: &InChannels, out: &OutChannels, env: Arc<EnvVar>) -> bool;

    /// Reset the condition state.
    ///
    /// This is called when `Graph::reset()` is invoked.
    /// Implementors MUST reset any internal counters or state here to ensure
    /// the loop behaves correctly on subsequent graph executions.
    fn reset(&mut self) {}
}

/// Loop node that repeats execution of a target node based on a condition
pub struct CountLoopCondition {
    max_iterations: usize,
    current_iteration: usize,
}

impl CountLoopCondition {
    /// Create a new CountLoopCondition with the specified maximum iterations
    pub fn new(max: usize) -> Self {
        Self {
            max_iterations: max,
            current_iteration: 0,
        }
    }
}

impl LoopCondition for CountLoopCondition {
    /// Determine if the loop should continue based on the current iteration count
    fn should_continue(
        &mut self,
        _input: &InChannels,
        _out: &OutChannels,
        _env: Arc<EnvVar>,
    ) -> bool {
        if self.current_iteration < self.max_iterations {
            self.current_iteration += 1;
            true
        } else {
            false
        }
    }

    fn reset(&mut self) {
        self.current_iteration = 0;
    }
}

/// A node that executes a loop control flow.
///
/// The `LoopNode` acts as a controller for a loop structure in the graph.
/// It typically sits at the end of a sequence of nodes that form the loop body.
///
/// # Mechanism
/// 1. **Execution**: When the graph execution reaches the `LoopNode`, it invokes the
///    `should_continue` method of its `LoopCondition`.
/// 2. **Evaluation**:
///    - If `should_continue` returns `true`: The node issues a `FlowControl::Loop` instruction.
///      The graph executor receives this and jumps execution back to the `target_node`.
///    - If `should_continue` returns `false`: The node issues a `FlowControl::Continue` instruction.
///      Execution proceeds to the next node in the topological order (exiting the loop).
///
/// # Iteration Semantics
/// The loop mechanism implements a **do-while** style loop (or repeat-until), where the
/// condition is checked *after* the body has executed at least once (assuming the `LoopNode`
/// is placed after the body nodes).
///
/// # Structure
/// To form a loop, the typical structure is:
/// `TargetNode -> [Body Nodes...] -> LoopNode`
///
/// - `target_node`: The ID of the node where execution should restart (the beginning of the loop).
/// - The `LoopNode` MUST have a dependency on the last node of the loop body.
/// - The backward jump is dynamic and does NOT require an explicit edge in the graph definition
///   (adding such an edge would cause a cycle detection error).
pub struct LoopNode {
    id: NodeId,
    name: NodeName,
    in_channels: InChannels,
    out_channels: OutChannels,
    target_node: NodeId,
    condition: Mutex<Box<dyn LoopCondition>>,
}

impl LoopNode {
    /// Create a new LoopNode
    pub fn new(
        name: NodeName,
        target_node: NodeId,
        condition: impl LoopCondition + 'static,
        node_table: &mut NodeTable,
    ) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
            target_node,
            condition: Mutex::new(Box::new(condition)),
        }
    }
}

#[async_trait]
impl Node for LoopNode {
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
        let should_continue = self
            .condition
            .lock()
            .unwrap_or_else(|poisoned| {
                log::warn!("LoopNode condition mutex was poisoned, recovering");
                poisoned.into_inner()
            })
            .should_continue(&self.in_channels, &self.out_channels, env);

        if should_continue {
            Output::Flow(FlowControl::loop_to_node(self.target_node.as_usize()))
        } else {
            Output::Flow(FlowControl::Continue)
        }
    }

    fn reset(&mut self) {
        self.condition
            .lock()
            .unwrap_or_else(|poisoned| {
                log::warn!("LoopNode condition mutex was poisoned during reset, recovering");
                poisoned.into_inner()
            })
            .reset();
    }
}
