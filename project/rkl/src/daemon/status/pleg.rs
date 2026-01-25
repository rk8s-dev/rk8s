use std::{collections::HashSet, sync::Arc, time::Duration};

use anyhow::{anyhow, bail};
use dashmap::DashMap;
use libcontainer::container::{self, Container};
use tokio::{select, sync::mpsc::UnboundedReceiver};
use tracing::info;
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

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PodLifecycleEventType {
    ContainerCreating,
    ContainerStarted,
    ContainerDied,
    ContainerRemoved,
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

#[derive(Debug)]
pub struct PodLifecycleEvent {
    pub pod_uid: Uuid,
    pub pod_name: String,
    pub event_type: PodLifecycleEventType,
    pub container: Container,
}

impl PLEG {
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

    pub fn run(&mut self) -> UnboundedReceiver<PodLifecycleEvent> {
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
        self.relist_task_handle = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(state.relist_duration).await;

                // Check for stop signal
                // If received, break the loop and end the task
                // Otherwise, continue relisting pods

                select! {
                    _ = &mut stop_signal_rx => {
                        break;
                    }
                    _ = async {
                        // Perform relist operation
                        if let Ok(events) = relist(&mut state).await {
                            for event in events {
                                tracing::info!(
                                    "[pleg] Detected pod lifecycle event: pod_uid={}, event_type={:?}, container_id={}",
                                    event.pod_uid,
                                    event.event_type,
                                    event.container.id()
                                );
                                if let Err(e) = state.event_tx.as_ref().unwrap().send(event) {
                                    tracing::error!("[pleg] Failed to send pod lifecycle event: {:?}", e);
                                }
                            }
                        }

                    } => {}
                }
            }
        }));

        event_rx
    }

    pub fn stop(&mut self) {
        if let Some(stop_signal_tx) = self.stop_signal_tx.take() {
            let _ = stop_signal_tx.send(());
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
    info!("[pleg] Relisting pods for lifecycle events detection");

    // get all pods from cri
    let pods = get_pods(&state.rks_addr, &state.tls_cfg)
        .await?
        .into_iter()
        .map(Arc::new)
        .collect::<Vec<_>>();
    info!("[pleg] get {} pods", pods.len());
    // update cached pod records
    set_current_pod_records(&state.pod_records, &pods)?;

    let mut total_events = Vec::new();
    // let mut updates = Vec::new();
    for entry in state.pod_records.iter() {
        info!("[pleg] checking pod {:?}: {:?}", entry.key(), entry.value());
        let pod_id = *entry.key();
        let pod_record = entry.value();

        let old_pod = pod_record.old_pod.clone();
        let current_pod = pod_record.current_pod.clone();

        // get all containers from old and current pod
        let old_containers = if let Some(old_pod) = &old_pod {
            old_pod
                .containers
                .iter()
                .chain(old_pod.sanboxes.iter())
                .map(Arc::new)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let current_containers = if let Some(current_pod) = &current_pod {
            current_pod
                .containers
                .iter()
                .chain(current_pod.sanboxes.iter())
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

        info!(
            "[pleg] all containers for pod {:?}: {:?}",
            pod_id, all_containers
        );

        let mut events = Vec::new();
        for container in all_containers {
            let cid = &container.state.id;
            let event = generate_event(&old_pod, &current_pod, cid)?;
            events.extend(event);
        }
        info!("[pleg] generated events: {:?}", events);

        if events.is_empty() {
            continue;
        }

        total_events.extend(events);
    }
    Ok(total_events)
}

fn generate_event(
    old_pod: &Option<Arc<Pod>>,
    current_pod: &Option<Arc<Pod>>,
    cid: &str,
) -> anyhow::Result<Vec<PodLifecycleEvent>> {
    info!("[pleg] Generating event for container id: {}", cid);
    let (pod_uid, pod_name) = if let Some(pod) = old_pod {
        (pod.id, pod.name.clone())
    } else if let Some(pod) = current_pod {
        (pod.id, pod.name.clone())
    } else {
        bail!("Both old and current pod are None");
    };
    info!("[pleg] pod_uid: {}, pod_name: {}", pod_uid, pod_name);

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

    info!(
        "[pleg] old_container_state: {:?}, current_container_state: {:?}",
        old_container_state, current_container_state
    );

    if old_container_state == current_container_state {
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
    for pod in pods {
        let pod_id = pod.id;
        if let Some(mut record) = pod_records.get_mut(&pod_id) {
            let temp = record.current_pod.replace(pod.clone());
            record.old_pod = temp;
        } else {
            let record = PodRecord {
                old_pod: None,
                current_pod: Some(pod.clone()),
            };
            pod_records.insert(pod_id, record);
        }
    }
    for record in pod_records.iter() {
        let pod_id = *record.key();
        if !pods.iter().any(|p| p.id == pod_id) {
            pod_records.remove(&pod_id);
        }
    }

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
            sanboxes: Vec::new(),
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
