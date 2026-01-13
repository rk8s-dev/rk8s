use std::sync::Arc;

use anyhow::{anyhow, bail};
use dashmap::DashMap;
use libcontainer::container::{self, Container};
use tokio::{select, sync::mpsc::UnboundedReceiver};
use tracing::info;
use uuid::Uuid;

use crate::{
    commands::pod::TLSConnectionArgs,
    daemon::status::{
        cache::PodStatusCache,
        pod::{Pod, get_pods},
    },
};

const POD_LIFECYCLE_DETECT_INTERVAL_SECS: u64 = 10;

#[derive(Debug, Clone)]
struct RelistDuration {
    relist_period: std::time::Duration,
    relist_threshold: std::time::Duration,
}

#[derive(Debug, Clone)]
struct PodRecord {
    old_pod: Option<Arc<Pod>>,
    current_pod: Option<Arc<Pod>>,
}

pub struct PLEG {
    rks_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    pod_status_cache: Arc<PodStatusCache>,
    pod_records: Arc<DashMap<Uuid, PodRecord>>,
    // pods_need_reinspect: Arc<DashMap<Uuid, ()>>,
    relist_duration: RelistDuration,
    relist_task_handle: Option<tokio::task::JoinHandle<()>>,
    event_tx: Option<tokio::sync::mpsc::UnboundedSender<PodLifecycleEvent>>,
    stop_signal_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Debug, Clone)]
struct State {
    rks_addr: String,
    tls_cfg: Arc<TLSConnectionArgs>,
    pod_records: Arc<DashMap<Uuid, PodRecord>>,
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
    pub fn new(rks_addr: String, tls_cfg: Arc<TLSConnectionArgs>) -> Self {
        Self {
            rks_addr,
            tls_cfg,
            pod_status_cache: Arc::new(PodStatusCache::new()),
            pod_records: Arc::new(DashMap::new()),
            relist_duration: RelistDuration {
                relist_period: std::time::Duration::from_secs(10),
                relist_threshold: std::time::Duration::from_secs(30),
            },
            relist_task_handle: None,
            event_tx: None,
            stop_signal_tx: None,
        }
    }

    pub fn start(&mut self) -> UnboundedReceiver<PodLifecycleEvent> {
        let (stop_signal_tx, mut stop_signal_rx) = tokio::sync::oneshot::channel();
        self.stop_signal_tx = Some(stop_signal_tx);
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        self.event_tx = Some(event_tx.clone());

        let mut state = State {
            rks_addr: self.rks_addr.clone(),
            tls_cfg: self.tls_cfg.clone(),
            pod_records: self.pod_records.clone(),
            event_tx: self.event_tx.clone(),
        };
        self.relist_task_handle = Some(tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(
                    POD_LIFECYCLE_DETECT_INTERVAL_SECS,
                ))
                .await;

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
                                    "Detected pod lifecycle event: pod_uid={}, event_type={:?}, container_id={}",
                                    event.pod_uid,
                                    event.event_type,
                                    event.container.id()
                                );
                                if let Err(e) = state.event_tx.as_ref().unwrap().send(event) {
                                    tracing::error!("Failed to send pod lifecycle event: {:?}", e);
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

async fn relist(state: &mut State) -> anyhow::Result<Vec<PodLifecycleEvent>> {
    info!("Relisting pods for lifecycle events detection");

    // get all pods from cri
    let pods = get_pods(&state.rks_addr, &state.tls_cfg)
        .await?
        .into_iter()
        .map(Arc::new)
        .collect::<Vec<_>>();
    // update cached pod records
    set_current_pod_records(&state.pod_records, &pods)?;

    let mut total_events = Vec::new();
    for entry in state.pod_records.iter() {
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
        let all_containers = old_containers
            .into_iter()
            .chain(current_containers.into_iter())
            .collect::<Vec<_>>();

        let mut events = Vec::new();
        for container in all_containers {
            let cid = &container.state.id;
            let event = generate_event(&old_pod, &current_pod, cid)?;
            events.extend(event);
        }

        if events.is_empty() {
            continue;
        }

        state.pod_records.insert(
            pod_id,
            PodRecord {
                old_pod: current_pod.clone(),
                current_pod: None,
            },
        );

        total_events.extend(events);
    }
    Ok(total_events)
}

fn generate_event(
    old_pod: &Option<Arc<Pod>>,
    current_pod: &Option<Arc<Pod>>,
    cid: &str,
) -> anyhow::Result<Vec<PodLifecycleEvent>> {
    let (pod_uid, pod_name) = if let Some(pod) = old_pod {
        (pod.id, pod.name.clone())
    } else if let Some(pod) = current_pod {
        (pod.id, pod.name.clone())
    } else {
        bail!("Both old and current pod are None");
    };

    let old_container = get_container(old_pod, cid).ok_or(anyhow!(
        "
        Cannot find container of {cid} in old pod
    "
    ))?;
    let old_container_state = create_container_state(&old_container)?;
    let current_container = get_container(current_pod, cid).ok_or(anyhow!(
        "
        Cannot find container of {cid} in current pod
    "
    ))?;
    let current_container_state = create_container_state(&current_container)?;

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
        let event = PodLifecycleEvent {
            pod_uid,
            pod_name: pod_name.clone(),
            event_type: et,
            container: current_container.clone(),
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
