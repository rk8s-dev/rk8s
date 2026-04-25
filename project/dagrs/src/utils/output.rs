//! Node output
//!
//! [`Output`] represents the result of a [`crate::node::Node`] execution.
//! Besides regular content, a node can emit a structured [`crate::DagrsError`]
//! or a control-flow instruction that affects the graph scheduler.
//!
//! # Example
//! In general, a Node may produce output or no output:
//! ```rust
//! use dagrs::Output;
//! let out = Output::new(10);
//! let non_out = Output::empty();
//! ```
//! When a predictable error occurs, return a structured error:
//! ```rust
//! use dagrs::{DagrsError, ErrorCode, Output};
//! let err_out = Output::error(DagrsError::new(
//!     ErrorCode::DgRun0006NodeExecutionFailed,
//!     "some error messages!",
//! ));
//! ```

use crate::connection::information_packet::Content;
use crate::{DagrsError, ErrorCode};
use std::any::Any;
use std::sync::Arc;

/// Instruction for loop control.
#[derive(Debug, Clone)]
pub struct LoopInstruction {
    pub jump_to_block_index: Option<usize>,
    pub jump_to_node: Option<usize>,
    pub context: Option<Arc<dyn Any + Send + Sync>>,
}

/// Control flow instructions for node execution.
#[derive(Debug, Clone)]
pub enum FlowControl {
    Continue,
    Loop(LoopInstruction),
    Branch(Vec<usize>),
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

/// [`Output`] carries node result data back to the graph executor.
#[derive(Clone, Debug)]
pub enum Output {
    Out(Option<Content>),
    Err(DagrsError),
    ConditionResult(bool),
    Flow(FlowControl),
}

impl Output {
    pub fn new<H: Send + Sync + 'static>(val: H) -> Self {
        Self::Out(Some(Content::new(val)))
    }

    pub fn empty() -> Self {
        Self::Out(None)
    }

    pub fn error(error: DagrsError) -> Self {
        Self::Err(error)
    }

    pub fn execution_failed(message: impl Into<String>) -> Self {
        Self::Err(DagrsError::new(
            ErrorCode::DgRun0006NodeExecutionFailed,
            message.into(),
        ))
    }

    pub(crate) fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }

    pub fn get_out(&self) -> Option<Content> {
        match self {
            Self::Out(out) => out.clone(),
            Self::Err(_) | Self::ConditionResult(_) | Self::Flow(_) => None,
        }
    }

    pub fn get_err(&self) -> Option<&DagrsError> {
        match self {
            Self::Err(err) => Some(err),
            Self::Out(_) | Self::ConditionResult(_) | Self::Flow(_) => None,
        }
    }

    pub(crate) fn conditional_result(&self) -> Option<bool> {
        match self {
            Self::ConditionResult(b) => Some(*b),
            _ => None,
        }
    }

    pub fn get_flow(&self) -> Option<&FlowControl> {
        match self {
            Self::Flow(flow) => Some(flow),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Out(None))
    }

    pub fn has_content(&self) -> bool {
        matches!(
            self,
            Self::Out(Some(_)) | Self::ConditionResult(_) | Self::Flow(_)
        )
    }
}
