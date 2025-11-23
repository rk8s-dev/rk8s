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

/// Test the basic creation of a Deployment and its reconciliation to create a ReplicaSet and Pods
#[tokio::test]
async fn test_deployment_basic_create() -> Result<()> {
    // Connect to xline
    let endpoints = vec![
        "http://172.20.0.3:2379".to_string(),
        "http://172.20.0.4:2379".to_string(),
        "http://172.20.0.5:2379".to_string(),
    ];
    let opts = XlineOptions {
        endpoints,
        config: None,
    };
    let store = Arc::new(XlineStore::new(opts).await?);

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
    let replicasets = store.list_replicasets().await?;
    let owned_rs: Vec<_> = replicasets
        .iter()
        .filter(|rs| {
            if let Some(owner_refs) = &rs.metadata.owner_references {
                owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment && owner.name == "test-deployment"
                })
            } else {
                false
            }
        })
        .collect();

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
    match store.delete_deployment("test-deployment").await {
        Ok(_) => println!("Deleted deployment"),
        Err(e) => eprintln!("Failed to delete deployment: {}", e),
    }

    for rs in &owned_rs {
        match store.delete_replicaset(&rs.metadata.name).await {
            Ok(_) => println!("Deleted ReplicaSet: {}", rs.metadata.name),
            Err(e) => eprintln!("Failed to delete ReplicaSet {}: {}", rs.metadata.name, e),
        }
    }

    // Wait a bit for GC to clean up Pods, then manually clean any remaining
    sleep(Duration::from_millis(500)).await;

    // Clean up any remaining pods owned by the ReplicaSets
    let all_pods = store.list_pods().await?;
    for pod in all_pods {
        if let Some(owner_refs) = &pod.metadata.owner_references {
            for owner_ref in owner_refs {
                if owner_ref.kind == ResourceKind::ReplicaSet
                    && owned_rs.iter().any(|rs| rs.metadata.uid == owner_ref.uid)
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

/// Test scaling a Deployment and verifying the ReplicaSet updates accordingly
#[tokio::test]
async fn test_deployment_scale() -> Result<()> {
    let endpoints = vec![
        "http://172.20.0.3:2379".to_string(),
        "http://172.20.0.4:2379".to_string(),
        "http://172.20.0.5:2379".to_string(),
    ];
    let opts = XlineOptions {
        endpoints,
        config: None,
    };
    let store = Arc::new(XlineStore::new(opts).await?);

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

    // Cleanup - delete all resources created by the test
    match store.delete_deployment("test-scale-deployment").await {
        Ok(_) => println!("Deleted deployment"),
        Err(e) => eprintln!("Failed to delete deployment: {}", e),
    }

    match store.delete_replicaset(&rs_scaled.metadata.name).await {
        Ok(_) => println!("Deleted ReplicaSet: {}", rs_scaled.metadata.name),
        Err(e) => eprintln!(
            "Failed to delete ReplicaSet {}: {}",
            rs_scaled.metadata.name, e
        ),
    }

    // Wait a bit for GC to clean up Pods, then manually clean any remaining
    sleep(Duration::from_millis(500)).await;

    // Clean up any remaining pods owned by the ReplicaSet
    let all_pods = store.list_pods().await?;
    for pod in all_pods {
        if let Some(owner_refs) = &pod.metadata.owner_references {
            if owner_refs.iter().any(|owner| {
                owner.kind == ResourceKind::ReplicaSet && owner.uid == rs_scaled.metadata.uid
            }) {
                match store.delete_pod(&pod.metadata.name).await {
                    Ok(_) => println!("Deleted Pod: {}", pod.metadata.name),
                    Err(e) => eprintln!("Failed to delete Pod {}: {}", pod.metadata.name, e),
                }
            }
        }
    }

    println!("Cleanup completed");

    // Assert after cleanup
    assert_eq!(rs_scaled.spec.replicas, 5);
    println!("ReplicaSet scaled to 5 replicas");

    Ok(())
}

/// Test that re-applying the same deployment yaml does not cause unnecessary updates
#[tokio::test]
async fn test_deployment_idempotency() -> Result<()> {
    let endpoints = vec![
        "http://172.20.0.3:2379".to_string(),
        "http://172.20.0.4:2379".to_string(),
        "http://172.20.0.5:2379".to_string(),
    ];
    let opts = XlineOptions {
        endpoints,
        config: None,
    };
    let store = Arc::new(XlineStore::new(opts).await?);

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
    let replicasets = store.list_replicasets().await?;
    let owned_rs: Vec<_> = replicasets
        .iter()
        .filter(|rs| {
            if let Some(owner_refs) = &rs.metadata.owner_references {
                owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment
                        && owner.name == "test-idempotency-deployment"
                })
            } else {
                false
            }
        })
        .collect();

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
    let replicasets_after = store.list_replicasets().await?;
    let owned_rs_after: Vec<_> = replicasets_after
        .iter()
        .filter(|rs| {
            if let Some(owner_refs) = &rs.metadata.owner_references {
                owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment
                        && owner.name == "test-idempotency-deployment"
                })
            } else {
                false
            }
        })
        .collect();

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
    match store.delete_deployment("test-idempotency-deployment").await {
        Ok(_) => println!("Deleted deployment"),
        Err(e) => eprintln!("Failed to delete deployment: {}", e),
    }

    match store.delete_replicaset(&rs_name).await {
        Ok(_) => println!("Deleted ReplicaSet: {}", rs_name),
        Err(e) => eprintln!("Failed to delete ReplicaSet {}: {}", rs_name, e),
    }

    sleep(Duration::from_millis(500)).await;

    let all_pods = store.list_pods().await?;
    for pod in all_pods {
        if let Some(owner_refs) = &pod.metadata.owner_references {
            if owner_refs
                .iter()
                .any(|owner| owner.kind == ResourceKind::ReplicaSet && owner.uid == rs_uid)
            {
                match store.delete_pod(&pod.metadata.name).await {
                    Ok(_) => println!("Deleted Pod: {}", pod.metadata.name),
                    Err(e) => eprintln!("Failed to delete Pod {}: {}", pod.metadata.name, e),
                }
            }
        }
    }

    println!("Cleanup completed");

    Ok(())
}

/// Test concurrent reconciliation attempts do not create duplicate ReplicaSets
#[tokio::test]
async fn test_deployment_concurrent_reconciliation() -> Result<()> {
    let endpoints = vec![
        "http://172.20.0.3:2379".to_string(),
        "http://172.20.0.4:2379".to_string(),
        "http://172.20.0.5:2379".to_string(),
    ];
    let opts = XlineOptions {
        endpoints,
        config: None,
    };
    let store = Arc::new(XlineStore::new(opts).await?);

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
    let replicasets = store.list_replicasets().await?;
    let owned_rs: Vec<_> = replicasets
        .iter()
        .filter(|rs| {
            if let Some(owner_refs) = &rs.metadata.owner_references {
                owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment
                        && owner.name == "test-concurrent-deployment"
                })
            } else {
                false
            }
        })
        .collect();

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
    let replicasets_after = store.list_replicasets().await?;
    let owned_rs_after: Vec<_> = replicasets_after
        .iter()
        .filter(|rs| {
            if let Some(owner_refs) = &rs.metadata.owner_references {
                owner_refs.iter().any(|owner| {
                    owner.kind == ResourceKind::Deployment
                        && owner.name == "test-concurrent-deployment"
                })
            } else {
                false
            }
        })
        .collect();

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
    match store.delete_deployment("test-concurrent-deployment").await {
        Ok(_) => println!("Deleted deployment"),
        Err(e) => eprintln!("Failed to delete deployment: {}", e),
    }

    match store.delete_replicaset(&rs_name).await {
        Ok(_) => println!("Deleted ReplicaSet: {}", rs_name),
        Err(e) => eprintln!("Failed to delete ReplicaSet {}: {}", rs_name, e),
    }

    sleep(Duration::from_millis(500)).await;

    let all_pods = store.list_pods().await?;
    for pod in all_pods {
        if let Some(owner_refs) = &pod.metadata.owner_references {
            if owner_refs
                .iter()
                .any(|owner| owner.kind == ResourceKind::ReplicaSet && owner.uid == rs_uid)
            {
                match store.delete_pod(&pod.metadata.name).await {
                    Ok(_) => println!("Deleted Pod: {}", pod.metadata.name),
                    Err(e) => eprintln!("Failed to delete Pod {}: {}", pod.metadata.name, e),
                }
            }
        }
    }

    println!("Cleanup completed");

    Ok(())
}

/// Setup test environment with all necessary controllers
async fn setup_test_manager(store: Arc<XlineStore>) -> Result<Arc<ControllerManager>> {
    let manager = Arc::new(ControllerManager::new());

    // Register GarbageCollector for cascading deletion
    let gc = GarbageCollector::new(store.clone());
    manager
        .clone()
        .register(Arc::new(RwLock::new(gc)), 2)
        .await?;

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
                    }],
                    init_containers: Vec::new(),
                    tolerations: Vec::new(),
                },
            },
        },
        status: DeploymentStatus::default(),
    }
}
