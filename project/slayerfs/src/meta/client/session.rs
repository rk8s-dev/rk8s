use std::sync::Arc;
use std::time::Duration;

use sea_orm::prelude::Uuid;
use tokio::sync::{Mutex, oneshot, watch};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::meta::store::{MetaError, MetaStore};

/// Client session identifier information
///
/// Uses UUID for globally unique identification instead of hostname+PID combination
/// to avoid conflicts when processes restart or multiple processes have same hostname+PID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId {
    pub uuid: Uuid,
    pub hostname: String,
    pub process_id: u32,
    pub created_at: std::time::SystemTime,
}

impl SessionId {
    /// Extract session identifier from payload, supporting both new UUID format and legacy format
    pub fn from_payload(payload: &[u8]) -> Result<Self, serde_json::Error> {
        let info: serde_json::Value = serde_json::from_slice(payload)?;

        // Try to extract UUID from payload first (new format)
        if let Some(uuid_str) = info.get("session_uuid").and_then(|v| v.as_str()) {
            match Uuid::parse_str(uuid_str) {
                Ok(uuid) => {
                    let hostname = info
                        .get("host_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let process_id =
                        info.get("process_id")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(std::process::id() as u64) as u32;
                    return Ok(Self {
                        uuid,
                        hostname,
                        process_id,
                        created_at: std::time::SystemTime::now(),
                    });
                }
                Err(e) => {
                    warn!(
                        "Malformed session_uuid in payload: '{}', error: {}. Falling back to new session UUID.",
                        uuid_str, e
                    );
                }
            }
        }

        // Fallback: create new session with current UUID (for backward compatibility)
        Ok(Self::current())
    }

    /// Create session identifier for current process with new UUID
    pub fn current() -> Self {
        Self {
            uuid: Uuid::new_v4(),
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            process_id: std::process::id(),
            created_at: std::time::SystemTime::now(),
        }
    }

    /// Create session identifier with specific UUID (for recovery/reconnect scenarios)
    pub fn with_uuid(uuid: Uuid) -> Self {
        Self {
            uuid,
            hostname: hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
            process_id: std::process::id(),
            created_at: std::time::SystemTime::now(),
        }
    }

    /// Format as session key using UUID as primary identifier
    pub fn to_key(&self) -> String {
        format!("session:{}", self.uuid)
    }

    /// Get legacy-style key for backward compatibility
    pub fn to_legacy_key(&self) -> String {
        format!("session:{}:{}", self.hostname, self.process_id)
    }
}

/// Session configuration options
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// Interval between heartbeat updates
    pub heartbeat_interval: Duration,
    /// Session timeout threshold for stale detection
    pub timeout_threshold: Duration,
    /// Maximum retry attempts for session recovery
    pub max_recovery_attempts: u32,
    /// Whether to enable automatic session recovery
    pub enable_recovery: bool,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval: Duration::from_secs(30),
            timeout_threshold: Duration::from_secs(300), // 5 minutes
            max_recovery_attempts: 3,
            enable_recovery: true,
        }
    }
}

/// Session state for atomic operations
struct SessionState {
    /// Current session identifier
    session_id: Option<SessionId>,
    /// Heartbeat task handle
    task_handle: Option<JoinHandle<()>>,
    /// Shutdown signal sender
    shutdown_sender: Option<oneshot::Sender<()>>,
    /// Watch channel to monitor heartbeat task status
    heartbeat_status_watcher: Option<watch::Receiver<bool>>,
    /// Whether session is currently running
    running: bool,
    /// Store reference for session operations
    store: Option<Arc<dyn MetaStore + Send + Sync>>,
    /// Number of consecutive heartbeat failures
    consecutive_failures: u32,
}

impl std::fmt::Debug for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionState")
            .field("session_id", &self.session_id)
            .field("task_handle", &self.task_handle.is_some())
            .field("shutdown_sender", &self.shutdown_sender.is_some())
            .field(
                "heartbeat_status_watcher",
                &self.heartbeat_status_watcher.is_some(),
            )
            .field("running", &self.running)
            .field("store", &self.store.is_some())
            .field("consecutive_failures", &self.consecutive_failures)
            .finish()
    }
}

/// Manages client session lifecycle (register, heartbeat, cleanup).
///
/// Improved version with:
/// - Single mutex to prevent race conditions
/// - Better error handling and state consistency
/// - Configurable timeout and retry policies
/// - UUID-based session identification
#[allow(dead_code)]
pub struct SessionManager {
    config: SessionConfig,
    state: Mutex<SessionState>,
}

#[allow(dead_code)]
impl SessionManager {
    pub fn new(heartbeat_interval: Duration) -> Self {
        Self::with_config(SessionConfig {
            heartbeat_interval,
            ..Default::default()
        })
    }

    pub fn with_config(config: SessionConfig) -> Self {
        Self {
            config,
            state: Mutex::new(SessionState {
                session_id: None,
                task_handle: None,
                shutdown_sender: None,
                heartbeat_status_watcher: None,
                running: false,
                store: None,
                consecutive_failures: 0,
            }),
        }
    }

    #[allow(dead_code)]
    pub async fn start(
        &self,
        store: Arc<dyn MetaStore + Send + Sync>,
        payload: Vec<u8>,
        update_existing: bool,
    ) -> Result<(), MetaError> {
        let mut state = self.state.lock().await;

        // Prevent concurrent starts using single mutex
        if state.running {
            debug!("SessionManager already running - skip start");
            return Ok(());
        }

        // Extract session identifier from payload for heartbeat refresh
        let session_id = match SessionId::from_payload(&payload) {
            Ok(id) => id,
            Err(err) => {
                warn!(
                    "Failed to parse session ID from payload: {}, using current",
                    err
                );
                SessionId::current()
            }
        };

        // Register or update the session before spawning the heartbeat loop
        // If registration fails, we return error without changing state
        store.new_session(&payload, update_existing).await?;

        // Mark as running only after successful registration
        state.running = true;
        state.session_id = Some(session_id.clone());
        state.store = Some(store.clone());
        state.consecutive_failures = 0;

        // Create shutdown channel and heartbeat task
        let (tx, mut rx) = oneshot::channel();
        let (status_tx, status_rx) = watch::channel(true); // true = heartbeat is running
        let config = self.config.clone();
        let store_clone = store.clone();
        let session_id_clone = session_id.clone();
        let payload_clone = payload.clone();

        let handle = tokio::spawn(async move {
            let mut consecutive_failures = 0u32;

            loop {
                tokio::select! {
                    _ = &mut rx => {
                        debug!("SessionManager heartbeat loop shutting down");
                        break;
                    }
                    _ = sleep(config.heartbeat_interval) => {
                        // Attempt to refresh the session
                        match store_clone.refresh_session_by_id(&session_id_clone).await {
                            Ok(()) => {
                                debug!("Session refreshed successfully");
                                consecutive_failures = 0;
                            }
                            Err(err) => {
                                consecutive_failures += 1;
                                warn!("Session refresh failed (attempt {}/{}): {}",
                                      consecutive_failures, config.max_recovery_attempts, err);

                                // Try recovery if enabled and within retry limit
                                if config.enable_recovery && consecutive_failures <= config.max_recovery_attempts {
                                    info!("Attempting session recovery...");
                                    if let Err(recovery_err) = store_clone.new_session(&payload_clone, true).await {
                                        error!("Session recovery failed: {}", recovery_err);
                                    } else {
                                        info!("Session recovered successfully");
                                        consecutive_failures = 0; // Reset on successful recovery
                                    }
                                } else if consecutive_failures > config.max_recovery_attempts {
                                    error!("Session heartbeat failed after {} attempts, stopping",
                                           consecutive_failures);
                                    // Notify SessionManager that heartbeat has failed permanently
                                    let _ = status_tx.send(false);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });

        // Store task handle and shutdown sender
        state.task_handle = Some(handle);
        state.shutdown_sender = Some(tx);
        state.heartbeat_status_watcher = Some(status_rx);

        info!(
            "Session started successfully with UUID: {}",
            session_id.uuid
        );
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn shutdown(&self) {
        let mut state = self.state.lock().await;

        if !state.running {
            debug!("SessionManager not running - nothing to shutdown");
            return;
        }

        // Send shutdown signal
        if let Some(tx) = state.shutdown_sender.take() {
            // It's ok if the receiver was already dropped
            let _ = tx.send(());
        }

        // Take the task handle and await its completion
        if let Some(handle) = state.task_handle.take() {
            // Drop the state lock before awaiting to avoid deadlock
            let session_id = state.session_id.clone();
            let store = state.store.clone();

            // Mark as not running before dropping the lock
            state.running = false;

            drop(state);

            // Await the heartbeat task completion
            if let Err(err) = handle.await {
                warn!("Failed to join session heartbeat task: {}", err);
            }

            // Clean up session data from store
            if let (Some(session_id), Some(store)) = (session_id, store) {
                debug!(
                    "Cleaning up session {} during shutdown",
                    session_id.to_key()
                );
                if let Err(err) = store.clean_session_by_id(&session_id).await {
                    warn!(
                        "Failed to clean up session {}: {}",
                        session_id.to_key(),
                        err
                    );
                } else {
                    info!("Successfully cleaned up session {}", session_id.to_key());
                }
            }
        } else {
            // No task handle, just mark as not running
            state.running = false;
            state.session_id = None;
            state.store = None;
            state.consecutive_failures = 0;
            state.heartbeat_status_watcher = None;
        }
    }

    /// Get current session status
    #[allow(dead_code)]
    pub async fn get_status(&self) -> SessionStatus {
        let mut state = self.state.lock().await;

        // Check heartbeat status through the watcher if available
        let heartbeat_active = if let Some(ref mut watcher) = state.heartbeat_status_watcher {
            // Check if heartbeat task has reported failure
            watcher
                .has_changed()
                .map(|_| *watcher.borrow())
                .unwrap_or(state.running)
        } else {
            state.running
        };

        // Update running state based on heartbeat status
        if !heartbeat_active && state.running {
            state.running = false;
            state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        }

        SessionStatus {
            running: heartbeat_active,
            session_id: state.session_id.clone(),
            consecutive_failures: state.consecutive_failures,
        }
    }

    /// Force session cleanup without shutting down the heartbeat task
    #[allow(dead_code)]
    pub async fn cleanup_session(&self) -> Result<(), MetaError> {
        // Acquire lock and extract session info
        let (session_id, store) = {
            let mut state = self.state.lock().await;
            if let (Some(session_id), Some(store)) = (state.session_id.clone(), state.store.clone())
            {
                // Mark as not running and clear session state
                state.running = false;
                state.session_id = None;
                state.store = None;
                state.consecutive_failures = 0;
                state.heartbeat_status_watcher = None;
                (session_id, store)
            } else {
                return Err(MetaError::Internal(
                    "No active session to cleanup".to_string(),
                ));
            }
        };
        // Perform cleanup outside the lock
        store.clean_session_by_id(&session_id).await?;
        Ok(())
    }
}

/// Session status information
#[derive(Debug, Clone)]
pub struct SessionStatus {
    pub running: bool,
    pub session_id: Option<SessionId>,
    pub consecutive_failures: u32,
}
