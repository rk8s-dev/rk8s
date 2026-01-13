use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use common::{
    ContainerSpec, LabelSelector, ObjectMeta, OwnerReference, PodSpec, PodStatus, PodTask,
    PodTemplateSpec, ReplicaSet, ReplicaSetSpec, Resource, ResourceKind,
};
use libvault::storage::xline::XlineOptions;
use rks::{
    api::xlinestore::XlineStore,
    controllers::{ControllerManager, ReplicaSetController, garbage_collector::GarbageCollector},
    protocol::config::load_config,
};
use serial_test::serial;
use tokio::sync::RwLock;
use tokio::time::{Duration, Instant, sleep};
use uuid::Uuid;

// Get xline endpoints from config
fn get_xline_endpoints() -> Vec<String> {
    let config_path = std::env::var("TEST_CONFIG_PATH").unwrap_or_else(|_| {
        format!(
            "{}/tests/config.yaml",
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    });

    match load_config(&config_path) {
        Ok(config) => config.xline_config.endpoints.clone(),
        Err(_) => vec!["127.0.0.1:2379".to_string()], // fallback
    }
}

async fn get_store() -> Option<Arc<XlineStore>> {
    let endpoints = get_xline_endpoints();
    let option = XlineOptions::new(endpoints);

    match tokio::time::timeout(Duration::from_secs(5), XlineStore::new(option)).await {
        Ok(Ok(store)) => Some(Arc::new(store)),
        Ok(Err(err)) => {
            eprintln!("Skipping graph builder test: unable to connect to xline ({err})");
            None
        }
        Err(_) => {
            eprintln!("Skipping graph builder test: timeout connecting to xline");
            None
        }
    }
}

fn pod_with_meta(name: &str, uid: Uuid, owners: Option<Vec<OwnerReference>>) -> PodTask {
    let container = ContainerSpec {
        name: "main".to_string(),
        image: "busybox:latest".to_string(),
        ports: vec![],
        args: vec![],
        resources: Some(common::ContainerRes {
            limits: Some(Resource {
                cpu: Some("100m".to_string()),
                memory: Some("50Mi".to_string()),
            }),
        }),
        liveness_probe: None,
        readiness_probe: None,
        security_context: None,
        env: None,
        volume_mounts: None,
        command: None,
        working_dir: None,
        startup_probe: None,
    };

    let meta = ObjectMeta {
        name: name.to_string(),
        namespace: "default".to_string(),
        uid,
        owner_references: owners,
        finalizers: None,
        ..Default::default()
    };

    PodTask {
        api_version: "v1".to_string(),
        kind: "Pod".to_string(),
        metadata: meta,
        spec: PodSpec {
            node_name: None,
            containers: vec![container],
            init_containers: vec![],
            tolerations: vec![],
        },
        status: PodStatus::default(),
    }
}

fn replicaset_with_meta(name: &str, uid: Uuid, replicas: i32) -> ReplicaSet {
    let mut labels = HashMap::new();
    labels.insert("app".to_string(), name.to_string());

    ReplicaSet {
        api_version: "apps/v1".to_string(),
        kind: "ReplicaSet".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "default".to_string(),
            uid,
            labels: labels.clone(),
            ..Default::default()
        },
        spec: ReplicaSetSpec {
            replicas,
            selector: LabelSelector {
                match_labels: labels.clone(),
                match_expressions: vec![],
            },
            template: PodTemplateSpec {
                metadata: ObjectMeta {
                    name: format!("{}-pod-template", name),
                    namespace: "default".to_string(),
                    labels: labels.clone(),
                    ..Default::default()
                },
                spec: PodSpec {
                    node_name: None,
                    containers: vec![ContainerSpec {
                        name: "main".to_string(),
                        image: "busybox:latest".to_string(),
                        ports: vec![],
                        args: vec![],
                        resources: Some(common::ContainerRes {
                            limits: Some(Resource {
                                cpu: Some("100m".to_string()),
                                memory: Some("50Mi".to_string()),
                            }),
                        }),
                        liveness_probe: None,
                        readiness_probe: None,
                        startup_probe: None,
                        security_context: None,
                        env: None,
                        volume_mounts: None,
                        command: None,
                        working_dir: None,
                    }],
                    init_containers: vec![],
                    tolerations: vec![],
                },
            },
        },
        status: Default::default(),
    }
}

async fn wait_for_replicaset_pods(
    store: &XlineStore,
    rs_name: &str,
    expected_count: usize,
    timeout: Duration,
) -> Result<Vec<PodTask>> {
    let start = Instant::now();
    loop {
        let pods = store.list_pods().await?;
        let matching: Vec<PodTask> = pods
            .into_iter()
            .filter(|p| {
                p.metadata
                    .owner_references
                    .as_ref()
                    .map(|refs| {
                        refs.iter()
                            .any(|o| o.kind == ResourceKind::ReplicaSet && o.name == rs_name)
                    })
                    .unwrap_or(false)
            })
            .collect();

        if matching.len() == expected_count {
            return Ok(matching);
        }

        if Instant::now().duration_since(start) > timeout {
            anyhow::bail!(
                "timeout waiting for {} pods owned by ReplicaSet {} (found {})",
                expected_count,
                rs_name,
                matching.len()
            );
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn clean_store(store: &XlineStore) -> Result<()> {
    let pods = store
        .list_pods()
        .await?
        .into_iter()
        .filter(|pod| pod.metadata.name.starts_with("garbage-collector-test"))
        .collect::<Vec<_>>();

    for pod in pods {
        store.delete_pod(&pod.metadata.name).await?;
    }

    let replicasets = store
        .list_replicasets()
        .await?
        .into_iter()
        .filter(|rs| rs.metadata.name.starts_with("garbage-collector-test"))
        .collect::<Vec<_>>();

    for rs in replicasets {
        store.delete_replicaset(&rs.metadata.name).await?;
    }
    Ok(())
}

async fn setup_gc(
    store: Arc<XlineStore>,
) -> Result<(Arc<ControllerManager>, Arc<RwLock<GarbageCollector>>)> {
    let mgr = Arc::new(ControllerManager::new());
    let gc = Arc::new(RwLock::new(GarbageCollector::new(store.clone())));
    mgr.clone().register(gc.clone(), 4).await?;
    mgr.clone().start_watch(store.clone()).await?;
    sleep(Duration::from_millis(500)).await;
    Ok((mgr, gc))
}

async fn setup_gc_with_rs(
    store: Arc<XlineStore>,
) -> Result<(
    Arc<ControllerManager>,
    Arc<RwLock<GarbageCollector>>,
    Arc<RwLock<ReplicaSetController>>,
)> {
    let mgr = Arc::new(ControllerManager::new());
    let gc = Arc::new(RwLock::new(GarbageCollector::new(store.clone())));
    let rs_ctrl = Arc::new(RwLock::new(ReplicaSetController::new(store.clone())));
    mgr.clone().register(gc.clone(), 4).await?;
    mgr.clone().register(rs_ctrl.clone(), 2).await?;
    mgr.clone().start_watch(store.clone()).await?;
    sleep(Duration::from_millis(500)).await;
    Ok((mgr, gc, rs_ctrl))
}

fn yaml_contains_finalizer(yaml: &str, finalizer: &str) -> Result<bool> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let has = doc
        .get("metadata")
        .and_then(|meta| meta.get("finalizers"))
        .and_then(|f| f.as_sequence())
        .map(|seq| seq.iter().any(|item| item.as_str() == Some(finalizer)))
        .unwrap_or(false);
    Ok(has)
}

fn yaml_contains_owner_reference(yaml: &str, owner_uid: Uuid) -> Result<bool> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let has = doc
        .get("metadata")
        .and_then(|meta| meta.get("ownerReferences"))
        .and_then(|refs| refs.as_sequence())
        .map(|seq| {
            let owner_uid_str = owner_uid.to_string();
            seq.iter().any(|item| {
                item.get("uid")
                    .and_then(|uid| uid.as_str())
                    .map(|uid| uid == owner_uid_str)
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);
    Ok(has)
}

fn yaml_owner_references(yaml: &str) -> Result<Vec<OwnerReference>> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let Some(seq) = doc
        .get("metadata")
        .and_then(|meta| meta.get("ownerReferences"))
        .and_then(|refs| refs.as_sequence())
    else {
        return Ok(vec![]);
    };

    seq.iter()
        .map(|item| serde_yaml::from_value(item.clone()).map_err(anyhow::Error::from))
        .collect()
}

fn yaml_uid(yaml: &str) -> Result<Uuid> {
    let doc: serde_yaml::Value = serde_yaml::from_str(yaml)?;
    let uid_str = doc
        .get("metadata")
        .and_then(|meta| meta.get("uid"))
        .and_then(|uid| uid.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing metadata.uid"))?;
    Ok(Uuid::parse_str(uid_str)?)
}

#[tokio::test]
#[serial]
async fn test_graph_builder_sync_initial_graph_state() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let owner_name = format!("garbage-collector-test-owner-{}", owner_uid);
    let dependent_name = format!("garbage-collector-test-dependent-{}", dependent_uid);

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    let dependent_owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(false),
    };
    let dependent_pod = pod_with_meta(
        &dependent_name,
        dependent_uid,
        Some(vec![dependent_owner_ref.clone()]),
    );
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;

    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    // small delay to ensure etcd has applied writes before listing
    sleep(Duration::from_millis(200)).await;

    let (_mgr, gc) = setup_gc(store.clone()).await?;

    sleep(Duration::from_micros(200)).await;

    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };

    let owner_node = uid_to_node_table
        .get(&owner_uid)
        .await
        .expect("owner node missing");
    let dependent_node = uid_to_node_table
        .get(&dependent_uid)
        .await
        .expect("dependent node missing");

    let dependent_arc = {
        let owner_guard = owner_node.read().await;
        assert_eq!(owner_guard.dependents_length(), 1);
        owner_guard.dependents()[0].clone()
    };

    let attached_uid = dependent_arc.read().await.identity().uid;
    assert_eq!(attached_uid, dependent_uid);

    {
        let dependent_guard = dependent_node.read().await;
        assert_eq!(dependent_guard.owners().len(), 1);
        let owner_ref = &dependent_guard.owners()[0];
        assert_eq!(owner_ref.uid, owner_uid);
        assert_eq!(owner_ref.name, owner_name);
        assert_eq!(owner_ref.kind, ResourceKind::Pod);
        assert_eq!(owner_ref.block_owner_deletion, Some(false));
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_deletes_pod_with_dangling_owner() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let dependent_name = format!("garbage-collector-test-dangling-{}", dependent_uid);

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: format!("garbage-collector-test-missing-owner-{}", owner_uid),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(false),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let (_mgr, _gc) = setup_gc(store.clone()).await?;

    sleep(Duration::from_millis(1000)).await;
    assert!(store.get_pod(&dependent_name).await?.is_none());

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_foreground_deletion_clears_finalizer() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let owner_name = format!("garbage-collector-test-foreground-owner-{}", owner_uid);
    let dependent_name = format!(
        "garbage-collector-test-foreground-dependent-{}",
        dependent_uid
    );

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let (_mgr, _gc) = setup_gc(store.clone()).await?;

    sleep(Duration::from_millis(500)).await;

    store
        .delete_object(
            ResourceKind::Pod,
            &owner_name,
            common::DeletePropagationPolicy::Foreground,
        )
        .await?;

    let remove_dependent_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent_name).await?.is_none() {
            break;
        }
        if Instant::now() >= remove_dependent_deadline {
            panic!("dependent pod {dependent_name} still present after foreground deletion");
        }
        sleep(Duration::from_millis(100)).await;
    }

    let clear_finalizer_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        match store.get_pod_yaml(&owner_name).await? {
            None => break,
            Some(yaml) => {
                if !yaml_contains_finalizer(&yaml, "DeletingDependents")? {
                    break;
                }
            }
        }
        if Instant::now() >= clear_finalizer_deadline {
            panic!("DeletingDependents finalizer still present on owner pod {owner_name}");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_background_cascade_deletion() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let owner_name = format!("garbage-collector-test-background-owner-{}", owner_uid);
    let dependent_name = format!(
        "garbage-collector-test-background-dependent-{}",
        dependent_uid
    );

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let (_mgr, _gc) = setup_gc(store.clone()).await?;

    store
        .delete_object(
            ResourceKind::Pod,
            &owner_name,
            common::DeletePropagationPolicy::Background,
        )
        .await?;

    assert!(store.get_pod(&owner_name).await?.is_none());

    let dependent_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent_name).await?.is_none() {
            break;
        }
        if Instant::now() >= dependent_deadline {
            panic!("dependent pod {dependent_name} was not deleted under background policy");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_handles_dynamic_pod_changes_after_start() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let (_mgr, gc) = setup_gc(store.clone()).await?;

    // allow the watch tasks to come up before mutating state
    sleep(Duration::from_millis(200)).await;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let owner_name = format!("garbage-collector-test-dynamic-owner-{}", owner_uid);
    let dependent_name = format!("garbage-collector-test-dynamic-dependent-{}", dependent_uid);

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };

    let graph_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if uid_to_node_table.contains_key(&owner_uid).await
            && uid_to_node_table.contains_key(&dependent_uid).await
        {
            break;
        }
        if Instant::now() >= graph_deadline {
            panic!("dependency graph was not populated for dynamically added pods");
        }
        sleep(Duration::from_millis(100)).await;
    }

    let owner_node = uid_to_node_table
        .get(&owner_uid)
        .await
        .expect("owner node missing after dynamic add");
    {
        let owner_guard = owner_node.read().await;
        assert!(owner_guard.is_observed());
        assert_eq!(owner_guard.dependents_length(), 1);
        let dependent_uid_attached = owner_guard.dependents()[0].read().await.identity().uid;
        assert_eq!(dependent_uid_attached, dependent_uid);
    }

    store
        .delete_object(
            ResourceKind::Pod,
            &owner_name,
            common::DeletePropagationPolicy::Background,
        )
        .await?;

    let dependent_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent_name).await?.is_none() {
            break;
        }
        if Instant::now() >= dependent_deadline {
            panic!(
                "dependent pod {dependent_name} was not deleted after dynamic background cascade"
            );
        }
        sleep(Duration::from_millis(100)).await;
    }

    let graph_cleanup_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if !uid_to_node_table.contains_key(&owner_uid).await
            && !uid_to_node_table.contains_key(&dependent_uid).await
        {
            break;
        }
        if Instant::now() >= graph_cleanup_deadline {
            panic!("dependency graph retained nodes for deleted pods");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_dynamic_dependent_lifecycle() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let (_mgr, gc) = setup_gc(store.clone()).await?;

    sleep(Duration::from_millis(200)).await;

    let owner_uid = Uuid::new_v4();
    let owner_name = format!("garbage-collector-test-dynamic-owner-only-{}", owner_uid);

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };

    let owner_ready_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(owner_node) = uid_to_node_table.get(&owner_uid).await {
            let owner_guard = owner_node.read().await;
            if owner_guard.is_observed() {
                assert_eq!(owner_guard.dependents_length(), 0);
                break;
            }
        }
        if Instant::now() >= owner_ready_deadline {
            panic!("owner node did not appear after dynamic insertion");
        }
        sleep(Duration::from_millis(100)).await;
    }

    let dependent_uid = Uuid::new_v4();
    let dependent_name = format!(
        "garbage-collector-test-dynamic-dependent-lifecycle-{}",
        dependent_uid
    );

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let dependents_ready_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let owner_has_dependent = if let Some(owner_node) = uid_to_node_table.get(&owner_uid).await
        {
            owner_node.read().await.dependents_length() == 1
        } else {
            false
        };

        if owner_has_dependent && uid_to_node_table.contains_key(&dependent_uid).await {
            break;
        }
        if Instant::now() >= dependents_ready_deadline {
            panic!("dynamic dependent was not linked to owner");
        }
        sleep(Duration::from_millis(100)).await;
    }

    let dependent_node = uid_to_node_table
        .get(&dependent_uid)
        .await
        .expect("dependent node missing after dynamic insertion");
    {
        let dependent_guard = dependent_node.read().await;
        assert_eq!(dependent_guard.owners().len(), 1);
        assert_eq!(dependent_guard.owners()[0].uid, owner_uid);
    }

    store.delete_pod(&dependent_name).await?;

    let dependent_removed_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let dependent_missing_in_store = store.get_pod(&dependent_name).await?.is_none();
        let dependent_missing_in_graph = !uid_to_node_table.contains_key(&dependent_uid).await;
        let owner_without_dependents =
            if let Some(owner_node) = uid_to_node_table.get(&owner_uid).await {
                owner_node.read().await.dependents_length() == 0
            } else {
                false
            };

        if dependent_missing_in_store && dependent_missing_in_graph && owner_without_dependents {
            break;
        }
        if Instant::now() >= dependent_removed_deadline {
            panic!("dynamic dependent removal did not propagate to GC graph");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_dynamic_foreground_deletion_clears_finalizer() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let (_mgr, gc) = setup_gc(store.clone()).await?;

    sleep(Duration::from_millis(200)).await;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let owner_name = format!(
        "garbage-collector-test-dynamic-foreground-owner-{}",
        owner_uid
    );
    let dependent_name = format!(
        "garbage-collector-test-dynamic-foreground-dependent-{}",
        dependent_uid
    );

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };
    let graph_ready_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let owner_ready = uid_to_node_table.contains_key(&owner_uid).await;
        let dependent_ready = uid_to_node_table.contains_key(&dependent_uid).await;

        if owner_ready && dependent_ready {
            break;
        }
        if Instant::now() >= graph_ready_deadline {
            panic!("GC graph did not observe dynamic foreground pods");
        }
        sleep(Duration::from_millis(100)).await;
    }

    store
        .delete_object(
            ResourceKind::Pod,
            &owner_name,
            common::DeletePropagationPolicy::Foreground,
        )
        .await?;

    let dependent_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent_name).await?.is_none() {
            break;
        }
        if Instant::now() >= dependent_deadline {
            panic!(
                "dependent pod {dependent_name} was not deleted after dynamic foreground cascade"
            );
        }
        sleep(Duration::from_millis(100)).await;
    }

    let finalizer_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        match store.get_pod_yaml(&owner_name).await? {
            None => break,
            Some(yaml) => {
                if !yaml_contains_finalizer(&yaml, "DeletingDependents")? {
                    break;
                }
            }
        }
        if Instant::now() >= finalizer_deadline {
            panic!("DeletingDependents finalizer persisted on dynamically deleted owner");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_orphan_policy_removes_owner_reference() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let owner_uid = Uuid::new_v4();
    let dependent_uid = Uuid::new_v4();
    let owner_name = format!("garbage-collector-test-orphan-owner-{}", owner_uid);
    let dependent_name = format!("garbage-collector-test-orphan-dependent-{}", dependent_uid);

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };

    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let (_mgr, _gc) = setup_gc(store.clone()).await?;

    store
        .delete_object(
            ResourceKind::Pod,
            &owner_name,
            common::DeletePropagationPolicy::Orphan,
        )
        .await?;

    let clear_owner_reference_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if let Some(yaml) = store.get_pod_yaml(&dependent_name).await? {
            if !yaml_contains_owner_reference(&yaml, owner_uid)? {
                break;
            }
        } else {
            panic!("dependent pod {dependent_name} unexpectedly deleted during orphan policy test");
        }

        if Instant::now() >= clear_owner_reference_deadline {
            panic!(
                "owner reference for uid {owner_uid} still present on dependent pod {dependent_name}"
            );
        }
        sleep(Duration::from_millis(100)).await;
    }

    let clear_finalizer_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        match store.get_pod_yaml(&owner_name).await? {
            None => break,
            Some(yaml) => {
                if !yaml_contains_finalizer(&yaml, "OrphanDependents")? {
                    break;
                }
            }
        }

        if Instant::now() >= clear_finalizer_deadline {
            panic!("OrphanDependents finalizer still present on owner pod {owner_name}");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_removes_mismatched_owner_reference() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let solid_owner_uid = Uuid::new_v4();
    let solid_owner_name = format!("garbage-collector-test-solid-owner-{}", solid_owner_uid);
    let solid_owner = pod_with_meta(&solid_owner_name, solid_owner_uid, None);
    let solid_yaml = serde_yaml::to_string(&solid_owner)?;
    store
        .insert_pod_yaml(&solid_owner_name, &solid_yaml)
        .await?;

    let backing_owner_uid = Uuid::new_v4();
    let backing_owner_name = format!(
        "garbage-collector-test-mismatch-owner-{}",
        backing_owner_uid
    );
    let backing_owner = pod_with_meta(&backing_owner_name, backing_owner_uid, None);
    let backing_yaml = serde_yaml::to_string(&backing_owner)?;
    store
        .insert_pod_yaml(&backing_owner_name, &backing_yaml)
        .await?;

    let mismatched_owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: backing_owner_name.clone(),
        uid: Uuid::new_v4(),
        controller: true,
        block_owner_deletion: Some(false),
    };
    let solid_owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: solid_owner_name.clone(),
        uid: solid_owner_uid,
        controller: true,
        block_owner_deletion: Some(false),
    };

    let dependent_uid = Uuid::new_v4();
    let dependent_name = format!(
        "garbage-collector-test-owner-cleanup-dependent-{}",
        dependent_uid
    );
    let dependent = pod_with_meta(
        &dependent_name,
        dependent_uid,
        Some(vec![solid_owner_ref.clone(), mismatched_owner_ref]),
    );
    let dependent_yaml = serde_yaml::to_string(&dependent)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let (_mgr, _gc) = setup_gc(store.clone()).await?;

    let cleanup_deadline = Instant::now() + Duration::from_secs(6);
    let mut last_refs = Vec::new();
    loop {
        if Instant::now() >= cleanup_deadline {
            anyhow::bail!(
                "owner references not cleaned: {:?}",
                last_refs
                    .into_iter()
                    .map(|o: OwnerReference| o.uid)
                    .collect::<Vec<_>>()
            );
        }

        if let Some(current_yaml) = store.get_pod_yaml(&dependent_name).await? {
            let owner_refs = yaml_owner_references(&current_yaml)?;
            last_refs = owner_refs.clone();
            if owner_refs.len() == 1 && owner_refs[0].uid == solid_owner_uid {
                break;
            }
        }

        sleep(Duration::from_millis(100)).await;
    }

    if let Some(cleaned_yaml) = store.get_pod_yaml(&dependent_name).await? {
        let owner_refs = yaml_owner_references(&cleaned_yaml)?;
        assert_eq!(owner_refs.len(), 1);
        assert_eq!(owner_refs[0].uid, solid_owner_uid);
        assert_eq!(owner_refs[0].name, solid_owner_name);
    } else {
        anyhow::bail!("dependent pod {dependent_name} unexpectedly deleted");
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_handles_owner_recreation_with_new_uid() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let original_owner_uid = Uuid::new_v4();
    let owner_name = format!(
        "garbage-collector-test-owner-recreate-{}",
        original_owner_uid
    );

    let dependent_uid = Uuid::new_v4();
    let dependent_name = format!(
        "garbage-collector-test-owner-recreate-dependent-{}",
        dependent_uid
    );

    let owner_pod = pod_with_meta(&owner_name, original_owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: original_owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };
    let dependent_pod = pod_with_meta(&dependent_name, dependent_uid, Some(vec![owner_ref]));
    let dependent_yaml = serde_yaml::to_string(&dependent_pod)?;
    store
        .insert_pod_yaml(&dependent_name, &dependent_yaml)
        .await?;

    let (_mgr, gc) = setup_gc(store.clone()).await?;

    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };
    let graph_ready_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if uid_to_node_table.contains_key(&original_owner_uid).await
            && uid_to_node_table.contains_key(&dependent_uid).await
        {
            break;
        }
        if Instant::now() >= graph_ready_deadline {
            panic!("dependency graph not ready for owner recreation test");
        }
        sleep(Duration::from_millis(100)).await;
    }

    store.delete_pod(&owner_name).await?;
    let recreated_owner_uid = Uuid::new_v4();
    let recreated_owner = pod_with_meta(&owner_name, recreated_owner_uid, None);
    let recreated_yaml = serde_yaml::to_string(&recreated_owner)?;
    store.insert_pod_yaml(&owner_name, &recreated_yaml).await?;

    let owner_deleted_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        let pod = store.get_pod(&owner_name).await?;
        if pod.is_none()
            || pod.is_some() && pod.as_ref().unwrap().metadata.uid != original_owner_uid
        {
            break;
        }
        if Instant::now() >= owner_deleted_deadline {
            panic!("owner pod {owner_name} still present after explicit deletion");
        }
        sleep(Duration::from_millis(100)).await;
    }

    let dependent_cleanup_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent_name).await?.is_none() {
            break;
        }
        if Instant::now() >= dependent_cleanup_deadline {
            panic!(
                "dependent pod {dependent_name} still exists after owner deletion removed its controlling uid"
            );
        }
        sleep(Duration::from_millis(100)).await;
    }

    let dependent_graph_cleanup_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if !uid_to_node_table.contains_key(&dependent_uid).await {
            break;
        }
        if Instant::now() >= dependent_graph_cleanup_deadline {
            panic!("dependency graph still contains dependent after owner deletion");
        }
        sleep(Duration::from_millis(100)).await;
    }

    if let Some(old_owner_node) = uid_to_node_table.get(&original_owner_uid).await {
        let guard = old_owner_node.read().await;
        assert!(guard.is_virtual());
        assert_eq!(guard.dependents_length(), 0);
    }

    let new_owner_graph_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if uid_to_node_table.contains_key(&recreated_owner_uid).await {
            break;
        }
        if Instant::now() >= new_owner_graph_deadline {
            panic!("graph did not observe recreated owner with new uid");
        }
        sleep(Duration::from_millis(100)).await;
    }

    if let Some(owner_yaml) = store.get_pod_yaml(&owner_name).await? {
        assert_eq!(yaml_uid(&owner_yaml)?, recreated_owner_uid);
    } else {
        anyhow::bail!("recreated owner pod {owner_name} unexpectedly missing");
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_three_level_cascade_on_owner_delete() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    clean_store(&store).await?;

    let owner_uid = Uuid::new_v4();
    let dependent1_uid = Uuid::new_v4();
    let dependent2_uid = Uuid::new_v4();

    let owner_name = format!("garbage-collector-test-chain-owner-{}", owner_uid);
    let dependent1_name = format!("garbage-collector-test-chain-dependent1-{}", dependent1_uid);
    let dependent2_name = format!("garbage-collector-test-chain-dependent2-{}", dependent2_uid);

    let owner_pod = pod_with_meta(&owner_name, owner_uid, None);
    let owner_yaml = serde_yaml::to_string(&owner_pod)?;
    store.insert_pod_yaml(&owner_name, &owner_yaml).await?;

    let owner_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: owner_name.clone(),
        uid: owner_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };
    let dependent1_pod = pod_with_meta(
        &dependent1_name,
        dependent1_uid,
        Some(vec![owner_ref.clone()]),
    );
    let dependent1_yaml = serde_yaml::to_string(&dependent1_pod)?;
    store
        .insert_pod_yaml(&dependent1_name, &dependent1_yaml)
        .await?;

    let dependent1_ref = OwnerReference {
        api_version: "v1".to_string(),
        kind: ResourceKind::Pod,
        name: dependent1_name.clone(),
        uid: dependent1_uid,
        controller: true,
        block_owner_deletion: Some(true),
    };
    let dependent2_pod =
        pod_with_meta(&dependent2_name, dependent2_uid, Some(vec![dependent1_ref]));
    let dependent2_yaml = serde_yaml::to_string(&dependent2_pod)?;
    store
        .insert_pod_yaml(&dependent2_name, &dependent2_yaml)
        .await?;

    let (_mgr, _gc) = setup_gc(store.clone()).await?;

    store
        .delete_object(
            ResourceKind::Pod,
            &owner_name,
            common::DeletePropagationPolicy::Background,
        )
        .await?;

    assert!(store.get_pod(&owner_name).await?.is_none());

    let dependent1_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent1_name).await?.is_none() {
            break;
        }
        if Instant::now() >= dependent1_deadline {
            panic!("dependent1 pod {dependent1_name} was not deleted");
        }
        sleep(Duration::from_millis(100)).await;
    }

    let dependent2_deadline = Instant::now() + Duration::from_secs(6);
    loop {
        if store.get_pod(&dependent2_name).await?.is_none() {
            break;
        }
        if Instant::now() >= dependent2_deadline {
            panic!("dependent2 pod {dependent2_name} was not deleted after owner removal");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_replicaset_background_cascade_deletion() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    let rs_uid = Uuid::new_v4();
    let rs_name = format!("garbage-collector-test-rs-background-{}", rs_uid);

    let rs = replicaset_with_meta(&rs_name, rs_uid, 2);
    let rs_yaml = serde_yaml::to_string(&rs)?;
    store.insert_replicaset_yaml(&rs_name, &rs_yaml).await?;

    let (_mgr, _gc, _rs_ctrl) = setup_gc_with_rs(store.clone()).await?;

    // 等待 ReplicaSet 创建 Pod
    let pods = wait_for_replicaset_pods(&store, &rs_name, 2, Duration::from_secs(10)).await?;
    assert_eq!(pods.len(), 2, "ReplicaSet should create 2 pods");

    // 删除 ReplicaSet (Background 策略)
    store
        .delete_object(
            ResourceKind::ReplicaSet,
            &rs_name,
            common::DeletePropagationPolicy::Background,
        )
        .await?;

    // 验证 ReplicaSet 被删除
    assert!(store.get_replicaset_yaml(&rs_name).await?.is_none());

    // 验证 Pod 被级联删除
    let pods_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining_pods =
            wait_for_replicaset_pods(&store, &rs_name, 0, Duration::from_millis(100))
                .await
                .unwrap_or_default();
        if remaining_pods.is_empty() {
            break;
        }
        if Instant::now() >= pods_deadline {
            panic!("pods owned by ReplicaSet {rs_name} were not deleted under background policy");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_replicaset_foreground_deletion_clears_finalizer() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    let rs_uid = Uuid::new_v4();
    let rs_name = format!("garbage-collector-test-rs-foreground-{}", rs_uid);

    let rs = replicaset_with_meta(&rs_name, rs_uid, 2);
    let rs_yaml = serde_yaml::to_string(&rs)?;
    store.insert_replicaset_yaml(&rs_name, &rs_yaml).await?;

    let (_mgr, gc, _rs_ctrl) = setup_gc_with_rs(store.clone()).await?;

    // 等待 ReplicaSet 创建 Pod
    let pods = wait_for_replicaset_pods(&store, &rs_name, 2, Duration::from_secs(10)).await?;
    assert_eq!(pods.len(), 2, "ReplicaSet should create 2 pods");

    // 等待 ReplicaSet 被添加到依赖图中并被观察
    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };
    let rs_observed_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(rs_node) = uid_to_node_table.get(&rs_uid).await {
            let rs_guard = rs_node.read().await;
            if rs_guard.is_observed() {
                break;
            }
        }
        if Instant::now() >= rs_observed_deadline {
            panic!("ReplicaSet {rs_name} was not observed in dependency graph");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 删除 ReplicaSet (Foreground 策略)
    store
        .delete_object(
            ResourceKind::ReplicaSet,
            &rs_name,
            common::DeletePropagationPolicy::Foreground,
        )
        .await?;

    // 验证 Pod 被删除
    let pods_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let remaining_pods =
            wait_for_replicaset_pods(&store, &rs_name, 0, Duration::from_millis(100))
                .await
                .unwrap_or_default();
        if remaining_pods.is_empty() {
            break;
        }
        if Instant::now() >= pods_deadline {
            panic!("pods owned by ReplicaSet {rs_name} were not deleted under foreground policy");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 验证 finalizer 被清除
    let finalizer_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match store.get_replicaset_yaml(&rs_name).await? {
            None => break,
            Some(yaml) => {
                if !yaml_contains_finalizer(&yaml, "DeletingDependents")? {
                    break;
                }
            }
        }
        if Instant::now() >= finalizer_deadline {
            panic!("DeletingDependents finalizer still present on ReplicaSet {rs_name}");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_replicaset_orphan_policy_removes_owner_reference() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    let rs_uid = Uuid::new_v4();
    let rs_name = format!("garbage-collector-test-rs-orphan-{}", rs_uid);

    let rs = replicaset_with_meta(&rs_name, rs_uid, 2);
    let rs_yaml = serde_yaml::to_string(&rs)?;
    store.insert_replicaset_yaml(&rs_name, &rs_yaml).await?;

    let (_mgr, gc, _rs_ctrl) = setup_gc_with_rs(store.clone()).await?;

    // 等待 ReplicaSet 创建 Pod
    let pods = wait_for_replicaset_pods(&store, &rs_name, 2, Duration::from_secs(10)).await?;
    assert_eq!(pods.len(), 2, "ReplicaSet should create 2 pods");
    let pod_name = &pods[0].metadata.name;

    // 等待 ReplicaSet 被添加到依赖图中并被观察
    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };
    let rs_observed_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(rs_node) = uid_to_node_table.get(&rs_uid).await {
            let rs_guard = rs_node.read().await;
            if rs_guard.is_observed() {
                break;
            }
        }
        if Instant::now() >= rs_observed_deadline {
            panic!("ReplicaSet {rs_name} was not observed in dependency graph");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 删除 ReplicaSet (Orphan 策略)
    store
        .delete_object(
            ResourceKind::ReplicaSet,
            &rs_name,
            common::DeletePropagationPolicy::Orphan,
        )
        .await?;

    // 验证 Pod 的 owner reference 被移除
    let clear_owner_reference_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(yaml) = store.get_pod_yaml(pod_name).await? {
            if !yaml_contains_owner_reference(&yaml, rs_uid)? {
                break;
            }
        } else {
            panic!("pod {pod_name} unexpectedly deleted during orphan policy test");
        }

        if Instant::now() >= clear_owner_reference_deadline {
            panic!("owner reference for ReplicaSet {rs_uid} still present on pod {pod_name}");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 验证 finalizer 被清除
    let clear_finalizer_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match store.get_replicaset_yaml(&rs_name).await? {
            None => break,
            Some(yaml) => {
                if !yaml_contains_finalizer(&yaml, "OrphanDependents")? {
                    break;
                }
            }
        }

        if Instant::now() >= clear_finalizer_deadline {
            panic!("OrphanDependents finalizer still present on ReplicaSet {rs_name}");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 验证 Pod 仍然存在（没有被删除）
    assert!(
        store.get_pod(pod_name).await?.is_some(),
        "pod {pod_name} should still exist after orphan policy"
    );

    clean_store(&store).await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_gc_replicaset_dependency_graph_sync() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();
    clean_store(&store).await?;

    let rs_uid = Uuid::new_v4();
    let rs_name = format!("garbage-collector-test-rs-graph-{}", rs_uid);

    let rs = replicaset_with_meta(&rs_name, rs_uid, 2);
    let rs_yaml = serde_yaml::to_string(&rs)?;
    store.insert_replicaset_yaml(&rs_name, &rs_yaml).await?;

    let (_mgr, gc, _rs_ctrl) = setup_gc_with_rs(store.clone()).await?;

    // 等待 ReplicaSet 创建 Pod
    let pods = wait_for_replicaset_pods(&store, &rs_name, 2, Duration::from_secs(10)).await?;
    assert_eq!(pods.len(), 2, "ReplicaSet should create 2 pods");

    // 验证所有 Pod 都有正确的 owner reference
    for pod in &pods {
        let pod_yaml = store.get_pod_yaml(&pod.metadata.name).await?;
        assert!(pod_yaml.is_some(), "Pod {} should exist", pod.metadata.name);
        let pod_yaml = pod_yaml.unwrap();
        assert!(
            yaml_contains_owner_reference(&pod_yaml, rs_uid)?,
            "Pod {} should have owner reference to ReplicaSet {}",
            pod.metadata.name,
            rs_uid
        );
    }

    // 验证依赖图包含 ReplicaSet 和 Pod
    let uid_to_node_table = {
        let gc_guard = gc.read().await;
        gc_guard.dependency_graph_builder.uid_to_node_table.clone()
    };

    let graph_ready_deadline = Instant::now() + Duration::from_secs(15);
    let pod_uids: Vec<Uuid> = pods.iter().map(|p| p.metadata.uid).collect();
    loop {
        let rs_node_exists = uid_to_node_table.contains_key(&rs_uid).await;
        let mut all_pods_in_graph = true;
        for uid in &pod_uids {
            if !uid_to_node_table.contains_key(uid).await {
                all_pods_in_graph = false;
                break;
            }
        }

        if rs_node_exists && all_pods_in_graph {
            break;
        }
        if Instant::now() >= graph_ready_deadline {
            panic!("dependency graph was not populated for ReplicaSet and its pods");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 验证所有 Pod 节点都有正确的 owner reference
    for pod_uid in &pod_uids {
        let pod_node = uid_to_node_table
            .get(pod_uid)
            .await
            .expect("Pod node missing");
        let pod_guard = pod_node.read().await;
        let has_rs_owner = pod_guard
            .owners()
            .iter()
            .any(|o| o.uid == rs_uid && o.kind == ResourceKind::ReplicaSet);
        assert!(
            has_rs_owner,
            "Pod {} should have owner reference to ReplicaSet {}",
            pod_uid, rs_uid
        );
    }

    // 等待 ReplicaSet 节点从虚拟节点转换为真实节点（如果它最初是虚拟的）
    let rs_observed_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(rs_node) = uid_to_node_table.get(&rs_uid).await {
            let rs_guard = rs_node.read().await;
            if rs_guard.is_observed() {
                break;
            }
        }
        if Instant::now() >= rs_observed_deadline {
            panic!("ReplicaSet node was not observed in dependency graph");
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 验证 ReplicaSet 节点有正确的 dependents
    // 等待所有 Pod 都被添加到 ReplicaSet 的 dependents 中
    let dependents_ready_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(rs_node) = uid_to_node_table.get(&rs_uid).await {
            let rs_guard = rs_node.read().await;
            let dependents_count = rs_guard.dependents_length();
            if dependents_count == 2 {
                // 验证所有 Pod UID 都在 dependents 中
                let mut found_pods = 0;
                for dep in rs_guard.dependents() {
                    let dep_uid = dep.read().await.identity().uid;
                    if pod_uids.contains(&dep_uid) {
                        found_pods += 1;
                    }
                }
                if found_pods == 2 {
                    break;
                }
            }
        }
        if Instant::now() >= dependents_ready_deadline {
            let actual_count = if let Some(rs_node) = uid_to_node_table.get(&rs_uid).await {
                rs_node.read().await.dependents_length()
            } else {
                0
            };
            // 打印调试信息
            let mut dependent_uids = Vec::new();
            if let Some(rs_node) = uid_to_node_table.get(&rs_uid).await {
                let rs_guard = rs_node.read().await;
                for dep in rs_guard.dependents() {
                    dependent_uids.push(dep.read().await.identity().uid);
                }
            }
            panic!(
                "ReplicaSet node should have 2 dependents, but has {}. Dependent UIDs: {:?}, Expected Pod UIDs: {:?}",
                actual_count, dependent_uids, pod_uids
            );
        }
        sleep(Duration::from_millis(100)).await;
    }

    // 删除 ReplicaSet
    store
        .delete_object(
            ResourceKind::ReplicaSet,
            &rs_name,
            common::DeletePropagationPolicy::Background,
        )
        .await?;

    // 验证依赖图被清理
    let graph_cleanup_deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let rs_missing = !uid_to_node_table.contains_key(&rs_uid).await;
        let mut pods_missing = true;
        for uid in &pod_uids {
            if uid_to_node_table.contains_key(uid).await {
                pods_missing = false;
                break;
            }
        }

        if rs_missing && pods_missing {
            break;
        }
        if Instant::now() >= graph_cleanup_deadline {
            panic!("dependency graph retained nodes for deleted ReplicaSet and pods");
        }
        sleep(Duration::from_millis(100)).await;
    }

    clean_store(&store).await?;
    Ok(())
}
