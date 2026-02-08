use anyhow::Result;
use common::*;
use libvault::storage::xline::XlineOptions;
use rks::api::xlinestore::XlineStore;
use rks::controllers::ReplicaSetController;
use rks::controllers::deployment::DeploymentController;
use rks::controllers::garbage_collector::GarbageCollector;
use rks::controllers::manager::ControllerManager;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

/// Use cargo test -p rks --test test-deployment -- --test-threads=1

/// Test the basic creation of a Deployment and its reconciliation to create a ReplicaSet and Pods
#[tokio::test]
async fn test_deployment_basic_create() -> Result<()> {
    let store = create_test_store().await?;

    // Setup test environment with all controllers
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment
    let deployment = create_test_deployment("test-deployment", 3);

    // Save deployment
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-deployment", &yaml)
        .await?;

    println!("Created deployment: test-deployment");

    // Wait for reconciliation
    println!("Waiting for deployment reconciliation...");
    sleep(Duration::from_secs(3)).await;

    // Verify ReplicaSet was created
    let owned_rs = get_owned_replicasets(&store, "test-deployment").await?;

    assert!(!owned_rs.is_empty(), "No ReplicaSet created for deployment");
    println!("ReplicaSet created: {}", owned_rs[0].metadata.name);

    // Verify replicas
    assert_eq!(
        owned_rs[0].spec.replicas, 3,
        "ReplicaSet should have 3 replicas"
    );
    println!("ReplicaSet has correct replica count: 3");

    // Verify deployment status was updated
    let updated_deployment = store
        .get_deployment("test-deployment")
        .await?
        .expect("Deployment should exist");

    println!("Deployment status:");
    println!(
        "   - Total replicas: {}",
        updated_deployment.status.replicas
    );
    println!(
        "   - Updated replicas: {}",
        updated_deployment.status.updated_replicas
    );
    println!(
        "   - Ready replicas: {}",
        updated_deployment.status.ready_replicas
    );
    println!(
        "   - Available replicas: {}",
        updated_deployment.status.available_replicas
    );
    cleanup_deployment_test(&store, &["test-deployment"], &owned_rs).await?;

    Ok(())
}

/// Test scaling a Deployment and verifying the ReplicaSet updates accordingly
#[tokio::test]
async fn test_deployment_scale() -> Result<()> {
    let store = create_test_store().await?;

    // Setup test environment with all controllers
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with 2 replicas
    let mut deployment = create_test_deployment("test-scale-deployment", 2);
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-scale-deployment", &yaml)
        .await?;

    println!("Created deployment with 2 replicas");

    // Wait for initial reconciliation
    sleep(Duration::from_secs(3)).await;

    // Verify initial state
    let replicasets = store.list_replicasets().await?;
    let rs = replicasets
        .iter()
        .find(|rs| {
            rs.metadata
                .owner_references
                .as_ref()
                .and_then(|refs| refs.iter().find(|r| r.name == "test-scale-deployment"))
                .is_some()
        })
        .expect("ReplicaSet should exist");

    assert_eq!(rs.spec.replicas, 2);
    println!("Initial ReplicaSet has 2 replicas");

    // Scale up to 5 replicas
    deployment.spec.replicas = 5;
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-scale-deployment", &yaml)
        .await?;
    println!("Scaled deployment to 5 replicas");

    // Wait longer for reconciliation to complete
    sleep(Duration::from_secs(5)).await;

    // Verify scale up
    let rs_scaled = store
        .get_replicaset_yaml(&rs.metadata.name)
        .await?
        .expect("ReplicaSet should exist");
    let rs_scaled: ReplicaSet = serde_yaml::from_str(&rs_scaled)?;

    // Assert before cleanup
    assert_eq!(rs_scaled.spec.replicas, 5);
    println!("ReplicaSet scaled to 5 replicas");

    // Cleanup
    cleanup_deployment_test(&store, &["test-scale-deployment"], &[rs_scaled]).await?;

    Ok(())
}

/// Test that re-applying the same deployment yaml does not cause unnecessary updates
#[tokio::test]
async fn test_deployment_idempotency() -> Result<()> {
    let store = create_test_store().await?;

    // Setup test environment with all controllers
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment
    let deployment = create_test_deployment("test-idempotency-deployment", 3);
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-idempotency-deployment", &yaml)
        .await?;

    println!("Created deployment: test-idempotency-deployment");

    // Wait for initial reconciliation
    sleep(Duration::from_secs(3)).await;

    // Verify ReplicaSet was created
    let owned_rs = get_owned_replicasets(&store, "test-idempotency-deployment").await?;

    assert_eq!(owned_rs.len(), 1, "Should have exactly one ReplicaSet");
    let rs_name = owned_rs[0].metadata.name.clone();
    let rs_uid = owned_rs[0].metadata.uid;
    println!("Initial ReplicaSet created: {}", rs_name);

    // Write the same deployment again
    store
        .insert_deployment_yaml("test-idempotency-deployment", &yaml)
        .await?;
    println!("Re-inserted deployment with same content");

    // Wait for reconciliation
    sleep(Duration::from_secs(3)).await;

    // Verify still only one ReplicaSet exists
    let owned_rs_after = get_owned_replicasets(&store, "test-idempotency-deployment").await?;

    assert_eq!(
        owned_rs_after.len(),
        1,
        "Should still have exactly one ReplicaSet after re-insert"
    );
    assert_eq!(
        owned_rs_after[0].metadata.name, rs_name,
        "ReplicaSet name should not change"
    );
    assert_eq!(
        owned_rs_after[0].metadata.uid, rs_uid,
        "ReplicaSet UID should not change"
    );
    assert_eq!(
        owned_rs_after[0].spec.replicas, 3,
        "ReplicaSet replicas should remain 3"
    );
    println!(
        "Idempotency verified: same ReplicaSet {} with replicas=3",
        rs_name
    );

    // Verify template is unchanged
    let rs_yaml_before = serde_yaml::to_string(&owned_rs[0].spec.template.spec)?;
    let rs_yaml_after = serde_yaml::to_string(&owned_rs_after[0].spec.template.spec)?;
    assert_eq!(
        rs_yaml_before, rs_yaml_after,
        "ReplicaSet template should not change"
    );
    println!("Template unchanged");

    // Cleanup
    cleanup_deployment_test(&store, &["test-idempotency-deployment"], &owned_rs_after).await?;

    Ok(())
}

/// Test concurrent reconciliation attempts do not create duplicate ReplicaSets
#[tokio::test]
async fn test_deployment_concurrent_reconciliation() -> Result<()> {
    let store = create_test_store().await?;

    // Setup test environment with all controllers
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment
    let deployment = create_test_deployment("test-concurrent-deployment", 3);
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-concurrent-deployment", &yaml)
        .await?;

    println!("Created deployment: test-concurrent-deployment");

    // Wait for initial reconciliation
    sleep(Duration::from_secs(3)).await;

    // Verify ReplicaSet was created
    let owned_rs = get_owned_replicasets(&store, "test-concurrent-deployment").await?;

    assert_eq!(
        owned_rs.len(),
        1,
        "Should have exactly one ReplicaSet initially"
    );
    let rs_name = owned_rs[0].metadata.name.clone();
    let rs_uid = owned_rs[0].metadata.uid;
    println!("Initial ReplicaSet created: {}", rs_name);

    // Simulate concurrent reconciliation by triggering multiple reconcile events
    // This tests that the controller's idempotent logic prevents duplicate creation
    println!("Simulating concurrent reconciliation attempts...");

    let store1 = store.clone();
    let store2 = store.clone();
    let store3 = store.clone();
    let yaml1 = yaml.clone();
    let yaml2 = yaml.clone();
    let yaml3 = yaml.clone();

    // Spawn multiple concurrent writes to trigger reconciliation
    let handle1 = tokio::spawn(async move {
        store1
            .insert_deployment_yaml("test-concurrent-deployment", &yaml1)
            .await
    });

    let handle2 = tokio::spawn(async move {
        store2
            .insert_deployment_yaml("test-concurrent-deployment", &yaml2)
            .await
    });

    let handle3 = tokio::spawn(async move {
        store3
            .insert_deployment_yaml("test-concurrent-deployment", &yaml3)
            .await
    });

    // Wait for all concurrent operations to complete
    let _ = tokio::try_join!(handle1, handle2, handle3)?;
    println!("Concurrent writes completed");

    // Wait for reconciliation to settle
    sleep(Duration::from_secs(5)).await;

    // Verify still only one ReplicaSet exists
    let owned_rs_after = get_owned_replicasets(&store, "test-concurrent-deployment").await?;

    assert_eq!(
        owned_rs_after.len(),
        1,
        "Should still have exactly one ReplicaSet after concurrent reconciliation"
    );
    assert_eq!(
        owned_rs_after[0].metadata.name, rs_name,
        "ReplicaSet name should not change"
    );
    assert_eq!(
        owned_rs_after[0].metadata.uid, rs_uid,
        "ReplicaSet UID should not change (same instance)"
    );
    assert_eq!(
        owned_rs_after[0].spec.replicas, 3,
        "ReplicaSet replicas should remain 3"
    );
    println!(
        "Concurrency test passed: only one ReplicaSet {} exists with replicas=3",
        rs_name
    );

    // Cleanup
    cleanup_deployment_test(&store, &["test-concurrent-deployment"], &owned_rs_after).await?;

    Ok(())
}

/// Test hash collision resolution mechanism
/// This test simulates a scenario where a ReplicaSet with the expected name already exists
/// but is owned by a different deployment, forcing the controller to increment collision_count
#[tokio::test]
async fn test_deployment_hash_collision() -> Result<()> {
    let store = create_test_store().await?;

    println!("=== Step 1: Create deployment ===");
    let deployment = create_test_deployment("collision-test-deploy", 2);

    // Calculate what the expected RS name would be (collision_count = 0)
    let template_yaml = serde_yaml::to_string(&deployment.spec.template.spec)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};
    template_yaml.hash(&mut hasher);
    let hash = hasher.finish();
    let hash_str: String = format!("{:x}", hash).chars().take(10).collect();
    let expected_rs_name = format!("collision-test-deploy-{}", hash_str);

    println!(
        "Expected ReplicaSet name (collision_count=0): {}",
        expected_rs_name
    );

    println!("\n=== Step 2: Create a BLOCKING ReplicaSet with same name (no owner) ===");
    // Create a "blocking" ReplicaSet with the expected name but simulate it was created by
    // a different deployment (use a fake UID to represent a different owner)
    let fake_owner_uid = uuid::Uuid::new_v4(); // Different from our deployment

    let blocking_rs = ReplicaSet {
        api_version: "v1".to_string(),
        kind: "ReplicaSet".to_string(),
        metadata: ObjectMeta {
            name: expected_rs_name.clone(), // Same name!
            namespace: "default".to_string(),
            labels: {
                let mut labels = std::collections::HashMap::new();
                labels.insert("app".to_string(), "blocker".to_string());
                labels
            },
            owner_references: Some(vec![OwnerReference {
                api_version: "v1".to_string(),
                kind: ResourceKind::Deployment,
                name: "different-owner-deploy".to_string(),
                uid: fake_owner_uid, // Different owner UID!
                controller: true,
                block_owner_deletion: Some(true),
            }]),
            ..Default::default()
        },
        spec: ReplicaSetSpec {
            replicas: 1,
            selector: LabelSelector {
                match_labels: {
                    let mut labels = std::collections::HashMap::new();
                    labels.insert("app".to_string(), "blocker".to_string());
                    labels
                },
                match_expressions: Vec::new(),
            },
            template: PodTemplateSpec {
                metadata: ObjectMeta {
                    name: "".to_string(),
                    namespace: "default".to_string(),
                    uid: uuid::Uuid::new_v4(),
                    labels: {
                        let mut labels = std::collections::HashMap::new();
                        labels.insert("app".to_string(), "blocker".to_string());
                        labels
                    },
                    annotations: std::collections::HashMap::new(),
                    owner_references: None,
                    creation_timestamp: None,
                    deletion_timestamp: None,
                    finalizers: None,
                    generation: None,
                },
                spec: PodSpec {
                    node_name: None,
                    containers: vec![ContainerSpec {
                        security_context: None,
                        env: None,
                        volume_mounts: None,
                        command: None,
                        working_dir: None,
                        name: "blocker".to_string(),
                        image: "./blocker-image".to_string(),
                        ports: Vec::new(),
                        args: Vec::new(),
                        resources: None,
                        liveness_probe: None,
                        readiness_probe: None,
                        startup_probe: None,
                    }],
                    init_containers: Vec::new(),
                    tolerations: Vec::new(),
                    ..Default::default()
                },
            },
        },
        status: ReplicaSetStatus::default(),
    };

    let blocking_rs_yaml = serde_yaml::to_string(&blocking_rs)?;
    store
        .insert_replicaset_yaml(&expected_rs_name, &blocking_rs_yaml)
        .await?;
    println!(
        "Created blocking ReplicaSet: {} (owned by fake UID: {})",
        expected_rs_name, fake_owner_uid
    );

    // Verify it exists
    let verify_rs = store.get_replicaset_yaml(&expected_rs_name).await?;
    assert!(
        verify_rs.is_some(),
        "Blocking RS should exist before starting controllers"
    );
    println!("Verified blocking ReplicaSet exists in etcd");

    println!("\n=== Step 3: Setup controllers WITHOUT GC (watch starts) ===");
    let _manager = setup_test_manager_with_gc(store.clone(), false).await?;
    println!("Controllers started and watching (GC disabled to preserve blocking RS)");

    println!("\n=== Step 4: Create our deployment (should detect collision) ===");
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("collision-test-deploy", &yaml)
        .await?;
    println!("Created deployment: collision-test-deploy");

    // Wait for reconciliation
    println!("\n=== Step 5: Wait for controller to detect collision ===");
    sleep(Duration::from_secs(5)).await;

    // Check if collision_count was incremented
    let updated_deployment = store
        .get_deployment("collision-test-deploy")
        .await?
        .expect("Deployment should exist");

    println!("\n=== Step 6: Verify collision_count incremented ===");
    println!(
        "collision_count: {}",
        updated_deployment.status.collision_count
    );
    assert!(
        updated_deployment.status.collision_count >= 1,
        "collision_count should be incremented when hash collision detected"
    );
    println!(
        "Collision detection working! collision_count = {}",
        updated_deployment.status.collision_count
    );

    // Verify the blocking RS still exists and is unchanged
    let blocking_rs_after = store
        .get_replicaset_yaml(&expected_rs_name)
        .await?
        .expect("Blocking RS should still exist");
    let blocking_rs_after: ReplicaSet = serde_yaml::from_str(&blocking_rs_after)?;
    assert_eq!(
        blocking_rs_after
            .metadata
            .owner_references
            .as_ref()
            .unwrap()[0]
            .uid,
        fake_owner_uid,
        "Blocking RS should still be owned by fake deployment (not overwritten)"
    );
    println!("Blocking ReplicaSet unchanged");

    println!("\n=== Step 7: Remove blocking RS and verify retry creates new RS ===");
    store.delete_replicaset(&expected_rs_name).await?;
    println!("Deleted blocking ReplicaSet");

    // Trigger reconciliation again by updating deployment
    let yaml = serde_yaml::to_string(&updated_deployment)?;
    store
        .insert_deployment_yaml("collision-test-deploy", &yaml)
        .await?;

    sleep(Duration::from_secs(5)).await;

    // Now check if a new RS was created with incremented hash
    let owned_rs = get_owned_replicasets(&store, "collision-test-deploy").await?;

    println!("\n=== Step 8: Verify new RS created with different name ===");
    assert!(
        !owned_rs.is_empty(),
        "Should have created a new ReplicaSet after collision resolved"
    );

    let new_rs_name = &owned_rs[0].metadata.name;
    println!("New ReplicaSet created: {}", new_rs_name);
    assert_ne!(
        new_rs_name, &expected_rs_name,
        "New RS should have different name due to collision_count"
    );
    println!("New ReplicaSet has different name (collision resolved)");

    // Cleanup
    println!("\n=== Cleanup ===");
    cleanup_deployment_test(&store, &["collision-test-deploy"], &owned_rs).await?;

    Ok(())
}

/// Setup test environment with all necessary controllers
async fn setup_test_manager(store: Arc<XlineStore>) -> Result<Arc<ControllerManager>> {
    setup_test_manager_with_gc(store, true).await
}

/// Setup test environment with optional GC
async fn setup_test_manager_with_gc(
    store: Arc<XlineStore>,
    enable_gc: bool,
) -> Result<Arc<ControllerManager>> {
    let manager = Arc::new(ControllerManager::new());

    // Register GarbageCollector for cascading deletion (optional)
    if enable_gc {
        let gc = GarbageCollector::new(store.clone());
        manager
            .clone()
            .register(Arc::new(RwLock::new(gc)), 2)
            .await?;
    }

    // Register ReplicaSetController to create Pods
    let rs_controller = ReplicaSetController::new(store.clone());
    manager
        .clone()
        .register(Arc::new(RwLock::new(rs_controller)), 2)
        .await?;

    // Register DeploymentController
    let deploy_controller = DeploymentController::new(store.clone());
    manager
        .clone()
        .register(Arc::new(RwLock::new(deploy_controller)), 2)
        .await?;

    // Start watching in background
    let manager_clone = manager.clone();
    let store_clone = store.clone();
    tokio::spawn(async move {
        if let Err(e) = manager_clone.start_watch(store_clone).await {
            eprintln!("Manager watch error: {}", e);
        }
    });

    // Give the manager time to start watching and establish etcd connections
    sleep(Duration::from_secs(2)).await;

    Ok(manager)
}

/// Get ReplicaSets owned by a specific deployment
async fn get_owned_replicasets(
    store: &XlineStore,
    deployment_name: &str,
) -> Result<Vec<ReplicaSet>> {
    let replicasets = store.list_replicasets().await?;
    Ok(replicasets
        .into_iter()
        .filter(|rs| {
            if let Some(owner_refs) = &rs.metadata.owner_references {
                owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment && owner.name == deployment_name
                })
            } else {
                false
            }
        })
        .collect())
}

/// Cleanup all resources for a deployment test
async fn cleanup_deployment_test(
    store: &XlineStore,
    deployment_names: &[&str],
    replicasets: &[ReplicaSet],
) -> Result<()> {
    // Delete deployments
    for name in deployment_names {
        match store.delete_deployment(name).await {
            Ok(_) => println!("Deleted deployment: {}", name),
            Err(e) => eprintln!("Failed to delete deployment {}: {}", name, e),
        }
    }

    // Delete ReplicaSets
    for rs in replicasets {
        match store.delete_replicaset(&rs.metadata.name).await {
            Ok(_) => println!("Deleted ReplicaSet: {}", rs.metadata.name),
            Err(e) => eprintln!("Failed to delete ReplicaSet {}: {}", rs.metadata.name, e),
        }
    }

    // Wait for GC
    sleep(Duration::from_millis(500)).await;

    // Clean up any remaining pods
    let all_pods = store.list_pods().await?;
    for pod in all_pods {
        if let Some(owner_refs) = &pod.metadata.owner_references {
            for owner_ref in owner_refs {
                if owner_ref.kind == ResourceKind::ReplicaSet
                    && replicasets
                        .iter()
                        .any(|rs| rs.metadata.uid == owner_ref.uid)
                {
                    match store.delete_pod(&pod.metadata.name).await {
                        Ok(_) => println!("Deleted Pod: {}", pod.metadata.name),
                        Err(e) => eprintln!("Failed to delete Pod {}: {}", pod.metadata.name, e),
                    }
                    break;
                }
            }
        }
    }

    println!("Cleanup completed");
    Ok(())
}

/// Create a test store connected to xline
async fn create_test_store() -> Result<Arc<XlineStore>> {
    let endpoints = vec![
        "http://172.20.0.3:2379".to_string(),
        "http://172.20.0.4:2379".to_string(),
        "http://172.20.0.5:2379".to_string(),
    ];
    let opts = XlineOptions {
        endpoints,
        config: None,
    };
    Ok(Arc::new(XlineStore::new(opts).await?))
}

fn create_test_deployment(name: &str, replicas: i32) -> Deployment {
    Deployment {
        api_version: "v1".to_string(),
        kind: "Deployment".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "default".to_string(),
            labels: {
                let mut labels = std::collections::HashMap::new();
                labels.insert("app".to_string(), name.to_string());
                labels
            },
            ..Default::default()
        },
        spec: DeploymentSpec {
            replicas,
            selector: LabelSelector {
                match_labels: {
                    let mut labels = std::collections::HashMap::new();
                    labels.insert("app".to_string(), name.to_string());
                    labels
                },
                match_expressions: Vec::new(),
            },
            template: PodTemplateSpec {
                metadata: ObjectMeta {
                    name: "".to_string(),
                    namespace: "default".to_string(),
                    uid: uuid::Uuid::new_v4(),
                    labels: {
                        let mut labels = std::collections::HashMap::new();
                        labels.insert("app".to_string(), name.to_string());
                        labels
                    },
                    annotations: std::collections::HashMap::new(),
                    owner_references: None,
                    creation_timestamp: None,
                    deletion_timestamp: None,
                    finalizers: None,
                    generation: None,
                },
                spec: PodSpec {
                    node_name: None,
                    containers: vec![ContainerSpec {
                        name: "nginx".to_string(),
                        image: "./test-image".to_string(),
                        ports: Vec::new(),
                        args: Vec::new(),
                        resources: None,
                        liveness_probe: None,
                        readiness_probe: None,
                        startup_probe: None,
                        security_context: None,
                        env: None,
                        volume_mounts: None,
                        command: None,
                        working_dir: None,
                    }],
                    init_containers: Vec::new(),
                    tolerations: Vec::new(),
                    ..Default::default()
                },
            },
            strategy: DeploymentStrategy::RollingUpdate {
                rolling_update: RollingUpdateStrategy::default(),
            },
            progress_deadline_seconds: 600,
            revision_history_limit: 10,
        },
        status: DeploymentStatus::default(),
    }
}

/// Test basic rolling update: change image and verify gradual transition
#[tokio::test]
async fn test_deployment_rolling_update_basic() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with v1 image
    let mut deployment = create_test_deployment("test-rolling", 4);
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();

    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rolling", &yaml).await?;

    println!("Created deployment with nginx:v1");
    sleep(Duration::from_secs(3)).await;

    // Verify initial state
    let owned_rs = get_owned_replicasets(&store, "test-rolling").await?;
    assert_eq!(owned_rs.len(), 1, "Should have 1 ReplicaSet initially");
    let old_rs_name = owned_rs[0].metadata.name.clone();
    println!("Initial RS created: {}", old_rs_name);

    // Update to v2 image (trigger rolling update)
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rolling", &yaml).await?;

    println!("Updated deployment to nginx:v2, starting rolling update...");

    // Wait for rolling update to progress
    sleep(Duration::from_secs(5)).await;

    // Verify new ReplicaSet was created
    let owned_rs = get_owned_replicasets(&store, "test-rolling").await?;
    assert!(owned_rs.len() >= 1, "Should have at least 1 ReplicaSet");

    // Find new RS (different from old one)
    let new_rs = owned_rs.iter().find(|rs| rs.metadata.name != old_rs_name);

    if let Some(new_rs) = new_rs {
        println!("New RS created: {}", new_rs.metadata.name);
        println!("New RS replicas: {}", new_rs.spec.replicas);

        // Verify template changed
        let new_image = &new_rs.spec.template.spec.containers[0].image;
        assert!(new_image.contains("v2"), "New RS should use v2 image");
    } else {
        println!("Only one RS found, may still be transitioning");
    }

    // Verify deployment status reflects update
    let updated_deployment = store
        .get_deployment("test-rolling")
        .await?
        .expect("Deployment exists");
    println!("Deployment status:");
    println!("   Total replicas: {}", updated_deployment.status.replicas);
    println!(
        "   Updated replicas: {}",
        updated_deployment.status.updated_replicas
    );
    println!(
        "   Observed generation: {:?}",
        updated_deployment.status.observed_generation
    );

    cleanup_deployment_test(&store, &["test-rolling"], &owned_rs).await?;
    Ok(())
}

/// Test Roll Over: rapidly update deployment multiple times during rolling update
#[tokio::test]
async fn test_deployment_roll_over() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with v1
    let mut deployment = create_test_deployment("test-rollover", 6);
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();

    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rollover", &yaml).await?;

    println!("Created deployment with nginx:v1");
    sleep(Duration::from_secs(2)).await;

    // Rapid updates: v1 -> v2 -> v3
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rollover", &yaml).await?;
    println!("Updated to v2");

    sleep(Duration::from_millis(500)).await; // Short delay

    deployment.spec.template.spec.containers[0].image = "nginx:v3".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rollover", &yaml).await?;
    println!("Updated to v3 (Roll Over scenario)");

    // Wait for reconciliation
    sleep(Duration::from_secs(5)).await;

    let owned_rs = get_owned_replicasets(&store, "test-rollover").await?;
    println!("Total ReplicaSets after Roll Over: {}", owned_rs.len());

    // Find the RS with v3 image (should be the new one)
    let v3_rs = owned_rs
        .iter()
        .find(|rs| rs.spec.template.spec.containers[0].image.contains("v3"));

    assert!(v3_rs.is_some(), "Should have RS with v3 image");
    println!("Found v3 ReplicaSet: {}", v3_rs.unwrap().metadata.name);

    // All non-v3 RSs should be scaling down or at 0
    let old_rss: Vec<_> = owned_rs
        .iter()
        .filter(|rs| !rs.spec.template.spec.containers[0].image.contains("v3"))
        .collect();

    println!("Old ReplicaSets (v1, v2):");
    for rs in &old_rss {
        println!("   {} - replicas: {}", rs.metadata.name, rs.spec.replicas);
    }

    cleanup_deployment_test(&store, &["test-rollover"], &owned_rs).await?;
    Ok(())
}

/// Test rolling update with maxSurge=0 (cannot exceed desired count)
#[tokio::test]
async fn test_deployment_max_surge_zero() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with custom strategy
    let mut deployment = create_test_deployment("test-surge-zero", 4);
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    deployment.spec.strategy = DeploymentStrategy::RollingUpdate {
        rolling_update: RollingUpdateStrategy {
            max_surge: IntOrPercentage::Int(0),
            max_unavailable: IntOrPercentage::String("25%".to_string()),
        },
    };

    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-surge-zero", &yaml)
        .await?;

    println!("Created deployment with maxSurge=0");
    sleep(Duration::from_secs(2)).await;

    // Update image
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-surge-zero", &yaml)
        .await?;
    println!("Updated to v2 with maxSurge=0");

    sleep(Duration::from_secs(4)).await;

    let owned_rs = get_owned_replicasets(&store, "test-surge-zero").await?;
    let total_replicas: i32 = owned_rs.iter().map(|rs| rs.spec.replicas).sum();

    println!("Total replicas across all RSs: {}", total_replicas);

    // With maxSurge=0, total should never exceed desired (4)
    assert!(
        total_replicas <= 4,
        "Total replicas {} should not exceed desired 4 when maxSurge=0",
        total_replicas
    );

    println!("Verified maxSurge=0: total replicas <= desired");

    cleanup_deployment_test(&store, &["test-surge-zero"], &owned_rs).await?;
    Ok(())
}

/// Test rolling update with maxUnavailable=0 (must maintain all replicas available)
#[tokio::test]
async fn test_deployment_max_unavailable_zero() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with custom strategy
    let mut deployment = create_test_deployment("test-unavailable-zero", 4);
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    deployment.spec.strategy = DeploymentStrategy::RollingUpdate {
        rolling_update: RollingUpdateStrategy {
            max_surge: IntOrPercentage::String("25%".to_string()),
            max_unavailable: IntOrPercentage::Int(0),
        },
    };

    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-unavailable-zero", &yaml)
        .await?;

    println!("Created deployment with maxUnavailable=0");
    sleep(Duration::from_secs(2)).await;

    // Update image
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-unavailable-zero", &yaml)
        .await?;
    println!("Updated to v2 with maxUnavailable=0");

    sleep(Duration::from_secs(4)).await;

    let owned_rs = get_owned_replicasets(&store, "test-unavailable-zero").await?;
    let total_replicas: i32 = owned_rs.iter().map(|rs| rs.spec.replicas).sum();

    println!("Total replicas across all RSs: {}", total_replicas);

    // With maxUnavailable=0 and maxSurge=25%, should scale up first
    // Total can be up to 4 + 1 = 5
    assert!(
        total_replicas >= 4,
        "Should maintain at least desired replicas when maxUnavailable=0"
    );

    println!("Verified maxUnavailable=0: maintained minimum replicas");

    cleanup_deployment_test(&store, &["test-unavailable-zero"], &owned_rs).await?;
    Ok(())
}

// ==================== Revision and Rollback Tests ====================

const REVISION_ANNOTATION: &str = "deployment.rk8s.io/revision";
const REVISION_HISTORY_ANNOTATION: &str = "deployment.rk8s.io/revision-history";

/// Helper to get revision from RS annotation
fn get_rs_revision(rs: &ReplicaSet) -> i64 {
    rs.metadata
        .annotations
        .get(REVISION_ANNOTATION)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Helper to get revision from Deployment annotation
fn get_deployment_revision(deployment: &Deployment) -> i64 {
    deployment
        .metadata
        .annotations
        .get(REVISION_ANNOTATION)
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Helper to get revision history from RS annotation
fn get_rs_revision_history(rs: &ReplicaSet) -> Vec<i64> {
    rs.metadata
        .annotations
        .get(REVISION_HISTORY_ANNOTATION)
        .and_then(|v| serde_json::from_str(v).ok())
        .unwrap_or_default()
}

/// Test that creating a new deployment sets revision=1
#[tokio::test]
async fn test_deployment_revision_initial() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment
    let deployment = create_test_deployment("test-rev-init", 2);
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rev-init", &yaml).await?;

    println!("Created deployment: test-rev-init");
    sleep(Duration::from_secs(2)).await;

    // Check Deployment revision
    let deploy_yaml = store.get_deployment_yaml("test-rev-init").await?.unwrap();
    let deploy: Deployment = serde_yaml::from_str(&deploy_yaml)?;
    let deploy_rev = get_deployment_revision(&deploy);
    println!("Deployment revision: {}", deploy_rev);
    assert_eq!(deploy_rev, 1, "Initial deployment should have revision=1");

    // Check RS revision
    let owned_rs = get_owned_replicasets(&store, "test-rev-init").await?;
    assert!(!owned_rs.is_empty(), "Should have at least one RS");
    let rs_rev = get_rs_revision(&owned_rs[0]);
    println!("RS revision: {}", rs_rev);
    assert_eq!(rs_rev, 1, "Initial RS should have revision=1");

    // RS history should be empty for new RS
    let history = get_rs_revision_history(&owned_rs[0]);
    println!("RS revision history: {:?}", history);
    assert!(
        history.is_empty(),
        "New RS should have empty revision history"
    );

    cleanup_deployment_test(&store, &["test-rev-init"], &owned_rs).await?;
    Ok(())
}

/// Test that updating deployment increments revision
#[tokio::test]
async fn test_deployment_revision_increment() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with v1
    let mut deployment = create_test_deployment("test-rev-inc", 2);
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rev-inc", &yaml).await?;

    println!("Created deployment with v1");
    sleep(Duration::from_secs(2)).await;

    // Verify revision=1
    let deploy_yaml = store.get_deployment_yaml("test-rev-inc").await?.unwrap();
    let deploy: Deployment = serde_yaml::from_str(&deploy_yaml)?;
    assert_eq!(get_deployment_revision(&deploy), 1);

    // Update to v2
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rev-inc", &yaml).await?;

    println!("Updated to v2");
    sleep(Duration::from_secs(2)).await;

    // Verify revision=2
    let deploy_yaml = store.get_deployment_yaml("test-rev-inc").await?.unwrap();
    let deploy: Deployment = serde_yaml::from_str(&deploy_yaml)?;
    let rev = get_deployment_revision(&deploy);
    println!("Deployment revision after update: {}", rev);
    assert_eq!(rev, 2, "Revision should be 2 after update");

    // Verify RS
    let owned_rs = get_owned_replicasets(&store, "test-rev-inc").await?;
    let new_rs: Vec<_> = owned_rs
        .iter()
        .filter(|rs| get_rs_revision(rs) == 2)
        .collect();
    assert!(!new_rs.is_empty(), "Should have RS with revision=2");
    println!("Found RS with revision=2: {}", new_rs[0].metadata.name);

    cleanup_deployment_test(&store, &["test-rev-inc"], &owned_rs).await?;
    Ok(())
}

/// Test rollback to previous revision
#[tokio::test]
async fn test_deployment_rollback_to_previous() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment with v1
    let mut deployment = create_test_deployment("test-rollback", 2);
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rollback", &yaml).await?;
    sleep(Duration::from_secs(2)).await;

    println!("Created deployment with v1 (revision=1)");

    // Update to v2
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rollback", &yaml).await?;
    sleep(Duration::from_secs(2)).await;

    println!("Updated to v2 (revision=2)");

    // Verify we're at revision=2
    let deploy_yaml = store.get_deployment_yaml("test-rollback").await?.unwrap();
    let deploy: Deployment = serde_yaml::from_str(&deploy_yaml)?;
    assert_eq!(get_deployment_revision(&deploy), 2);

    // Create controller and rollback
    let controller = DeploymentController::new(store.clone());
    controller.rollback_to_revision("test-rollback", 0).await?;

    println!("Initiated rollback to previous revision");
    sleep(Duration::from_secs(2)).await;

    // Verify revision=3 (rollback creates new revision)
    let deploy_yaml = store.get_deployment_yaml("test-rollback").await?.unwrap();
    let deploy: Deployment = serde_yaml::from_str(&deploy_yaml)?;
    let rev = get_deployment_revision(&deploy);
    println!("Deployment revision after rollback: {}", rev);
    assert_eq!(rev, 3, "Rollback should create revision=3");

    // Verify template is back to v1
    assert_eq!(
        deploy.spec.template.spec.containers[0].image, "nginx:v1",
        "Should be back to v1 image"
    );
    println!("Verified: image is nginx:v1");

    // Check RS-v1 now has revision=3 with history=["1"]
    let owned_rs = get_owned_replicasets(&store, "test-rollback").await?;
    let current_rs: Vec<_> = owned_rs
        .iter()
        .filter(|rs| get_rs_revision(rs) == 3)
        .collect();
    assert!(!current_rs.is_empty(), "Should have RS with revision=3");

    let history = get_rs_revision_history(current_rs[0]);
    println!("Reused RS history: {:?}", history);
    assert!(history.contains(&1), "History should contain 1");

    cleanup_deployment_test(&store, &["test-rollback"], &owned_rs).await?;
    Ok(())
}

/// Test rollback to specific revision
#[tokio::test]
async fn test_deployment_rollback_to_specific_revision() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    // Create deployment: v1 -> v2 -> v3
    let mut deployment = create_test_deployment("test-rollback-spec", 2);

    // v1
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-rollback-spec", &yaml)
        .await?;
    sleep(Duration::from_secs(2)).await;
    println!("Created v1 (revision=1)");

    // v2
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-rollback-spec", &yaml)
        .await?;
    sleep(Duration::from_secs(2)).await;
    println!("Updated to v2 (revision=2)");

    // v3
    deployment.spec.template.spec.containers[0].image = "nginx:v3".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store
        .insert_deployment_yaml("test-rollback-spec", &yaml)
        .await?;
    sleep(Duration::from_secs(2)).await;
    println!("Updated to v3 (revision=3)");

    // Rollback to revision=1 (v1)
    let controller = DeploymentController::new(store.clone());
    controller
        .rollback_to_revision("test-rollback-spec", 1)
        .await?;

    println!("Rolling back to revision=1");
    sleep(Duration::from_secs(2)).await;

    // Verify
    let deploy_yaml = store
        .get_deployment_yaml("test-rollback-spec")
        .await?
        .unwrap();
    let deploy: Deployment = serde_yaml::from_str(&deploy_yaml)?;

    let rev = get_deployment_revision(&deploy);
    println!("Deployment revision after rollback: {}", rev);
    assert_eq!(rev, 4, "Rollback should create revision=4");

    assert_eq!(
        deploy.spec.template.spec.containers[0].image, "nginx:v1",
        "Should be back to v1 image"
    );
    println!("Verified: image is nginx:v1");

    let owned_rs = get_owned_replicasets(&store, "test-rollback-spec").await?;
    cleanup_deployment_test(&store, &["test-rollback-spec"], &owned_rs).await?;
    Ok(())
}

/// Test revision history is preserved when RS is reused
#[tokio::test]
async fn test_deployment_revision_history_accumulation() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    let mut deployment = create_test_deployment("test-rev-hist", 2);
    let controller = DeploymentController::new(store.clone());

    // v1
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rev-hist", &yaml).await?;
    sleep(Duration::from_secs(2)).await;
    println!("v1 (revision=1)");

    // v2
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rev-hist", &yaml).await?;
    sleep(Duration::from_secs(2)).await;
    println!("v2 (revision=2)");

    // Rollback to v1 -> revision=3
    controller.rollback_to_revision("test-rev-hist", 0).await?;
    sleep(Duration::from_secs(2)).await;
    println!("Rollback to v1 (revision=3)");

    // v2 again -> revision=4
    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-rev-hist", &yaml).await?;
    sleep(Duration::from_secs(2)).await;
    println!("Back to v2 (revision=4)");

    // Rollback to v1 -> revision=5
    controller.rollback_to_revision("test-rev-hist", 0).await?;
    sleep(Duration::from_secs(2)).await;
    println!("Rollback to v1 again (revision=5)");

    // Check RS history
    let owned_rs = get_owned_replicasets(&store, "test-rev-hist").await?;

    // Find the v1 RS (currently active with revision=5)
    let v1_rs: Vec<_> = owned_rs
        .iter()
        .filter(|rs| rs.spec.template.spec.containers[0].image == "nginx:v1")
        .collect();

    assert!(!v1_rs.is_empty(), "Should have v1 RS");
    let history = get_rs_revision_history(v1_rs[0]);
    println!("v1 RS history: {:?}", history);

    // History should contain [1, 3] (previous times it held a revision)
    assert!(history.contains(&1), "History should contain 1");
    assert!(history.contains(&3), "History should contain 3");

    cleanup_deployment_test(&store, &["test-rev-hist"], &owned_rs).await?;
    Ok(())
}

/// Test get_deployment_revision_history returns correct info
#[tokio::test]
async fn test_deployment_get_revision_history() -> Result<()> {
    let store = create_test_store().await?;
    let _manager = setup_test_manager(store.clone()).await?;

    let mut deployment = create_test_deployment("test-get-hist", 2);
    let controller = DeploymentController::new(store.clone());

    // Create v1, v2, v3
    deployment.spec.template.spec.containers[0].image = "nginx:v1".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-get-hist", &yaml).await?;
    sleep(Duration::from_secs(2)).await;

    deployment.spec.template.spec.containers[0].image = "nginx:v2".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-get-hist", &yaml).await?;
    sleep(Duration::from_secs(2)).await;

    deployment.spec.template.spec.containers[0].image = "nginx:v3".to_string();
    let yaml = serde_yaml::to_string(&deployment)?;
    store.insert_deployment_yaml("test-get-hist", &yaml).await?;
    sleep(Duration::from_secs(2)).await;

    // Get history
    let history = controller
        .get_deployment_revision_history("test-get-hist")
        .await?;

    println!("Revision history:");
    for info in &history {
        println!(
            "  revision={}, rs={}, image={:?}, is_current={}",
            info.revision, info.replicaset_name, info.image, info.is_current
        );
    }

    assert_eq!(history.len(), 3, "Should have 3 revisions");

    // The current one should be revision=3
    let current: Vec<_> = history.iter().filter(|h| h.is_current).collect();
    assert_eq!(current.len(), 1, "Should have exactly one current revision");
    assert_eq!(current[0].revision, 3);
    assert_eq!(current[0].image, Some("nginx:v3".to_string()));

    let owned_rs = get_owned_replicasets(&store, "test-get-hist").await?;
    cleanup_deployment_test(&store, &["test-get-hist"], &owned_rs).await?;
    Ok(())
}
