use serial_test::serial;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

use common::{
    ContainerRes, ContainerSpec, Node, NodeAddress, NodeCondition, NodeSpec as XlineNodeSpec,
    NodeStatus, ObjectMeta, PodSpec as XlinePodSpec, PodStatus, PodTask, Resource,
};
use etcd_client::{Client, DeleteOptions};
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::with_xline::run_scheduler_with_xline;

const ETCD_ENDPOINTS: &[&str] = &["127.0.0.1:2379"];

struct EtcdTestClient {
    client: Client,
}

impl EtcdTestClient {
    async fn new() -> Result<Self, anyhow::Error> {
        let client = Client::connect(ETCD_ENDPOINTS, None).await?;
        Ok(Self { client })
    }

    async fn put_node(&mut self, node: &Node) -> Result<(), anyhow::Error> {
        let key = format!("/registry/nodes/{}", node.metadata.name);
        let value = serde_yaml::to_string(node)?;
        self.client.put(key, value, None).await?;
        Ok(())
    }

    async fn put_pod(&mut self, pod: &PodTask) -> Result<(), anyhow::Error> {
        let key = format!("/registry/pods/{}", pod.metadata.name);
        let value = serde_yaml::to_string(pod)?;
        self.client.put(key, value, None).await?;
        Ok(())
    }

    async fn delete_node(&mut self, node_name: &str) -> Result<(), anyhow::Error> {
        let key = format!("/registry/nodes/{node_name}");
        self.client.delete(key, None).await?;
        Ok(())
    }

    async fn delete_pod(&mut self, pod_name: &str) -> Result<(), anyhow::Error> {
        let key = format!("/registry/pods/{pod_name}");
        self.client.delete(key, None).await?;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<(), anyhow::Error> {
        self.client
            .delete("/registry/nodes/", Some(DeleteOptions::new().with_prefix()))
            .await?;
        self.client
            .delete("/registry/pods/", Some(DeleteOptions::new().with_prefix()))
            .await?;
        Ok(())
    }
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
        spec: XlineNodeSpec {
            pod_cidr: "10.244.0.0/24".to_string(),
            taints: vec![],
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
        spec: XlinePodSpec {
            node_name: None,
            containers: vec![ContainerSpec {
                name: "app".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec![],
                args: vec![],
                resources,
            }],
            init_containers: vec![],
            tolerations: vec![],
        },
        status: PodStatus { pod_ip: None },
    }
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_basic_scheduling() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node = create_test_node("test-node-1", "4", "4Gi");
    etcd_client
        .put_node(&node)
        .await
        .expect("Failed to put node");

    let pod = create_test_pod("test-pod-1", Some("1"), Some("1Gi"));
    etcd_client.put_pod(&pod).await.expect("Failed to put pod");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should produce assignment within timeout"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "test-pod-1");
    assert_eq!(assignment.node_name, "test-node-1");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_node_watch() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let pod = create_test_pod("test-pod-2", Some("2"), Some("2Gi"));
    etcd_client.put_pod(&pod).await.expect("Failed to put pod");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let result = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(
        result.is_err() || result.unwrap().is_none(),
        "No assignment should occur without nodes"
    );

    let node = create_test_node("test-node-2", "8", "8Gi");
    etcd_client
        .put_node(&node)
        .await
        .expect("Failed to put node");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should produce assignment after node addition"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "test-pod-2");
    assert_eq!(assignment.node_name, "test-node-2");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_pod_watch() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node = create_test_node("test-node-3", "4", "4Gi");
    etcd_client
        .put_node(&node)
        .await
        .expect("Failed to put node");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let result = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(
        result.is_err() || result.unwrap().is_none(),
        "No assignment should occur without pods"
    );

    let pod = create_test_pod("test-pod-3", Some("1"), Some("1Gi"));
    etcd_client.put_pod(&pod).await.expect("Failed to put pod");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should produce assignment after pod addition"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "test-pod-3");
    assert_eq!(assignment.node_name, "test-node-3");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_multiple_pods_and_nodes() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node1 = create_test_node("node-1", "4", "4Gi");
    let node2 = create_test_node("node-2", "8", "8Gi");
    etcd_client
        .put_node(&node1)
        .await
        .expect("Failed to put node1");
    etcd_client
        .put_node(&node2)
        .await
        .expect("Failed to put node2");

    let pod1 = create_test_pod("pod-1", Some("1"), Some("1Gi"));
    let pod2 = create_test_pod("pod-2", Some("6"), Some("6Gi"));
    etcd_client
        .put_pod(&pod1)
        .await
        .expect("Failed to put pod1");
    etcd_client
        .put_pod(&pod2)
        .await
        .expect("Failed to put pod2");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let mut assignments = Vec::new();
    for _ in 0..2 {
        let result = timeout(Duration::from_secs(5), rx.recv()).await;
        assert!(result.is_ok(), "Scheduler should produce assignments");
        let assignment = result
            .unwrap()
            .unwrap()
            .expect("Assignment should be successful");
        assignments.push((assignment.pod_name, assignment.node_name));
    }
    assignments.sort();

    let expected = vec![
        ("pod-1".to_string(), "node-1".to_string()),
        ("pod-2".to_string(), "node-2".to_string()),
    ];
    assert_eq!(assignments, expected);

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_node_deletion() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node1 = create_test_node("deletable-node", "4", "4Gi");
    let node2 = create_test_node("permanent-node", "4", "4Gi");
    etcd_client
        .put_node(&node1)
        .await
        .expect("Failed to put node1");
    etcd_client
        .put_node(&node2)
        .await
        .expect("Failed to put node2");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    etcd_client
        .delete_node("deletable-node")
        .await
        .expect("Failed to delete node");

    let pod = create_test_pod("test-pod-after-deletion", Some("1"), Some("1Gi"));
    etcd_client.put_pod(&pod).await.expect("Failed to put pod");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should still work after node deletion"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "test-pod-after-deletion");
    assert_eq!(assignment.node_name, "permanent-node");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_pod_deletion() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node = create_test_node("test-node", "4", "4Gi");
    etcd_client
        .put_node(&node)
        .await
        .expect("Failed to put node");

    let pod1 = create_test_pod("pod-to-delete", Some("1"), Some("1Gi"));
    let pod2 = create_test_pod("pod-to-keep", Some("1"), Some("1Gi"));
    etcd_client
        .put_pod(&pod1)
        .await
        .expect("Failed to put pod1");
    etcd_client
        .put_pod(&pod2)
        .await
        .expect("Failed to put pod2");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let mut assignments = Vec::new();
    for _ in 0..2 {
        let result = timeout(Duration::from_secs(5), rx.recv()).await;
        assert!(result.is_ok(), "Scheduler should produce assignments");
        let assignment = result
            .unwrap()
            .unwrap()
            .expect("Assignment should be successful");
        assignments.push(assignment.pod_name);
    }
    assignments.sort();

    assert_eq!(assignments, vec!["pod-to-delete", "pod-to-keep"]);

    etcd_client
        .delete_pod("pod-to-delete")
        .await
        .expect("Failed to delete pod");

    let pod3 = create_test_pod("new-pod", Some("1"), Some("1Gi"));
    etcd_client
        .put_pod(&pod3)
        .await
        .expect("Failed to put new pod");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(result.is_ok(), "Scheduler should work after pod deletion");

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "new-pod");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_existing_assignment() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node = create_test_node("existing-node", "4", "4Gi");
    etcd_client
        .put_node(&node)
        .await
        .expect("Failed to put node");

    let mut assigned_pod = create_test_pod("already-assigned", Some("1"), Some("1Gi"));
    assigned_pod.spec.node_name = Some("existing-node".to_string());
    etcd_client
        .put_pod(&assigned_pod)
        .await
        .expect("Failed to put assigned pod");

    let unassigned_pod = create_test_pod("not-assigned", Some("1"), Some("1Gi"));
    etcd_client
        .put_pod(&unassigned_pod)
        .await
        .expect("Failed to put unassigned pod");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should only schedule unassigned pods"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "not-assigned");
    assert_eq!(assignment.node_name, "existing-node");

    let no_more_result = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(
        no_more_result.is_err() || no_more_result.unwrap().is_none(),
        "No more assignments should occur"
    );

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_pod_reassume() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    let node = create_test_node("reassume-node", "4", "4Gi");
    etcd_client
        .put_node(&node)
        .await
        .expect("Failed to put node");

    let pod = create_test_pod("reassume-pod", Some("1"), Some("1Gi"));
    etcd_client.put_pod(&pod).await.expect("Failed to put pod");

    let (unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        ETCD_ENDPOINTS,
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should produce initial assignment"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "reassume-pod");
    assert_eq!(assignment.node_name, "reassume-node");

    unassume_tx
        .send("reassume-pod".to_string())
        .expect("Failed to send reassume request");

    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should produce reassigned assignment"
    );

    let reassignment = result
        .unwrap()
        .unwrap()
        .expect("Reassignment should be successful");
    assert_eq!(reassignment.pod_name, "reassume-pod");
    assert_eq!(reassignment.node_name, "reassume-node");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}
