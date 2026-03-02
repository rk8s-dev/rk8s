use crate::node::Node;
use crate::utils::env::EnvVar;
use crate::utils::output::Output;
use async_trait::async_trait;
use std::sync::Arc;

/// Retry decision returned by the `on_retry` hook.
///
/// This enum allows hooks to control whether a failed node should be retried.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDecision {
    /// Retry the node execution.
    Retry,
    /// Do not retry, fail immediately.
    Fail,
}

/// Execution hook trait for monitoring node execution
///
/// Hooks allow users to inject custom logic at specific points in a node's lifecycle.
/// They are useful for logging, monitoring, error reporting, or modifying execution context.
///
/// # Thread Safety
/// Hooks are executed in the same async task as the node.
/// The `ExecutionHook` trait requires `Send + Sync`, allowing hooks to be shared across threads.
/// Implementations should ensure thread safety if they access shared state.
///
/// # Performance
/// Hooks are awaited, meaning they will block the node execution until they complete.
/// Implementations should be efficient and avoid long-running blocking operations.
///
/// # Exception Handling
/// If a hook panics, it may crash the node execution task.
/// Implementations should handle their own errors internally.
#[async_trait]
pub trait ExecutionHook: Send + Sync {
    /// Called before a node starts execution.
    ///
    /// # Arguments
    /// * `node` - The node about to execute.
    /// * `env` - The global environment variables.
    async fn before_node_run(&self, node: &dyn Node, env: &Arc<EnvVar>);

    /// Called after a node completes execution (successfully).
    ///
    /// # Arguments
    /// * `node` - The node that executed.
    /// * `output` - The output produced by the node.
    /// * `env` - The global environment variables.
    async fn after_node_run(&self, node: &dyn Node, output: &Output, env: &Arc<EnvVar>);

    /// Called when a node execution fails (returns an error or panics).
    ///
    /// # Arguments
    /// * `error` - The error that occurred (as a Send + Sync trait object).
    /// * `env` - The global environment variables.
    async fn on_error(&self, error: &(dyn std::error::Error + Send + Sync), env: &Arc<EnvVar>);

    /// Called when a node is about to be retried after a failure.
    ///
    /// This hook is called before each retry attempt.
    /// The implementation can decide whether to allow the retry or fail immediately.
    ///
    /// # Arguments
    /// * `node` - The node that will be retried.
    /// * `error` - The error from the previous attempt.
    /// * `attempt` - The current retry attempt number (1-indexed, so first retry is attempt 1).
    /// * `max_retries` - The maximum number of retries configured for the node.
    /// * `env` - The global environment variables.
    ///
    /// # Returns
    /// * `RetryDecision::Retry` - Proceed with the retry.
    /// * `RetryDecision::Fail` - Abort retries and fail the node.
    ///
    /// # Default Implementation
    /// The default implementation always returns `RetryDecision::Retry`.
    async fn on_retry(
        &self,
        _node: &dyn Node,
        _error: &(dyn std::error::Error + Send + Sync),
        _attempt: u32,
        _max_retries: u32,
        _env: &Arc<EnvVar>,
    ) -> RetryDecision {
        RetryDecision::Retry
    }

    /// Called when a node is skipped due to conditional logic or branch pruning.
    ///
    /// # Arguments
    /// * `node` - The node that was skipped.
    /// * `env` - The global environment variables.
    ///
    /// # Default Implementation
    /// The default implementation does nothing.
    async fn on_skip(&self, _node: &dyn Node, _env: &Arc<EnvVar>) {}
}
