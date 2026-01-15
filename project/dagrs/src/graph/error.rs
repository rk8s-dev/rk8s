#[derive(Clone, Debug)]
pub enum GraphError {
    GraphLoopDetected,
    GraphNotActive,
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
    /// Contains the original error message when runtime creation failed
    RuntimeCreationFailed(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}
