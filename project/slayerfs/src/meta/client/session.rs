use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{debug, warn};

use crate::meta::store::{MetaError, MetaStore};

/// Manages client session lifecycle (register, heartbeat, cleanup).
#[allow(dead_code)]
pub(crate) struct SessionManager {
    heartbeat_interval: Duration,
    handle: Mutex<Option<JoinHandle<()>>>,
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    /// true when a session is active or being started. Used to prevent
    /// concurrent start() calls from racing.
    running: AtomicBool,
}

#[allow(dead_code)]
impl SessionManager {
    pub(crate) fn new(heartbeat_interval: Duration) -> Self {
        Self {
            heartbeat_interval,
            handle: Mutex::new(None),
            shutdown_tx: Mutex::new(None),
            running: AtomicBool::new(false),
        }
    }

    #[allow(dead_code)]
    pub(crate) async fn start(
        &self,
        store: Arc<dyn MetaStore + Send + Sync>,
        payload: Vec<u8>,
        update_existing: bool,
    ) -> Result<(), MetaError> {
        // Prevent concurrent starts: set `running` to true if and only if
        // there is no active session. We keep the flag set for the whole
        // lifetime of the session and clear it on failure or shutdown. This
        // avoids the TOCTOU where two concurrent `start()` callers both pass
        // the empty-handle check and spawn duplicate heartbeat tasks.
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            debug!("SessionManager already running or starting - skip start");
            return Ok(());
        }

        // Register or update the session before spawning the heartbeat loop.
        // If registration fails we must clear `running` so subsequent
        // attempts are allowed.
        if let Err(err) = store.new_session(&payload, update_existing).await {
            self.running.store(false, Ordering::SeqCst);
            return Err(err);
        }

        // Create the shutdown channel and prepare the heartbeat task, but
        // don't store them into the manager fields until we hold both locks.
        // This prevents a race where `shutdown()` could send the shutdown
        // signal before the new handle is stored, leaving the spawned task
        // unnotified.
        let (tx, mut rx) = oneshot::channel();
        let interval = self.heartbeat_interval;
        let store_clone = store.clone();

        let handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut rx => {
                        debug!("SessionManager heartbeat loop shutting down");
                        break;
                    }
                    _ = sleep(interval) => {
                        if let Err(err) = store_clone.refresh_session().await {
                            warn!("Refresh session failed: {err}");
                        }
                    }
                }
            }
        });

        // Acquire both locks in a fixed order (shutdown_tx then handle) and
        // keep them held until the manager fields are updated. Holding the
        // locks prevents a concurrent `shutdown()` from sending the signal
        // before we've stored the newly created sender/handle.
        let mut shutdown_guard = self.shutdown_tx.lock().await;
        let mut handle_guard = self.handle.lock().await;
        *handle_guard = Some(handle);
        *shutdown_guard = Some(tx);

        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) async fn shutdown(&self) {
        // Acquire locks in the same order (`shutdown_tx` then `handle`) to
        // avoid deadlocks. Hold the locks while taking and signaling so a
        // concurrent `start()` cannot race to replace the fields between
        // these operations.
        let mut shutdown_guard = self.shutdown_tx.lock().await;
        let mut handle_guard = self.handle.lock().await;

        if let Some(tx) = shutdown_guard.take() {
            // It's ok if the receiver was already dropped.
            let _ = tx.send(());
        }

        if let Some(handle) = handle_guard.take() {
            // Drop locks before awaiting the join to avoid holding locks
            // across an await point which could cause deadlocks if the
            // spawned task tries to call back into this manager. We already
            // removed the handles from the manager so the task will observe
            // the shutdown signal and exit.
            drop(shutdown_guard);
            drop(handle_guard);
            if let Err(err) = handle.await {
                warn!("Failed to join session heartbeat task: {err}");
            }

            // Allow future start() calls.
            self.running.store(false, Ordering::SeqCst);
        }
    }
}
