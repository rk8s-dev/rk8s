//! Pod Lifecycle Event Generator (PLEG).
//!
//! PLEG is responsible for detecting container state transitions on the local
//! node. It works by periodically *relisting* all containers through the
//! container runtime and comparing the current snapshot with the previous one.
//! Any state change (e.g. a container that was `Created` is now `Running`) is
//! translated into a [`PodLifecycleEvent`] and sent to the [`super::super::pod_worker::PodWorker`]
//! via an unbounded MPSC channel.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut pleg = PLEG::new(server_addr, tls_cfg, Duration::from_secs(10));
//! let event_rx = pleg.run();
//! // Pass event_rx to PodWorker::new(...)
//! ```
//!
//! The relist interval controls how quickly state changes are detected. A
//! shorter interval increases responsiveness at the cost of higher CPU and
//! network usage.

use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{anyhow, bail};
use dashmap::DashMap;
use libcontainer::container::{self, Container};
use tokio::{select, sync::mpsc::UnboundedReceiver};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::{
    commands::pod::TLSConnectionArgs,
    daemon::status::pod::{Pod, get_pods},
};

#[derive(Debug, Clone)]
struct PodRecord {
    old_pod: Option<Arc<Pod>>,
    current_pod: Option<Arc<Pod>>,
}

/// Pod Lifecycle Event Generator.
///
/// Periodically queries the container runtime for current container states,
/// diffs them against the previous snapshot, and emits [`PodLifecycleEvent`]s.
///
/// # Lifecycle
///
/// 1. Create with [`PLEG::new`], supplying the rks server address, TLS config,
///    and the relist interval.
/// 2. Call [`PLEG::run`] to start the background relist loop. It returns an
///    [`UnboundedReceiver`] of events.
/// 3. Drop or call [`PLEG::stop`] to cancel the background task.
#[allow(clippy::upper_case_acronyms)]
pub struct PLEG {
    rks_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    pod_records: Arc<DashMap<Uuid, PodRecord>>,
    relist_duration: Duration,
    relist_task_handle: Option<tokio::task::JoinHandle<()>>,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<PodLifecycleEvent>>,
    stop_signal_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug, Clone)]
struct State {
    rks_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    pod_records: Arc<DashMap<Uuid, PodRecord>>,
    relist_duration: Duration,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<PodLifecycleEvent>>,
}

/// Discriminant for the kind of state transition detected by PLEG.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodLifecycleEventType {
    /// Container entered the `Creating` / `Created` state.
    ContainerCreating,
    /// Container transitioned to `Running`.
    ContainerStarted,
    /// Container transitioned to `Stopped` / exited.
    ContainerDied,
    /// Container was removed from the runtime entirely.
    ContainerRemoved,
    /// Container state changed to an unrecognised / unknown value.
    ContainerChanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ContainerState {
    Waiting,
    Running,
    Exited,
    Unknown,
    NonExistent,
}

/// A single pod lifecycle event emitted by PLEG.
///
/// Consumed by [`super::super::pod_worker::PodWorker`] to update the cached
/// [`common::PodStatus`] and trigger restart / probe logic.
#[derive(Debug)]
pub struct PodLifecycleEvent {
    /// UID of the pod this event belongs to.
    pub pod_uid: Uuid,
    /// Human-readable pod name (used for logging).
    pub pod_name: String,
    /// The kind of state transition that occurred.
    pub event_type: PodLifecycleEventType,
    /// The container whose state changed.
    pub container: Container,
}

impl PLEG {
    /// Creates a new PLEG instance.
    ///
    /// * `rks_addr` — Address of the rks API server (used to fetch pod lists).
    /// * `tls_cfg` — TLS configuration for QUIC connections.
    /// * `relist_duration` — Interval between consecutive relist cycles.
    pub fn new(
        rks_addr: String,
        tls_cfg: Arc<TLSConnectionArgs>,
        relist_duration: Duration,
    ) -> Self {
        Self {
            rks_addr,
            tls_cfg,
            pod_records: Arc::new(DashMap::new()),
            relist_duration,
            relist_task_handle: None,
            event_tx: None,
            stop_signal_tx: None,
        }
    }

    /// Starts the background relist loop and returns a receiver for lifecycle events.
    ///
    /// If called while already running, the existing relist loop is stopped and a
    /// fresh loop is started. The returned [`UnboundedReceiver`] should be passed to
    /// [`super::super::pod_worker::PodWorker::new`].
    pub fn run(&mut self) -> UnboundedReceiver<PodLifecycleEvent> {
        if let Some(handle) = &self.relist_task_handle {
            if !handle.is_finished() {
                warn!("[pleg] run() called while already running; restarting relist loop.");
                self.stop();
            } else {
                self.relist_task_handle = None;
                self.stop_signal_tx = None;
                self.event_tx = None;
            }
        }

        let (stop_signal_tx, mut stop_signal_rx) = tokio::sync::oneshot::channel();
        self.stop_signal_tx = Some(stop_signal_tx);
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        self.event_tx = Some(event_tx.clone());

        let mut state = State {
            rks_addr: self.rks_addr.clone(),
            tls_cfg: self.tls_cfg.clone(),
            pod_records: self.pod_records.clone(),
            relist_duration: self.relist_duration,
            event_tx: self.event_tx.clone(),
        };
        debug!(
            relist_duration = ?state.relist_duration,
            "[pleg] Starting relist loop"
        );
        self.relist_task_handle = Some(tokio::spawn(async move {
            loop {
                select! {
                    _ = &mut stop_signal_rx => {
                        debug!("[pleg] Received stop signal, exiting relist loop");
                        break;
                    }
                    _ = tokio::time::sleep(state.relist_duration) => {
                        // Perform relist operation
                        match relist(&mut state).await {
                            Ok(events) => {
                                let Some(event_tx) = state.event_tx.as_ref() else {
                                    tracing::warn!("[pleg] event channel is not available, stopping relist loop");
                                    break;
                                };
                                if events.is_empty() {
                                    tracing::debug!("[pleg] No pod lifecycle events detected in this relist cycle");
                                    continue;
                                }
                                info!(event_count = events.len(), "[pleg] Detected pod lifecycle events");
                                for event in events {
                                    tracing::debug!(
                                        pod_uid = %event.pod_uid,
                                        pod_name = %event.pod_name,
                                        event_type = ?event.event_type,
                                        container_id = %event.container.id(),
                                        "[pleg] Emitting pod lifecycle event"
                                    );
                                    if let Err(e) = event_tx.send(event) {
                                        tracing::error!("[pleg] Failed to send pod lifecycle event: {:?}", e);
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, "[pleg] Relist failed");
                            }
                        }
                    }
                }
            }
        }));

        event_rx
    }

    /// Signals the background relist loop to stop.
    pub fn stop(&mut self) {
        if let Some(stop_signal_tx) = self.stop_signal_tx.take() {
            let _ = stop_signal_tx.send(());
        }
        if let Some(relist_task_handle) = self.relist_task_handle.take() {
            relist_task_handle.abort();
        }
        self.event_tx = None;
    }
}

impl Drop for PLEG {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn relist(state: &mut State) -> anyhow::Result<Vec<PodLifecycleEvent>> {
    debug!("[pleg] Relisting pods for lifecycle event detection");

    // get all pods from cri
    let pods = get_pods(&state.rks_addr, &state.tls_cfg)
        .await?
        .into_iter()
        .map(Arc::new)
        .collect::<Vec<_>>();
    debug!(
        "[pleg] Retrieved {} pods from local runtime snapshot",
        pods.len()
    );
    // update cached pod records
    set_current_pod_records(&state.pod_records, &pods)?;

    let mut total_events = Vec::new();
    // let mut updates = Vec::new();
    for entry in state.pod_records.iter() {
        debug!("[pleg] checking pod {:?}: {:?}", entry.key(), entry.value());
        let pod_id = *entry.key();
        let pod_record = entry.value();

        let old_pod = pod_record.old_pod.clone();
        let current_pod = pod_record.current_pod.clone();

        // get all containers from old and current pod
        let old_containers = if let Some(old_pod) = &old_pod {
            old_pod
                .containers
                .iter()
                .chain(old_pod.sandboxes.iter())
                .map(Arc::new)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let current_containers = if let Some(current_pod) = &current_pod {
            current_pod
                .containers
                .iter()
                .chain(current_pod.sandboxes.iter())
                .map(Arc::new)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let mut seen_container_ids = HashSet::new();
        let all_containers = old_containers
            .into_iter()
            .chain(current_containers.into_iter())
            .filter(|c| seen_container_ids.insert(c.state.id.clone()))
            .collect::<Vec<_>>();

        debug!(
            "[pleg] all containers for pod {:?}: {:?}",
            pod_id, all_containers
        );

        let mut events = Vec::new();
        for container in all_containers {
            let cid = &container.state.id;
            let event = generate_event(&old_pod, &current_pod, cid)?;
            events.extend(event);
        }
        debug!("[pleg] generated events: {:?}", events);

        if events.is_empty() {
            continue;
        }

        total_events.extend(events);
    }
    debug!(
        pod_record_count = state.pod_records.len(),
        total_event_count = total_events.len(),
        "[pleg] Relist cycle completed"
    );
    Ok(total_events)
}

fn generate_event(
    old_pod: &Option<Arc<Pod>>,
    current_pod: &Option<Arc<Pod>>,
    cid: &str,
) -> anyhow::Result<Vec<PodLifecycleEvent>> {
    debug!(container_id = %cid, "[pleg] Generating lifecycle event candidates");
    let (pod_uid, pod_name) = if let Some(pod) = old_pod {
        (pod.id, pod.name.clone())
    } else if let Some(pod) = current_pod {
        (pod.id, pod.name.clone())
    } else {
        bail!("Both old and current pod are None");
    };
    debug!(pod_uid = %pod_uid, pod_name = %pod_name, "[pleg] Resolved pod identity for event generation");

    let old_container = get_container(old_pod, cid);
    let old_container_state = match old_container.as_ref() {
        Some(container) => create_container_state(container)?,
        None => ContainerState::NonExistent,
    };
    let current_container = get_container(current_pod, cid);
    let current_container_state = match current_container.as_ref() {
        Some(container) => create_container_state(container)?,
        None => ContainerState::NonExistent,
    };

    debug!(
        pod_uid = %pod_uid,
        pod_name = %pod_name,
        container_id = %cid,
        old_container_state = ?old_container_state,
        current_container_state = ?current_container_state,
        "[pleg] Computed container state transition"
    );

    if old_container_state == current_container_state {
        debug!(
            pod_uid = %pod_uid,
            pod_name = %pod_name,
            container_id = %cid,
            "[pleg] Container state unchanged; skipping event generation"
        );
        return Ok(Vec::new());
    }

    let event_type = match current_container_state {
        ContainerState::Waiting => vec![PodLifecycleEventType::ContainerCreating],
        ContainerState::Running => vec![PodLifecycleEventType::ContainerStarted],
        ContainerState::Exited => vec![PodLifecycleEventType::ContainerDied],
        ContainerState::NonExistent => match old_container_state {
            ContainerState::Exited => vec![PodLifecycleEventType::ContainerRemoved],
            _ => vec![
                PodLifecycleEventType::ContainerDied,
                PodLifecycleEventType::ContainerRemoved,
            ],
        },
        ContainerState::Unknown => vec![PodLifecycleEventType::ContainerChanged],
    };

    let mut events = Vec::new();
    for et in event_type {
        let container_for_event = current_container
            .as_ref()
            .or(old_container.as_ref())
            .ok_or_else(|| anyhow!("Cannot find container {cid} in both old and current pod"))?;
        let event = PodLifecycleEvent {
            pod_uid,
            pod_name: pod_name.clone(),
            event_type: et,
            container: container_for_event.clone(),
        };
        events.push(event);
    }
    debug!(
        pod_uid = %pod_uid,
        pod_name = %pod_name,
        container_id = %cid,
        generated_event_count = events.len(),
        "[pleg] Generated lifecycle events for container transition"
    );
    Ok(events)
}

fn create_container_state(container: &Container) -> anyhow::Result<ContainerState> {
    match container.state.status {
        container::ContainerStatus::Creating => Ok(ContainerState::Waiting),
        container::ContainerStatus::Created => Ok(ContainerState::Waiting),
        container::ContainerStatus::Running => Ok(ContainerState::Running),
        container::ContainerStatus::Stopped => Ok(ContainerState::Exited),
        _ => Ok(ContainerState::Unknown),
    }
}

fn get_container(pod: &Option<Arc<Pod>>, cid: &str) -> Option<Container> {
    pod.as_ref()
        .and_then(|p| p.get_container_by_id(cid))
        .cloned()
}

fn set_current_pod_records(
    pod_records: &Arc<DashMap<Uuid, PodRecord>>,
    pods: &[Arc<Pod>],
) -> anyhow::Result<()> {
    debug!(
        incoming_pod_count = pods.len(),
        cached_record_count = pod_records.len(),
        "[pleg] Updating cached pod records before diff"
    );
    let mut updated_records = 0usize;
    let mut inserted_records = 0usize;
    for pod in pods {
        let pod_id = pod.id;
        if let Some(mut record) = pod_records.get_mut(&pod_id) {
            let temp = record.current_pod.replace(pod.clone());
            record.old_pod = temp;
            updated_records += 1;
        } else {
            let record = PodRecord {
                old_pod: None,
                current_pod: Some(pod.clone()),
            };
            pod_records.insert(pod_id, record);
            inserted_records += 1;
        }
    }
    let mut removed_records = 0usize;
    for record in pod_records.iter() {
        let pod_id = *record.key();
        if !pods.iter().any(|p| p.id == pod_id) {
            pod_records.remove(&pod_id);
            removed_records += 1;
        }
    }
    debug!(
        updated_records,
        inserted_records,
        removed_records,
        cached_record_count = pod_records.len(),
        "[pleg] Cached pod records updated"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_container(id: &str, status: container::ContainerStatus) -> Container {
        let mut container = Container::default();
        container.state.id = id.to_string();
        container.state.status = status;
        container
    }

    fn make_pod(id: Uuid, name: &str, containers: Vec<Container>) -> Arc<Pod> {
        Arc::new(Pod {
            id,
            name: name.to_string(),
            namespace: "default".to_string(),
            containers,
            sandboxes: Vec::new(),
        })
    }

    #[test]
    fn create_container_state_maps_statuses() {
        let container = make_container("c1", container::ContainerStatus::Creating);
        assert_eq!(
            create_container_state(&container).unwrap(),
            ContainerState::Waiting
        );

        let container = make_container("c1", container::ContainerStatus::Created);
        assert_eq!(
            create_container_state(&container).unwrap(),
            ContainerState::Waiting
        );

        let container = make_container("c1", container::ContainerStatus::Running);
        assert_eq!(
            create_container_state(&container).unwrap(),
            ContainerState::Running
        );

        let container = make_container("c1", container::ContainerStatus::Stopped);
        assert_eq!(
            create_container_state(&container).unwrap(),
            ContainerState::Exited
        );

        let container = make_container("c1", container::ContainerStatus::Paused);
        assert_eq!(
            create_container_state(&container).unwrap(),
            ContainerState::Unknown
        );
    }

    #[test]
    fn generate_event_reports_container_started() {
        let pod_id = Uuid::new_v4();
        let old_pod = make_pod(
            pod_id,
            "pod-start",
            vec![make_container("c1", container::ContainerStatus::Created)],
        );
        let current_pod = make_pod(
            pod_id,
            "pod-start",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );

        let events = generate_event(&Some(old_pod), &Some(current_pod), "c1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type,
            PodLifecycleEventType::ContainerStarted
        );
    }

    #[test]
    fn generate_event_reports_container_started_when_old_missing() {
        let pod_id = Uuid::new_v4();
        let current_pod = make_pod(
            pod_id,
            "pod-start",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );

        let events = generate_event(&None, &Some(current_pod), "c1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type,
            PodLifecycleEventType::ContainerStarted
        );
    }

    #[test]
    fn generate_event_reports_container_died() {
        let pod_id = Uuid::new_v4();
        let old_pod = make_pod(
            pod_id,
            "pod-died",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );
        let current_pod = make_pod(
            pod_id,
            "pod-died",
            vec![make_container("c1", container::ContainerStatus::Stopped)],
        );

        let events = generate_event(&Some(old_pod), &Some(current_pod), "c1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, PodLifecycleEventType::ContainerDied);
    }

    #[test]
    fn generate_event_reports_container_changed() {
        let pod_id = Uuid::new_v4();
        let old_pod = make_pod(
            pod_id,
            "pod-changed",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );
        let current_pod = make_pod(
            pod_id,
            "pod-changed",
            vec![make_container("c1", container::ContainerStatus::Paused)],
        );

        let events = generate_event(&Some(old_pod), &Some(current_pod), "c1").unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].event_type,
            PodLifecycleEventType::ContainerChanged
        );
    }

    #[test]
    fn generate_event_returns_empty_when_no_state_change() {
        let pod_id = Uuid::new_v4();
        let old_pod = make_pod(
            pod_id,
            "pod-same",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );
        let current_pod = make_pod(
            pod_id,
            "pod-same",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );

        let events = generate_event(&Some(old_pod), &Some(current_pod), "c1").unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn set_current_pod_records_tracks_old_and_current() {
        let pod_records = Arc::new(DashMap::new());
        let pod_id = Uuid::new_v4();
        let pod_first = make_pod(
            pod_id,
            "pod-first",
            vec![make_container("c1", container::ContainerStatus::Running)],
        );
        let pod_second = make_pod(
            pod_id,
            "pod-second",
            vec![make_container("c1", container::ContainerStatus::Stopped)],
        );

        set_current_pod_records(&pod_records, &[pod_first.clone()]).unwrap();
        let record = pod_records.get(&pod_id).unwrap();
        assert!(record.old_pod.is_none());
        assert_eq!(record.current_pod.as_ref().unwrap().name, "pod-first");
        drop(record);

        set_current_pod_records(&pod_records, &[pod_second.clone()]).unwrap();
        let record = pod_records.get(&pod_id).unwrap();
        assert_eq!(record.old_pod.as_ref().unwrap().name, "pod-first");
        assert_eq!(record.current_pod.as_ref().unwrap().name, "pod-second");
    }
}
