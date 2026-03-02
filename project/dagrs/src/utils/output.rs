//! Node output
//!
//! [`Output`] represents the output of the Node respectively.
//!
//! Users should consider the output results of the Node when defining the specific
//! behavior of the Node. The input results may be: normal output, no output, or Node
//! execution error message.
//! It should be noted that the content stored in [`Output`] must implement the [`Clone`] trait.
//!
//! # Example
//! In general, a Node may produce output or no output:
//! ```rust
//! use dagrs::Output;
//! let out=Output::new(10);
//! let non_out=Output::empty();
//! ```
//! In some special cases, when a predictable error occurs in the execution of a Node's
//! specific behavior, the user can choose to return the error message as the output of
//! the Node. Of course, this will cause subsequent Nodes to abandon execution.
//!
//! ```rust
//! use dagrs::Output;
//! use dagrs::Content;
//! let err_out = Output::Err("some error messages!".to_string());

use crate::connection::information_packet::Content;
use std::any::Any;
use std::sync::Arc;

/// Instruction for loop control.
///
/// This struct defines where the execution flow should jump to when a loop repeats.
#[derive(Debug, Clone)]
pub struct LoopInstruction {
    /// The index of the execution block to jump to.
    ///
    /// The graph is partitioned into blocks (groups of nodes) based on control flow boundaries.
    /// If this is set, the executor will set the program counter (PC) to this index.
    ///
    /// **Priority**: If both `jump_to_block_index` and `jump_to_node` are set, `jump_to_block_index` takes precedence.
    pub jump_to_block_index: Option<usize>,

    /// The ID of the specific node to jump to.
    ///
    /// If set, the executor will look up the block index containing this node and jump there.
    /// This is the preferred way to specify a jump target as it is robust against graph structural changes.
    pub jump_to_node: Option<usize>,

    /// Optional context data to carry over to the next iteration.
    ///
    /// This can be used to pass state or counters between iterations, although using a stateful
    /// `LoopCondition` is often simpler.
    pub context: Option<Arc<dyn Any + Send + Sync>>,
}

/// Control flow instructions for node execution.
///
/// Nodes can return these instructions via `Output::Flow` to influence the graph's execution path.
/// The execution engine interprets these instructions after a node completes successfully.
#[derive(Debug, Clone)]
pub enum FlowControl {
    /// Continue execution normally.
    ///
    /// The graph executor proceeds to the next node(s) in the topological order.
    /// This is the default behavior if no flow control is specified.
    Continue,

    /// Loop: request to jump back to a previous point in the graph.
    ///
    /// This causes the executor to modify its Program Counter (PC) to restart execution
    /// from the specified target. Useful for implementing iterative algorithms.
    Loop(LoopInstruction),

    /// Branch: specify downstream node IDs (as usize) that should be activated.
    ///
    /// Used by `RouterNode`. Only the nodes specified in the list will be scheduled for execution.
    /// All other downstream nodes (and their descendants) connected to this node will be skipped/pruned.
    ///
    /// # Semantics
    /// - If the list is empty, no downstream nodes will run.
    /// - If a downstream node is NOT in this list, it (and its children) are skipped.
    Branch(Vec<usize>),

    /// Abort: stop graph execution immediately.
    ///
    /// This signal halts the entire graph execution. Remaining nodes will not be executed.
    /// The graph returns `Ok(())` as if it finished, but early.
    Abort,
}

impl FlowControl {
    pub fn loop_to_block(index: usize) -> Self {
        Self::Loop(LoopInstruction {
            jump_to_block_index: Some(index),
            jump_to_node: None,
            context: None,
        })
    }

    pub fn loop_to_node(node_id: usize) -> Self {
        Self::Loop(LoopInstruction {
            jump_to_block_index: None,
            jump_to_node: Some(node_id),
            context: None,
        })
    }
}

/// [`Output`] represents the output of a node. Different from information packet (`Content`,
/// used to communicate with other Nodes), `Output` carries the information that `Node`
/// needs to pass to the `Graph`.
#[derive(Clone, Debug)]
pub enum Output {
    Out(Option<Content>),
    Err(String),
    ErrWithExitCode(Option<i32>, Option<Content>),
    /// ...
    ConditionResult(bool),
    /// Control flow signal
    Flow(FlowControl),
}

impl Output {
    /// Construct a new [`Output`].
    ///
    /// Since the return value may be transferred between threads,
    /// [`Send`], [`Sync`] is needed.
    pub fn new<H: Send + Sync + 'static>(val: H) -> Self {
        Self::Out(Some(Content::new(val)))
    }

    /// Construct an empty [`Output`].
    pub fn empty() -> Self {
        Self::Out(None)
    }

    /// Construct an [`Output`]` with an error message.
    pub fn error(msg: String) -> Self {
        Self::Err(msg)
    }

    /// Construct an [`Output`]` with an exit code and an optional error message.
    pub fn error_with_exit_code(code: Option<i32>, msg: Option<Content>) -> Self {
        Self::ErrWithExitCode(code, msg)
    }

    /// Determine whether [`Output`] stores error information.
    pub(crate) fn is_err(&self) -> bool {
        match self {
            Self::Err(_) | Self::ErrWithExitCode(_, _) => true,
            Self::Out(_) | Self::ConditionResult(_) | Self::Flow(_) => false,
        }
    }

    /// Get the contents of [`Output`].
    pub fn get_out(&self) -> Option<Content> {
        match self {
            Self::Out(out) => out.clone(),
            Self::Err(_)
            | Self::ErrWithExitCode(_, _)
            | Self::ConditionResult(_)
            | Self::Flow(_) => None,
        }
    }

    /// Get error information stored in [`Output`].
    pub fn get_err(&self) -> Option<String> {
        match self {
            Self::Out(_) | Self::ConditionResult(_) | Self::Flow(_) => None,
            Self::Err(err) => Some(err.to_string()),
            Self::ErrWithExitCode(code, _) => {
                let error_code = code.map_or("".to_string(), |v| v.to_string());
                Some(format!("code: {error_code}"))
            }
        }
    }

    /// Get the condition result stored in [`Output`].
    ///
    /// Returns `Some(bool)` if this is a `ConditionResult` variant,
    /// otherwise returns `None`.
    pub(crate) fn conditional_result(&self) -> Option<bool> {
        match self {
            Self::ConditionResult(b) => Some(*b),
            _ => None,
        }
    }

    /// Get the flow control instruction stored in [`Output`].
    pub fn get_flow(&self) -> Option<&FlowControl> {
        match self {
            Self::Flow(flow) => Some(flow),
            _ => None,
        }
    }

    /// Check if the output is empty (Out(None)).
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Out(None))
    }

    /// Check if the output has content (not empty and not an error).
    pub fn has_content(&self) -> bool {
        matches!(
            self,
            Self::Out(Some(_)) | Self::ConditionResult(_) | Self::Flow(_)
        )
    }
}
