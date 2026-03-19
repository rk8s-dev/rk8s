#[derive(Clone, Debug)]
pub enum GraphError {
    GraphLoopDetected,
    GraphNotActive,
    NodeIdError(usize),
    ExecutionFailed {
        node_name: String,
        node_id: usize,
        error: String,
    },
    PanicOccurred {
        node_name: String,
        node_id: usize,
    },
    MultipleErrors(Vec<GraphError>),
    /// A blocking `start` API was invoked from within an async runtime context.
    BlockingCallInAsyncContext(String),
    /// Max loop limit exceeded
    LoopLimitExceeded(usize),
    /// Checkpoint operation failed
    CheckpointError(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for GraphError {}
