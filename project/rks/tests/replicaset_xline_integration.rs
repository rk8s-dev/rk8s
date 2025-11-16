use anyhow::Result;
use libvault::storage::xline::XlineOptions;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::time::{Instant, sleep};

use common::{
    ContainerSpec, LabelSelector, ObjectMeta, PodSpec, PodTask, PodTemplateSpec, ReplicaSet,
    ReplicaSetSpec, ResourceKind,
};
use rks::api::xlinestore::XlineStore;
use rks::controllers::{ControllerManager, ReplicaSetController};
use serial_test::serial;

#[derive(Deserialize)]
struct TestCfg {
    xline_config: XlineCfg,
}

#[derive(Deserialize)]
struct XlineCfg {
    endpoints: Vec<String>,
}

fn load_test_config() -> Result<TestCfg> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let path = std::path::Path::new(manifest).join("tests/config.yaml");
    let s = std::fs::read_to_string(path)?;
    let cfg: TestCfg = serde_yaml::from_str(&s)?;
    Ok(cfg)
}

async fn setup_store_and_manager() -> Result<(
    Arc<XlineStore>,
    Arc<ControllerManager>,
    Arc<RwLock<ReplicaSetController>>,
)> {
    let _ = env_logger::builder().is_test(true).try_init();

    let cfg = load_test_config()?;
    let endpoints: Vec<String> = cfg.xline_config.endpoints;
    let option = XlineOptions::new(endpoints);
    let store: Arc<XlineStore> = Arc::new(XlineStore::new(option).await?);

    // clean up any existing ReplicaSets or Pods from previous tests
    cleanup_pods_by_prefix(&store, "test-rs").await?;
    cleanup_replicasets_by_prefix(&store, "test-rs").await?;

    let mgr = Arc::new(ControllerManager::new());
    let rs_ctrl = Arc::new(RwLock::new(ReplicaSetController::new(store.clone())));
    mgr.clone().register(rs_ctrl.clone(), 2).await?;
    mgr.clone().start_watch(store.clone()).await?;
    tokio::time::sleep(Duration::from_secs(1)).await;
    Ok((store, mgr, rs_ctrl))
}

fn make_test_replicaset(name: &str, replicas: i32) -> ReplicaSet {
    let mut labels = HashMap::new();
    labels.insert("app".to_string(), "test".to_string());
    ReplicaSet {
        api_version: "v1".to_string(),
        kind: "ReplicaSet".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "default".to_string(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            ..Default::default()
        },
        spec: ReplicaSetSpec {
            replicas,
            selector: LabelSelector {
                match_labels: labels.clone(),
                match_expressions: Vec::new(),
            },
            template: PodTemplateSpec {
                metadata: ObjectMeta {
                    name: format!("{}-pod-template", name),
                    namespace: "default".to_string(),
                    labels: labels.clone(),
                    annotations: HashMap::new(),
                    ..Default::default()
                },
                spec: PodSpec {
                    node_name: None,
                    containers: vec![ContainerSpec {
                        name: "c".to_string(),
                        image: "busybox:latest".to_string(),
                        ports: Vec::new(),
                        args: Vec::new(),
                        resources: None,
                        liveness_probe: None,
                        readiness_probe: None,
                        startup_probe: None,
                    }],
                    init_containers: Vec::new(),
                    tolerations: Vec::new(),
                },
            },
        },
        status: Default::default(),
    }
}

/// Helper to clean up all pods whose name contains the given prefix.
async fn cleanup_pods_by_prefix(store: &Arc<XlineStore>, prefix: &str) -> Result<()> {
    let pods = store.list_pods().await?;
    for p in pods {
        if p.metadata.name.contains(prefix) {
            let _ = store.delete_pod(&p.metadata.name).await;
        }
    }
    Ok(())
}

async fn cleanup_replicasets_by_prefix(store: &Arc<XlineStore>, prefix: &str) -> Result<()> {
    let replicasets = store.list_replicasets().await?;
    for rs in replicasets {
        if rs.metadata.name.contains(prefix) {
            let _ = store.delete_replicaset(&rs.metadata.name).await;
        }
    }
    Ok(())
}

async fn wait_for_pod_prefix_count(
    store: &Arc<XlineStore>,
    prefix: &str,
    expected: usize,
    timeout: Duration,
) -> Result<Vec<PodTask>> {
    let start = Instant::now();
    loop {
        let pods = store.list_pods().await?;
        let matching: Vec<PodTask> = pods
            .into_iter()
            .filter(|p| p.metadata.name.contains(prefix))
            .collect();

        if matching.len() == expected {
            return Ok(matching);
        }

        if Instant::now().duration_since(start) > timeout {
            let details: Vec<String> = matching
                .iter()
                .map(|p| {
                    let owners = p
                        .metadata
                        .owner_references
                        .as_ref()
                        .map(|refs| {
                            refs.iter()
                                .map(|o| format!("{}:{}", o.kind, o.name))
                                .collect::<Vec<_>>()
                                .join(",")
                        })
                        .unwrap_or_else(|| "<none>".to_string());
                    format!("{}[owners:{}]", p.metadata.name, owners)
                })
                .collect();

            return Err(anyhow::anyhow!(
                "timed out waiting for {} pods with prefix {} (found {} -> [{}])",
                expected,
                prefix,
                matching.len(),
                details.join("; ")
            ));
        }

        sleep(Duration::from_millis(500)).await;
    }
}

/// Ensures that creating a ReplicaSet automatically creates the desired number of Pods.
#[serial]
#[tokio::test]
async fn test_replicaset_creates_pods() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;
    let rs = make_test_replicaset("test-rs-create", 3);
    let yaml = serde_yaml::to_string(&rs)?;
    store
        .insert_replicaset_yaml(&rs.metadata.name, &yaml)
        .await?;
    sleep(Duration::from_secs(3)).await;

    let pods = store.list_pods().await?;
    let found = pods.iter().any(|p| {
        p.metadata
            .labels
            .get("app")
            .map(|v| v == "test")
            .unwrap_or(false)
    });

    // cleanup
    let _ = store.delete_replicaset(&rs.metadata.name).await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-create").await;

    assert!(
        found,
        "replicaset controller should create pods for the replicaset"
    );
    Ok(())
}

/// Ensures that when a Pod managed by a ReplicaSet is deleted, the controller recreates it.
#[serial]
#[tokio::test]
async fn test_replicaset_recreates_after_delete() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;
    let rs = make_test_replicaset("test-rs-recreate", 3);
    let yaml = serde_yaml::to_string(&rs)?;
    store
        .insert_replicaset_yaml(&rs.metadata.name, &yaml)
        .await?;
    sleep(Duration::from_secs(3)).await;

    let pods = store.list_pods().await?;
    let matching: Vec<_> = pods
        .into_iter()
        .filter(|p| {
            p.metadata
                .labels
                .get("app")
                .map(|v| v == "test")
                .unwrap_or(false)
        })
        .collect();
    assert!(!matching.is_empty(), "setup should have at least one pod");

    for p in matching.iter() {
        let name = p.metadata.name.clone();
        store.delete_pod(&name).await?;
    }

    sleep(Duration::from_secs(4)).await;

    let pods_after = store.list_pods().await?;
    let found_after = pods_after.iter().any(|p| {
        p.metadata
            .labels
            .get("app")
            .map(|v| v == "test")
            .unwrap_or(false)
    });

    // cleanup
    let _ = store.delete_replicaset(&rs.metadata.name).await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-recreate").await;

    assert!(
        found_after,
        "replicaset controller should have recreated a pod after deletion"
    );
    Ok(())
}

/// Ensures that scaling down the ReplicaSet (reducing replicas) correctly deletes extra Pods.
#[serial]
#[tokio::test]
async fn test_replicaset_scales_down() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;
    let rs = make_test_replicaset("test-rs-scale-down", 3);
    let yaml = serde_yaml::to_string(&rs)?;
    store
        .insert_replicaset_yaml(&rs.metadata.name, &yaml)
        .await?;
    let _ =
        wait_for_pod_prefix_count(&store, "test-rs-scale-down", 3, Duration::from_secs(15)).await?;

    let mut updated_rs = rs.clone();
    updated_rs.spec.replicas = 1;
    let yaml = serde_yaml::to_string(&updated_rs)?;
    store
        .insert_replicaset_yaml(&updated_rs.metadata.name, &yaml)
        .await?;
    let matching =
        wait_for_pod_prefix_count(&store, "test-rs-scale-down", 1, Duration::from_secs(15)).await?;

    // cleanup
    let _ = store.delete_replicaset(&rs.metadata.name).await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-scale-down").await;

    assert_eq!(
        matching.len(),
        1,
        "replicaset controller should scale down pods"
    );
    Ok(())
}

/// Ensures that scaling up the ReplicaSet creates additional Pods to meet the new desired replicas.
#[serial]
#[tokio::test]
async fn test_replicaset_scales_up() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;
    let rs = make_test_replicaset("test-rs-scale-up", 1);
    let yaml = serde_yaml::to_string(&rs)?;
    store
        .insert_replicaset_yaml(&rs.metadata.name, &yaml)
        .await?;
    let _ =
        wait_for_pod_prefix_count(&store, "test-rs-scale-up", 1, Duration::from_secs(15)).await?;

    let mut updated_rs = rs.clone();
    updated_rs.spec.replicas = 3;
    let yaml = serde_yaml::to_string(&updated_rs)?;
    store
        .insert_replicaset_yaml(&updated_rs.metadata.name, &yaml)
        .await?;
    let matching =
        wait_for_pod_prefix_count(&store, "test-rs-scale-up", 3, Duration::from_secs(20)).await?;

    // cleanup
    let _ = store.delete_replicaset(&rs.metadata.name).await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-scale-up").await;

    assert_eq!(matching.len(), 3, "replicaset should scale up to 3 pods");
    Ok(())
}

/// Ensures that a ReplicaSet can scale up and then scale down within the same workflow.
#[serial]
#[tokio::test]
async fn test_replicaset_create_then_scale_up_and_down() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;
    let rs = make_test_replicaset("test-rs-scale-both", 1);
    let yaml = serde_yaml::to_string(&rs)?;
    store
        .insert_replicaset_yaml(&rs.metadata.name, &yaml)
        .await?;
    let _ =
        wait_for_pod_prefix_count(&store, "test-rs-scale-both", 1, Duration::from_secs(15)).await?;

    let mut scaled_up_rs = rs.clone();
    scaled_up_rs.spec.replicas = 3;
    let yaml = serde_yaml::to_string(&scaled_up_rs)?;
    store
        .insert_replicaset_yaml(&scaled_up_rs.metadata.name, &yaml)
        .await?;
    let _ =
        wait_for_pod_prefix_count(&store, "test-rs-scale-both", 3, Duration::from_secs(20)).await?;

    let mut scaled_down_rs = scaled_up_rs.clone();
    scaled_down_rs.spec.replicas = 1;
    let yaml = serde_yaml::to_string(&scaled_down_rs)?;
    store
        .insert_replicaset_yaml(&scaled_down_rs.metadata.name, &yaml)
        .await?;
    let matching =
        wait_for_pod_prefix_count(&store, "test-rs-scale-both", 1, Duration::from_secs(20)).await?;

    // cleanup
    let _ = store.delete_replicaset(&rs.metadata.name).await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-scale-both").await;

    assert_eq!(
        matching.len(),
        1,
        "replicaset should scale back down to 1 pod"
    );
    Ok(())
}

/// Ensures that two ReplicaSets with different label selectors manage only their own Pods.
#[serial]
#[tokio::test]
async fn test_two_replicasets_independent() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;

    let mut rs_a = make_test_replicaset("test-rs-independent-a", 2);
    rs_a.spec
        .selector
        .match_labels
        .insert("tier".to_string(), "frontend".to_string());
    rs_a.spec
        .template
        .metadata
        .labels
        .insert("tier".to_string(), "frontend".to_string());
    let yaml_a = serde_yaml::to_string(&rs_a)?;
    store
        .insert_replicaset_yaml(&rs_a.metadata.name, &yaml_a)
        .await?;

    let mut rs_b = make_test_replicaset("test-rs-independent-b", 1);
    rs_b.spec
        .selector
        .match_labels
        .insert("tier".to_string(), "backend".to_string());
    rs_b.spec
        .template
        .metadata
        .labels
        .insert("tier".to_string(), "backend".to_string());
    let yaml_b = serde_yaml::to_string(&rs_b)?;
    store
        .insert_replicaset_yaml(&rs_b.metadata.name, &yaml_b)
        .await?;

    sleep(Duration::from_secs(5)).await;

    let pods = store.list_pods().await?;
    let pods_a: Vec<_> = pods
        .iter()
        .filter(|p| p.metadata.name.contains("test-rs-independent-a"))
        .collect();
    let pods_b: Vec<_> = pods
        .iter()
        .filter(|p| p.metadata.name.contains("test-rs-independent-b"))
        .collect();

    // cleanup
    let _ = cleanup_replicasets_by_prefix(&store, "test-rs-independent").await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-independent").await;

    assert_eq!(pods_a.len(), 2, "RS A should manage 2 pods");
    assert_eq!(pods_b.len(), 1, "RS B should manage 1 pod");
    Ok(())
}

/// Ensures that a ReplicaSet adopts orphan pods matching its selector by adding ownerReferences.
#[serial]
#[tokio::test]
async fn test_replicaset_adopts_orphan_pods() -> Result<()> {
    let (store, _mgr, _rs_ctrl) = setup_store_and_manager().await?;

    // Create 2 orphan pods with matching labels (no ownerReference)
    let mut orphan_labels = HashMap::new();
    orphan_labels.insert("app".to_string(), "test-adopt".to_string());

    for i in 1..=2 {
        let pod = PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: ObjectMeta {
                name: format!("test-rs-adopt-orphan-{}", i),
                namespace: "default".to_string(),
                labels: orphan_labels.clone(),
                annotations: HashMap::new(),
                owner_references: None, // No owner - this is an orphan pod
                ..Default::default()
            },
            spec: PodSpec {
                node_name: None,
                containers: vec![ContainerSpec {
                    name: "c".to_string(),
                    image: "busybox:latest".to_string(),
                    ports: Vec::new(),
                    args: Vec::new(),
                    resources: None,
                    liveness_probe: None,
                    readiness_probe: None,
                    startup_probe: None,
                }],
                init_containers: Vec::new(),
                tolerations: Vec::new(),
            },
            status: Default::default(),
        };
        let yaml = serde_yaml::to_string(&pod)?;
        store.insert_pod_yaml(&pod.metadata.name, &yaml).await?;
    }

    tokio::time::sleep(Duration::from_millis(500)).await;

    // Create a ReplicaSet with replicas=3 and matching selector
    let mut rs = make_test_replicaset("test-rs-adopt", 3);
    rs.spec.selector.match_labels.clear();
    rs.spec
        .selector
        .match_labels
        .insert("app".to_string(), "test-adopt".to_string());
    rs.spec.template.metadata.labels.clear();
    rs.spec
        .template
        .metadata
        .labels
        .insert("app".to_string(), "test-adopt".to_string());

    let yaml = serde_yaml::to_string(&rs)?;
    store
        .insert_replicaset_yaml(&rs.metadata.name, &yaml)
        .await?;

    // Wait for reconcile to adopt orphans and create 1 new pod
    let matching =
        wait_for_pod_prefix_count(&store, "test-rs-adopt", 3, Duration::from_secs(15)).await?;

    // Verify that orphan pods now have ownerReferences
    let mut adopted_count = 0;
    let mut created_count = 0;

    for pod in &matching {
        if pod.metadata.name.contains("orphan") {
            // This was an orphan pod, should now have ownerReference
            assert!(
                pod.metadata.owner_references.is_some(),
                "Orphan pod {} should have ownerReference after adoption",
                pod.metadata.name
            );

            let owners = pod.metadata.owner_references.as_ref().unwrap();
            assert_eq!(owners.len(), 1, "Should have exactly one owner");
            assert_eq!(owners[0].kind, ResourceKind::ReplicaSet);
            assert_eq!(owners[0].name, "test-rs-adopt");
            assert!(owners[0].controller);

            adopted_count += 1;
        } else {
            // This is a newly created pod by RS
            created_count += 1;
        }
    }

    // cleanup
    let _ = store.delete_replicaset(&rs.metadata.name).await;
    let _ = cleanup_pods_by_prefix(&store, "test-rs-adopt").await;

    assert_eq!(adopted_count, 2, "Should have adopted 2 orphan pods");
    assert_eq!(created_count, 1, "Should have created 1 new pod");
    assert_eq!(matching.len(), 3, "Total should be 3 pods");

    Ok(())
}
