use common::{
    ContainerRes, ContainerSpec, Node, NodeAddress, NodeCondition, NodeSpec, NodeStatus,
    ObjectMeta, PodSpec, PodStatus, PodTask, Resource,
};
use libscheduler::plugins::{Plugins, node_resources_fit::ScoringStrategy};
use serial_test::serial;
use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use rks::protocol::config::load_config;
use rks::{api::xlinestore::XlineStore, scheduler::Scheduler};

// Get xline endpoints from config
fn get_xline_endpoints() -> Vec<String> {
    let config_path = std::env::var("TEST_CONFIG_PATH").unwrap_or_else(|_| {
        format!(
            "{}/tests/config.yaml",
            std::env::var("CARGO_MANIFEST_DIR").unwrap()
        )
    });

    match load_config(&config_path) {
        Ok(config) => config.xline_config.endpoints,
        Err(_) => vec!["127.0.0.1:2379".to_string()], // fallback
    }
}

async fn get_store() -> Option<XlineStore> {
    let endpoints = get_xline_endpoints();
    let endpoints_str: Vec<&str> = endpoints.iter().map(|s| s.as_str()).collect();

    let xline_store = match XlineStore::new(&endpoints_str).await {
        Ok(store) => store,
        Err(_) => {
            println!("Skipping test - xline not available");
            return None;
        }
    };
    Some(xline_store)
}

fn create_test_node(name: &str, cpu: &str, memory: &str) -> Node {
    let mut capacity = HashMap::new();
    capacity.insert("cpu".to_string(), cpu.to_string());
    capacity.insert("memory".to_string(), memory.to_string());

    let mut allocatable = HashMap::new();
    allocatable.insert("cpu".to_string(), cpu.to_string());
    allocatable.insert("memory".to_string(), memory.to_string());

    Node {
        api_version: "v1".to_string(),
        kind: "Node".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "".to_string(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
        },
        spec: NodeSpec {
            pod_cidr: "10.244.0.0/24".to_string(),
        },
        status: NodeStatus {
            capacity,
            allocatable,
            addresses: vec![
                NodeAddress {
                    address_type: "InternalIP".to_string(),
                    address: "192.168.1.100".to_string(),
                },
                NodeAddress {
                    address_type: "Hostname".to_string(),
                    address: name.to_string(),
                },
            ],
            conditions: vec![NodeCondition {
                condition_type: common::NodeConditionType::Ready,
                status: common::ConditionStatus::True,
                last_heartbeat_time: None,
            }],
        },
    }
}

fn create_test_pod(name: &str, cpu_limit: Option<&str>, memory_limit: Option<&str>) -> PodTask {
    let resources = if cpu_limit.is_some() || memory_limit.is_some() {
        Some(ContainerRes {
            limits: Some(Resource {
                cpu: cpu_limit.map(|s| s.to_string()),
                memory: memory_limit.map(|s| s.to_string()),
            }),
        })
    } else {
        None
    };

    PodTask {
        api_version: "v1".to_string(),
        kind: "Pod".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "default".to_string(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
        },
        spec: PodSpec {
            node_name: None,
            containers: vec![ContainerSpec {
                name: "app".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec![],
                args: vec![],
                resources,
            }],
            init_containers: vec![],
        },
        status: PodStatus { pod_ip: None },
    }
}

async fn run_scheduler(xline_store: Arc<XlineStore>) -> Result<()> {
    let endpoints = get_xline_endpoints();
    let endpoints_str: Vec<&str> = endpoints.iter().map(|s| s.as_str()).collect();

    // Create and run the actual Scheduler
    let scoring_strategy = ScoringStrategy::LeastAllocated;
    let plugins = Plugins::default();

    let scheduler = Scheduler::try_new(
        &endpoints_str,
        xline_store.clone(),
        scoring_strategy,
        plugins,
    )
    .await?;

    // Start the scheduler in the background
    scheduler.run().await;

    Ok(())
}

async fn cleanup() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = store.unwrap();

    // Clean up all pods and nodes
    let pod_names = store.list_pod_names().await?;
    for pod_name in pod_names {
        if pod_name.contains("scheduler-test") {
            store.delete_pod(&pod_name).await?;
        }
    }

    let node_names = store.list_node_names().await?;
    for node_name in node_names {
        if node_name.contains("scheduler-test") {
            store.delete_node(&node_name).await?;
        }
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn test_scheduler_creation_with_xline() {
    let xline_store = get_store().await;
    if xline_store.is_none() {
        return;
    }
    let xline_store = xline_store.unwrap();
    let endpoints = get_xline_endpoints();
    let endpoints_str: Vec<&str> = endpoints.iter().map(|s| s.as_str()).collect();
    let scoring_strategy = ScoringStrategy::LeastAllocated;
    let plugins = Plugins::default();

    let result = Scheduler::try_new(
        &endpoints_str,
        Arc::new(xline_store),
        scoring_strategy,
        plugins,
    )
    .await;

    assert!(
        result.is_ok(),
        "Failed to create scheduler: {:?}",
        result.err()
    );
}

#[tokio::test]
#[serial]
async fn test_pod_assignment_one() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = Arc::new(store.unwrap());

    run_scheduler(store.clone()).await?;

    // add an dummy node
    let node_name = "scheduler-test-node";
    let node = create_test_node(node_name, "4", "4Gi");
    store
        .insert_node_yaml(node_name, &serde_yaml::to_string(&node).unwrap())
        .await
        .expect("Insert node yaml failed");

    // Create a PodTask with proper structure for the scheduler
    let pod_name = "scheduler-test-pod";
    let pod_task = create_test_pod(pod_name, Some("1"), Some("1Gi"));

    let initial_pod_yaml = serde_yaml::to_string(&pod_task).expect("Failed to serialize pod");

    // Insert initial pod without node assignment
    store
        .insert_pod_yaml(pod_name, &initial_pod_yaml)
        .await
        .expect("Insert pod yaml failed");

    // Wait a bit for the scheduler to potentially process the pod
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Check if the pod was assigned to a node
    let final_pod_yaml = store
        .get_pod_yaml(pod_name)
        .await
        .expect("Get final pod yaml failed");

    if let Some(yaml) = final_pod_yaml {
        let final_pod = serde_yaml::from_str::<PodTask>(&yaml)?;
        assert!(final_pod.spec.node_name.is_some());
    }

    // Clean up
    cleanup().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_xline_pod_assignment_multiply() -> Result<()> {
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = Arc::new(store.unwrap());

    run_scheduler(store.clone()).await?;

    // add an dummy node
    let node_name = "scheduler-test-node";
    let node = create_test_node(node_name, "4", "4Gi");
    store
        .insert_node_yaml(node_name, &serde_yaml::to_string(&node).unwrap())
        .await
        .expect("Insert node yaml failed");

    // Create a PodTask with proper structure for the scheduler
    let pod_names = vec![
        "scheduler-test-pod-1",
        "scheduler-test-pod-2",
        "scheduler-test-pod-3",
    ];

    for pod_name in &pod_names {
        let pod_task = create_test_pod(pod_name, Some("1"), Some("1Gi"));
        let initial_pod_yaml = serde_yaml::to_string(&pod_task).expect("Failed to serialize pod");
        store
            .insert_pod_yaml(pod_name, &initial_pod_yaml)
            .await
            .expect("Insert pod yaml failed");
    }

    // Insert initial pod without node assignment
    for pod_name in &pod_names {
        // Wait a bit for the scheduler to potentially process the pod
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        // Check if the pod was assigned to a node
        let final_pod_yaml = store
            .get_pod_yaml(pod_name)
            .await
            .expect("Get final pod yaml failed");

        if let Some(yaml) = final_pod_yaml {
            let final_pod = serde_yaml::from_str::<PodTask>(&yaml)?;
            assert!(final_pod.spec.node_name.is_some());
        }
    }
    cleanup().await?;
    Ok(())
}

#[tokio::test]
#[serial]
async fn test_no_pod_assignment_when_no_nodes() -> Result<()> {
    cleanup().await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    let store = get_store().await;
    if store.is_none() {
        return Ok(());
    }
    let store = Arc::new(store.unwrap());

    run_scheduler(store.clone()).await?;

    // Create a PodTask with proper structure for the scheduler
    let pod_name = "scheduler-test-pod-no-nodes";
    let pod_task = create_test_pod(pod_name, Some("1"), Some("1Gi"));

    let initial_pod_yaml = serde_yaml::to_string(&pod_task).expect("Failed to serialize pod");

    // Insert initial pod without node assignment
    store
        .insert_pod_yaml(pod_name, &initial_pod_yaml)
        .await
        .expect("Insert pod yaml failed");

    // Wait a bit for the scheduler to potentially process the pod
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Check if the pod was assigned to a node
    let final_pod_yaml = store
        .get_pod_yaml(pod_name)
        .await
        .expect("Get final pod yaml failed");

    if let Some(yaml) = final_pod_yaml {
        let final_pod = serde_yaml::from_str::<PodTask>(&yaml)?;
        assert!(final_pod.spec.node_name.is_none());
    }

    // Clean up
    cleanup().await?;
    Ok(())
}
