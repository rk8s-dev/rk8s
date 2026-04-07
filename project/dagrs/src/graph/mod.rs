mod abstract_graph;
pub mod error;
pub mod event;
pub mod loop_subgraph;

use std::hash::Hash;
use std::sync::atomic::Ordering;
use std::{
    collections::{HashMap, HashSet},
    panic::AssertUnwindSafe,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crate::{
    DagrsError, DagrsResult, ErrorCode, Output,
    connection::{in_channel::InChannel, information_packet::Content, out_channel::OutChannel},
    graph::event::{GraphEvent, SkipReason, TerminationStatus},
    node::{Node, NodeId, NodeTable},
    utils::checkpoint::{
        Checkpoint, CheckpointConfig, CheckpointStore, NodeExecStatus, NodeState, StoredOutputKind,
        checkpoint_cmp,
    },
    utils::hook::{ExecutionHook, RetryDecision},
    utils::output::FlowControl,
    utils::{env::EnvVar, execstate::ExecState},
};

use log::{debug, error, info, warn};
use tokio::sync::Mutex;
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio::task;

use abstract_graph::AbstractGraph;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionStatus {
    Succeeded,
    Aborted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetPolicy {
    KeepEnv,
    ResetEnv,
}

#[derive(Clone, Debug, Default)]
pub struct RunOptions {
    pub run_id: Option<String>,
}

#[derive(Clone, Debug)]
struct RunContext {
    run_id: String,
    started_at_unix_secs: u64,
    start_pc: usize,
    start_loop_count: usize,
    initial_completed_total: usize,
    initial_skipped_total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionReport {
    pub run_id: String,
    pub status: CompletionStatus,
    pub started_at_unix_secs: u64,
    pub ended_at_unix_secs: u64,
    pub node_total: usize,
    pub node_succeeded: usize,
    pub node_failed: usize,
    pub node_skipped: usize,
}

/// [`Graph`] is dagrs's main body.
///
/// ['Graph'] is a network that satisfies FBP logic, provides node dependencies, and runs all of its nodes completely asynchronously
/// A `Graph` contains multiple nodes, which can be added as long as they implement the [`Node`] trait.
/// Each node defines specific execution logic by implementing the `Action` trait and overriding the `run` method.
///
/// The execution process of a `Graph` proceeds as follows:
/// - The user creates a set of nodes, each implementing the [`Node`] trait. These nodes can be created programmatically
///   or Generate auto_node using parse.
/// - Dependencies between nodes are defined, creating a directed acyclic graph (DAG) structure.
/// - During execution, nodes communicate via input/output channels (`InChannel` and `OutChannel`).
///   These channels support both point-to-point communication (using `MPSC`) and broadcasting (using `Broadcast`).
/// - After all nodes complete their execution, marking the graph as inactive.
///   This ensures that the `Graph` cannot be executed again without resetting its state.
///
/// The [`Graph`] is designed to efficiently manage task execution with built-in fault tolerance and flexible scheduling.
///
/// # Checkpoint Support
///
/// The graph supports checkpoint-based state persistence for fault tolerance:
/// - Configure a checkpoint store with `set_checkpoint_store()`
/// - Enable automatic checkpointing with `set_checkpoint_config()`
/// - Manually save checkpoints with `save_checkpoint()`
/// - Resume from checkpoints with `resume_from_checkpoint()`
pub struct Graph {
    /// Define the Net struct that holds all nodes
    pub(crate) nodes: HashMap<NodeId, Arc<Mutex<dyn Node>>>,
    /// Store a task's running result.Execution results will be read
    /// and written asynchronously by several threads.
    pub(crate) execute_states: HashMap<NodeId, Arc<ExecState>>,
    /// Count all the nodes
    pub(crate) node_count: usize,
    /// Global environment variables for this Net job.
    /// It should be set before the Net job runs.
    pub(crate) env: Arc<EnvVar>,
    /// Mark whether the net task can continue to execute.
    /// When an error occurs during the execution of any task, This flag will still be set to true
    pub(crate) is_active: Arc<AtomicBool>,
    /// Node's in_degree, used for check loop
    pub(crate) in_degree: HashMap<NodeId, usize>,
    /// Stores the blocks of nodes divided by conditional nodes.
    /// Each block is a HashSet of NodeIds that represents a group of nodes that will be executed together.
    pub(crate) blocks: Vec<HashSet<NodeId>>,
    /// Maps NodeId to the index of the block it belongs to.
    /// This is built during `check_loop_and_partition` and used during execution.
    pub(crate) node_block_map: HashMap<NodeId, usize>,
    /// Abstract representation of the graph structure, used for cycle detection
    pub(crate) abstract_graph: AbstractGraph,
    /// Registered execution hooks
    pub(crate) hooks: Arc<RwLock<Vec<Box<dyn ExecutionHook>>>>,
    /// Event broadcaster
    pub(crate) event_sender: broadcast::Sender<GraphEvent>,
    /// Maximum number of loop iterations allowed to prevent infinite loops.
    pub(crate) max_loop_count: usize,
    /// Checkpoint store for state persistence
    pub(crate) checkpoint_store: Option<Arc<dyn CheckpointStore>>,
    /// Checkpoint configuration
    pub(crate) checkpoint_config: CheckpointConfig,
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Graph {
    const LOOP_NODE_ITERATIONS_METADATA_KEY: &'static str = "loop_node_iterations";

    /// Constructs a new `Graph`
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Graph {
            nodes: HashMap::new(),
            node_count: 0,
            execute_states: HashMap::new(),
            env: Arc::new(EnvVar::new(NodeTable::default())),
            is_active: Arc::new(AtomicBool::new(true)),
            in_degree: HashMap::new(),
            blocks: vec![],
            node_block_map: HashMap::new(),
            abstract_graph: AbstractGraph::new(),
            hooks: Arc::new(RwLock::new(Vec::new())),
            event_sender: tx,
            max_loop_count: 1000,
            checkpoint_store: None,
            checkpoint_config: CheckpointConfig::default(),
        }
    }

    /// Set the maximum number of loop iterations.
    pub fn set_max_loop_count(&mut self, count: usize) {
        self.max_loop_count = count;
    }

    /// Set the checkpoint store for state persistence.
    ///
    /// # Example
    /// ```ignore
    /// use dagrs::utils::checkpoint::MemoryCheckpointStore;
    ///
    /// let store = MemoryCheckpointStore::new();
    /// graph.set_checkpoint_store(Box::new(store));
    /// ```
    pub fn set_checkpoint_store(&mut self, store: Box<dyn CheckpointStore>) {
        self.checkpoint_store = Some(Arc::from(store));
    }

    /// Set the checkpoint configuration.
    ///
    /// # Example
    /// ```ignore
    /// use dagrs::utils::checkpoint::CheckpointConfig;
    ///
    /// let config = CheckpointConfig::enabled()
    ///     .with_node_interval(5)
    ///     .with_max_checkpoints(10);
    /// graph.set_checkpoint_config(config);
    /// ```
    pub fn set_checkpoint_config(&mut self, config: CheckpointConfig) {
        self.checkpoint_config = config;
    }

    /// Save the current execution state as a checkpoint.
    ///
    /// This method captures:
    /// - Current program counter (block index)
    /// - Loop iteration count
    /// - Active nodes set
    /// - Execution state of each node
    ///
    /// # Arguments
    /// * `pc` - Current program counter (block index)
    /// * `loop_count` - Current loop iteration count
    /// * `active_nodes` - Set of currently active node IDs
    ///
    /// # Returns
    /// * `Ok(CheckpointId)` - The ID of the saved checkpoint
    /// * `Err(DagrsError)` - If saving fails
    pub async fn save_checkpoint(
        &self,
        pc: usize,
        loop_count: usize,
        active_nodes: &HashSet<NodeId>,
    ) -> DagrsResult<String> {
        self.save_checkpoint_with_loop_node_iterations(pc, loop_count, active_nodes, None)
            .await
    }

    async fn save_checkpoint_with_loop_node_iterations(
        &self,
        pc: usize,
        loop_count: usize,
        active_nodes: &HashSet<NodeId>,
        loop_node_iterations: Option<&HashMap<NodeId, usize>>,
    ) -> DagrsResult<String> {
        let store = self.checkpoint_store.as_ref().ok_or_else(|| {
            DagrsError::new(
                ErrorCode::DgChk0001StoreNotConfigured,
                "checkpoint store not configured",
            )
        })?;

        let mut checkpoint = Checkpoint::new(pc, loop_count);
        checkpoint.set_active_nodes(active_nodes);

        // Capture node execution states with output data
        for (node_id, exec_state) in &self.execute_states {
            let output = exec_state.get_full_output();
            let output_content = output.get_out();
            let serialized_output = output_content.as_ref().and_then(Self::try_serialize_output);
            let mut node_state = if output.is_err() {
                NodeState::failed(node_id.0)
            } else if exec_state.is_success() {
                NodeState::succeeded(node_id.0)
            } else if !active_nodes.contains(node_id) {
                NodeState::skipped(node_id.0)
            } else {
                NodeState::pending(node_id.0)
            };

            // Try to capture output summary for debugging
            if let Some(content) = output_content {
                // Try to get a debug representation of common types
                let summary = Self::get_output_summary(&content);
                if let Some(s) = summary {
                    node_state = node_state.with_summary(s);
                }

                // Try to serialize if the content is a serializable primitive
                if let Some((kind, data)) = serialized_output {
                    node_state = node_state.with_output_kind(kind).with_output_data(data);
                } else if exec_state.is_success() {
                    let summary = node_state.output_summary.clone().unwrap_or_else(|| {
                        "output is not checkpoint-serializable; node will rerun on resume"
                            .to_string()
                    });
                    node_state = node_state.with_summary(summary);
                }
            } else if let Some(err) = output.get_err() {
                node_state = node_state.with_summary(format!("Error: {err}"));
            }

            checkpoint.add_node_state(node_state);
        }

        // Add metadata
        checkpoint.add_metadata("node_count", self.node_count.to_string());
        checkpoint.add_metadata("blocks_count", self.blocks.len().to_string());
        if let Some(loop_node_iterations) = loop_node_iterations
            && let Some(serialized) = Self::serialize_loop_node_iterations(loop_node_iterations)
        {
            checkpoint.add_metadata(Self::LOOP_NODE_ITERATIONS_METADATA_KEY, serialized);
        }

        store.save(&checkpoint).await?;

        // Enforce max checkpoints limit
        if self.checkpoint_config.max_checkpoints > 0 {
            self.cleanup_old_checkpoints(store.as_ref()).await?;
        }

        // Emit event
        let completed_nodes = self
            .execute_states
            .values()
            .filter(|s| !s.get_full_output().is_empty())
            .count();
        let _ = self.event_sender.send(GraphEvent::CheckpointSaved {
            checkpoint_id: checkpoint.id.clone(),
            pc,
            completed_nodes,
        });

        info!(
            "Checkpoint saved: {} (pc={}, loop_count={})",
            checkpoint.id, pc, loop_count
        );
        Ok(checkpoint.id)
    }

    fn serialize_loop_node_iterations(
        loop_node_iterations: &HashMap<NodeId, usize>,
    ) -> Option<String> {
        if loop_node_iterations.is_empty() {
            return None;
        }
        let payload: HashMap<usize, usize> = loop_node_iterations
            .iter()
            .map(|(node_id, count)| (node_id.as_usize(), *count))
            .collect();
        serde_json::to_string(&payload).ok()
    }

    fn deserialize_loop_node_iterations(checkpoint: &Checkpoint) -> Option<HashMap<NodeId, usize>> {
        let payload = checkpoint
            .metadata
            .get(Self::LOOP_NODE_ITERATIONS_METADATA_KEY)?;
        let parsed: HashMap<usize, usize> = serde_json::from_str(payload).ok()?;
        Some(
            parsed
                .into_iter()
                .map(|(node_id, count)| (NodeId(node_id), count))
                .collect(),
        )
    }

    /// Try to get a human-readable summary of the output content
    fn get_output_summary(content: &Content) -> Option<String> {
        // Try common types
        if let Some(v) = content.get::<String>() {
            return Some(if v.len() > 100 {
                format!("String({}...)", &v[..100])
            } else {
                format!("String({})", v)
            });
        }
        if let Some(v) = content.get::<i32>() {
            return Some(format!("i32({})", v));
        }
        if let Some(v) = content.get::<i64>() {
            return Some(format!("i64({})", v));
        }
        if let Some(v) = content.get::<u32>() {
            return Some(format!("u32({})", v));
        }
        if let Some(v) = content.get::<u64>() {
            return Some(format!("u64({})", v));
        }
        if let Some(v) = content.get::<f64>() {
            return Some(format!("f64({})", v));
        }
        if let Some(v) = content.get::<bool>() {
            return Some(format!("bool({})", v));
        }
        if let Some(v) = content.get::<Vec<u8>>() {
            return Some(format!("Vec<u8>(len={})", v.len()));
        }
        None
    }

    /// Try to serialize output content to JSON bytes
    fn try_serialize_output(content: &Content) -> Option<(StoredOutputKind, Vec<u8>)> {
        // Try common serializable types
        if let Some(v) = content.get::<String>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::String, data));
        }
        if let Some(v) = content.get::<i32>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::I32, data));
        }
        if let Some(v) = content.get::<i64>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::I64, data));
        }
        if let Some(v) = content.get::<u32>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::U32, data));
        }
        if let Some(v) = content.get::<u64>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::U64, data));
        }
        if let Some(v) = content.get::<f64>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::F64, data));
        }
        if let Some(v) = content.get::<bool>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::Bool, data));
        }
        if let Some(v) = content.get::<Vec<String>>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::VecString, data));
        }
        if let Some(v) = content.get::<Vec<i32>>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::VecI32, data));
        }
        if let Some(v) = content.get::<Vec<i64>>() {
            return serde_json::to_vec(v)
                .ok()
                .map(|data| (StoredOutputKind::VecI64, data));
        }
        None
    }

    fn restore_output_content(kind: StoredOutputKind, data: &[u8]) -> DagrsResult<Content> {
        let invalid_checkpoint =
            |message: String| DagrsError::new(ErrorCode::DgChk0003InvalidCheckpoint, message);

        match kind {
            StoredOutputKind::String => serde_json::from_slice::<String>(data)
                .map(Content::new)
                .map_err(|err| {
                    invalid_checkpoint(format!("failed to restore String output: {err}"))
                }),
            StoredOutputKind::I32 => serde_json::from_slice::<i32>(data)
                .map(Content::new)
                .map_err(|err| invalid_checkpoint(format!("failed to restore i32 output: {err}"))),
            StoredOutputKind::I64 => serde_json::from_slice::<i64>(data)
                .map(Content::new)
                .map_err(|err| invalid_checkpoint(format!("failed to restore i64 output: {err}"))),
            StoredOutputKind::U32 => serde_json::from_slice::<u32>(data)
                .map(Content::new)
                .map_err(|err| invalid_checkpoint(format!("failed to restore u32 output: {err}"))),
            StoredOutputKind::U64 => serde_json::from_slice::<u64>(data)
                .map(Content::new)
                .map_err(|err| invalid_checkpoint(format!("failed to restore u64 output: {err}"))),
            StoredOutputKind::F64 => serde_json::from_slice::<f64>(data)
                .map(Content::new)
                .map_err(|err| invalid_checkpoint(format!("failed to restore f64 output: {err}"))),
            StoredOutputKind::Bool => serde_json::from_slice::<bool>(data)
                .map(Content::new)
                .map_err(|err| invalid_checkpoint(format!("failed to restore bool output: {err}"))),
            StoredOutputKind::VecString => serde_json::from_slice::<Vec<String>>(data)
                .map(Content::new)
                .map_err(|err| {
                    invalid_checkpoint(format!("failed to restore Vec<String> output: {err}"))
                }),
            StoredOutputKind::VecI32 => serde_json::from_slice::<Vec<i32>>(data)
                .map(Content::new)
                .map_err(|err| {
                    invalid_checkpoint(format!("failed to restore Vec<i32> output: {err}"))
                }),
            StoredOutputKind::VecI64 => serde_json::from_slice::<Vec<i64>>(data)
                .map(Content::new)
                .map_err(|err| {
                    invalid_checkpoint(format!("failed to restore Vec<i64> output: {err}"))
                }),
        }
    }

    /// Load a checkpoint by ID.
    ///
    /// # Arguments
    /// * `checkpoint_id` - The ID of the checkpoint to load
    ///
    /// # Returns
    /// * `Ok(Checkpoint)` - The loaded checkpoint
    /// * `Err(DagrsError)` - If loading fails
    pub async fn load_checkpoint(&self, checkpoint_id: &str) -> DagrsResult<Checkpoint> {
        let store = self.checkpoint_store.as_ref().ok_or_else(|| {
            DagrsError::new(
                ErrorCode::DgChk0001StoreNotConfigured,
                "checkpoint store not configured",
            )
        })?;
        store.load(&checkpoint_id.to_string()).await
    }

    /// Get the latest checkpoint.
    ///
    /// # Returns
    /// * `Ok(Some(Checkpoint))` - The latest checkpoint if one exists
    /// * `Ok(None)` - If no checkpoints exist
    /// * `Err(DagrsError)` - If loading fails
    pub async fn get_latest_checkpoint(&self) -> DagrsResult<Option<Checkpoint>> {
        let store = self.checkpoint_store.as_ref().ok_or_else(|| {
            DagrsError::new(
                ErrorCode::DgChk0001StoreNotConfigured,
                "checkpoint store not configured",
            )
        })?;
        store.latest().await
    }

    /// List all available checkpoint IDs.
    pub async fn list_checkpoints(&self) -> DagrsResult<Vec<String>> {
        let store = self.checkpoint_store.as_ref().ok_or_else(|| {
            DagrsError::new(
                ErrorCode::DgChk0001StoreNotConfigured,
                "checkpoint store not configured",
            )
        })?;
        store.list().await
    }

    /// Delete a checkpoint by ID.
    pub async fn delete_checkpoint(&self, checkpoint_id: &str) -> DagrsResult<()> {
        let store = self.checkpoint_store.as_ref().ok_or_else(|| {
            DagrsError::new(
                ErrorCode::DgChk0001StoreNotConfigured,
                "checkpoint store not configured",
            )
        })?;
        store.delete(&checkpoint_id.to_string()).await
    }

    /// Clean up old checkpoints to maintain the max_checkpoints limit.
    async fn cleanup_old_checkpoints(&self, store: &dyn CheckpointStore) -> DagrsResult<()> {
        let ids = store.list().await?;
        if ids.len() <= self.checkpoint_config.max_checkpoints {
            return Ok(());
        }

        // Load all checkpoints and sort by timestamp
        let mut checkpoints = Vec::new();
        for id in ids {
            if let Ok(cp) = store.load(&id).await {
                checkpoints.push(cp);
            }
        }
        checkpoints.sort_by(checkpoint_cmp);

        // Delete oldest checkpoints
        let to_delete = checkpoints
            .len()
            .saturating_sub(self.checkpoint_config.max_checkpoints);
        for checkpoint in checkpoints.into_iter().take(to_delete) {
            store.delete(&checkpoint.id).await?;
            debug!("Deleted old checkpoint: {}", checkpoint.id);
        }

        Ok(())
    }

    fn rebuild_active_nodes_from_checkpoint(checkpoint: &Checkpoint) -> HashSet<NodeId> {
        let mut active_nodes: HashSet<_> = checkpoint
            .get_active_nodes()
            .into_iter()
            .filter(|node_id| {
                !matches!(
                    checkpoint
                        .node_states
                        .get(&node_id.0)
                        .map(|state| state.status),
                    Some(NodeExecStatus::Skipped)
                )
            })
            .collect();
        for (node_id_val, node_state) in &checkpoint.node_states {
            let node_id = NodeId(*node_id_val);
            match node_state.status {
                NodeExecStatus::Pending | NodeExecStatus::Running | NodeExecStatus::Failed => {
                    active_nodes.insert(node_id);
                }
                NodeExecStatus::Succeeded => {
                    active_nodes.insert(node_id);
                }
                NodeExecStatus::Skipped => {
                    active_nodes.remove(&node_id);
                }
            }
        }
        active_nodes
    }

    fn checkpoint_receiver_still_needs_input(
        &self,
        checkpoint: &Checkpoint,
        receiver_id: NodeId,
    ) -> bool {
        if let Some(concrete_receivers) = self.abstract_graph.unfold_node(receiver_id) {
            concrete_receivers.iter().any(|receiver_id| {
                !matches!(
                    checkpoint
                        .node_states
                        .get(&receiver_id.0)
                        .map(|state| state.status),
                    Some(NodeExecStatus::Succeeded | NodeExecStatus::Skipped)
                )
            })
        } else {
            !matches!(
                checkpoint
                    .node_states
                    .get(&receiver_id.0)
                    .map(|state| state.status),
                Some(NodeExecStatus::Succeeded | NodeExecStatus::Skipped)
            )
        }
    }

    fn succeeded_node_requires_rerun(
        &self,
        checkpoint: &Checkpoint,
        node_id: NodeId,
        node_state: &NodeState,
    ) -> bool {
        if !matches!(node_state.status, NodeExecStatus::Succeeded)
            || node_state.output_data.is_some()
            || node_state.output_summary.is_none()
        {
            return false;
        }

        let edge_source = self
            .abstract_graph
            .get_abstract_node_id(&node_id)
            .copied()
            .unwrap_or(node_id);
        self.abstract_graph
            .edges
            .get(&edge_source)
            .is_some_and(|receiver_ids| {
                receiver_ids.iter().copied().any(|receiver_id| {
                    self.checkpoint_receiver_still_needs_input(checkpoint, receiver_id)
                })
            })
    }

    fn resume_block_from_checkpoint(&self, checkpoint: &Checkpoint) -> DagrsResult<usize> {
        let mut start_pc = checkpoint.pc;

        for node_id in checkpoint
            .node_states
            .iter()
            .filter_map(|(node_id, state)| {
                (matches!(
                    state.status,
                    NodeExecStatus::Pending | NodeExecStatus::Running | NodeExecStatus::Failed
                ) || self.succeeded_node_requires_rerun(checkpoint, NodeId(*node_id), state))
                .then_some(NodeId(*node_id))
            })
        {
            let block_index = self.node_block_map.get(&node_id).ok_or_else(|| {
                DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "checkpoint node is missing from the current block map",
                )
                .with_checkpoint(checkpoint.id.clone())
                .with_node_id(node_id.as_usize())
            })?;
            start_pc = start_pc.min(*block_index);
        }

        Ok(start_pc)
    }

    fn prune_succeeded_nodes_from_resume_span(
        &self,
        checkpoint: &Checkpoint,
        active_nodes: &mut HashSet<NodeId>,
        start_pc: usize,
    ) -> DagrsResult<()> {
        let start_block_has_rerun_nodes =
            checkpoint.node_states.iter().any(|(node_id_val, state)| {
                if !matches!(
                    state.status,
                    NodeExecStatus::Pending | NodeExecStatus::Running | NodeExecStatus::Failed
                ) {
                    return false;
                }

                self.node_block_map
                    .get(&NodeId(*node_id_val))
                    .is_some_and(|block_index| *block_index == start_pc)
            });

        for (node_id_val, node_state) in &checkpoint.node_states {
            let node_id = NodeId(*node_id_val);
            if !matches!(node_state.status, NodeExecStatus::Succeeded)
                || self.succeeded_node_requires_rerun(checkpoint, node_id, node_state)
            {
                continue;
            }

            let block_index = self.node_block_map.get(&node_id).ok_or_else(|| {
                DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "checkpoint node is missing from the current block map",
                )
                .with_checkpoint(checkpoint.id.clone())
                .with_node_id(node_id.as_usize())
            })?;

            if *block_index > start_pc || (*block_index == start_pc && start_block_has_rerun_nodes)
            {
                active_nodes.remove(&node_id);
            }
        }

        Ok(())
    }

    fn node_should_receive_replayed_input(
        checkpoint: &Checkpoint,
        active_nodes: &HashSet<NodeId>,
        node_id: NodeId,
    ) -> bool {
        if !active_nodes.contains(&node_id) {
            return false;
        }

        !matches!(
            checkpoint
                .node_states
                .get(&node_id.0)
                .map(|state| state.status),
            Some(NodeExecStatus::Succeeded | NodeExecStatus::Skipped)
        )
    }

    fn checkpoint_progress_totals(&self, checkpoint: &Checkpoint) -> (usize, usize) {
        let skipped_total = checkpoint
            .node_states
            .values()
            .filter(|node_state| matches!(node_state.status, NodeExecStatus::Skipped))
            .count();
        let completed_total = checkpoint
            .node_states
            .iter()
            .filter(|(node_id, node_state)| {
                matches!(node_state.status, NodeExecStatus::Skipped)
                    || (matches!(node_state.status, NodeExecStatus::Succeeded)
                        && !self.succeeded_node_requires_rerun(
                            checkpoint,
                            NodeId(**node_id),
                            node_state,
                        ))
            })
            .count();
        (completed_total, skipped_total)
    }

    async fn set_sender_enabled_for_receivers(
        &self,
        sender: NodeId,
        receiver_ids: &[NodeId],
        enabled: bool,
        checkpoint_id: Option<&str>,
    ) -> DagrsResult<()> {
        for receiver_id in receiver_ids {
            let receiver = self.nodes.get(receiver_id).ok_or_else(|| {
                let mut err = DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "receiver referenced during checkpoint restore does not exist",
                )
                .with_node_id(receiver_id.as_usize());
                if let Some(checkpoint_id) = checkpoint_id {
                    err = err.with_checkpoint(checkpoint_id.to_string());
                }
                err
            })?;
            let mut receiver_guard = receiver.lock().await;
            if enabled {
                receiver_guard.input_channels().enable_sender(sender);
            } else {
                receiver_guard.input_channels().disable_sender(sender);
            }
        }
        Ok(())
    }

    fn validate_checkpoint(&self, checkpoint: &Checkpoint) -> DagrsResult<()> {
        if checkpoint.pc >= self.blocks.len() {
            return Err(DagrsError::new(
                ErrorCode::DgChk0003InvalidCheckpoint,
                format!(
                    "checkpoint program counter {} is out of bounds for graph with {} blocks",
                    checkpoint.pc,
                    self.blocks.len()
                ),
            )
            .with_checkpoint(checkpoint.id.clone())
            .with_detail("pc", checkpoint.pc.to_string())
            .with_detail("blocks_len", self.blocks.len().to_string()));
        }

        for node_id in &checkpoint.active_nodes {
            let node_id = NodeId(*node_id);
            if !self.nodes.contains_key(&node_id) {
                return Err(DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "checkpoint active node does not exist in the graph",
                )
                .with_checkpoint(checkpoint.id.clone())
                .with_node_id(node_id.as_usize()));
            }
        }

        for node_id in checkpoint.node_states.keys() {
            let node_id = NodeId(*node_id);
            if !self.nodes.contains_key(&node_id) {
                return Err(DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "checkpoint node state does not exist in the graph",
                )
                .with_checkpoint(checkpoint.id.clone())
                .with_node_id(node_id.as_usize()));
            }
        }

        Ok(())
    }

    async fn restore_checkpoint_state(
        &mut self,
        checkpoint: &Checkpoint,
        active_nodes: &HashSet<NodeId>,
    ) -> DagrsResult<()> {
        let mut restored_outputs = Vec::new();
        let mut skipped_senders = Vec::new();
        let loop_node_iterations = Self::deserialize_loop_node_iterations(checkpoint);

        for (node_id, node) in &self.nodes {
            let mut node_guard = node.lock().await;
            let restore_count = if let Some(loop_node_iterations) = &loop_node_iterations {
                loop_node_iterations
                    .get(node_id)
                    .copied()
                    .unwrap_or_default()
            } else {
                checkpoint.loop_count
            };
            node_guard
                .restore_from_checkpoint(restore_count)
                .map_err(|err| err.with_checkpoint(checkpoint.id.clone()))?;
        }

        for (node_id_val, node_state) in &checkpoint.node_states {
            let node_id = NodeId(*node_id_val);
            let exec_state = self.execute_states.get(&node_id).ok_or_else(|| {
                DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "checkpoint references a node that is not initialised",
                )
                .with_node_id(node_id.as_usize())
                .with_checkpoint(checkpoint.id.clone())
            })?;

            match node_state.status {
                NodeExecStatus::Succeeded => {
                    if self.succeeded_node_requires_rerun(checkpoint, node_id, node_state) {
                        continue;
                    }
                    if let Some(data) = &node_state.output_data {
                        let kind = node_state.output_kind.ok_or_else(|| {
                            DagrsError::new(
                                ErrorCode::DgChk0003InvalidCheckpoint,
                                "checkpoint output is missing its type tag",
                            )
                            .with_node_id(node_id.as_usize())
                            .with_checkpoint(checkpoint.id.clone())
                        })?;
                        let content = Self::restore_output_content(kind, data).map_err(|err| {
                            err.with_node_id(node_id.as_usize())
                                .with_checkpoint(checkpoint.id.clone())
                        })?;
                        exec_state.set_output(Output::Out(Some(content.clone())));
                        exec_state.exe_success();
                        restored_outputs.push((node_id, content));
                    } else {
                        exec_state.exe_success();
                    }
                }
                NodeExecStatus::Failed => {
                    let message = node_state
                        .output_summary
                        .clone()
                        .unwrap_or_else(|| "node failed before checkpoint was saved".to_string());
                    exec_state.set_output(Output::error(
                        DagrsError::new(ErrorCode::DgRun0006NodeExecutionFailed, message)
                            .with_node_id(node_id.as_usize()),
                    ));
                    exec_state.exe_fail();
                }
                NodeExecStatus::Skipped => {
                    skipped_senders.push(node_id);
                }
                NodeExecStatus::Pending | NodeExecStatus::Running => {}
            }
        }

        for skipped_sender in skipped_senders {
            if let Some(receiver_ids) = self.abstract_graph.edges.get(&skipped_sender) {
                let active_receivers: Vec<_> = receiver_ids
                    .iter()
                    .copied()
                    .filter(|receiver_id| {
                        Self::node_should_receive_replayed_input(
                            checkpoint,
                            active_nodes,
                            *receiver_id,
                        )
                    })
                    .collect();
                self.set_sender_enabled_for_receivers(
                    skipped_sender,
                    &active_receivers,
                    false,
                    Some(checkpoint.id.as_str()),
                )
                .await?;
            }
        }

        for (node_id, content) in restored_outputs {
            let node = self.nodes.get(&node_id).ok_or_else(|| {
                DagrsError::new(
                    ErrorCode::DgChk0003InvalidCheckpoint,
                    "checkpoint references a node that does not exist in the graph",
                )
                .with_node_id(node_id.as_usize())
                .with_checkpoint(checkpoint.id.clone())
            })?;
            let receiver_ids = {
                let mut node_guard = node.lock().await;
                node_guard.output_channels().get_receiver_ids()
            };
            let replay_receivers: Vec<_> = receiver_ids
                .into_iter()
                .filter(|receiver_id| {
                    Self::node_should_receive_replayed_input(checkpoint, active_nodes, *receiver_id)
                })
                .collect();
            self.set_sender_enabled_for_receivers(
                node_id,
                &replay_receivers,
                true,
                Some(checkpoint.id.as_str()),
            )
            .await?;

            let mut node_guard = node.lock().await;
            let output_channels = node_guard.output_channels();
            for receiver_id in replay_receivers {
                output_channels
                    .send_to(&receiver_id, content.clone())
                    .await
                    .map_err(|err| err.with_checkpoint(checkpoint.id.clone()))?;
            }
        }

        Ok(())
    }

    pub async fn resume_from_checkpoint(
        &mut self,
        checkpoint_id: &str,
    ) -> DagrsResult<ExecutionReport> {
        self.resume_from_checkpoint_with(checkpoint_id, RunOptions::default())
            .await
    }

    pub async fn resume_from_checkpoint_with(
        &mut self,
        checkpoint_id: &str,
        opts: RunOptions,
    ) -> DagrsResult<ExecutionReport> {
        let started_at_unix_secs = Self::current_unix_secs();
        let run_id = Self::run_id(opts.run_id);

        let checkpoint = match self.load_checkpoint(checkpoint_id).await {
            Ok(checkpoint) => checkpoint,
            Err(err) => {
                self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
                return Err(err);
            }
        };

        info!(
            "Resuming from checkpoint: {} (pc={}, loop_count={})",
            checkpoint.id, checkpoint.pc, checkpoint.loop_count
        );

        if let Err(err) = self.reset_with(ResetPolicy::KeepEnv).await {
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }

        self.init();
        if let Err(err) = self.check_loop_and_partition().await {
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }
        if let Err(err) = self.validate_checkpoint(&checkpoint) {
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }

        let start_pc = match self.resume_block_from_checkpoint(&checkpoint) {
            Ok(start_pc) => start_pc,
            Err(err) => {
                self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
                return Err(err);
            }
        };
        let mut active_nodes = Self::rebuild_active_nodes_from_checkpoint(&checkpoint);
        if let Err(err) =
            self.prune_succeeded_nodes_from_resume_span(&checkpoint, &mut active_nodes, start_pc)
        {
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }
        let (completed_total, skipped_total) = self.checkpoint_progress_totals(&checkpoint);
        if let Err(err) = self
            .restore_checkpoint_state(&checkpoint, &active_nodes)
            .await
        {
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }

        let _ = self.event_sender.send(GraphEvent::CheckpointRestored {
            checkpoint_id: checkpoint.id.clone(),
            pc: checkpoint.pc,
        });

        let ctx = RunContext {
            run_id,
            started_at_unix_secs,
            start_pc,
            start_loop_count: checkpoint.loop_count,
            initial_completed_total: completed_total,
            initial_skipped_total: skipped_total,
        };

        self.run_internal(
            ctx,
            Self::deserialize_loop_node_iterations(&checkpoint).unwrap_or_default(),
            active_nodes,
            false,
        )
        .await
    }

    fn current_unix_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn run_id(run_id: Option<String>) -> String {
        run_id.unwrap_or_else(|| format!("run_{}", Self::current_unix_secs()))
    }

    fn emit_termination(&self, status: TerminationStatus, error: Option<DagrsError>) {
        let _ = self
            .event_sender
            .send(GraphEvent::ExecutionTerminated { status, error });
        self.is_active.store(false, Ordering::Relaxed);
    }

    fn build_report(
        &self,
        run_id: String,
        status: CompletionStatus,
        started_at_unix_secs: u64,
        node_skipped: usize,
    ) -> ExecutionReport {
        let node_failed = self
            .execute_states
            .values()
            .filter(|state| state.get_full_output().is_err())
            .count();
        let node_succeeded = self
            .execute_states
            .values()
            .filter(|state| state.is_success() && !state.get_full_output().is_err())
            .count();

        ExecutionReport {
            run_id,
            status,
            started_at_unix_secs,
            ended_at_unix_secs: Self::current_unix_secs(),
            node_total: self.nodes.len(),
            node_succeeded,
            node_failed,
            node_skipped,
        }
    }

    fn aggregate_errors(errors: &[DagrsError]) -> DagrsError {
        if let Some(err) = errors.first()
            && errors.len() == 1
        {
            return err.clone();
        }
        DagrsError::new(
            ErrorCode::DgRun0006NodeExecutionFailed,
            "multiple node failures occurred during graph execution",
        )
        .with_detail("error_count", errors.len().to_string())
    }

    async fn run_internal(
        &mut self,
        ctx: RunContext,
        initial_loop_node_iterations: HashMap<NodeId, usize>,
        initial_active_nodes: HashSet<NodeId>,
        reset_nodes: bool,
    ) -> DagrsResult<ExecutionReport> {
        let RunContext {
            run_id,
            started_at_unix_secs,
            start_pc,
            start_loop_count,
            initial_completed_total,
            initial_skipped_total,
        } = ctx;

        let condition_flag = Arc::new(Mutex::new(true));
        let errors = Arc::new(Mutex::new(Vec::<DagrsError>::new()));
        let mut skipped_total = initial_skipped_total;
        let mut completed_total = initial_completed_total;

        if reset_nodes {
            for node in self.nodes.values() {
                let mut node = node.lock().await;
                node.reset();
            }
        }

        let mut pc = start_pc;
        let mut loop_count = start_loop_count;
        let mut loop_node_iterations = initial_loop_node_iterations;
        let mut active_nodes = initial_active_nodes;

        let mut parents_map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (parent, children) in &self.abstract_graph.edges {
            for child in children {
                parents_map.entry(*child).or_default().push(*parent);
            }
        }

        let mut nodes_since_checkpoint = 0usize;
        let checkpoint_start_time = std::time::Instant::now();
        let mut status = CompletionStatus::Succeeded;

        while pc < self.blocks.len() {
            if self.checkpoint_config.enabled && self.checkpoint_store.is_some() {
                let should_checkpoint = self.should_create_checkpoint(
                    nodes_since_checkpoint,
                    checkpoint_start_time.elapsed().as_secs(),
                );
                if should_checkpoint {
                    if let Err(err) = self
                        .save_checkpoint_with_loop_node_iterations(
                            pc,
                            loop_count,
                            &active_nodes,
                            Some(&loop_node_iterations),
                        )
                        .await
                    {
                        error!("Failed to save automatic checkpoint: {err}");
                    } else {
                        nodes_since_checkpoint = 0;
                    }
                }
            }

            let block = &self.blocks[pc];
            let mut active_block_nodes = Vec::new();
            let mut skipped_block_nodes = Vec::new();

            for id in block {
                if active_nodes.contains(id) {
                    active_block_nodes.push(*id);
                } else {
                    skipped_block_nodes.push(*id);
                }
            }

            for node_id in skipped_block_nodes {
                let already_restored_terminal = self
                    .execute_states
                    .get(&node_id)
                    .is_some_and(|state| state.is_success() || state.get_full_output().is_err());
                if already_restored_terminal {
                    continue;
                }

                skipped_total += 1;
                completed_total += 1;
                let _ = self.event_sender.send(GraphEvent::NodeSkipped {
                    id: node_id,
                    reason: SkipReason::PrunedByControlFlow,
                });
                if let Some(node) = self.nodes.get(&node_id) {
                    let node_guard = node.lock().await;
                    let hooks_guard = self.hooks.read().await;
                    for hook in hooks_guard.iter() {
                        hook.on_skip(&*node_guard, &self.env).await;
                    }
                }
            }

            if active_block_nodes.is_empty() {
                let _ = self.event_sender.send(GraphEvent::Progress {
                    completed: completed_total,
                    total: self.nodes.len(),
                });
                pc += 1;
                continue;
            }

            let mut tasks = vec![];
            for node_id in &active_block_nodes {
                let node = self.nodes.get(node_id).ok_or_else(|| {
                    DagrsError::new(ErrorCode::DgBld0001NodeNotFound, "node not found")
                        .with_node_id(node_id.as_usize())
                })?;
                let execute_state = self
                    .execute_states
                    .get(node_id)
                    .ok_or_else(|| {
                        DagrsError::new(
                            ErrorCode::DgRun0006NodeExecutionFailed,
                            "execution state not initialised for node",
                        )
                        .with_node_id(node_id.as_usize())
                    })?
                    .clone();
                let env = Arc::clone(&self.env);
                let node = Arc::clone(node);
                let condition_flag = Arc::clone(&condition_flag);
                let errors = Arc::clone(&errors);
                let hooks = Arc::clone(&self.hooks);
                let event_sender = self.event_sender.clone();

                tasks.push(task::spawn(async move {
                    use futures::FutureExt;

                    let node_ref: Arc<Mutex<dyn Node>> = node.clone();
                    let node_guard = node.lock().await;
                    let node_name = node_guard.name().to_string();
                    let node_id = node_guard.id();
                    let max_retries = node_guard.max_retries();

                    {
                        let hooks_guard = hooks.read().await;
                        for hook in hooks_guard.iter() {
                            hook.before_node_run(&*node_guard, &env).await;
                        }
                    }

                    let _ = event_sender.send(GraphEvent::NodeStart {
                        id: node_id,
                        timestamp: Graph::current_unix_secs(),
                    });

                    drop(node_guard);

                    let mut attempt = 0u32;
                    let final_output = loop {
                        let mut node_guard = node_ref.lock().await;
                        let retry_delay = node_guard.retry_delay_ms(attempt + 1);
                        let started_at = std::time::Instant::now();
                        let result = AssertUnwindSafe(node_guard.run(env.clone()))
                            .catch_unwind()
                            .await;
                        let duration_ms = started_at.elapsed().as_millis() as u64;

                        match result {
                            Ok(out) if out.is_err() => {
                                let err = out.get_err().cloned().unwrap_or_else(|| {
                                    DagrsError::new(
                                        ErrorCode::DgRun0006NodeExecutionFailed,
                                        "node execution failed without an error payload",
                                    )
                                    .with_node(node_id.as_usize(), node_name.clone())
                                });

                                if attempt < max_retries {
                                    attempt += 1;
                                    let _ = event_sender.send(GraphEvent::NodeRetry {
                                        id: node_id,
                                        attempt,
                                        max_retries,
                                        error: err.clone(),
                                    });

                                    let mut should_retry = true;
                                    {
                                        let hooks_guard = hooks.read().await;
                                        for hook in hooks_guard.iter() {
                                            let decision = hook
                                                .on_retry(
                                                    &*node_guard,
                                                    &err,
                                                    attempt,
                                                    max_retries,
                                                    &env,
                                                )
                                                .await;
                                            if decision == RetryDecision::Fail {
                                                should_retry = false;
                                                break;
                                            }
                                        }
                                    }

                                    if should_retry {
                                        warn!(
                                            "Retrying node [name: {}, id: {}] attempt {}/{} after {}ms - {}",
                                            node_name,
                                            node_id.0,
                                            attempt,
                                            max_retries,
                                            retry_delay,
                                            err
                                        );
                                        drop(node_guard);
                                        tokio::time::sleep(Duration::from_millis(retry_delay)).await;
                                        continue;
                                    }
                                }

                                break Ok((out, duration_ms));
                            }
                            Ok(out) => break Ok((out, duration_ms)),
                            Err(_) => {
                                break Err(
                                    DagrsError::new(
                                        ErrorCode::DgRun0001TaskPanicked,
                                        "node execution panicked",
                                    )
                                    .with_node(node_id.as_usize(), node_name.clone()),
                                )
                            }
                        }
                    };

                    match final_output {
                        Ok((out, duration_ms)) => {
                            let mut node_guard = node_ref.lock().await;
                            if out.is_err() {
                                let err = out.get_err().cloned().unwrap_or_else(|| {
                                    DagrsError::new(
                                        ErrorCode::DgRun0006NodeExecutionFailed,
                                        "node execution failed without an error payload",
                                    )
                                    .with_node(node_id.as_usize(), node_name.clone())
                                });
                                error!(
                                    "Execution failed [name: {}, id: {}] - {} (after {} retries)",
                                    node_name, node_id.0, err, attempt
                                );
                                let _ = event_sender.send(GraphEvent::NodeFailed {
                                    id: node_id,
                                    error: err.clone(),
                                });
                                node_guard.input_channels().close_all().await;
                                node_guard.output_channels().close_all().await;
                                execute_state.set_output(out.clone());
                                execute_state.exe_fail();
                                errors.lock().await.push(err);
                            } else {
                                {
                                    let hooks_guard = hooks.read().await;
                                    for hook in hooks_guard.iter() {
                                        hook.after_node_run(&*node_guard, &out, &env).await;
                                    }
                                }

                                if let Some(false) = out.conditional_result() {
                                    let mut cf = condition_flag.lock().await;
                                    *cf = false;
                                }

                                let _ = event_sender.send(GraphEvent::NodeSuccess {
                                    id: node_id,
                                    duration_ms,
                                });
                                execute_state.set_output(out.clone());
                                execute_state.exe_success();
                            }
                            let receiver_ids = node_guard.output_channels().get_receiver_ids();
                            Ok::<_, DagrsError>((node_id, out, receiver_ids))
                        }
                        Err(err) => {
                            let mut node_guard = node_ref.lock().await;
                            node_guard.input_channels().close_all().await;
                            node_guard.output_channels().close_all().await;
                            let _ = event_sender.send(GraphEvent::NodeFailed {
                                id: node_id,
                                error: err.clone(),
                            });
                            execute_state.set_output(Output::error(err.clone()));
                            execute_state.exe_fail();
                            errors.lock().await.push(err.clone());
                            Ok::<_, DagrsError>((node_id, Output::error(err), Vec::new()))
                        }
                    }
                }));
            }

            let mut results = Vec::new();
            for result in futures::future::join_all(tasks).await {
                match result {
                    Ok(Ok(output)) => results.push(output),
                    Ok(Err(err)) => errors.lock().await.push(err),
                    Err(join_err) => {
                        errors.lock().await.push(DagrsError::new(
                            ErrorCode::DgRun0002TaskJoinFailed,
                            format!("node task join failed: {join_err}"),
                        ));
                    }
                }
            }

            completed_total += active_block_nodes.len();
            nodes_since_checkpoint += active_block_nodes.len();
            let _ = self.event_sender.send(GraphEvent::Progress {
                completed: completed_total,
                total: self.nodes.len(),
            });

            let errors_guard = errors.lock().await;
            if !errors_guard.is_empty() {
                if self.checkpoint_config.enabled && self.checkpoint_store.is_some() {
                    let _ = self
                        .save_checkpoint_with_loop_node_iterations(
                            pc,
                            loop_count,
                            &active_nodes,
                            Some(&loop_node_iterations),
                        )
                        .await;
                }
                let err = Self::aggregate_errors(&errors_guard);
                drop(errors_guard);
                self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
                return Err(err);
            }
            drop(errors_guard);

            for (node_id, output, receiver_ids) in &results {
                if !output.is_err() {
                    self.set_sender_enabled_for_receivers(*node_id, receiver_ids, true, None)
                        .await?;
                }
            }

            if !(*condition_flag.lock().await) {
                break;
            }

            let mut next_pc = pc + 1;
            let mut should_abort = false;
            let mut backward_loop_source = None;
            for (node_id, output, _) in results {
                if self.handle_flow_control(
                    output,
                    node_id,
                    &mut active_nodes,
                    &self.node_block_map,
                    &parents_map,
                    &mut next_pc,
                    self.blocks.len(),
                    pc,
                    &mut backward_loop_source,
                )? {
                    should_abort = true;
                    break;
                }
            }

            if should_abort {
                status = CompletionStatus::Aborted;
                break;
            }

            if next_pc < pc {
                loop_count += 1;
                if let Some(node_id) = backward_loop_source {
                    *loop_node_iterations.entry(node_id).or_insert(0) += 1;
                }
                if loop_count >= self.max_loop_count {
                    let err = DagrsError::new(
                        ErrorCode::DgRun0003LoopLimitExceeded,
                        "maximum loop limit exceeded",
                    )
                    .with_detail("limit", self.max_loop_count.to_string());
                    self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
                    return Err(err);
                }

                if self.checkpoint_config.enabled
                    && self.checkpoint_config.on_loop_iteration
                    && self.checkpoint_store.is_some()
                {
                    let _ = self
                        .save_checkpoint_with_loop_node_iterations(
                            next_pc,
                            loop_count,
                            &active_nodes,
                            Some(&loop_node_iterations),
                        )
                        .await;
                }

                let _ = self.event_sender.send(GraphEvent::LoopIteration {
                    iteration: loop_count,
                    block_index: next_pc,
                });

                active_nodes = self.nodes.keys().cloned().collect();
            }

            pc = next_pc;
        }

        let termination_status = match status {
            CompletionStatus::Succeeded => TerminationStatus::Succeeded,
            CompletionStatus::Aborted => TerminationStatus::Aborted,
        };
        let termination_error =
            matches!(status, CompletionStatus::Aborted).then(DagrsError::aborted);
        self.emit_termination(termination_status, termination_error);

        Ok(self.build_report(run_id, status, started_at_unix_secs, skipped_total))
    }

    /// Check if a checkpoint should be created based on configuration.
    fn should_create_checkpoint(&self, nodes_completed: usize, seconds_elapsed: u64) -> bool {
        if let Some(interval) = self.checkpoint_config.interval_nodes
            && nodes_completed >= interval
        {
            return true;
        }
        if let Some(interval) = self.checkpoint_config.interval_seconds
            && seconds_elapsed >= interval
        {
            return true;
        }
        false
    }

    /// Reset the graph state but keep the nodes.
    /// This method is async because it needs to acquire locks on nodes.
    pub async fn reset(&mut self) -> DagrsResult<()> {
        self.reset_with(ResetPolicy::KeepEnv).await
    }

    pub async fn reset_with(&mut self, policy: ResetPolicy) -> DagrsResult<()> {
        self.execute_states = HashMap::new();
        if matches!(policy, ResetPolicy::ResetEnv) {
            self.env = Arc::new(EnvVar::new(NodeTable::default()));
        }
        self.is_active = Arc::new(AtomicBool::new(true));
        self.blocks.clear();
        self.node_block_map.clear();

        // Re-create channels for all edges to support graph reuse
        // 1. Clear existing channels
        for node in self.nodes.values() {
            let mut node = node.lock().await;
            node.input_channels().0.clear();
            node.input_channels().clear_disabled();
            node.output_channels().0.clear();
            node.reset();
        }

        // 2. Re-establish connections based on abstract_graph
        // Clone edges to avoid borrowing conflict
        let edges = self.abstract_graph.edges.clone();

        for (from_id, to_ids) in edges {
            // Resolve abstract node ID to concrete node IDs for folded loop nodes.
            // If from_id is an abstract (folded) node, we need to find the actual
            // concrete nodes that should send data.
            let concrete_from_ids: Vec<NodeId> = self
                .abstract_graph
                .unfold_node(from_id)
                .cloned()
                .unwrap_or_else(|| vec![from_id]);

            for concrete_from_id in concrete_from_ids {
                let mut rx_map: HashMap<NodeId, mpsc::Receiver<Content>> = HashMap::new();

                // Resolve to_ids: unfold any abstract nodes to their concrete counterparts
                let concrete_to_ids: Vec<NodeId> = to_ids
                    .iter()
                    .flat_map(|to_id| {
                        self.abstract_graph
                            .unfold_node(*to_id)
                            .cloned()
                            .unwrap_or_else(|| vec![*to_id])
                    })
                    .collect();

                // Setup OutChannels for 'concrete_from_id'
                if let Some(node_lock) = self.nodes.get(&concrete_from_id) {
                    let mut node = node_lock.lock().await;
                    let out_channels = node.output_channels();

                    for to_id in &concrete_to_ids {
                        // Only create channel if target node exists in the graph
                        if self.nodes.contains_key(to_id) {
                            let (tx, rx) = mpsc::channel::<Content>(32);
                            out_channels.insert(*to_id, Arc::new(Mutex::new(OutChannel::Mpsc(tx))));
                            rx_map.insert(*to_id, rx);
                        }
                    }
                }

                // Setup InChannels for concrete to_ids
                for (to_id, rx) in rx_map {
                    if let Some(node_lock) = self.nodes.get(&to_id) {
                        let mut node = node_lock.lock().await;
                        node.input_channels()
                            .insert(concrete_from_id, Arc::new(Mutex::new(InChannel::Mpsc(rx))));
                    }
                }
            }
        }
        Ok(())
    }

    /// Register a new execution hook
    pub async fn add_hook(&mut self, hook: Box<dyn ExecutionHook>) {
        let mut hooks = self.hooks.write().await;
        hooks.push(hook);
    }

    /// Subscribe to graph events
    pub fn subscribe(&self) -> broadcast::Receiver<GraphEvent> {
        self.event_sender.subscribe()
    }

    fn try_lock_node_for_build<'a>(
        node: &'a Arc<Mutex<dyn Node>>,
        context: &str,
    ) -> DagrsResult<tokio::sync::MutexGuard<'a, dyn Node>> {
        node.try_lock().map_err(|_| {
            DagrsError::new(
                ErrorCode::DgBld0005ConcurrentBuildMutation,
                format!(
                    "failed to acquire node lock while building graph ({context}); build the graph from a single context"
                ),
            )
        })
    }

    /// Adds a new node to the `Graph`
    pub fn add_node(&mut self, node: impl Node + 'static) -> DagrsResult<NodeId> {
        if let Some(loop_structure) = node.loop_structure() {
            // Expand loop subgraph, and update concrete node id -> abstract node id mapping in abstract_graph
            let abstract_node_id = node.id();

            if self.nodes.contains_key(&abstract_node_id) {
                return Err(DagrsError::new(
                    ErrorCode::DgBld0003DuplicateNodeId,
                    "duplicate node id detected while adding loop subgraph",
                )
                .with_node_id(abstract_node_id.as_usize()));
            }

            let concrete_ids = loop_structure
                .iter()
                .map(|n| {
                    Self::try_lock_node_for_build(n, "loop_structure node id")
                        .map(|guard| guard.id())
                })
                .collect::<DagrsResult<Vec<_>>>()?;
            let mut seen_concrete_ids = HashSet::new();
            for concrete_id in &concrete_ids {
                if !seen_concrete_ids.insert(*concrete_id) {
                    return Err(DagrsError::new(
                        ErrorCode::DgBld0003DuplicateNodeId,
                        "duplicate node id detected inside loop subgraph",
                    )
                    .with_node_id(concrete_id.as_usize()));
                }
                if self.nodes.contains_key(concrete_id) {
                    return Err(DagrsError::new(
                        ErrorCode::DgBld0003DuplicateNodeId,
                        "duplicate node id detected while expanding loop subgraph",
                    )
                    .with_node_id(concrete_id.as_usize()));
                }
            }

            log::debug!("Add node {:?} to abstract graph", abstract_node_id);
            self.abstract_graph
                .add_folded_node(abstract_node_id, concrete_ids.clone());

            for (node, concrete_id) in loop_structure.into_iter().zip(concrete_ids) {
                log::debug!("Add node {:?} to concrete graph", concrete_id);
                self.nodes.insert(concrete_id, node.clone());
                self.in_degree.entry(concrete_id).or_insert(0);
            }
            Ok(abstract_node_id)
        } else {
            let id = node.id();
            if self.nodes.contains_key(&id) {
                return Err(DagrsError::new(
                    ErrorCode::DgBld0003DuplicateNodeId,
                    "duplicate node id detected",
                )
                .with_node_id(id.as_usize()));
            }
            let node = Arc::new(Mutex::new(node));
            self.node_count += 1;
            self.nodes.insert(id, node);
            self.in_degree.insert(id, 0);
            self.abstract_graph.add_node(id);

            log::debug!("Add node {:?} to concrete & abstract graph", id);
            Ok(id)
        }
    }
    /// Adds an edge between two nodes in the `Graph`.
    /// If the outgoing port of the sending node is empty and the number of receiving nodes is > 1, use the broadcast channel
    /// An MPSC channel is used if the outgoing port of the sending node is empty and the number of receiving nodes is equal to 1
    /// If the outgoing port of the sending node is not empty, adding any number of receiving nodes will change all relevant channels to broadcast
    pub fn add_edge<I>(&mut self, from_id: NodeId, all_to_ids: I) -> DagrsResult<()>
    where
        I: IntoIterator<Item = NodeId>,
    {
        let to_ids = Self::remove_duplicates(all_to_ids);
        let mut rx_map: HashMap<NodeId, mpsc::Receiver<Content>> = HashMap::new();
        if !self.nodes.contains_key(&from_id) {
            return Err(
                DagrsError::new(ErrorCode::DgBld0001NodeNotFound, "source node not found")
                    .with_node_id(from_id.as_usize()),
            );
        }
        for to_id in &to_ids {
            if !self.nodes.contains_key(to_id) {
                return Err(DagrsError::new(
                    ErrorCode::DgBld0001NodeNotFound,
                    "target node not found",
                )
                .with_node_id(to_id.as_usize()));
            }
        }

        // Update channels
        {
            let from_node_lock = self.nodes.get_mut(&from_id).ok_or_else(|| {
                DagrsError::new(ErrorCode::DgBld0001NodeNotFound, "source node not found")
                    .with_node_id(from_id.as_usize())
            })?;
            let mut from_node = Self::try_lock_node_for_build(
                from_node_lock,
                "add_edge from node output channels",
            )?;
            let from_channel = from_node.output_channels();

            for to_id in &to_ids {
                if !from_channel.0.contains_key(to_id) {
                    // Update the abstract graph first so an abstract-edge error
                    // cannot leave the concrete channel map partially mutated.
                    self.abstract_graph.add_edge(from_id, *to_id)?;

                    let (tx, rx) = mpsc::channel::<Content>(32);
                    from_channel.insert(*to_id, Arc::new(Mutex::new(OutChannel::Mpsc(tx.clone()))));
                    rx_map.insert(*to_id, rx);
                    self.in_degree
                        .entry(*to_id)
                        .and_modify(|e| *e += 1)
                        .or_insert(0);
                }
            }
        }
        for to_id in &to_ids {
            if let Some(to_node_lock) = self.nodes.get_mut(to_id) {
                let mut to_node =
                    Self::try_lock_node_for_build(to_node_lock, "add_edge to node input channels")?;
                let to_channel = to_node.input_channels();
                if let Some(rx) = rx_map.remove(to_id) {
                    to_channel.insert(from_id, Arc::new(Mutex::new(InChannel::Mpsc(rx))));
                }
            }
        }
        Ok(())
    }

    /// Initializes the network, setting up the nodes.
    pub(crate) fn init(&mut self) {
        self.execute_states.reserve(self.nodes.len());
        self.nodes.keys().for_each(|node| {
            self.execute_states
                .insert(*node, Arc::new(ExecState::new()));
        });
    }

    /// Executes a single DAG within an existing async runtime.
    ///
    /// Use this method when you are already running inside an async context,
    /// for example inside `#[tokio::main]` or inside a Tokio task.
    ///
    /// Runtime ownership stays with the caller. `dagrs` does not create a
    /// runtime in this path.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let mut graph = build_graph_somehow();
    ///
    ///     // Use `async_start` because we are already inside a Tokio runtime.
    ///     graph.async_start().await?;
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn async_start(&mut self) -> DagrsResult<ExecutionReport> {
        self.async_start_with(RunOptions::default()).await
    }

    pub async fn async_start_with(&mut self, opts: RunOptions) -> DagrsResult<ExecutionReport> {
        let started_at_unix_secs = Self::current_unix_secs();
        let run_id = Self::run_id(opts.run_id);

        self.init();
        if let Err(err) = self.check_loop_and_partition().await {
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }

        if !self.is_active.load(Ordering::Relaxed) {
            let err = DagrsError::new(
                ErrorCode::DgRun0004GraphNotActive,
                "graph is not active; call reset() before executing again",
            );
            self.emit_termination(TerminationStatus::Failed, Some(err.clone()));
            return Err(err);
        }

        let ctx = RunContext {
            run_id,
            started_at_unix_secs,
            start_pc: 0,
            start_loop_count: 0,
            initial_completed_total: 0,
            initial_skipped_total: 0,
        };

        self.run_internal(
            ctx,
            HashMap::new(),
            self.nodes.keys().cloned().collect(),
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_flow_control(
        &self,
        output: Output,
        node_id: NodeId,
        active_nodes: &mut HashSet<NodeId>,
        node_block_map: &HashMap<NodeId, usize>,
        parents_map: &HashMap<NodeId, Vec<NodeId>>,
        next_pc: &mut usize,
        blocks_len: usize,
        current_pc: usize,
        backward_loop_source: &mut Option<NodeId>,
    ) -> DagrsResult<bool> {
        if let Some(flow) = output.get_flow() {
            match flow {
                FlowControl::Loop(instr) => {
                    if let Some(idx) = instr.jump_to_block_index {
                        // Validate that the jump target is within valid block range
                        if idx >= blocks_len {
                            error!(
                                "Graph configuration error: jump_to_block_index {} is out of bounds (blocks count: {})",
                                idx, blocks_len
                            );
                            return Err(
                                DagrsError::new(
                                    ErrorCode::DgRun0006NodeExecutionFailed,
                                    format!(
                                        "jump_to_block_index {idx} is out of bounds; valid range is 0..{blocks_len}"
                                    ),
                                )
                                .with_node_id(node_id.as_usize()),
                            );
                        }
                        *next_pc = idx;
                        if idx < current_pc {
                            *backward_loop_source = Some(node_id);
                        }
                    } else if let Some(nid) = instr.jump_to_node {
                        if let Some(&idx) = node_block_map.get(&NodeId(nid)) {
                            // Validate that the resolved block index is within valid range
                            // This should not happen in normal operation, but is a defensive check
                            if idx >= blocks_len {
                                error!(
                                    "Internal error: node_block_map contains invalid block index {} for node {} (blocks count: {})",
                                    idx, nid, blocks_len
                                );
                                return Err(
                                    DagrsError::new(
                                        ErrorCode::DgRun0006NodeExecutionFailed,
                                        format!(
                                            "node_block_map resolved invalid block index {idx} for node {nid}"
                                        ),
                                    )
                                    .with_node_id(node_id.as_usize()),
                                );
                            }
                            *next_pc = idx;
                            if idx < current_pc {
                                *backward_loop_source = Some(node_id);
                            }
                        } else {
                            error!(
                                "Graph configuration error: invalid jump target node {} not found in block map. \
                                   This is likely due to an incorrect node ID in the LoopInstruction.",
                                nid
                            );
                            return Err(DagrsError::new(
                                ErrorCode::DgRun0006NodeExecutionFailed,
                                format!("invalid jump target node {nid} not found"),
                            )
                            .with_node_id(node_id.as_usize()));
                        }
                    }
                }
                FlowControl::Branch(ids) => {
                    // Event: BranchSelected
                    let _ = self.event_sender.send(GraphEvent::BranchSelected {
                        node_id,
                        selected_branches: ids.clone(),
                    });

                    let allowed: HashSet<usize> = ids.iter().cloned().collect();
                    if let Some(children) = self.abstract_graph.edges.get(&node_id) {
                        let mut to_prune = Vec::new();
                        let empty_vec = Vec::new();

                        // 1. Identify immediate children to prune
                        for child in children {
                            if !allowed.contains(&child.0) {
                                // Check if child has ANY active parent *excluding* current node_id
                                let parents = parents_map.get(child).unwrap_or(&empty_vec);
                                let has_other_active_parent = parents
                                    .iter()
                                    .any(|p| *p != node_id && active_nodes.contains(p));

                                if !has_other_active_parent && active_nodes.remove(child) {
                                    // Child is pruned. Now check its children.
                                    if let Some(descendants) = self.abstract_graph.edges.get(child)
                                    {
                                        for desc in descendants {
                                            to_prune.push(*desc);
                                        }
                                    }
                                }
                            }
                        }

                        // 2. Recursively prune descendants
                        while let Some(pruned_id) = to_prune.pop() {
                            // Only prune if NO active parents remain
                            let parents = parents_map.get(&pruned_id).unwrap_or(&empty_vec);
                            let has_active_parent =
                                parents.iter().any(|p| active_nodes.contains(p));

                            if !has_active_parent && active_nodes.remove(&pruned_id) {
                                // If the node was active and is now pruned, schedule its children for check
                                if let Some(descendants) = self.abstract_graph.edges.get(&pruned_id)
                                {
                                    for desc in descendants {
                                        to_prune.push(*desc);
                                    }
                                }
                            }
                        }
                    }
                }
                FlowControl::Abort => {
                    // Set next_pc beyond the last block to exit the outer while loop
                    *next_pc = usize::MAX;
                    return Ok(true);
                }
                FlowControl::Continue => {}
            }
        }
        Ok(false)
    }

    /// Checks for cycles in the abstract graph, and partitions the graph into blocks.
    /// - Groups nodes into blocks, creating a new block whenever a conditional node / loop is encountered
    ///
    /// Returns true if the graph contains a cycle, false otherwise.
    pub async fn check_loop_and_partition(&mut self) -> DagrsResult<()> {
        // Check for cycles and get topological sort
        let sorted_nodes = match self.abstract_graph.get_topological_sort() {
            Some(nodes) => nodes,
            None => {
                return Err(DagrsError::new(
                    ErrorCode::DgBld0004GraphLoopDetected,
                    "graph contains a loop",
                ));
            }
        };

        // Split into blocks based on conditional nodes
        let mut current_block = HashSet::new();
        self.blocks.clear();
        self.node_block_map.clear();

        for node_id in sorted_nodes {
            if let Some(unfolded_nodes) = self.abstract_graph.unfold_node(node_id) {
                // Create new block for unfolded nodes
                if !current_block.is_empty() {
                    self.blocks.push(current_block);
                    current_block = HashSet::new();
                }

                for node_id in unfolded_nodes {
                    current_block.insert(*node_id);
                }
                self.blocks.push(current_block);
                current_block = HashSet::new();
            } else {
                current_block.insert(node_id);

                // Create new block if conditional node / loop encountered
                let Some(node) = self.nodes.get(&node_id) else {
                    return Err(DagrsError::new(
                        ErrorCode::DgBld0001NodeNotFound,
                        "node not found",
                    )
                    .with_node_id(node_id.as_usize()));
                };
                // Use an async lock here to avoid blocking the runtime
                let node_guard = node.lock().await;
                if node_guard.is_condition() {
                    self.blocks.push(current_block);
                    current_block = HashSet::new();
                }
            }
        }

        // Add any remaining nodes to final block
        if !current_block.is_empty() {
            self.blocks.push(current_block);
        }

        // Build node_block_map
        for (i, block) in self.blocks.iter().enumerate() {
            for node_id in block {
                self.node_block_map.insert(*node_id, i);
            }
        }

        debug!("Split the graph into blocks: {:?}", self.blocks);

        Ok(())
    }

    /// Get the output of all tasks.
    pub fn get_results<T: Send + Sync + 'static>(&self) -> HashMap<NodeId, Option<Arc<T>>> {
        self.execute_states
            .iter()
            .map(|(&id, state)| {
                let output = match state.get_output() {
                    Some(content) => content.into_inner(),
                    None => None,
                };
                (id, output)
            })
            .collect()
    }

    pub fn get_outputs(&self) -> HashMap<NodeId, Output> {
        self.execute_states
            .iter()
            .map(|(&id, state)| {
                let t = state.get_full_output();
                (id, t)
            })
            .collect()
    }

    /// Before the dag starts executing, set the dag's global environment variable.
    pub fn set_env(&mut self, env: EnvVar) {
        self.env = Arc::new(env);
    }

    ///Remove duplicate elements
    fn remove_duplicates<T, I>(items: I) -> Vec<T>
    where
        I: IntoIterator<Item = T>,
        T: Eq + Hash + Clone,
    {
        let mut seen = HashSet::new();
        items
            .into_iter()
            .filter(|item| seen.insert(item.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::loop_subgraph::LoopSubgraph;
    use crate::node::conditional_node::{Condition, ConditionalNode};
    use crate::node::default_node::DefaultNode;
    use crate::{
        Checkpoint, Content, EnvVar, InChannels, MemoryCheckpointStore, Node, NodeName, NodeState,
        NodeTable, OutChannels, Output, StoredOutputKind, action::Action,
    };
    use async_trait::async_trait;
    use std::{
        sync::{Arc, Mutex as StdMutex},
        time::Duration,
    };

    /// An implementation of [`Action`] that returns [`Output::Out`] containing a String "Hello world" from default_node.rs.
    #[derive(Default)]
    pub struct HelloAction;
    #[async_trait]
    impl Action for HelloAction {
        async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
            Output::Out(Some(Content::new("Hello world".to_string())))
        }
    }

    impl HelloAction {
        pub fn new() -> Self {
            Self
        }
    }

    struct NonSerializableOutput;

    struct NonSerializableAction;

    #[async_trait]
    impl Action for NonSerializableAction {
        async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
            Output::new(NonSerializableOutput)
        }
    }

    struct FixedIdNode {
        id: NodeId,
        name: NodeName,
        in_channels: InChannels,
        out_channels: OutChannels,
    }

    struct RestoreProbeNode {
        id: NodeId,
        name: NodeName,
        in_channels: InChannels,
        out_channels: OutChannels,
        restored_count: Arc<StdMutex<Option<usize>>>,
    }

    impl RestoreProbeNode {
        fn new(
            id: NodeId,
            name: impl Into<NodeName>,
            restored_count: Arc<StdMutex<Option<usize>>>,
        ) -> Self {
            Self {
                id,
                name: name.into(),
                in_channels: InChannels::default(),
                out_channels: OutChannels::default(),
                restored_count,
            }
        }
    }

    #[async_trait]
    impl Node for RestoreProbeNode {
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

        async fn run(&mut self, _: Arc<EnvVar>) -> Output {
            Output::empty()
        }

        fn restore_from_checkpoint(&mut self, loop_count: usize) -> DagrsResult<()> {
            *self.restored_count.lock().unwrap() = Some(loop_count);
            Ok(())
        }
    }

    impl FixedIdNode {
        fn new(id: NodeId, name: impl Into<NodeName>) -> Self {
            Self {
                id,
                name: name.into(),
                in_channels: InChannels::default(),
                out_channels: OutChannels::default(),
            }
        }
    }

    #[async_trait]
    impl Node for FixedIdNode {
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

        async fn run(&mut self, _: Arc<EnvVar>) -> Output {
            Output::empty()
        }
    }

    /// Test for execute a graph.
    ///
    /// Step 1: create a graph and two DefaultNode.
    ///
    /// Step 2: add the nodes to graph.
    ///
    /// Step 3: add the edge between Node X and "Node Y.
    ///
    /// Step 4: Run the graph and verify the output saved in the graph structure.

    #[tokio::test]
    async fn test_graph_execution() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();

        let node_name = "Node X";
        let node = DefaultNode::new(NodeName::from(node_name), &mut node_table);
        let node_id = node.id();

        let node1_name = "Node Y";
        let node1 = DefaultNode::with_action(
            NodeName::from(node1_name),
            HelloAction::new(),
            &mut node_table,
        );
        let node1_id = node1.id();

        graph.add_node(node).unwrap();
        graph.add_node(node1).unwrap();

        graph.add_edge(node_id, vec![node1_id]).unwrap();

        match graph.async_start().await {
            Ok(_) => {
                let out = graph.execute_states[&node1_id].get_output().unwrap();
                let out: &String = out.get().unwrap();
                assert_eq!(out, "Hello world");
            }
            Err(e) => {
                eprintln!("Graph execution failed: {:?}", e);
            }
        }
    }

    /// A test condition that always fails.
    ///
    /// This condition is used in tests to verify the behavior of conditional nodes
    /// when their condition evaluates to false. The `run` method always returns false,
    /// simulating a failing condition.
    struct FailingCondition;
    #[async_trait::async_trait]
    impl Condition for FailingCondition {
        async fn run(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> bool {
            false
        }
    }

    /// Step 1: Create a new graph and node table.
    ///
    /// Step 2: Create two nodes - a conditional node that will fail and a hello action node.
    ///
    /// Step 3: Add nodes to graph and set up dependencies between them.
    ///
    /// Step 4: Run the graph and verify the conditional node fails as expected.
    #[tokio::test]
    async fn test_conditional_execution() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();

        // Create conditional node that will fail
        let node_a_name = "Node A";
        let node_a = ConditionalNode::with_condition(
            NodeName::from(node_a_name),
            FailingCondition,
            &mut node_table,
        );
        let node_a_id = node_a.id();

        // Create hello action node
        let node_b_name = "Node B";
        let node_b = DefaultNode::with_action(
            NodeName::from(node_b_name),
            HelloAction::new(),
            &mut node_table,
        );
        let node_b_id = node_b.id();

        // Add nodes to graph
        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();

        // Add edge from A to B
        graph.add_edge(node_a_id, vec![node_b_id]).unwrap();

        // Execute graph
        let report = graph.async_start().await.unwrap();
        assert_eq!(report.status, CompletionStatus::Succeeded);
        assert!(graph.execute_states[&node_a_id].is_success());
        assert!(graph.execute_states[&node_b_id].get_output().is_none());
    }

    #[tokio::test]
    async fn test_add_edge_works_in_async_context() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();

        let node_a = DefaultNode::new(NodeName::from("Node A"), &mut node_table);
        let node_b = DefaultNode::new(NodeName::from("Node B"), &mut node_table);

        let node_a_id = node_a.id();
        let node_b_id = node_b.id();

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_edge(node_a_id, vec![node_b_id]).unwrap();

        let result = graph.async_start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_add_edge_supports_multiple_loop_subgraph_targets() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();

        let source = DefaultNode::new(NodeName::from("Source"), &mut node_table);
        let source_id = source.id();
        let inner_a = DefaultNode::new(NodeName::from("Inner A"), &mut node_table);
        let inner_a_id = inner_a.id();
        let inner_b = DefaultNode::new(NodeName::from("Inner B"), &mut node_table);
        let inner_b_id = inner_b.id();

        let mut loop_subgraph = LoopSubgraph::new(NodeName::from("Loop"), &mut node_table);
        loop_subgraph.add_node(inner_a).unwrap();
        loop_subgraph.add_node(inner_b).unwrap();

        graph.add_node(source).unwrap();
        graph.add_node(loop_subgraph).unwrap();
        graph
            .add_edge(source_id, vec![inner_a_id, inner_b_id])
            .expect("fan-out into folded loop subgraph targets should succeed");

        let source_node = graph.nodes.get(&source_id).unwrap().clone();
        let mut receiver_ids = {
            let mut source_guard = source_node.lock().await;
            source_guard.output_channels().get_receiver_ids()
        };
        receiver_ids.sort_unstable();
        assert_eq!(receiver_ids, vec![inner_a_id, inner_b_id]);

        for inner_id in [inner_a_id, inner_b_id] {
            let inner_node = graph.nodes.get(&inner_id).unwrap().clone();
            let sender_ids = {
                let mut inner_guard = inner_node.lock().await;
                inner_guard.input_channels().get_sender_ids()
            };
            assert_eq!(sender_ids, vec![source_id]);
        }

        let abstract_edges = graph.abstract_graph.edges.get(&source_id).unwrap();
        assert_eq!(
            abstract_edges.len(),
            1,
            "the abstract graph should fold both concrete targets into one loop-subgraph edge",
        );
    }

    #[test]
    fn test_add_loop_subgraph_is_atomic_on_duplicate_member_id() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();
        let existing_id = NodeId(41);

        graph
            .add_node(FixedIdNode::new(existing_id, "existing"))
            .unwrap();
        let mut loop_subgraph = LoopSubgraph::new(NodeName::from("Loop"), &mut node_table);
        let loop_id = loop_subgraph.id();
        loop_subgraph
            .add_node(FixedIdNode::new(existing_id, "duplicate"))
            .unwrap();
        loop_subgraph
            .add_node(FixedIdNode::new(NodeId(42), "inner"))
            .unwrap();

        let err = graph
            .add_node(loop_subgraph)
            .expect_err("duplicate loop member ids should fail before mutating graph state");

        assert_eq!(err.code, ErrorCode::DgBld0003DuplicateNodeId);
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(graph.abstract_graph.size(), 1);
        assert!(graph.abstract_graph.unfold_node(loop_id).is_none());
        assert!(
            graph
                .abstract_graph
                .get_abstract_node_id(&NodeId(42))
                .is_none()
        );
    }

    #[test]
    fn test_rebuild_active_nodes_from_checkpoint_without_snapshot() {
        let mut checkpoint = Checkpoint::with_id("rebuild_active_nodes", 0, 0);
        checkpoint.add_node_state(NodeState::pending(1));
        checkpoint.add_node_state(NodeState::failed(2));
        checkpoint.add_node_state(NodeState::succeeded(3));
        checkpoint.add_node_state(NodeState::skipped(4));

        let mut active_nodes: Vec<_> = Graph::rebuild_active_nodes_from_checkpoint(&checkpoint)
            .into_iter()
            .map(|id| id.as_usize())
            .collect();
        active_nodes.sort_unstable();

        assert_eq!(active_nodes, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn test_validate_checkpoint_rejects_out_of_bounds_pc() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();
        let node = DefaultNode::new(NodeName::from("Node A"), &mut node_table);
        let node_id = node.id();

        graph.add_node(node).unwrap();
        graph.init();
        graph.check_loop_and_partition().await.unwrap();

        let mut checkpoint = Checkpoint::with_id("invalid_pc", graph.blocks.len(), 0);
        checkpoint.active_nodes.insert(node_id.as_usize());
        checkpoint.add_node_state(NodeState::pending(node_id.as_usize()));

        let err = graph
            .validate_checkpoint(&checkpoint)
            .expect_err("checkpoint pc should be rejected when it is out of bounds");
        assert_eq!(err.code, ErrorCode::DgChk0003InvalidCheckpoint);
    }

    #[tokio::test]
    async fn test_save_checkpoint_keeps_unserializable_success_as_succeeded() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();
        let node = DefaultNode::with_action(
            NodeName::from("NonSerializable"),
            NonSerializableAction,
            &mut node_table,
        );
        let node_id = node.id();

        graph.add_node(node).unwrap();
        graph.set_checkpoint_store(Box::new(MemoryCheckpointStore::new()));
        graph.async_start().await.unwrap();

        let active_nodes = graph.nodes.keys().copied().collect();
        let checkpoint_id = graph.save_checkpoint(0, 0, &active_nodes).await.unwrap();
        let checkpoint = graph.load_checkpoint(&checkpoint_id).await.unwrap();
        let node_state = checkpoint
            .node_states
            .get(&node_id.as_usize())
            .expect("checkpoint should capture the node state");

        assert_eq!(node_state.status, NodeExecStatus::Succeeded);
        assert!(node_state.output_data.is_none());
    }

    #[tokio::test]
    async fn test_restore_checkpoint_uses_per_loop_node_iterations() {
        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();
        let first_id = node_table.alloc_id_for("loop-a");
        let second_id = node_table.alloc_id_for("loop-b");

        let first_restored = Arc::new(StdMutex::new(None));
        let second_restored = Arc::new(StdMutex::new(None));

        graph
            .add_node(RestoreProbeNode::new(
                first_id,
                "loop-a",
                first_restored.clone(),
            ))
            .unwrap();
        graph
            .add_node(RestoreProbeNode::new(
                second_id,
                "loop-b",
                second_restored.clone(),
            ))
            .unwrap();
        graph.init();

        let mut checkpoint = Checkpoint::with_id("per_loop_restore", 0, 9);
        checkpoint.add_node_state(NodeState::pending(first_id.as_usize()));
        checkpoint.add_node_state(NodeState::pending(second_id.as_usize()));
        checkpoint.add_metadata(
            Graph::LOOP_NODE_ITERATIONS_METADATA_KEY,
            serde_json::to_string(&HashMap::from([(first_id.as_usize(), 3usize)])).unwrap(),
        );

        let active_nodes = Graph::rebuild_active_nodes_from_checkpoint(&checkpoint);
        graph
            .restore_checkpoint_state(&checkpoint, &active_nodes)
            .await
            .unwrap();

        assert_eq!(*first_restored.lock().unwrap(), Some(3));
        assert_eq!(*second_restored.lock().unwrap(), Some(0));
    }

    #[tokio::test]
    async fn test_restore_checkpoint_does_not_replay_inputs_to_completed_receivers() {
        struct AlwaysTrueCondition;

        #[async_trait]
        impl Condition for AlwaysTrueCondition {
            async fn run(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> bool {
                true
            }
        }

        let mut graph = Graph::new();
        let mut node_table = NodeTable::new();

        let node_a = DefaultNode::with_action(
            NodeName::from("Node A"),
            HelloAction::new(),
            &mut node_table,
        );
        let node_a_id = node_a.id();
        let node_b = DefaultNode::new(NodeName::from("Node B"), &mut node_table);
        let node_b_id = node_b.id();
        let gate = ConditionalNode::with_condition(
            NodeName::from("Gate"),
            AlwaysTrueCondition,
            &mut node_table,
        );
        let gate_id = gate.id();
        let node_c = DefaultNode::new(NodeName::from("Node C"), &mut node_table);
        let node_c_id = node_c.id();

        graph.add_node(node_a).unwrap();
        graph.add_node(node_b).unwrap();
        graph.add_node(gate).unwrap();
        graph.add_node(node_c).unwrap();
        graph.add_edge(node_a_id, vec![node_b_id]).unwrap();
        graph.add_edge(node_b_id, vec![gate_id]).unwrap();
        graph.add_edge(gate_id, vec![node_c_id]).unwrap();

        graph.init();
        graph.check_loop_and_partition().await.unwrap();
        assert_eq!(graph.blocks.len(), 2);

        let mut checkpoint = Checkpoint::with_id("completed_receivers", 1, 0);
        checkpoint.active_nodes.extend([
            node_a_id.as_usize(),
            node_b_id.as_usize(),
            gate_id.as_usize(),
            node_c_id.as_usize(),
        ]);
        checkpoint.add_node_state(
            NodeState::succeeded(node_a_id.as_usize())
                .with_output_kind(StoredOutputKind::String)
                .with_output_data(serde_json::to_vec(&"Hello world".to_string()).unwrap()),
        );
        checkpoint.add_node_state(NodeState::succeeded(node_b_id.as_usize()));
        checkpoint.add_node_state(NodeState::succeeded(gate_id.as_usize()));
        checkpoint.add_node_state(NodeState::pending(node_c_id.as_usize()));

        let active_nodes = Graph::rebuild_active_nodes_from_checkpoint(&checkpoint);
        graph
            .restore_checkpoint_state(&checkpoint, &active_nodes)
            .await
            .unwrap();

        let node_b = graph.nodes.get(&node_b_id).unwrap().clone();
        let replay_result = {
            let mut node_b_guard = node_b.lock().await;
            tokio::time::timeout(
                Duration::from_millis(50),
                node_b_guard.input_channels().recv_from(&node_a_id),
            )
            .await
        };

        assert!(
            replay_result.is_err(),
            "completed downstream nodes should not receive replayed inputs from checkpoints",
        );
    }
}
