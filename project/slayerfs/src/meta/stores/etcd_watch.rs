//! Etcd Watch Worker for Cache Invalidation
//!
//! Monitors etcd changes and invalidates local cache to maintain consistency
//! across multiple clients.

use crate::meta::entities::etcd::{EtcdDirChildren, EtcdEntryInfo, EtcdForwardEntry};
use crate::meta::store::MetaError;
use etcd_client::{Client as EtcdClient, EventType, WatchOptions};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};
use tracing::{debug, error, info, warn};

/// Cache invalidation events from etcd watch
#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum CacheInvalidationEvent {
    /// Invalidate specific inode cache (fallback for DELETE or parse errors)
    InvalidateInode(i64),

    /// Invalidate parent's children cache (fallback for complex operations)
    InvalidateParentChildren(i64),

    /// Incrementally add a child to parent's cached children
    /// Avoids full directory cache reload for single file creation
    AddChild {
        parent_ino: i64,
        name: String,
        child_ino: i64,
    },

    /// Incrementally remove a child from parent's cached children
    /// Avoids full directory cache reload for single file deletion
    RemoveChild { parent_ino: i64, name: String },

    /// Directly update inode metadata from r: key PUT event
    /// Avoids re-fetching from etcd (chmod, chown, utimens operations)
    UpdateInodeMetadata { ino: i64, metadata: EtcdEntryInfo },

    /// Directly update directory children from c: key PUT event
    /// Replaces cached children list without re-fetching
    UpdateChildren {
        parent_ino: i64,
        children: HashMap<String, i64>, // name -> inode mapping
    },
}

/// Etcd watch worker configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Enable watch worker (default: false for backward compatibility)
    #[serde(default)]
    pub enabled: bool,

    /// Watch key prefixes (default: empty = watch all keys)
    #[serde(default)]
    pub prefixes: Vec<String>,

    /// Buffer size for event channel
    #[serde(default = "default_buffer_size")]
    pub event_buffer_size: usize,

    /// Enable debug logging
    #[serde(default)]
    pub debug: bool,
}

fn default_buffer_size() -> usize {
    1000
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            prefixes: vec![],
            event_buffer_size: 1000,
            debug: false,
        }
    }
}

impl WatchConfig {
    pub fn effective_prefixes(&self) -> Vec<String> {
        if self.prefixes.is_empty() {
            vec!["".to_string()] // Default: watch all keys
        } else {
            self.prefixes.clone()
        }
    }

    pub fn from_env_or_default() -> Self {
        Self {
            enabled: std::env::var("SLAYERFS_WATCH_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            prefixes: std::env::var("SLAYERFS_WATCH_PREFIXES")
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            event_buffer_size: std::env::var("SLAYERFS_WATCH_BUFFER_SIZE")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
            debug: std::env::var("SLAYERFS_WATCH_DEBUG")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
        }
    }
}

/// Etcd watch worker
///
/// # Responsibilities
/// 1. Watch etcd key changes (PUT/DELETE events)
/// 2. Parse changed keys and generate cache invalidation events
/// 3. Send events to MetaClient for cache invalidation
///
/// # Architecture
/// ```text
/// etcd Watch Stream
///       │
///       ▼
///   WatchWorker
///       │
///       ├─ Parse Key: f:10:file.txt → parent=10, name=file.txt
///       ├─ Parse Key: r:100 → inode=100
///       └─ Parse Key: c:10 → parent=10
///       │
///       ▼
///   mpsc::Sender<CacheInvalidationEvent>
///       │
///       ▼
///   MetaClient (invalidate cache)
/// ```
pub struct EtcdWatchWorker {
    client: EtcdClient,
    config: WatchConfig,
    event_tx: mpsc::Sender<CacheInvalidationEvent>,
    /// One JoinHandle per prefix watch task (empty prefix = watch all)
    worker_handles: Vec<JoinHandle<()>>,
}

impl EtcdWatchWorker {
    pub fn new(
        client: EtcdClient,
        config: WatchConfig,
    ) -> (Self, mpsc::Receiver<CacheInvalidationEvent>) {
        let (event_tx, event_rx) = mpsc::channel(config.event_buffer_size);

        let worker = Self {
            client,
            config,
            event_tx,
            worker_handles: Vec::new(),
        };

        (worker, event_rx)
    }

    /// Start watch worker(s) in background
    pub fn start(&mut self) -> Result<(), MetaError> {
        self.stop();

        let prefixes = self.config.effective_prefixes();

        for prefix in prefixes {
            let client = self.client.clone();
            let config = self.config.clone();
            let event_tx = self.event_tx.clone();
            let prefix_clone = prefix.clone();

            let handle = tokio::spawn(async move {
                if let Err(e) =
                    Self::watch_loop_for_prefix(client, prefix_clone.clone(), config, event_tx)
                        .await
                {
                    error!(prefix = %prefix_clone, "Watch worker fatal error: {}", e);
                }
            });

            self.worker_handles.push(handle);
            info!(prefix = %prefix, "Etcd watch worker started for prefix");
        }

        info!(
            count = self.worker_handles.len(),
            "All etcd watch workers started"
        );
        Ok(())
    }

    /// Stop watch worker
    #[allow(dead_code)]
    pub fn stop(&mut self) {
        let count = self.worker_handles.len();
        for handle in self.worker_handles.drain(..) {
            handle.abort();
        }
        if count > 0 {
            info!(count = count, "Etcd watch workers stopped");
        }
    }

    async fn watch_loop_for_prefix(
        mut client: EtcdClient,
        prefix: String,
        config: WatchConfig,
        event_tx: mpsc::Sender<CacheInvalidationEvent>,
    ) -> Result<(), MetaError> {
        info!(prefix = %prefix, "Starting etcd watch loop for prefix");

        loop {
            let options = WatchOptions::new().with_prefix();
            let (_watcher, mut stream) = match client.watch(prefix.clone(), Some(options)).await {
                Ok((w, s)) => (w, s),
                Err(e) => {
                    error!(prefix = %prefix, "Failed to create watch stream: {}", e);
                    time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            info!(prefix = %prefix, "Watch stream established");

            while let Some(resp) = stream.message().await.transpose() {
                match resp {
                    Ok(resp) => {
                        if resp.canceled() {
                            warn!(prefix = %prefix, "Watch canceled, reconnecting...");
                            break;
                        }

                        for event in resp.events() {
                            if let Err(e) = Self::handle_watch_event(event, &event_tx, &config) {
                                error!(prefix = %prefix, "Failed to handle watch event: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!(prefix = %prefix, "Watch stream error: {}", e);
                        break;
                    }
                }
            }

            warn!(prefix = %prefix, "Watch stream closed, reconnecting in 1s...");
            time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Handle single watch event
    fn handle_watch_event(
        event: &etcd_client::Event,
        event_tx: &mpsc::Sender<CacheInvalidationEvent>,
        config: &WatchConfig,
    ) -> Result<(), MetaError> {
        let event_type = event.event_type();
        let kv = match event.kv() {
            Some(kv) => kv,
            None => return Ok(()), // No key-value, skip
        };

        let key = String::from_utf8_lossy(kv.key()).to_string();
        let value = kv.value().to_vec();

        if config.debug {
            debug!("Watch event: {:?} on key: {}", event_type, key);
        }

        // Parse key and generate invalidation events
        let invalidation_events = Self::parse_key_to_events(&key, event_type, &value);

        for inv_event in invalidation_events {
            if config.debug {
                debug!("Generated invalidation: {:?}", inv_event);
            }

            // Send to MetaClient (non-blocking, drop if full)
            if event_tx.try_send(inv_event).is_err() {
                warn!("Event channel full, dropping event");
            }
        }

        Ok(())
    }

    /// Parse etcd key to cache invalidation events
    ///
    /// # Key Formats
    /// - `f:{parent}:{name}` - Forward index (parent, name) → inode
    /// - `r:{inode}` - Reverse index inode → metadata
    /// - `c:{inode}` - Children index inode → children set
    ///
    /// # Event Generation Rules
    /// - `f:*` PUT → Parse value to get child_ino, generate AddChild event
    /// - `f:*` DELETE → Generate RemoveChild event
    /// - `r:*` PUT → Parse EtcdEntryInfo JSON, generate UpdateInodeMetadata event
    /// - `r:*` DELETE → Invalidate inode cache (coarse-grained)
    /// - `c:*` PUT → Parse EtcdDirChildren JSON (HashMap<String, i64>), generate UpdateChildren event
    /// - `c:*` DELETE → Invalidate parent children (coarse-grained)
    ///
    /// # Arguments
    /// - `key`: etcd key string
    /// - `event_type`: PUT or DELETE
    /// - `value`: etcd value bytes (for extracting data from PUT events)
    fn parse_key_to_events(
        key: &str,
        event_type: EventType,
        value: &[u8],
    ) -> Vec<CacheInvalidationEvent> {
        let mut events = Vec::new();

        // Parse key prefix
        let parts: Vec<&str> = key.split(':').collect();
        if parts.is_empty() {
            return events;
        }

        match parts[0] {
            "f" if parts.len() >= 3 => {
                // Forward index: f:{parent}:{name}
                if let Ok(parent_ino) = parts[1].parse::<i64>() {
                    let name = parts[2..].join(":"); // Handle names with colons

                    match event_type {
                        EventType::Put => {
                            // Parse EtcdForwardEntry using deserialize_meta (binary-safe)
                            if let Ok(forward_entry) = crate::meta::serialization::deserialize_meta::<
                                EtcdForwardEntry,
                            >(value)
                            {
                                events.push(CacheInvalidationEvent::AddChild {
                                    parent_ino,
                                    name,
                                    child_ino: forward_entry.inode,
                                });
                                return events;
                            }

                            // Fallback: value parse failed
                            warn!(
                                "Failed to parse EtcdForwardEntry from f: key PUT, using coarse-grained invalidation"
                            );
                            events
                                .push(CacheInvalidationEvent::InvalidateParentChildren(parent_ino));
                        }
                        EventType::Delete => {
                            events.push(CacheInvalidationEvent::RemoveChild { parent_ino, name });
                        }
                    }
                }
            }
            "r" if parts.len() >= 2 => {
                // Reverse index: r:{inode} → EtcdEntryInfo JSON
                if let Ok(inode) = parts[1].parse::<i64>() {
                    match event_type {
                        EventType::Put => {
                            // Parse EtcdEntryInfo using serde_json (EtcdEntryInfo NOT migrated - has bitflags)
                            if let Ok(value_str) = std::str::from_utf8(value)
                                && let Ok(metadata) =
                                    serde_json::from_str::<EtcdEntryInfo>(value_str)
                            {
                                events.push(CacheInvalidationEvent::UpdateInodeMetadata {
                                    ino: inode,
                                    metadata,
                                });
                                return events;
                            }

                            // Fallback: JSON parse failed
                            warn!(
                                "Failed to parse EtcdEntryInfo JSON from r: key PUT, using invalidate"
                            );
                            events.push(CacheInvalidationEvent::InvalidateInode(inode));
                        }
                        EventType::Delete => {
                            // Coarse-grained: inode deleted
                            events.push(CacheInvalidationEvent::InvalidateInode(inode));
                        }
                    }
                }
            }
            "c" if parts.len() >= 2 => {
                // Children index: c:{parent_inode} → EtcdDirChildren JSON
                // Format: {"inode":1,"children":{"a.txt":4,"one":3}}
                if let Ok(parent_ino) = parts[1].parse::<i64>() {
                    match event_type {
                        EventType::Put => {
                            // Parse EtcdDirChildren using deserialize_meta (binary-safe)
                            if let Ok(dir_children) = crate::meta::serialization::deserialize_meta::<
                                EtcdDirChildren,
                            >(value)
                            {
                                events.push(CacheInvalidationEvent::UpdateChildren {
                                    parent_ino,
                                    children: dir_children.children,
                                });
                                return events;
                            }

                            // Fallback: parse failed
                            warn!(
                                "Failed to parse EtcdDirChildren from c: key PUT, using invalidate"
                            );
                            events
                                .push(CacheInvalidationEvent::InvalidateParentChildren(parent_ino));
                        }
                        EventType::Delete => {
                            // Coarse-grained: children deleted
                            events
                                .push(CacheInvalidationEvent::InvalidateParentChildren(parent_ino));
                        }
                    }
                }
            }
            _ => {
                // Unknown key format - safe fallback
                warn!("Unknown etcd key format: {}", key);
            }
        }

        events
    }
}

impl Drop for EtcdWatchWorker {
    fn drop(&mut self) {
        for handle in self.worker_handles.drain(..) {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_watch_config_effective_prefixes() {
        let config = WatchConfig {
            enabled: true,
            prefixes: vec!["f:".into(), "r:".into()],
            ..Default::default()
        };
        assert_eq!(config.effective_prefixes(), vec!["f:", "r:"]);
    }

    #[test]
    fn test_watch_config_effective_prefixes_empty() {
        let config = WatchConfig {
            enabled: true,
            prefixes: vec![],
            ..Default::default()
        };
        assert_eq!(config.effective_prefixes(), vec![""]);
    }

    #[test]
    fn test_watch_config_default() {
        let config = WatchConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.effective_prefixes(), vec![""]);
    }

    #[test]
    fn test_watch_config_from_env() {
        unsafe {
            std::env::set_var("SLAYERFS_WATCH_ENABLED", "true");
            std::env::set_var("SLAYERFS_WATCH_PREFIXES", "f:, r: , c:");
        }
        let config = WatchConfig::from_env_or_default();
        assert!(config.enabled);
        assert_eq!(config.prefixes, vec!["f:", "r:", "c:"]);
        unsafe {
            std::env::remove_var("SLAYERFS_WATCH_ENABLED");
            std::env::remove_var("SLAYERFS_WATCH_PREFIXES");
        }
    }

    #[test]
    fn test_parse_forward_key_put() {
        let json = br#"{"parent_inode":10,"name":"file.txt","inode":100,"is_file":true}"#;
        let events = EtcdWatchWorker::parse_key_to_events("f:10:file.txt", EventType::Put, json);
        assert_eq!(events.len(), 1);
        match &events[0] {
            CacheInvalidationEvent::AddChild {
                parent_ino,
                name,
                child_ino,
            } => {
                assert_eq!(*parent_ino, 10);
                assert_eq!(name, "file.txt");
                assert_eq!(*child_ino, 100);
            }
            _ => panic!("Expected AddChild event"),
        }
    }

    #[test]
    fn test_parse_forward_key_delete() {
        let events = EtcdWatchWorker::parse_key_to_events("f:10:file.txt", EventType::Delete, b"");
        assert_eq!(events.len(), 1);
        match &events[0] {
            CacheInvalidationEvent::RemoveChild { parent_ino, name } => {
                assert_eq!(*parent_ino, 10);
                assert_eq!(name, "file.txt");
            }
            _ => panic!("Expected RemoveChild event"),
        }
    }

    #[test]
    fn test_parse_forward_key_invalid_value() {
        let events =
            EtcdWatchWorker::parse_key_to_events("f:10:file.txt", EventType::Put, b"invalid");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            CacheInvalidationEvent::InvalidateParentChildren(10)
        ));
    }

    #[test]
    fn test_parse_reverse_key() {
        let events = EtcdWatchWorker::parse_key_to_events("r:100", EventType::Put, b"");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            CacheInvalidationEvent::InvalidateInode(100)
        ));
    }

    #[test]
    fn test_parse_children_key_put() {
        let json = r#"{"inode":50,"children":{"file.txt":100,"dir":200}}"#;
        let events = EtcdWatchWorker::parse_key_to_events("c:50", EventType::Put, json.as_bytes());
        assert_eq!(events.len(), 1);
        match &events[0] {
            CacheInvalidationEvent::UpdateChildren {
                parent_ino,
                children,
            } => {
                assert_eq!(*parent_ino, 50);
                assert_eq!(children.len(), 2);
                assert_eq!(children.get("file.txt"), Some(&100));
                assert_eq!(children.get("dir"), Some(&200));
            }
            _ => panic!("Expected UpdateChildren event"),
        }
    }

    #[test]
    fn test_parse_children_key_delete() {
        let events = EtcdWatchWorker::parse_key_to_events("c:50", EventType::Delete, b"");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            CacheInvalidationEvent::InvalidateParentChildren(50)
        ));
    }

    #[test]
    fn test_parse_children_key_invalid_json() {
        let events = EtcdWatchWorker::parse_key_to_events("c:50", EventType::Put, b"invalid json");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            CacheInvalidationEvent::InvalidateParentChildren(50)
        ));
    }
}
