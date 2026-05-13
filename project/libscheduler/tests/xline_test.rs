use serial_test::serial;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

use common::{
    Affinity, ContainerRes, ContainerSpec, GangSpec as XlineGangSpec, LabelSelector, Node,
    NodeAddress, NodeCondition, NodeSpec as XlineNodeSpec, NodeStatus, ObjectMeta, PodAffinity,
    PodAffinityTerm, PodSpec as XlinePodSpec, PodStatus, PodTask, Resource,
    TopologyConstraint as XlineTopologyConstraint,
};
use etcd_client::{Client, DeleteOptions};
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::with_xline::run_scheduler_with_xline;
use libscheduler::with_xline::utils;
use libvault::storage::xline::XlineOptions;

const ETCD_ENDPOINTS: &[&str] = &["127.0.0.1:2379"];

fn xline_options() -> XlineOptions {
    XlineOptions::new(
        ETCD_ENDPOINTS
            .iter()
            .map(|endpoint| endpoint.to_string())
            .collect(),
    )
}

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
            ..Default::default()
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
                ..Default::default()
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
            ..Default::default()
        },
        spec: XlinePodSpec {
            node_name: None,
            containers: vec![ContainerSpec {
                name: "app".to_string(),
                image: "nginx:latest".to_string(),
                ports: vec![],
                args: vec![],
                tty: false,
                gpus: None,
                resources,
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
            affinity: None,
            ..Default::default()
        },
        status: PodStatus::default(),
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
        xline_options(),
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
        xline_options(),
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
        xline_options(),
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
        xline_options(),
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
        xline_options(),
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
        xline_options(),
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
        xline_options(),
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
        xline_options(),
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

#[tokio::test]
#[serial]
async fn test_pod_affinity_conversion_from_xline() {
    // Test that pod affinity is correctly read and converted from Xline
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    // Create a pod with pod affinity using the standard create_test_pod function
    let mut pod = create_test_pod("affinity-test-pod", Some("1"), Some("1Gi"));

    // Create a pod affinity term: require pod with label app=web in same zone
    let pod_affinity_term = PodAffinityTerm {
        label_selector: Some(LabelSelector {
            match_labels: {
                let mut map = HashMap::new();
                map.insert("app".to_string(), "web".to_string());
                map
            },
            match_expressions: vec![],
        }),
        topology_key: "zone".to_string(),
        namespaces: None,
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: None,
    };

    pod.spec.affinity = Some(affinity);

    // Put the pod to Xline
    etcd_client.put_pod(&pod).await.expect("Failed to put pod");

    // Get the pod back from Xline using the utility function
    let mut client = etcd_client.client.clone();
    let pod_result = utils::get_pod(&mut client, "affinity-test-pod")
        .await
        .expect("Failed to get pod");
    let retrieved_pod: common::PodTask = pod_result.expect("Pod should exist");

    // Convert the pod task to pod info using the conversion function
    let pod_info = utils::convert_pod_task_to_pod_info(retrieved_pod);

    // Verify that affinity was correctly converted
    assert!(pod_info.spec.affinity.is_some());
    let affinity = pod_info.spec.affinity.unwrap();
    assert!(affinity.pod_affinity.is_some());
    assert!(affinity.node_affinity.is_none());
    assert!(affinity.pod_anti_affinity.is_none());

    let pod_affinity = affinity.pod_affinity.unwrap();
    assert!(
        pod_affinity
            .required_during_scheduling_ignored_during_execution
            .is_some()
    );
    let terms = pod_affinity
        .required_during_scheduling_ignored_during_execution
        .unwrap();
    assert_eq!(terms.len(), 1);
    let term = &terms[0];
    assert_eq!(term.topology_key, "zone");
    assert!(term.label_selector.is_some());
    let selector = term.label_selector.as_ref().unwrap();
    assert_eq!(selector.match_labels.get("app"), Some(&"web".to_string()));
    assert!(selector.match_expressions.is_empty());

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

#[tokio::test]
#[serial]
async fn test_scheduler_with_xline_pod_affinity_scheduling() {
    let mut etcd_client = EtcdTestClient::new()
        .await
        .expect("Failed to connect to etcd");
    etcd_client.cleanup().await.expect("Failed to cleanup etcd");

    // Create two nodes with zone labels
    let mut node1 = create_test_node("node-zone-a", "4", "4Gi");
    node1
        .metadata
        .labels
        .insert("zone".to_string(), "zone-a".to_string());

    let mut node2 = create_test_node("node-zone-b", "4", "4Gi");
    node2
        .metadata
        .labels
        .insert("zone".to_string(), "zone-b".to_string());

    etcd_client
        .put_node(&node1)
        .await
        .expect("Failed to put node1");
    etcd_client
        .put_node(&node2)
        .await
        .expect("Failed to put node2");

    // Create an existing pod with label app=web scheduled on node-zone-a
    let mut web_pod = create_test_pod("web-pod", Some("1"), Some("1Gi"));
    web_pod
        .metadata
        .labels
        .insert("app".to_string(), "web".to_string());
    web_pod.spec.node_name = Some("node-zone-a".to_string());
    etcd_client
        .put_pod(&web_pod)
        .await
        .expect("Failed to put web-pod");

    // Create a new pod with required affinity to web app pods in same zone
    let mut selector_labels = HashMap::new();
    selector_labels.insert("app".to_string(), "web".to_string());
    let label_selector = Some(LabelSelector {
        match_labels: selector_labels,
        match_expressions: vec![],
    });

    let pod_affinity_term = PodAffinityTerm {
        label_selector,
        topology_key: "zone".to_string(),
        namespaces: None,
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: None,
    };

    let mut affinity_pod = create_test_pod("affinity-pod", Some("1"), Some("1Gi"));
    affinity_pod.spec.affinity = Some(affinity);

    etcd_client
        .put_pod(&affinity_pod)
        .await
        .expect("Failed to put affinity-pod");

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        xline_options(),
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    // Wait for assignment - should schedule affinity-pod to node-zone-a
    let result = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(
        result.is_ok(),
        "Scheduler should produce assignment within timeout"
    );

    let assignment = result
        .unwrap()
        .unwrap()
        .expect("Assignment should be successful");
    assert_eq!(assignment.pod_name, "affinity-pod");
    assert_eq!(assignment.node_name, "node-zone-a");

    etcd_client.cleanup().await.expect("Failed to cleanup etcd");
}

// ─── GPU Gang scheduling helpers ────────────────────────────────────────────

const NVLINK_KEY: &str = "topology.rk8s.io/nvlink-domain";

/// Build a Node with GPU-capacity labels understood by the scheduler's
/// `convert_k8s_node_to_node_info`:
///   nvidia.com/gpu.count        → GpuResources::total
///   nvidia.com/gpu.memory-gib   → GpuResources::memory_per_gpu (GiB → bytes)
///   nvidia.com/gpu.product      → GpuResources::model
///   topology.rk8s.io/nvlink-domain → used by TopologyCoAffinityFilter
fn create_gpu_node(
    name: &str,
    gpu_count: u32,
    mem_gib: u64,
    model: &str,
    nvlink_domain: &str,
) -> Node {
    let cpu = "128";
    let memory = "512Gi";
    let mut capacity = HashMap::new();
    capacity.insert("cpu".to_string(), cpu.to_string());
    capacity.insert("memory".to_string(), memory.to_string());
    let mut allocatable = capacity.clone();
    allocatable.insert("cpu".to_string(), cpu.to_string());
    allocatable.insert("memory".to_string(), memory.to_string());

    let mut labels = HashMap::new();
    labels.insert("nvidia.com/gpu.count".to_string(), gpu_count.to_string());
    labels.insert("nvidia.com/gpu.memory-gib".to_string(), mem_gib.to_string());
    labels.insert("nvidia.com/gpu.product".to_string(), model.to_string());
    labels.insert(NVLINK_KEY.to_string(), nvlink_domain.to_string());

    Node {
        api_version: "v1".to_string(),
        kind: "Node".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "".to_string(),
            labels,
            annotations: HashMap::new(),
            ..Default::default()
        },
        spec: XlineNodeSpec {
            pod_cidr: "10.244.0.0/24".to_string(),
            taints: vec![],
        },
        status: NodeStatus {
            capacity,
            allocatable,
            addresses: vec![NodeAddress {
                address_type: "Hostname".to_string(),
                address: name.to_string(),
            }],
            conditions: vec![NodeCondition {
                condition_type: common::NodeConditionType::Ready,
                status: common::ConditionStatus::True,
                last_heartbeat_time: None,
            }],
        },
    }
}

/// Build a PodTask with:
///   - container GPU resource limits  (read by `convert_pod_task_to_pod_info`)
///   - gang spec                       (read by the scheduler for All-or-Nothing)
///   - topology constraint on NVLink domain (read by TopologyCoAffinityFilter)
fn create_gang_pod(
    name: &str,
    gpu_request: u32,
    gpu_memory_gib: u64,
    gang_id: &str,
    gang_size: u32,
) -> PodTask {
    PodTask {
        api_version: "v1".to_string(),
        kind: "Pod".to_string(),
        metadata: ObjectMeta {
            name: name.to_string(),
            namespace: "default".to_string(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            ..Default::default()
        },
        spec: XlinePodSpec {
            node_name: None,
            containers: vec![ContainerSpec {
                name: "inference".to_string(),
                image: "nvidia/cuda:12.0-base".to_string(),
                ports: vec![],
                args: vec![],
                tty: false,
                resources: Some(ContainerRes {
                    limits: Some(Resource {
                        cpu: Some("8".to_string()),
                        memory: Some("32Gi".to_string()),
                        gpu: Some(gpu_request),
                        gpu_memory: Some(format!("{gpu_memory_gib}Gi")),
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
            affinity: None,
            gang: Some(XlineGangSpec {
                id: gang_id.to_string(),
                size: gang_size,
            }),
            topology_constraints: vec![XlineTopologyConstraint {
                topology_key: NVLINK_KEY.to_string(),
                same_value: true,
            }],
            ..Default::default()
        },
        status: PodStatus::default(),
    }
}

// ─── GPU Gang scheduling tests ───────────────────────────────────────────────

/// TP=4 scenario (matching the HTML animation):
///   node-A: 8 GPU, domain0  — can fit 4 pods × 2 GPU = 8 GPU
///   node-B: 4 GPU, domain1  — cannot fit a 4-pod gang of 2 GPU each
///
/// TopologyCoAffinityFilter ensures that once pod-1 lands on domain0,
/// pods 2-4 are constrained to domain0 as well.
/// GangStateStore holds all 4 assumes and releases assignments atomically.
/// Expected: all 4 assignments → node-A.
#[tokio::test]
#[serial]
async fn test_gpu_gang_all_pods_land_on_same_nvlink_domain() {
    let mut etcd = EtcdTestClient::new()
        .await
        .expect("Failed to connect to Xline/etcd");
    etcd.cleanup().await.expect("cleanup failed");

    // node-A: 8 GPU / 40 GiB per card / domain0
    etcd.put_node(&create_gpu_node(
        "gpu-node-a",
        8,
        40,
        "A800-SXM4-40GB",
        "domain0",
    ))
    .await
    .expect("put gpu-node-a");
    // node-B: 4 GPU / 40 GiB per card / domain1 — topology mismatch after pod-1 lands
    etcd.put_node(&create_gpu_node(
        "gpu-node-b",
        4,
        40,
        "A800-SXM4-40GB",
        "domain1",
    ))
    .await
    .expect("put gpu-node-b");

    // 4 pods forming gang "tp4-prefill", each requesting 2 GPU / 20 GiB
    for i in 0..4u32 {
        etcd.put_pod(&create_gang_pod(
            &format!("tp4-pod-{i}"),
            2,
            20,
            "tp4-prefill",
            4,
        ))
        .await
        .expect("put gang pod");
    }

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        xline_options(),
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    let mut assignments = Vec::new();
    for _ in 0..4 {
        let res = timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out — gang never filled")
            .expect("scheduler channel closed")
            .expect("scheduling error");
        assignments.push(res);
    }

    // All four must land on node-A (domain0 is the only domain that can fit
    // 4 pods × 2 GPU = 8 GPU, while node-B only has 4 GPU in domain1).
    for a in &assignments {
        assert_eq!(
            a.node_name, "gpu-node-a",
            "pod {} landed on wrong node (expected gpu-node-a)",
            a.pod_name
        );
    }

    // Verify every pod in the gang was assigned.
    let mut names: Vec<_> = assignments.iter().map(|a| a.pod_name.clone()).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["tp4-pod-0", "tp4-pod-1", "tp4-pod-2", "tp4-pod-3"]
    );

    etcd.cleanup().await.expect("cleanup failed");
}

/// Gang scheduling without topology constraint — All-or-Nothing still applies.
/// Three pods form a gang; no topology_constraints means any node is valid.
/// With a single node that has enough resources, all three should be assigned.
#[tokio::test]
#[serial]
async fn test_gpu_gang_without_topology_constraint_is_atomic() {
    let mut etcd = EtcdTestClient::new()
        .await
        .expect("Failed to connect to Xline/etcd");
    etcd.cleanup().await.expect("cleanup failed");

    etcd.put_node(&create_gpu_node(
        "single-gpu-node",
        8,
        40,
        "A800-SXM4-40GB",
        "domain0",
    ))
    .await
    .expect("put node");

    // 3 pods, no topology constraint (gang only, not topology-bound)
    for i in 0..3u32 {
        let pod = PodTask {
            api_version: "v1".to_string(),
            kind: "Pod".to_string(),
            metadata: ObjectMeta {
                name: format!("notopo-pod-{i}"),
                namespace: "default".to_string(),
                labels: HashMap::new(),
                annotations: HashMap::new(),
                ..Default::default()
            },
            spec: XlinePodSpec {
                node_name: None,
                containers: vec![ContainerSpec {
                    name: "worker".to_string(),
                    image: "nvidia/cuda:12.0-base".to_string(),
                    ports: vec![],
                    args: vec![],
                    tty: false,
                    resources: Some(ContainerRes {
                        limits: Some(Resource {
                            cpu: Some("4".to_string()),
                            memory: Some("16Gi".to_string()),
                            gpu: Some(2),
                            gpu_memory: None,
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
                affinity: None,
                gang: Some(XlineGangSpec {
                    id: "no-topo-gang".to_string(),
                    size: 3,
                }),
                topology_constraints: vec![], // no topology constraint
                ..Default::default()
            },
            status: PodStatus::default(),
        };
        etcd.put_pod(&pod).await.expect("put pod");
    }

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        xline_options(),
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    // All 3 must be assigned atomically (gang fills at 3/3).
    let mut names = Vec::new();
    for _ in 0..3 {
        let res = timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out — no-topology gang never filled")
            .expect("scheduler channel closed")
            .expect("scheduling error");
        assert_eq!(res.node_name, "single-gpu-node");
        names.push(res.pod_name);
    }
    names.sort();
    assert_eq!(names, vec!["notopo-pod-0", "notopo-pod-1", "notopo-pod-2"]);

    etcd.cleanup().await.expect("cleanup failed");
}

/// Insufficient resources — gang can never be fully assumed.
///   node: 4 GPU, gang needs 4 pods × 2 GPU = 8 GPU → only 2 pods fit.
///
/// The gang times out, all assumes are rolled back, and no Assignment is
/// ever sent.  We use the scheduler's shortened timeout via `with_gang_timeout`
/// — but because `run_scheduler_with_xline` doesn't expose that knob, we
/// verify the absence of assignments during a short observation window and
/// accept that the real rollback happens later (the key invariant is that
/// a partial gang does NOT generate assignments).
#[tokio::test]
#[serial]
async fn test_gpu_gang_partial_fill_produces_no_assignment() {
    let mut etcd = EtcdTestClient::new()
        .await
        .expect("Failed to connect to Xline/etcd");
    etcd.cleanup().await.expect("cleanup failed");

    // Only 4 GPU available — a 4-pod gang of 2 GPU each needs 8 GPU total.
    etcd.put_node(&create_gpu_node(
        "small-gpu-node",
        4,
        40,
        "A800-SXM4-40GB",
        "domain0",
    ))
    .await
    .expect("put node");

    for i in 0..4u32 {
        etcd.put_pod(&create_gang_pod(
            &format!("partial-pod-{i}"),
            2,
            20,
            "partial-gang",
            4,
        ))
        .await
        .expect("put pod");
    }

    let (_unassume_tx, unassume_rx) = mpsc::unbounded_channel();
    let mut rx = run_scheduler_with_xline(
        xline_options(),
        ScoringStrategy::LeastAllocated,
        Plugins::default(),
        unassume_rx,
    )
    .await
    .expect("Failed to start scheduler");

    // The gang can never fill (only 2 pods can be assumed), so no Assignment
    // should appear within a reasonable observation window.
    let result = timeout(Duration::from_secs(3), rx.recv()).await;
    assert!(
        result.is_err(),
        "no Assignment should be produced for a gang that cannot fill"
    );

    etcd.cleanup().await.expect("cleanup failed");
}
