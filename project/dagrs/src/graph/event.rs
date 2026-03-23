use crate::{DagrsError, node::NodeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipReason {
    PrunedByControlFlow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminationStatus {
    Succeeded,
    Failed,
    Aborted,
}

/// Events emitted during graph execution.
#[derive(Clone, Debug)]
pub enum GraphEvent {
    NodeStart {
        id: NodeId,
        timestamp: u64,
    },
    NodeSuccess {
        id: NodeId,
        duration_ms: u64,
    },
    NodeFailed {
        id: NodeId,
        error: DagrsError,
    },
    NodeSkipped {
        id: NodeId,
        reason: SkipReason,
    },
    NodeRetry {
        id: NodeId,
        attempt: u32,
        max_retries: u32,
        error: DagrsError,
    },
    LoopIteration {
        iteration: usize,
        block_index: usize,
    },
    BranchSelected {
        node_id: NodeId,
        selected_branches: Vec<usize>,
    },
    /// Progress is emitted when the executor finishes a whole block, not after every node.
    ///
    /// This keeps progress monotonic across branch pruning and checkpoint resume, but means
    /// consumers that want per-node UI updates should combine it with node-level events.
    Progress {
        completed: usize,
        total: usize,
    },
    CheckpointSaved {
        checkpoint_id: String,
        pc: usize,
        completed_nodes: usize,
    },
    CheckpointRestored {
        checkpoint_id: String,
        pc: usize,
    },
    ExecutionTerminated {
        status: TerminationStatus,
        error: Option<DagrsError>,
    },
}
