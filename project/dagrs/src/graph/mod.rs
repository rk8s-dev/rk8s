mod abstract_graph;
pub mod error;
pub mod event;
pub mod loop_subgraph;

use std::hash::Hash;
use std::sync::atomic::Ordering;
use std::{
    collections::{HashMap, HashSet},
    panic::{self, AssertUnwindSafe},
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use crate::{
    Output,
    connection::{in_channel::InChannel, information_packet::Content, out_channel::OutChannel},
    graph::event::GraphEvent,
    node::{Node, NodeId, NodeTable},
    utils::checkpoint::{
        Checkpoint, CheckpointConfig, CheckpointError, CheckpointStore, NodeState,
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
use error::GraphError;

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
    /// * `Err(CheckpointError)` - If saving fails
    pub async fn save_checkpoint(
        &self,
        pc: usize,
        loop_count: usize,
        active_nodes: &HashSet<NodeId>,
    ) -> Result<String, CheckpointError> {
        let store = self
            .checkpoint_store
            .as_ref()
            .ok_or(CheckpointError::StoreNotConfigured)?;

        let mut checkpoint = Checkpoint::new(pc, loop_count);
        checkpoint.set_active_nodes(active_nodes);

        // Capture node execution states with output data
        for (node_id, exec_state) in &self.execute_states {
            let output = exec_state.get_full_output();
            let completed = !output.is_empty();
            let success = !output.is_err();

            let mut node_state = if completed {
                NodeState::completed(node_id.0, success)
            } else {
                NodeState::pending(node_id.0)
            };

            // Try to capture output summary for debugging
            if let Some(content) = output.get_out() {
                // Try to get a debug representation of common types
                let summary = Self::get_output_summary(&content);
                if let Some(s) = summary {
                    node_state = node_state.with_summary(s);
                }

                // Try to serialize if the content is a serializable primitive
                if let Some(data) = Self::try_serialize_output(&content) {
                    node_state = node_state.with_output_data(data);
                }
            } else if let Some(err) = output.get_err() {
                node_state = node_state.with_summary(format!("Error: {}", err));
            }

            checkpoint.add_node_state(node_state);
        }

        // Add metadata
        checkpoint.add_metadata("node_count", self.node_count.to_string());
        checkpoint.add_metadata("blocks_count", self.blocks.len().to_string());

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
    fn try_serialize_output(content: &Content) -> Option<Vec<u8>> {
        // Try common serializable types
        if let Some(v) = content.get::<String>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<i32>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<i64>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<u32>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<u64>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<f64>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<bool>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<Vec<String>>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<Vec<i32>>() {
            return serde_json::to_vec(v).ok();
        }
        if let Some(v) = content.get::<Vec<i64>>() {
            return serde_json::to_vec(v).ok();
        }
        None
    }

    /// Load a checkpoint by ID.
    ///
    /// # Arguments
    /// * `checkpoint_id` - The ID of the checkpoint to load
    ///
    /// # Returns
    /// * `Ok(Checkpoint)` - The loaded checkpoint
    /// * `Err(CheckpointError)` - If loading fails
    pub async fn load_checkpoint(
        &self,
        checkpoint_id: &str,
    ) -> Result<Checkpoint, CheckpointError> {
        let store = self
            .checkpoint_store
            .as_ref()
            .ok_or(CheckpointError::StoreNotConfigured)?;
        store.load(&checkpoint_id.to_string()).await
    }

    /// Get the latest checkpoint.
    ///
    /// # Returns
    /// * `Ok(Some(Checkpoint))` - The latest checkpoint if one exists
    /// * `Ok(None)` - If no checkpoints exist
    /// * `Err(CheckpointError)` - If loading fails
    pub async fn get_latest_checkpoint(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        let store = self
            .checkpoint_store
            .as_ref()
            .ok_or(CheckpointError::StoreNotConfigured)?;
        store.latest().await
    }

    /// List all available checkpoint IDs.
    pub async fn list_checkpoints(&self) -> Result<Vec<String>, CheckpointError> {
        let store = self
            .checkpoint_store
            .as_ref()
            .ok_or(CheckpointError::StoreNotConfigured)?;
        store.list().await
    }

    /// Delete a checkpoint by ID.
    pub async fn delete_checkpoint(&self, checkpoint_id: &str) -> Result<(), CheckpointError> {
        let store = self
            .checkpoint_store
            .as_ref()
            .ok_or(CheckpointError::StoreNotConfigured)?;
        store.delete(&checkpoint_id.to_string()).await
    }

    /// Clean up old checkpoints to maintain the max_checkpoints limit.
    async fn cleanup_old_checkpoints(
        &self,
        store: &dyn CheckpointStore,
    ) -> Result<(), CheckpointError> {
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
        checkpoints.sort_by_key(|c| c.timestamp);

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

    /// Resume graph execution from a checkpoint.
    ///
    /// This method:
    /// 1. Loads the checkpoint
    /// 2. Restores execution state
    /// 3. Continues execution from the saved point
    ///
    /// # Arguments
    /// * `checkpoint_id` - The ID of the checkpoint to resume from
    ///
    /// # Returns
    /// * `Ok(())` - If execution completes successfully
    /// * `Err(GraphError)` - If execution fails
    ///
    /// # Example
    /// ```ignore
    /// // Resume from a specific checkpoint
    /// graph.resume_from_checkpoint("ckpt_123").await?;
    ///
    /// // Or resume from the latest checkpoint
    /// if let Some(checkpoint) = graph.get_latest_checkpoint().await? {
    ///     graph.resume_from_checkpoint(&checkpoint.id).await?;
    /// }
    /// ```
    pub async fn resume_from_checkpoint(&mut self, checkpoint_id: &str) -> Result<(), GraphError> {
        let checkpoint = self
            .load_checkpoint(checkpoint_id)
            .await
            .map_err(|e| GraphError::CheckpointError(e.to_string()))?;

        info!(
            "Resuming from checkpoint: {} (pc={}, loop_count={})",
            checkpoint.id, checkpoint.pc, checkpoint.loop_count
        );

        // Emit event
        let _ = self.event_sender.send(GraphEvent::CheckpointRestored {
            checkpoint_id: checkpoint.id.clone(),
            pc: checkpoint.pc,
        });

        // Initialize if not already done
        if self.blocks.is_empty() {
            self.init();
            let is_loop = self.check_loop_and_partition().await;
            if is_loop {
                return Err(GraphError::GraphLoopDetected);
            }
        }

        // Restore active nodes
        let active_nodes = checkpoint.get_active_nodes();

        // Restore execution states for completed nodes
        for (node_id_val, node_state) in &checkpoint.node_states {
            let node_id = NodeId(*node_id_val);
            if let Some(exec_state) = self.execute_states.get(&node_id)
                && node_state.completed
            {
                if node_state.success {
                    exec_state.exe_success();
                } else {
                    exec_state.exe_fail();
                }
            }
        }

        // Continue execution from checkpoint
        self.run_from_checkpoint(checkpoint.pc, checkpoint.loop_count, active_nodes)
            .await
    }

    /// Internal method to run from a specific checkpoint state.
    async fn run_from_checkpoint(
        &mut self,
        start_pc: usize,
        start_loop_count: usize,
        initial_active_nodes: HashSet<NodeId>,
    ) -> Result<(), GraphError> {
        let condition_flag = Arc::new(Mutex::new(true));
        let errors = Arc::new(Mutex::new(Vec::new()));

        let mut pc = start_pc;
        let mut loop_count = start_loop_count;
        let mut active_nodes = initial_active_nodes;

        // Build parents map for pruning logic
        let mut parents_map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (parent, children) in &self.abstract_graph.edges {
            for child in children {
                parents_map.entry(*child).or_default().push(*parent);
            }
        }

        // Track nodes completed since last checkpoint
        let mut nodes_since_checkpoint = 0usize;
        let checkpoint_start_time = std::time::Instant::now();

        // Start the nodes by blocks
        while pc < self.blocks.len() {
            // Check if we should create a checkpoint
            if self.checkpoint_config.enabled && self.checkpoint_store.is_some() {
                let should_checkpoint = self.should_create_checkpoint(
                    nodes_since_checkpoint,
                    checkpoint_start_time.elapsed().as_secs(),
                );
                if should_checkpoint {
                    if let Err(e) = self.save_checkpoint(pc, loop_count, &active_nodes).await {
                        error!("Failed to save automatic checkpoint: {}", e);
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

            // Handle skipped nodes
            for node_id in skipped_block_nodes {
                let _ = self
                    .event_sender
                    .send(GraphEvent::NodeSkipped { id: node_id });
                debug!("Skipped node [id: {}]", node_id.0);

                // Hook: on_skip
                if let Some(node) = self.nodes.get(&node_id) {
                    let node_guard = node.lock().await;
                    let hooks_guard = self.hooks.read().await;
                    for hook in hooks_guard.iter() {
                        hook.on_skip(&*node_guard, &self.env).await;
                    }
                }
            }

            if active_block_nodes.is_empty() {
                pc += 1;
                continue;
            }

            let mut tasks = vec![];
            for node_id in active_block_nodes.iter().cloned() {
                let node = self
                    .nodes
                    .get(&node_id)
                    .ok_or(GraphError::NodeIdError(node_id.0))?;
                let execute_state = self
                    .execute_states
                    .get(&node_id)
                    .ok_or(GraphError::NodeIdError(node_id.0))?
                    .clone();
                let env = Arc::clone(&self.env);
                let node = Arc::clone(node);
                let condition_flag = condition_flag.clone();
                let errors = errors.clone();
                let hooks = self.hooks.clone();
                let event_sender = self.event_sender.clone();

                let task = task::spawn({
                    let errors = Arc::clone(&errors);
                    async move {
                        let node_ref: Arc<Mutex<dyn Node>> = node.clone();
                        let node_guard = node.lock().await;
                        let node_name = node_guard.name().to_string();
                        let node_id = node_guard.id();
                        let id_val = node_id.0;
                        let max_retries = node_guard.max_retries();

                        // Hook: before_node_run
                        {
                            let hooks_guard = hooks.read().await;
                            for hook in hooks_guard.iter() {
                                hook.before_node_run(&*node_guard, &env).await;
                            }
                        }
                        // Event: NodeStart
                        let _ = event_sender.send(GraphEvent::NodeStart {
                            id: node_id,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        });

                        // Execute with retry logic
                        let mut attempt = 0u32;

                        // Drop guard before retry loop to allow re-acquisition
                        drop(node_guard);

                        let final_output = loop {
                            // Re-acquire lock for each attempt
                            let mut node_guard = node_ref.lock().await;
                            let retry_delay = node_guard.retry_delay_ms(attempt + 1);

                            let env_for_run = env.clone();

                            // Run the node with panic protection
                            let result = panic::catch_unwind(AssertUnwindSafe(|| async move {
                                node_guard.run(env_for_run).await
                            }));

                            match result {
                                Ok(future) => {
                                    let out = future.await;

                                    if out.is_err() {
                                        let error_msg = out.get_err().unwrap_or_default();

                                        // Check if we should retry
                                        if attempt < max_retries {
                                            attempt += 1;

                                            // Emit NodeRetry event
                                            let _ = event_sender.send(GraphEvent::NodeRetry {
                                                id: node_id,
                                                attempt,
                                                max_retries,
                                                error: error_msg.clone(),
                                            });

                                            // Call on_retry hook and check decision
                                            let err_obj = GraphError::ExecutionFailed {
                                                node_name: node_name.clone(),
                                                node_id: id_val,
                                                error: error_msg.clone(),
                                            };

                                            let mut should_retry = true;
                                            {
                                                let node_guard = node_ref.lock().await;
                                                let hooks_guard = hooks.read().await;
                                                for hook in hooks_guard.iter() {
                                                    let decision = hook
                                                        .on_retry(
                                                            &*node_guard,
                                                            &err_obj,
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
                                                    id_val,
                                                    attempt,
                                                    max_retries,
                                                    retry_delay,
                                                    error_msg
                                                );
                                                tokio::time::sleep(Duration::from_millis(
                                                    retry_delay,
                                                ))
                                                .await;
                                                continue;
                                            }
                                        }

                                        // No more retries, fail
                                        break Ok(out);
                                    } else {
                                        // Success
                                        break Ok(out);
                                    }
                                }
                                Err(panic_err) => {
                                    // Panic occurred - don't retry panics
                                    break Err(panic_err);
                                }
                            }
                        };

                        // Process the final output
                        match final_output {
                            Ok(out) => {
                                let node_guard = node_ref.lock().await;

                                // Hook: after_node_run
                                {
                                    let hooks_guard = hooks.read().await;
                                    for hook in hooks_guard.iter() {
                                        hook.after_node_run(&*node_guard, &out, &env).await;
                                    }
                                }

                                if out.is_err() {
                                    let error_msg = out.get_err().unwrap_or_default();
                                    error!(
                                        "Execution failed [name: {}, id: {}] - {} (after {} retries)",
                                        node_name, id_val, error_msg, attempt
                                    );
                                    let _ = event_sender.send(GraphEvent::NodeFailed {
                                        id: node_id,
                                        error: error_msg.clone(),
                                    });

                                    let err_obj = GraphError::ExecutionFailed {
                                        node_name: node_name.clone(),
                                        node_id: id_val,
                                        error: error_msg.clone(),
                                    };
                                    {
                                        let hooks_guard = hooks.read().await;
                                        for hook in hooks_guard.iter() {
                                            hook.on_error(&err_obj, &env).await;
                                        }
                                    }

                                    execute_state.set_output(out.clone());
                                    execute_state.exe_fail();
                                    let mut errors_lock = errors.lock().await;
                                    errors_lock.push(err_obj);
                                } else {
                                    if let Some(false) = out.conditional_result() {
                                        let mut cf = condition_flag.lock().await;
                                        *cf = false;
                                        info!(
                                            "Condition failed on [name: {}, id: {}]. The rest nodes will abort.",
                                            node_name, id_val,
                                        )
                                    }
                                    let _ =
                                        event_sender.send(GraphEvent::NodeSuccess { id: node_id });

                                    execute_state.set_output(out.clone());
                                    execute_state.exe_success();
                                    debug!(
                                        "Execution succeed [name: {}, id: {}]{}",
                                        node_name,
                                        id_val,
                                        if attempt > 0 {
                                            format!(" (after {} retries)", attempt)
                                        } else {
                                            String::new()
                                        }
                                    );
                                }

                                (node_id, out)
                            }
                            Err(_) => {
                                // Panic occurred
                                let mut node_guard = node_ref.lock().await;
                                node_guard.input_channels().close_all();
                                node_guard.output_channels().close_all();

                                error!("Execution panic [name: {}, id: {}]", node_name, id_val);
                                let _ = event_sender.send(GraphEvent::NodeFailed {
                                    id: node_id,
                                    error: "Panic".to_string(),
                                });

                                execute_state.set_output(Output::Err("Panic".to_string()));
                                execute_state.exe_fail();

                                let err_obj = GraphError::PanicOccurred {
                                    node_name: node_name.clone(),
                                    node_id: id_val,
                                };
                                {
                                    let hooks_guard = hooks.read().await;
                                    for hook in hooks_guard.iter() {
                                        hook.on_error(&err_obj, &env).await;
                                    }
                                }

                                let mut errors_lock = errors.lock().await;
                                errors_lock.push(err_obj);
                                (node_id, Output::Err("Panic".to_string()))
                            }
                        }
                    }
                });
                tasks.push(task);
            }

            let results: Vec<Result<(NodeId, Output), tokio::task::JoinError>> =
                futures::future::join_all(tasks).await;

            // Update nodes completed count
            nodes_since_checkpoint += active_block_nodes.len();

            // Check for errors immediately
            let errors_guard = errors.lock().await;
            if !errors_guard.is_empty() {
                // Save checkpoint before failing
                if self.checkpoint_config.enabled && self.checkpoint_store.is_some() {
                    let _ = self.save_checkpoint(pc, loop_count, &active_nodes).await;
                }

                if errors_guard.len() == 1 {
                    return Err(errors_guard[0].clone());
                } else {
                    return Err(GraphError::MultipleErrors(errors_guard.clone()));
                }
            }
            drop(errors_guard);

            if !(*condition_flag.lock().await) {
                break;
            }

            let mut next_pc = pc + 1;
            let mut should_abort = false;

            for (node_id, output) in results.into_iter().flatten() {
                if self.handle_flow_control(
                    output,
                    node_id,
                    &mut active_nodes,
                    &self.node_block_map,
                    &parents_map,
                    &mut next_pc,
                    self.blocks.len(),
                )? {
                    should_abort = true;
                    break;
                }
            }

            if should_abort {
                break;
            }

            // Check for loop (backward jump)
            if next_pc < pc {
                loop_count += 1;
                if loop_count >= self.max_loop_count {
                    return Err(GraphError::LoopLimitExceeded(self.max_loop_count));
                }

                // Save checkpoint on loop iteration if configured
                if self.checkpoint_config.enabled
                    && self.checkpoint_config.on_loop_iteration
                    && self.checkpoint_store.is_some()
                {
                    let _ = self
                        .save_checkpoint(next_pc, loop_count, &active_nodes)
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

        let _ = self.event_sender.send(GraphEvent::GraphFinished);

        self.is_active
            .store(false, std::sync::atomic::Ordering::Relaxed);

        Ok(())
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
    pub async fn reset(&mut self) {
        self.execute_states = HashMap::new();
        self.env = Arc::new(EnvVar::new(NodeTable::default()));
        self.is_active = Arc::new(AtomicBool::new(true));
        self.blocks.clear();
        self.node_block_map.clear();

        // Re-create channels for all edges to support graph reuse
        // 1. Clear existing channels
        for node in self.nodes.values() {
            let mut node = node.lock().await;
            node.input_channels().0.clear();
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

    /// Adds a new node to the `Graph`
    pub fn add_node(&mut self, node: impl Node + 'static) {
        if let Some(loop_structure) = node.loop_structure() {
            // Expand loop subgraph, and update concrete node id -> abstract node id mapping in abstract_graph
            let abstract_node_id = node.id();

            log::debug!("Add node {:?} to abstract graph", abstract_node_id);
            self.abstract_graph.add_folded_node(
                abstract_node_id,
                loop_structure
                    .iter()
                    .map(|n| n.blocking_lock().id())
                    .collect(),
            );

            for node in loop_structure {
                let concrete_id = node.blocking_lock().id();
                log::debug!("Add node {:?} to concrete graph", concrete_id);
                self.nodes.insert(concrete_id, node.clone());
            }
        } else {
            let id = node.id();
            let node = Arc::new(Mutex::new(node));
            self.node_count += 1;
            self.nodes.insert(id, node);
            self.in_degree.insert(id, 0);
            self.abstract_graph.add_node(id);

            log::debug!("Add node {:?} to concrete & abstract graph", id);
        }
    }
    /// Adds an edge between two nodes in the `Graph`.
    /// If the outgoing port of the sending node is empty and the number of receiving nodes is > 1, use the broadcast channel
    /// An MPSC channel is used if the outgoing port of the sending node is empty and the number of receiving nodes is equal to 1
    /// If the outgoing port of the sending node is not empty, adding any number of receiving nodes will change all relevant channels to broadcast
    pub fn add_edge(&mut self, from_id: NodeId, all_to_ids: Vec<NodeId>) {
        let to_ids = Self::remove_duplicates(all_to_ids);
        let mut rx_map: HashMap<NodeId, mpsc::Receiver<Content>> = HashMap::new();

        // Update channels
        {
            let from_node_lock = self.nodes.get_mut(&from_id).unwrap();
            let mut from_node = from_node_lock.blocking_lock();
            let from_channel = from_node.output_channels();

            for to_id in &to_ids {
                if !from_channel.0.contains_key(to_id) {
                    let (tx, rx) = mpsc::channel::<Content>(32);
                    from_channel.insert(*to_id, Arc::new(Mutex::new(OutChannel::Mpsc(tx.clone()))));
                    rx_map.insert(*to_id, rx);
                    self.in_degree
                        .entry(*to_id)
                        .and_modify(|e| *e += 1)
                        .or_insert(0);

                    // Update abstract graph
                    self.abstract_graph.add_edge(from_id, *to_id);
                }
            }
        }
        for to_id in &to_ids {
            if let Some(to_node_lock) = self.nodes.get_mut(to_id) {
                let mut to_node = to_node_lock.blocking_lock();
                let to_channel = to_node.input_channels();
                if let Some(rx) = rx_map.remove(to_id) {
                    to_channel.insert(from_id, Arc::new(Mutex::new(InChannel::Mpsc(rx))));
                }
            }
        }
    }

    /// Initializes the network, setting up the nodes.
    pub(crate) fn init(&mut self) {
        self.execute_states.reserve(self.nodes.len());
        self.nodes.keys().for_each(|node| {
            self.execute_states
                .insert(*node, Arc::new(ExecState::new()));
        });
    }

    /// This function is used for the execution of a single dag.
    pub fn start(&mut self) -> Result<(), GraphError> {
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| GraphError::RuntimeCreationFailed(e.to_string()))?;
        runtime.block_on(async { self.async_start().await })
    }
    /// Executes a single DAG within an existing async runtime.
    ///
    /// Use this method when you are already running inside an async context
    /// (for example, inside a `tokio::main` function or a task spawned on a
    /// Tokio runtime) and you do **not** want `Graph` to create and manage
    /// its own Tokio runtime.
    ///
    /// Unlike [`Graph::start`], this method:
    /// - Does not create a new Tokio runtime.
    /// - Assumes it is called on a thread where a Tokio runtime is already
    ///   active.
    /// - Can be `await`-ed like any other async function.
    ///
    /// # Requirements
    ///
    /// - A Tokio runtime must be active on the current thread when this
    ///   method is called.
    /// - The graph must have been properly configured (nodes and edges
    ///   added) before calling this method.
    ///
    /// If those conditions are not met, execution may fail at runtime.
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
    pub async fn async_start(&mut self) -> Result<(), GraphError> {
        self.init();
        let is_loop = self.check_loop_and_partition().await;
        if is_loop {
            return Err(GraphError::GraphLoopDetected);
        }

        if !self.is_active.load(Ordering::Relaxed) {
            return Err(GraphError::GraphNotActive);
        }
        self.run().await
    }

    /// Executes the graph's nodes in a concurrent manner, respecting the block structure.
    ///
    /// - Executes nodes in blocks, where blocks are separated by conditional nodes
    /// - Runs nodes within each block concurrently using Tokio tasks
    /// - Handles node execution failures and panics
    /// - Supports conditional execution - if a conditional node returns false, remaining blocks are aborted
    /// - Tracks execution state and errors for each node
    ///
    /// # Returns
    /// - `Ok(())` if all nodes execute successfully
    /// - `Err(GraphError)` if any node fails or panics during execution
    ///   - Returns single error if only one failure occurs
    ///   - Returns `MultipleErrors` if multiple nodes fail
    async fn run(&mut self) -> Result<(), GraphError> {
        let condition_flag = Arc::new(Mutex::new(true));
        let errors = Arc::new(Mutex::new(Vec::new()));

        // Reset all nodes
        for node in self.nodes.values() {
            let mut node = node.lock().await;
            node.reset();
        }

        let mut pc = 0;
        let mut loop_count = 0;
        let mut active_nodes: HashSet<NodeId> = self.nodes.keys().cloned().collect();

        // Build parents map for pruning logic
        let mut parents_map: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (parent, children) in &self.abstract_graph.edges {
            for child in children {
                parents_map.entry(*child).or_default().push(*parent);
            }
        }

        // Start the nodes by blocks
        while pc < self.blocks.len() {
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

            // Handle skipped nodes
            // Note: We do NOT close output channels here to support loop execution.
            // Channels will be closed when the entire graph finishes.
            for node_id in skipped_block_nodes {
                let _ = self
                    .event_sender
                    .send(GraphEvent::NodeSkipped { id: node_id });
                debug!("Skipped node [id: {}]", node_id.0);

                // Hook: on_skip
                if let Some(node) = self.nodes.get(&node_id) {
                    let node_guard = node.lock().await;
                    let hooks_guard = self.hooks.read().await;
                    for hook in hooks_guard.iter() {
                        hook.on_skip(&*node_guard, &self.env).await;
                    }
                }
            }

            if active_block_nodes.is_empty() {
                pc += 1;
                continue;
            }

            let mut tasks = vec![];
            for node_id in active_block_nodes {
                let node = self
                    .nodes
                    .get(&node_id)
                    .ok_or(GraphError::NodeIdError(node_id.0))?;
                let execute_state = self
                    .execute_states
                    .get(&node_id)
                    .ok_or(GraphError::NodeIdError(node_id.0))?
                    .clone();
                let env = Arc::clone(&self.env);
                let node = Arc::clone(node);
                let condition_flag = condition_flag.clone();
                let errors = errors.clone();
                let hooks = self.hooks.clone();
                let event_sender = self.event_sender.clone();

                let task = task::spawn({
                    let errors = Arc::clone(&errors);
                    async move {
                        let node_ref: Arc<Mutex<dyn Node>> = node.clone();
                        let node_guard = node.lock().await;
                        let node_name = node_guard.name().to_string();
                        let node_id = node_guard.id();
                        let id_val = node_id.0;
                        let max_retries = node_guard.max_retries();

                        // Hook: before_node_run
                        {
                            let hooks_guard = hooks.read().await;
                            for hook in hooks_guard.iter() {
                                hook.before_node_run(&*node_guard, &env).await;
                            }
                        }
                        // Event: NodeStart
                        let _ = event_sender.send(GraphEvent::NodeStart {
                            id: node_id,
                            timestamp: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs(),
                        });

                        // Drop guard before retry loop
                        drop(node_guard);

                        // Execute with retry logic
                        let mut attempt = 0u32;

                        let final_output = loop {
                            // Re-acquire lock for each attempt
                            let mut node_guard = node_ref.lock().await;
                            let retry_delay = node_guard.retry_delay_ms(attempt + 1);

                            let env_for_run = env.clone();
                            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                                // We need to wrap node_guard in a way that can be moved
                                // Since we can't move the guard into the closure, execute directly
                                async move { node_guard.run(env_for_run).await }
                            }));

                            match result {
                                Ok(future) => {
                                    let out = future.await;

                                    if out.is_err() {
                                        let error_msg = out.get_err().unwrap_or_default();

                                        // Check if we should retry
                                        if attempt < max_retries {
                                            attempt += 1;

                                            // Emit NodeRetry event
                                            let _ = event_sender.send(GraphEvent::NodeRetry {
                                                id: node_id,
                                                attempt,
                                                max_retries,
                                                error: error_msg.clone(),
                                            });

                                            // Call on_retry hook and check decision
                                            let err_obj = GraphError::ExecutionFailed {
                                                node_name: node_name.clone(),
                                                node_id: id_val,
                                                error: error_msg.clone(),
                                            };

                                            let mut should_retry = true;
                                            {
                                                let node_guard = node_ref.lock().await;
                                                let hooks_guard = hooks.read().await;
                                                for hook in hooks_guard.iter() {
                                                    let decision = hook
                                                        .on_retry(
                                                            &*node_guard,
                                                            &err_obj,
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
                                                    id_val,
                                                    attempt,
                                                    max_retries,
                                                    retry_delay,
                                                    error_msg
                                                );
                                                tokio::time::sleep(Duration::from_millis(
                                                    retry_delay,
                                                ))
                                                .await;
                                                continue;
                                            }
                                        }

                                        // No more retries, fail
                                        break Ok(out);
                                    } else {
                                        // Success
                                        break Ok(out);
                                    }
                                }
                                Err(panic_err) => {
                                    // Panic occurred - don't retry panics
                                    break Err(panic_err);
                                }
                            }
                        };

                        match final_output {
                            Ok(out) => {
                                let node = node_ref.lock().await;

                                // Hook: after_node_run
                                {
                                    let hooks_guard = hooks.read().await;
                                    for hook in hooks_guard.iter() {
                                        hook.after_node_run(&*node, &out, &env).await;
                                    }
                                }

                                if out.is_err() {
                                    let error_msg = out.get_err().unwrap_or_default();
                                    error!(
                                        "Execution failed [name: {}, id: {}] - {} (after {} retries)",
                                        node_name, id_val, error_msg, attempt
                                    );
                                    let _ = event_sender.send(GraphEvent::NodeFailed {
                                        id: node_id,
                                        error: error_msg.clone(),
                                    });

                                    // Hook: on_error
                                    let err_obj = GraphError::ExecutionFailed {
                                        node_name: node_name.clone(),
                                        node_id: id_val,
                                        error: error_msg.clone(),
                                    };
                                    {
                                        let hooks_guard = hooks.read().await;
                                        for hook in hooks_guard.iter() {
                                            hook.on_error(&err_obj, &env).await;
                                        }
                                    }

                                    execute_state.set_output(out.clone());
                                    execute_state.exe_fail();
                                    let mut errors_lock = errors.lock().await;
                                    errors_lock.push(err_obj);
                                } else {
                                    if let Some(false) = out.conditional_result() {
                                        let mut cf = condition_flag.lock().await;
                                        *cf = false;
                                        info!(
                                            "Condition failed on [name: {}, id: {}]. The rest nodes will abort.",
                                            node_name, id_val,
                                        )
                                    }
                                    let _ =
                                        event_sender.send(GraphEvent::NodeSuccess { id: node_id });

                                    execute_state.set_output(out.clone());
                                    execute_state.exe_success();
                                    debug!(
                                        "Execution succeed [name: {}, id: {}]{}",
                                        node_name,
                                        id_val,
                                        if attempt > 0 {
                                            format!(" (after {} retries)", attempt)
                                        } else {
                                            String::new()
                                        }
                                    );
                                }

                                (node_id, out)
                            }
                            Err(_) => {
                                let mut node_guard: tokio::sync::MutexGuard<dyn Node> =
                                    node_ref.lock().await;
                                node_guard.input_channels().close_all();
                                node_guard.output_channels().close_all();

                                error!("Execution panic [name: {}, id: {}]", node_name, id_val,);
                                let _ = event_sender.send(GraphEvent::NodeFailed {
                                    id: node_id,
                                    error: "Panic".to_string(),
                                });

                                execute_state.set_output(Output::Err("Panic".to_string()));
                                execute_state.exe_fail();

                                let err_obj = GraphError::PanicOccurred {
                                    node_name: node_name.clone(),
                                    node_id: id_val,
                                };
                                // Hook: on_error
                                {
                                    let hooks_guard = hooks.read().await;
                                    for hook in hooks_guard.iter() {
                                        hook.on_error(&err_obj, &env).await;
                                    }
                                }

                                let mut errors_lock = errors.lock().await;
                                errors_lock.push(err_obj);
                                (node_id, Output::Err("Panic".to_string()))
                            }
                        }
                    }
                });
                tasks.push(task);
            }

            let results: Vec<Result<(NodeId, Output), tokio::task::JoinError>> =
                futures::future::join_all(tasks).await;

            // Check for errors immediately
            let errors_guard = errors.lock().await;
            if !errors_guard.is_empty() {
                if errors_guard.len() == 1 {
                    return Err(errors_guard[0].clone());
                } else {
                    return Err(GraphError::MultipleErrors(errors_guard.clone()));
                }
            }
            drop(errors_guard);

            if !(*condition_flag.lock().await) {
                break;
            }

            let mut next_pc = pc + 1;
            let mut should_abort = false;

            for (node_id, output) in results.into_iter().flatten() {
                if self.handle_flow_control(
                    output,
                    node_id,
                    &mut active_nodes,
                    &self.node_block_map,
                    &parents_map,
                    &mut next_pc,
                    self.blocks.len(),
                )? {
                    should_abort = true;
                    break;
                }
            }

            if should_abort {
                break;
            }

            // Check for loop (backward jump)
            // Only increment loop counter on actual backward jumps (next_pc < pc)
            // not on staying at the same block (next_pc == pc)
            if next_pc < pc {
                loop_count += 1;
                if loop_count >= self.max_loop_count {
                    return Err(GraphError::LoopLimitExceeded(self.max_loop_count));
                }

                // Event: LoopIteration
                let _ = self.event_sender.send(GraphEvent::LoopIteration {
                    iteration: loop_count,
                    block_index: next_pc,
                });

                // Reset active_nodes on loop iteration to allow dynamic routing.
                // This ensures that nodes pruned by a router in a previous iteration
                // can be selected in subsequent iterations (e.g., alternating between branches).
                active_nodes = self.nodes.keys().cloned().collect();
            }

            pc = next_pc;
        }

        // Send GraphFinished event BEFORE setting is_active to false
        // to avoid race conditions where subscribers see the flag change
        // before receiving the event.
        let _ = self.event_sender.send(GraphEvent::GraphFinished);

        self.is_active
            .store(false, std::sync::atomic::Ordering::Relaxed);

        Ok(())
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
    ) -> Result<bool, GraphError> {
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
                            return Err(GraphError::ExecutionFailed {
                                node_name: format!("Node-{}", node_id.0),
                                node_id: node_id.0,
                                error: format!(
                                    "Graph configuration error: jump_to_block_index {} is out of bounds. \
                                     Valid range is 0..{}",
                                    idx, blocks_len
                                ),
                            });
                        }
                        *next_pc = idx;
                    } else if let Some(nid) = instr.jump_to_node {
                        if let Some(&idx) = node_block_map.get(&NodeId(nid)) {
                            // Validate that the resolved block index is within valid range
                            // This should not happen in normal operation, but is a defensive check
                            if idx >= blocks_len {
                                error!(
                                    "Internal error: node_block_map contains invalid block index {} for node {} (blocks count: {})",
                                    idx, nid, blocks_len
                                );
                                return Err(GraphError::ExecutionFailed {
                                    node_name: format!("Node-{}", node_id.0),
                                    node_id: node_id.0,
                                    error: format!(
                                        "Internal error: node_block_map contains invalid block index {} for node {}. \
                                         This indicates a bug in the graph partitioning logic.",
                                        idx, nid
                                    ),
                                });
                            }
                            *next_pc = idx;
                        } else {
                            error!(
                                "Graph configuration error: invalid jump target node {} not found in block map. \
                                   This is likely due to an incorrect node ID in the LoopInstruction.",
                                nid
                            );
                            return Err(GraphError::ExecutionFailed {
                                node_name: format!("Node-{}", node_id.0),
                                node_id: node_id.0,
                                error: format!(
                                    "Graph configuration error: invalid jump target node {} not found. \
                                     Ensure the node ID exists in the graph.",
                                    nid
                                ),
                            });
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
    pub async fn check_loop_and_partition(&mut self) -> bool {
        // Check for cycles and get topological sort
        let sorted_nodes = match self.abstract_graph.get_topological_sort() {
            Some(nodes) => nodes,
            None => return true,
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
                let node = self.nodes.get(&node_id).unwrap();
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

        false
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
    fn remove_duplicates<T>(vec: Vec<T>) -> Vec<T>
    where
        T: Eq + Hash + Clone,
    {
        let mut seen = HashSet::new();
        vec.into_iter().filter(|x| seen.insert(x.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::conditional_node::{Condition, ConditionalNode};
    use crate::node::default_node::DefaultNode;
    use crate::{
        Content, EnvVar, InChannels, Node, NodeName, NodeTable, OutChannels, Output, action::Action,
    };
    use async_trait::async_trait;
    use std::sync::Arc;

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
            Self::default()
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

    #[test]
    fn test_graph_execution() {
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

        graph.add_node(node);
        graph.add_node(node1);

        graph.add_edge(node_id, vec![node1_id]);

        match graph.start() {
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
    #[test]
    fn test_conditional_execution() {
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
        graph.add_node(node_a);
        graph.add_node(node_b);

        // Add edge from A to B
        graph.add_edge(node_a_id, vec![node_b_id]);

        // Execute graph
        match graph.start() {
            Ok(_) => {
                // Node A should have failed
                assert!(graph.execute_states[&node_a_id].get_output().is_none());
            }
            Err(e) => {
                assert!(matches!(e, GraphError::ExecutionFailed { .. }));
            }
        }
    }
}
