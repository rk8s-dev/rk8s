//! Checkpoint mechanism for state persistence and recovery
//!
//! This module provides the ability to save and restore graph execution state,
//! enabling:
//! - Fault tolerance: Resume execution after failures
//! - Long-running workflows: Persist progress periodically
//! - Debugging: Inspect execution state at specific points
//!
//! # Architecture
//!
//! The checkpoint system consists of:
//! - [`Checkpoint`]: The serializable state snapshot
//! - [`CheckpointStore`]: Trait for pluggable storage backends
//! - [`MemoryCheckpointStore`]: In-memory storage (for testing)
//! - [`FileCheckpointStore`]: File-based persistent storage
//!
//! # Example
//!
//! ```ignore
//! use dagrs::{Graph, checkpoint::{FileCheckpointStore, CheckpointConfig}};
//!
//! let mut graph = Graph::new();
//! // ... add nodes ...
//!
//! // Configure checkpointing
//! let store = FileCheckpointStore::new("/tmp/checkpoints");
//! graph.set_checkpoint_store(Box::new(store));
//! graph.set_checkpoint_interval(5); // Checkpoint every 5 nodes
//!
//! // Run with automatic checkpointing
//! graph.async_start().await?;
//!
//! // Or resume from a checkpoint
//! graph.resume_from_checkpoint("checkpoint_123").await?;
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::node::NodeId;

/// Checkpoint identifier
pub type CheckpointId = String;

/// Represents the execution state of a single node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    /// Node ID
    pub node_id: usize,
    /// Whether the node has completed execution
    pub completed: bool,
    /// Whether execution was successful
    pub success: bool,
    /// Serialized output data (if serializable)
    ///
    /// This field stores JSON-serialized output data for nodes that produce
    /// serializable output. The data can be restored when resuming from a checkpoint.
    ///
    /// Note: Only outputs that implement `serde::Serialize` can be stored here.
    /// For non-serializable outputs, this field will be `None`.
    pub output_data: Option<Vec<u8>>,
    /// Human-readable output summary (for debugging)
    #[serde(default)]
    pub output_summary: Option<String>,
}

impl NodeState {
    /// Create a new NodeState with completed status
    pub fn completed(node_id: usize, success: bool) -> Self {
        Self {
            node_id,
            completed: true,
            success,
            output_data: None,
            output_summary: None,
        }
    }

    /// Create a new NodeState for a pending node
    pub fn pending(node_id: usize) -> Self {
        Self {
            node_id,
            completed: false,
            success: false,
            output_data: None,
            output_summary: None,
        }
    }

    /// Set serialized output data
    pub fn with_output_data(mut self, data: Vec<u8>) -> Self {
        self.output_data = Some(data);
        self
    }

    /// Set output summary
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.output_summary = Some(summary.into());
        self
    }
}

/// Represents a complete checkpoint of graph execution state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Unique identifier for this checkpoint
    pub id: CheckpointId,
    /// Timestamp when checkpoint was created (Unix epoch seconds)
    pub timestamp: u64,
    /// Current program counter (block index)
    pub pc: usize,
    /// Number of loop iterations completed
    pub loop_count: usize,
    /// Set of currently active node IDs
    pub active_nodes: HashSet<usize>,
    /// Execution state for each node
    pub node_states: HashMap<usize, NodeState>,
    /// Serialized environment variables (optional)
    pub env_data: Option<Vec<u8>>,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

impl Checkpoint {
    /// Create a new checkpoint with generated ID
    pub fn new(pc: usize, loop_count: usize) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let id = format!("ckpt_{}_{}", timestamp, pc);

        Self {
            id,
            timestamp,
            pc,
            loop_count,
            active_nodes: HashSet::new(),
            node_states: HashMap::new(),
            env_data: None,
            metadata: HashMap::new(),
        }
    }

    /// Create checkpoint with a specific ID
    pub fn with_id(id: impl Into<String>, pc: usize, loop_count: usize) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id: id.into(),
            timestamp,
            pc,
            loop_count,
            active_nodes: HashSet::new(),
            node_states: HashMap::new(),
            env_data: None,
            metadata: HashMap::new(),
        }
    }

    /// Set active nodes from NodeId set
    pub fn set_active_nodes(&mut self, nodes: &HashSet<NodeId>) {
        self.active_nodes = nodes.iter().map(|id| id.0).collect();
    }

    /// Get active nodes as NodeId set
    pub fn get_active_nodes(&self) -> HashSet<NodeId> {
        self.active_nodes.iter().map(|id| NodeId(*id)).collect()
    }

    /// Add node state
    pub fn add_node_state(&mut self, state: NodeState) {
        self.node_states.insert(state.node_id, state);
    }

    /// Add metadata
    pub fn add_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }
}

/// Error types for checkpoint operations
#[derive(Debug, Clone)]
pub enum CheckpointError {
    /// Checkpoint not found
    NotFound(CheckpointId),
    /// Serialization error
    SerializationError(String),
    /// Deserialization error  
    DeserializationError(String),
    /// Storage I/O error
    StorageError(String),
    /// Invalid checkpoint data
    InvalidCheckpoint(String),
    /// Checkpoint store not configured
    StoreNotConfigured,
}

impl std::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckpointError::NotFound(id) => write!(f, "Checkpoint not found: {}", id),
            CheckpointError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            CheckpointError::DeserializationError(msg) => {
                write!(f, "Deserialization error: {}", msg)
            }
            CheckpointError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            CheckpointError::InvalidCheckpoint(msg) => write!(f, "Invalid checkpoint: {}", msg),
            CheckpointError::StoreNotConfigured => write!(f, "Checkpoint store not configured"),
        }
    }
}

impl std::error::Error for CheckpointError {}

/// Trait for checkpoint storage backends
///
/// Implement this trait to provide custom checkpoint storage (e.g., database, cloud storage).
#[async_trait]
pub trait CheckpointStore: Send + Sync {
    /// Save a checkpoint
    async fn save(&self, checkpoint: &Checkpoint) -> Result<(), CheckpointError>;

    /// Load a checkpoint by ID
    async fn load(&self, id: &CheckpointId) -> Result<Checkpoint, CheckpointError>;

    /// Delete a checkpoint
    async fn delete(&self, id: &CheckpointId) -> Result<(), CheckpointError>;

    /// List all checkpoint IDs
    async fn list(&self) -> Result<Vec<CheckpointId>, CheckpointError>;

    /// Get the latest checkpoint
    async fn latest(&self) -> Result<Option<Checkpoint>, CheckpointError>;

    /// Clear all checkpoints
    async fn clear(&self) -> Result<(), CheckpointError>;
}

/// In-memory checkpoint store (for testing and temporary storage)
#[derive(Default)]
pub struct MemoryCheckpointStore {
    checkpoints: std::sync::RwLock<HashMap<CheckpointId, Checkpoint>>,
}

impl MemoryCheckpointStore {
    pub fn new() -> Self {
        Self {
            checkpoints: std::sync::RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl CheckpointStore for MemoryCheckpointStore {
    async fn save(&self, checkpoint: &Checkpoint) -> Result<(), CheckpointError> {
        let mut store = self.checkpoints.write().map_err(|e| {
            CheckpointError::StorageError(format!("Failed to acquire write lock: {}", e))
        })?;
        store.insert(checkpoint.id.clone(), checkpoint.clone());
        Ok(())
    }

    async fn load(&self, id: &CheckpointId) -> Result<Checkpoint, CheckpointError> {
        let store = self.checkpoints.read().map_err(|e| {
            CheckpointError::StorageError(format!("Failed to acquire read lock: {}", e))
        })?;
        store
            .get(id)
            .cloned()
            .ok_or(CheckpointError::NotFound(id.clone()))
    }

    async fn delete(&self, id: &CheckpointId) -> Result<(), CheckpointError> {
        let mut store = self.checkpoints.write().map_err(|e| {
            CheckpointError::StorageError(format!("Failed to acquire write lock: {}", e))
        })?;
        store.remove(id);
        Ok(())
    }

    async fn list(&self) -> Result<Vec<CheckpointId>, CheckpointError> {
        let store = self.checkpoints.read().map_err(|e| {
            CheckpointError::StorageError(format!("Failed to acquire read lock: {}", e))
        })?;
        Ok(store.keys().cloned().collect())
    }

    async fn latest(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        let store = self.checkpoints.read().map_err(|e| {
            CheckpointError::StorageError(format!("Failed to acquire read lock: {}", e))
        })?;
        Ok(store.values().max_by_key(|c| c.timestamp).cloned())
    }

    async fn clear(&self) -> Result<(), CheckpointError> {
        let mut store = self.checkpoints.write().map_err(|e| {
            CheckpointError::StorageError(format!("Failed to acquire write lock: {}", e))
        })?;
        store.clear();
        Ok(())
    }
}

/// File-based checkpoint store for persistent storage
///
/// This implementation uses `tokio::fs` for non-blocking async I/O operations,
/// making it safe to use in async contexts without blocking the runtime.
pub struct FileCheckpointStore {
    base_path: PathBuf,
}

impl FileCheckpointStore {
    /// Create a new file-based checkpoint store
    ///
    /// # Arguments
    /// * `base_path` - Directory where checkpoint files will be stored
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    fn checkpoint_path(&self, id: &CheckpointId) -> Result<PathBuf, CheckpointError> {
        // Security: Validate checkpoint ID to prevent path traversal attacks
        if id.contains('/') || id.contains('\\') || id.contains("..") {
            return Err(CheckpointError::InvalidCheckpoint(
                "Checkpoint ID contains invalid characters".to_string(),
            ));
        }
        Ok(self.base_path.join(format!("{}.json", id)))
    }

    async fn ensure_dir(&self) -> Result<(), CheckpointError> {
        tokio::fs::create_dir_all(&self.base_path)
            .await
            .map_err(|e| {
                CheckpointError::StorageError(format!(
                    "Failed to create checkpoint directory: {}",
                    e
                ))
            })
    }
}

#[async_trait]
impl CheckpointStore for FileCheckpointStore {
    async fn save(&self, checkpoint: &Checkpoint) -> Result<(), CheckpointError> {
        self.ensure_dir().await?;

        let json = serde_json::to_string_pretty(checkpoint)
            .map_err(|e| CheckpointError::SerializationError(e.to_string()))?;

        let path = self.checkpoint_path(&checkpoint.id)?;
        tokio::fs::write(&path, json).await.map_err(|e| {
            CheckpointError::StorageError(format!("Failed to write checkpoint file: {}", e))
        })?;

        Ok(())
    }

    async fn load(&self, id: &CheckpointId) -> Result<Checkpoint, CheckpointError> {
        let path = self.checkpoint_path(id)?;

        // Use async metadata check instead of sync path.exists()
        match tokio::fs::metadata(&path).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CheckpointError::NotFound(id.clone()));
            }
            Err(e) => {
                return Err(CheckpointError::StorageError(format!(
                    "Failed to check checkpoint file: {}",
                    e
                )));
            }
        }

        let json = tokio::fs::read_to_string(&path).await.map_err(|e| {
            CheckpointError::StorageError(format!("Failed to read checkpoint file: {}", e))
        })?;

        serde_json::from_str(&json)
            .map_err(|e| CheckpointError::DeserializationError(e.to_string()))
    }

    async fn delete(&self, id: &CheckpointId) -> Result<(), CheckpointError> {
        let path = self.checkpoint_path(id)?;

        // Use async metadata check instead of sync path.exists()
        match tokio::fs::metadata(&path).await {
            Ok(_) => {
                tokio::fs::remove_file(&path).await.map_err(|e| {
                    CheckpointError::StorageError(format!(
                        "Failed to delete checkpoint file: {}",
                        e
                    ))
                })?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist, nothing to delete
            }
            Err(e) => {
                return Err(CheckpointError::StorageError(format!(
                    "Failed to check checkpoint file: {}",
                    e
                )));
            }
        }

        Ok(())
    }

    async fn list(&self) -> Result<Vec<CheckpointId>, CheckpointError> {
        self.ensure_dir().await?;

        let mut entries = tokio::fs::read_dir(&self.base_path).await.map_err(|e| {
            CheckpointError::StorageError(format!("Failed to read checkpoint directory: {}", e))
        })?;

        let mut ids = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| {
            CheckpointError::StorageError(format!("Failed to read directory entry: {}", e))
        })? {
            if let Some(name) = entry.file_name().to_str()
                && name.ends_with(".json")
            {
                ids.push(name.trim_end_matches(".json").to_string());
            }
        }

        Ok(ids)
    }

    async fn latest(&self) -> Result<Option<Checkpoint>, CheckpointError> {
        let ids = self.list().await?;

        let mut latest: Option<Checkpoint> = None;
        for id in ids {
            if let Ok(checkpoint) = self.load(&id).await
                && latest
                    .as_ref()
                    .is_none_or(|l| checkpoint.timestamp > l.timestamp)
            {
                latest = Some(checkpoint);
            }
        }

        Ok(latest)
    }

    async fn clear(&self) -> Result<(), CheckpointError> {
        let ids = self.list().await?;
        for id in ids {
            self.delete(&id).await?;
        }
        Ok(())
    }
}

/// Configuration for automatic checkpointing
#[derive(Debug, Clone)]
pub struct CheckpointConfig {
    /// Enable automatic checkpointing
    pub enabled: bool,
    /// Checkpoint every N completed nodes
    pub interval_nodes: Option<usize>,
    /// Checkpoint every N seconds
    pub interval_seconds: Option<u64>,
    /// Checkpoint on loop iteration
    pub on_loop_iteration: bool,
    /// Checkpoint before conditional nodes
    pub before_conditional: bool,
    /// Maximum number of checkpoints to keep (0 = unlimited)
    pub max_checkpoints: usize,
}

impl Default for CheckpointConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_nodes: Some(10),
            interval_seconds: None,
            on_loop_iteration: true,
            before_conditional: true,
            max_checkpoints: 5,
        }
    }
}

impl CheckpointConfig {
    /// Create a new checkpoint config with automatic checkpointing enabled
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Set the node interval for checkpointing
    pub fn with_node_interval(mut self, interval: usize) -> Self {
        self.interval_nodes = Some(interval);
        self
    }

    /// Set the time interval for checkpointing
    pub fn with_time_interval(mut self, seconds: u64) -> Self {
        self.interval_seconds = Some(seconds);
        self
    }

    /// Enable checkpointing on loop iterations
    pub fn with_loop_checkpoint(mut self, enabled: bool) -> Self {
        self.on_loop_iteration = enabled;
        self
    }

    /// Set maximum number of checkpoints to retain
    pub fn with_max_checkpoints(mut self, max: usize) -> Self {
        self.max_checkpoints = max;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_checkpoint_store() {
        let store = MemoryCheckpointStore::new();

        // Create and save checkpoint
        let mut checkpoint = Checkpoint::new(5, 2);
        checkpoint.add_metadata("test_key", "test_value");
        checkpoint.active_nodes.insert(1);
        checkpoint.active_nodes.insert(2);

        store.save(&checkpoint).await.unwrap();

        // Load checkpoint
        let loaded = store.load(&checkpoint.id).await.unwrap();
        assert_eq!(loaded.pc, 5);
        assert_eq!(loaded.loop_count, 2);
        assert!(loaded.active_nodes.contains(&1));
        assert!(loaded.active_nodes.contains(&2));
        assert_eq!(
            loaded.metadata.get("test_key"),
            Some(&"test_value".to_string())
        );

        // List checkpoints
        let ids = store.list().await.unwrap();
        assert_eq!(ids.len(), 1);

        // Get latest
        let latest = store.latest().await.unwrap();
        assert!(latest.is_some());

        // Delete checkpoint
        store.delete(&checkpoint.id).await.unwrap();
        let ids = store.list().await.unwrap();
        assert_eq!(ids.len(), 0);
    }

    #[test]
    fn test_checkpoint_creation() {
        let checkpoint = Checkpoint::new(10, 3);
        assert_eq!(checkpoint.pc, 10);
        assert_eq!(checkpoint.loop_count, 3);
        assert!(checkpoint.id.starts_with("ckpt_"));
    }

    #[test]
    fn test_checkpoint_config() {
        let config = CheckpointConfig::enabled()
            .with_node_interval(5)
            .with_time_interval(60)
            .with_max_checkpoints(10);

        assert!(config.enabled);
        assert_eq!(config.interval_nodes, Some(5));
        assert_eq!(config.interval_seconds, Some(60));
        assert_eq!(config.max_checkpoints, 10);
    }
}
