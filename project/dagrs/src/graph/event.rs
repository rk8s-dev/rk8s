use crate::node::NodeId;

/// Events emitted during graph execution.
///
/// These events are broadcast via the `event_sender` channel in `Graph`.
/// Consumers can subscribe to these events to monitor the execution progress,
/// collect metrics, or trigger side effects (e.g., logging, UI updates).
///
/// # Handling Events
/// Subscribers receive a `broadcast::Receiver`. Note that if the subscriber
/// cannot keep up with the event rate, they may receive `Lagged` errors.
#[derive(Clone, Debug)]
pub enum GraphEvent {
    /// Emitted when a node starts execution.
    ///
    /// * `id`: The ID of the node that started.
    /// * `timestamp`: The Unix timestamp (in seconds) when execution began.
    NodeStart { id: NodeId, timestamp: u64 },

    /// Emitted when a node completes execution successfully.
    ///
    /// * `id`: The ID of the node that succeeded.
    NodeSuccess { id: NodeId },

    /// Emitted when a node execution fails (returns an error or panics).
    ///
    /// * `id`: The ID of the node that failed.
    /// * `error`: A string description of the error.
    NodeFailed { id: NodeId, error: String },

    /// Emitted when a node is skipped due to conditional logic (e.g., a branch not taken).
    ///
    /// * `id`: The ID of the skipped node.
    NodeSkipped { id: NodeId },

    /// Emitted when a node is about to be retried after a failure.
    ///
    /// * `id`: The ID of the node being retried.
    /// * `attempt`: The current retry attempt number (1-indexed).
    /// * `max_retries`: The maximum number of retries configured.
    /// * `error`: The error message from the previous attempt.
    NodeRetry {
        id: NodeId,
        attempt: u32,
        max_retries: u32,
        error: String,
    },

    /// Emitted when a loop iteration begins.
    ///
    /// * `iteration`: The current iteration number (0-indexed).
    /// * `block_index`: The block index that the loop is jumping back to.
    LoopIteration {
        iteration: usize,
        block_index: usize,
    },

    /// Emitted when a branch decision is made by a router/conditional node.
    ///
    /// * `node_id`: The ID of the decision node.
    /// * `selected_branches`: The IDs (as usize) of the branches that were selected.
    BranchSelected {
        node_id: NodeId,
        selected_branches: Vec<usize>,
    },

    /// Emitted periodically to report execution progress.
    ///
    /// * `completed`: Number of nodes that have completed execution.
    /// * `total`: Total number of nodes in the graph.
    Progress { completed: usize, total: usize },

    /// Emitted when a checkpoint is saved.
    ///
    /// * `checkpoint_id`: The unique identifier of the saved checkpoint.
    /// * `pc`: The program counter (block index) at checkpoint time.
    /// * `completed_nodes`: Number of nodes that had completed at checkpoint time.
    CheckpointSaved {
        checkpoint_id: String,
        pc: usize,
        completed_nodes: usize,
    },

    /// Emitted when execution resumes from a checkpoint.
    ///
    /// * `checkpoint_id`: The identifier of the checkpoint being resumed from.
    /// * `pc`: The program counter (block index) to resume from.
    CheckpointRestored { checkpoint_id: String, pc: usize },

    /// Emitted when the entire graph execution finishes (success or failure).
    ///
    /// This is the final event in the stream.
    /// Consumers can use this to signal the end of monitoring.
    GraphFinished,
}
