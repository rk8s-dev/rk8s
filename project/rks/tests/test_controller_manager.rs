use anyhow::{Context, Result};
use async_trait::async_trait;
use common::ResourceKind;
use libvault::storage::xline::XlineOptions;
use rks::api::xlinestore::XlineStore;
use rks::controllers::manager::{Controller, ControllerManager, ResourceWatchResponse, WatchEvent};
use serial_test::serial;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use tokio::time::{Instant, sleep};

struct RecordingController {
    name: &'static str,
    watched: Vec<ResourceKind>,
    events: Arc<Mutex<Vec<ResourceWatchResponse>>>,
}

impl RecordingController {
    fn new(
        name: &'static str,
        watched: Vec<ResourceKind>,
        events: Arc<Mutex<Vec<ResourceWatchResponse>>>,
    ) -> Self {
        Self {
            name,
            watched,
            events,
        }
    }
}

#[async_trait]
impl Controller for RecordingController {
    fn name(&self) -> &'static str {
        self.name
    }

    fn watch_resources(&self) -> Vec<ResourceKind> {
        self.watched.clone()
    }

    async fn handle_watch_response(&mut self, response: &ResourceWatchResponse) -> Result<()> {
        self.events.lock().await.push(response.clone());
        Ok(())
    }
}

fn load_test_endpoints() -> Result<Vec<String>> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let config_path = std::env::var("TEST_CONFIG_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| Path::new(manifest).join("tests/config.yaml"));
    let config = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&config)
        .with_context(|| format!("failed to parse {}", config_path.display()))?;
    let endpoints = yaml["xline_config"]["endpoints"]
        .as_sequence()
        .context("tests/config.yaml is missing xline_config.endpoints")?
        .iter()
        .filter_map(|endpoint| endpoint.as_str().map(ToOwned::to_owned))
        .collect();
    Ok(endpoints)
}

async fn create_test_store() -> Result<Arc<XlineStore>> {
    let option = XlineOptions::new(load_test_endpoints()?);
    Ok(Arc::new(XlineStore::new(option).await?))
}

fn unique_name(prefix: &str) -> Result<String> {
    Ok(format!(
        "{prefix}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis()
    ))
}

fn load_fixture_with_name(file_name: &str, name: &str) -> Result<String> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(manifest).join("tests").join(file_name);
    let fixture = fs::read_to_string(&path)
        .with_context(|| format!("failed to read fixture {}", path.display()))?;
    let mut yaml: serde_yaml::Value = serde_yaml::from_str(&fixture)
        .with_context(|| format!("failed to parse fixture {}", path.display()))?;
    let metadata = yaml["metadata"]
        .as_mapping_mut()
        .context("fixture metadata must be a mapping")?;
    metadata.insert(
        serde_yaml::Value::String("name".to_string()),
        serde_yaml::Value::String(name.to_string()),
    );
    Ok(serde_yaml::to_string(&yaml)?)
}

fn pod_yaml(name: &str, image: &str, version: &str) -> Result<String> {
    let mut yaml: serde_yaml::Value =
        serde_yaml::from_str(&load_fixture_with_name("test-pod.yaml", name)?)?;
    yaml["metadata"]["labels"] = serde_yaml::to_value(serde_json::json!({ "version": version }))?;
    yaml["spec"]["containers"][0]["image"] = serde_yaml::Value::String(image.to_string());
    Ok(serde_yaml::to_string(&yaml)?)
}

async fn wait_for_event<F>(
    events: &Arc<Mutex<Vec<ResourceWatchResponse>>>,
    timeout: Duration,
    predicate: F,
) -> Result<ResourceWatchResponse>
where
    F: Fn(&ResourceWatchResponse) -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        let found = {
            let recorded = events.lock().await;
            recorded
                .iter()
                .find(|response| predicate(response))
                .cloned()
        };
        if let Some(response) = found {
            return Ok(response);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timeout waiting for matching watch event");
        }
        sleep(Duration::from_millis(100)).await;
    }
}

async fn wait_for_add_event(
    events: &Arc<Mutex<Vec<ResourceWatchResponse>>>,
    kind: ResourceKind,
    key: &str,
    timeout: Duration,
) -> Result<ResourceWatchResponse> {
    wait_for_event(events, timeout, |response| {
        response.kind == kind
            && response.key == key
            && matches!(response.event, WatchEvent::Add { .. })
    })
    .await
}

async fn has_event_for(
    events: &Arc<Mutex<Vec<ResourceWatchResponse>>>,
    kind: ResourceKind,
    key: &str,
) -> bool {
    events.lock().await.iter().any(|response| {
        response.kind == kind
            && response.key == key
            && matches!(response.event, WatchEvent::Add { .. })
    })
}

#[tokio::test]
#[serial]
async fn test_controller_manager_dispatches_only_to_matching_watchers() -> Result<()> {
    let store = create_test_store().await?;
    let manager = Arc::new(ControllerManager::new());

    let pod_events = Arc::new(Mutex::new(Vec::new()));
    let job_events = Arc::new(Mutex::new(Vec::new()));
    let mixed_events = Arc::new(Mutex::new(Vec::new()));

    let pod_controller = Arc::new(RwLock::new(RecordingController::new(
        "pod-recorder",
        vec![ResourceKind::Pod],
        pod_events.clone(),
    )));
    let job_controller = Arc::new(RwLock::new(RecordingController::new(
        "job-recorder",
        vec![ResourceKind::Job],
        job_events.clone(),
    )));
    let mixed_controller = Arc::new(RwLock::new(RecordingController::new(
        "mixed-recorder",
        vec![ResourceKind::Pod, ResourceKind::Job],
        mixed_events.clone(),
    )));

    manager.clone().register(pod_controller, 1).await?;
    manager.clone().register(job_controller, 1).await?;
    manager.clone().register(mixed_controller, 1).await?;

    let pod_name = unique_name("manager-watch-pod")?;
    let job_name = unique_name("manager-watch-job")?;

    let _ = store.delete_pod(&pod_name).await;
    let _ = store.delete_job(&job_name).await;

    manager.clone().start_watch(store.clone()).await?;
    sleep(Duration::from_millis(300)).await;

    let pod_yaml = load_fixture_with_name("test-pod.yaml", &pod_name)?;
    let job_yaml = load_fixture_with_name("test_job.yaml", &job_name)?;

    store.insert_pod_yaml(&pod_name, &pod_yaml).await?;
    store.insert_job_yaml(&job_name, &job_yaml).await?;

    wait_for_add_event(
        &pod_events,
        ResourceKind::Pod,
        &pod_name,
        Duration::from_secs(5),
    )
    .await?;
    wait_for_add_event(
        &job_events,
        ResourceKind::Job,
        &job_name,
        Duration::from_secs(5),
    )
    .await?;
    wait_for_add_event(
        &mixed_events,
        ResourceKind::Pod,
        &pod_name,
        Duration::from_secs(5),
    )
    .await?;
    wait_for_add_event(
        &mixed_events,
        ResourceKind::Job,
        &job_name,
        Duration::from_secs(5),
    )
    .await?;

    assert!(
        !has_event_for(&pod_events, ResourceKind::Job, &job_name).await,
        "Pod-only controller should not receive Job events"
    );
    assert!(
        !has_event_for(&job_events, ResourceKind::Pod, &pod_name).await,
        "Job-only controller should not receive Pod events"
    );

    manager.shutdown();
    let _ = store.delete_pod(&pod_name).await;
    let _ = store.delete_job(&job_name).await;

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_controller_manager_replays_snapshot_items_to_matching_watchers() -> Result<()> {
    let store = create_test_store().await?;
    let manager = Arc::new(ControllerManager::new());
    let pod_events = Arc::new(Mutex::new(Vec::new()));

    let pod_controller = Arc::new(RwLock::new(RecordingController::new(
        "snapshot-pod-recorder",
        vec![ResourceKind::Pod],
        pod_events.clone(),
    )));
    manager.clone().register(pod_controller, 1).await?;

    let pod_name = unique_name("manager-snapshot-pod")?;
    let _ = store.delete_pod(&pod_name).await;

    let yaml = pod_yaml(&pod_name, "busybox:1.0", "snapshot")?;
    store.insert_pod_yaml(&pod_name, &yaml).await?;

    manager.clone().start_watch(store.clone()).await?;

    let response = wait_for_add_event(
        &pod_events,
        ResourceKind::Pod,
        &pod_name,
        Duration::from_secs(5),
    )
    .await?;
    match response.event {
        WatchEvent::Add {
            yaml: replayed_yaml,
        } => {
            assert!(
                replayed_yaml.contains("busybox:1.0"),
                "snapshot replay should forward the stored pod yaml"
            );
        }
        other => anyhow::bail!("expected Add from snapshot replay, got {other:?}"),
    }

    manager.shutdown();
    let _ = store.delete_pod(&pod_name).await;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_controller_manager_translates_update_and_delete_events() -> Result<()> {
    let store = create_test_store().await?;
    let manager = Arc::new(ControllerManager::new());
    let pod_events = Arc::new(Mutex::new(Vec::new()));

    let pod_controller = Arc::new(RwLock::new(RecordingController::new(
        "event-pod-recorder",
        vec![ResourceKind::Pod],
        pod_events.clone(),
    )));
    manager.clone().register(pod_controller, 1).await?;

    let pod_name = unique_name("manager-update-pod")?;
    let _ = store.delete_pod(&pod_name).await;

    manager.clone().start_watch(store.clone()).await?;
    sleep(Duration::from_millis(300)).await;

    let initial_yaml = pod_yaml(&pod_name, "busybox:1.0", "v1")?;
    store.insert_pod_yaml(&pod_name, &initial_yaml).await?;
    wait_for_add_event(
        &pod_events,
        ResourceKind::Pod,
        &pod_name,
        Duration::from_secs(5),
    )
    .await?;

    let updated_yaml = pod_yaml(&pod_name, "busybox:2.0", "v2")?;
    store.insert_pod_yaml(&pod_name, &updated_yaml).await?;

    let update_response = wait_for_event(&pod_events, Duration::from_secs(5), |response| {
        response.kind == ResourceKind::Pod
            && response.key == pod_name
            && matches!(response.event, WatchEvent::Update { .. })
    })
    .await?;

    match update_response.event {
        WatchEvent::Update { old_yaml, new_yaml } => {
            assert!(
                old_yaml.contains("busybox:1.0"),
                "update event should keep previous yaml in old_yaml"
            );
            assert!(
                new_yaml.contains("busybox:2.0"),
                "update event should carry the new yaml"
            );
        }
        other => anyhow::bail!("expected Update event, got {other:?}"),
    }

    store.delete_pod(&pod_name).await?;

    let delete_response = wait_for_event(&pod_events, Duration::from_secs(5), |response| {
        response.kind == ResourceKind::Pod
            && response.key == pod_name
            && matches!(response.event, WatchEvent::Delete { .. })
    })
    .await?;

    match delete_response.event {
        WatchEvent::Delete { yaml } => {
            assert!(
                yaml.contains("busybox:2.0"),
                "delete event should use prev_kv and forward the latest yaml"
            );
        }
        other => anyhow::bail!("expected Delete event, got {other:?}"),
    }

    manager.shutdown();
    Ok(())
}
