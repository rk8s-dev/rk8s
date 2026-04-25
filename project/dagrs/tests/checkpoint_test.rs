//! Tests for Checkpoint mechanism (REQ-002)
//!
//! This module tests the checkpoint system for state persistence and recovery.
//! Key features tested:
//! - Checkpoint creation and storage
//! - Checkpoint loading and restoration
//! - Resuming execution from checkpoint
//! - Automatic checkpointing configuration
//! - Memory and file-based checkpoint stores

use async_trait::async_trait;
use dagrs::graph::event::{GraphEvent, TerminationStatus};
use dagrs::node::action::Action;
use dagrs::node::conditional_node::{Condition, ConditionalNode};
use dagrs::node::default_node::DefaultNode;
use dagrs::node::loop_node::CountLoopCondition;
use dagrs::{
    Checkpoint, CheckpointConfig, CheckpointStore, EnvVar, Graph, InChannels,
    MemoryCheckpointStore, Node, NodeExecStatus, NodeState, NodeTable, OutChannels, Output,
    StoredOutputKind,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

async fn collect_events_until_terminated(
    mut receiver: tokio::sync::broadcast::Receiver<GraphEvent>,
) -> Vec<GraphEvent> {
    let mut events = Vec::new();
    while let Ok(Ok(event)) =
        tokio::time::timeout(Duration::from_millis(200), receiver.recv()).await
    {
        let terminated = matches!(event, GraphEvent::ExecutionTerminated { .. });
        events.push(event);
        if terminated {
            break;
        }
    }
    events
}

/// Action that counts executions
#[derive(Clone)]
struct CounterAction {
    count: Arc<Mutex<usize>>,
}

#[async_trait]
impl Action for CounterAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        let mut c = self.count.lock().unwrap();
        *c += 1;
        Output::new(*c)
    }
}

#[derive(Clone)]
struct ProduceCheckpointedValue {
    runs: Arc<Mutex<usize>>,
}

#[async_trait]
impl Action for ProduceCheckpointedValue {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        *self.runs.lock().unwrap() += 1;
        Output::new("checkpointed".to_string())
    }
}

#[derive(Clone)]
struct CaptureCheckpointedValue {
    source_id: dagrs::NodeId,
    seen: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Action for CaptureCheckpointedValue {
    async fn run(&self, input: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        let content = input.recv_from(&self.source_id).await.unwrap();
        let value = content.get::<String>().unwrap().clone();
        self.seen.lock().unwrap().push(value);
        Output::empty()
    }
}

#[derive(Clone)]
struct ObserveActiveSendersAction {
    expected_sender: dagrs::NodeId,
    observed_senders: Arc<Mutex<Vec<Vec<usize>>>>,
}

#[async_trait]
impl Action for ObserveActiveSendersAction {
    async fn run(&self, input: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        let mut sender_ids: Vec<_> = input
            .get_sender_ids()
            .into_iter()
            .map(|id| id.as_usize())
            .collect();
        sender_ids.sort_unstable();
        self.observed_senders.lock().unwrap().push(sender_ids);

        let content = input.recv_from(&self.expected_sender).await.unwrap();
        let value = content.get::<String>().unwrap().clone();
        Output::new(value)
    }
}

struct AlwaysTrueCondition;

#[async_trait]
impl Condition for AlwaysTrueCondition {
    async fn run(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> bool {
        true
    }
}

struct NonSerializablePayload(&'static str);

#[derive(Clone)]
struct ProduceNonSerializableValue {
    runs: Arc<Mutex<usize>>,
}

#[async_trait]
impl Action for ProduceNonSerializableValue {
    async fn run(&self, _: &mut InChannels, out: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        *self.runs.lock().unwrap() += 1;
        let _ = out
            .broadcast(dagrs::Content::new(NonSerializablePayload("custom")))
            .await;
        Output::new(NonSerializablePayload("custom"))
    }
}

#[derive(Clone)]
struct FailOnceAfterReceivingNonSerializable {
    source_id: dagrs::NodeId,
    seen: Arc<Mutex<Vec<String>>>,
    fail_first: Arc<AtomicBool>,
}

#[async_trait]
impl Action for FailOnceAfterReceivingNonSerializable {
    async fn run(&self, input: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        let content = input.recv_from(&self.source_id).await.unwrap();
        let value = content
            .get::<NonSerializablePayload>()
            .unwrap()
            .0
            .to_string();
        self.seen.lock().unwrap().push(value);

        if self.fail_first.swap(false, Ordering::SeqCst) {
            Output::execution_failed("intentional failure after checkpoint")
        } else {
            Output::empty()
        }
    }
}

#[tokio::test]
async fn test_checkpoint_store_configuration() {
    let mut graph = Graph::new();
    let store = MemoryCheckpointStore::new();

    // Configure checkpoint store
    graph.set_checkpoint_store(Box::new(store));

    // Configure checkpoint settings
    let config = CheckpointConfig::enabled()
        .with_node_interval(5)
        .with_max_checkpoints(3);
    graph.set_checkpoint_config(config);

    // Configuration is set - verified through subsequent operations
    // (direct field access is private, which is correct encapsulation)
}

#[tokio::test]
async fn test_manual_checkpoint_save_and_load() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    // Set up graph with nodes
    let node_a = DefaultNode::new("A".to_string(), &mut table);
    let node_b = DefaultNode::new("B".to_string(), &mut table);
    let id_a = node_a.id();
    let id_b = node_b.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_a, vec![id_b]).unwrap();

    graph.set_checkpoint_store(Box::new(MemoryCheckpointStore::new()));

    // Run the graph first to initialize it
    graph.async_start().await.unwrap();

    // Reset and try to list checkpoints (should work if store is configured)
    let checkpoints = graph.list_checkpoints().await.unwrap();
    // Initially empty since we didn't enable auto-checkpoint
    assert!(checkpoints.is_empty());
}

#[tokio::test]
async fn test_checkpoint_list_and_delete() {
    let store = MemoryCheckpointStore::new();

    // Create multiple checkpoints
    let cp1 = Checkpoint::with_id("test_1", 1, 0);
    let cp2 = Checkpoint::with_id("test_2", 2, 1);
    let cp3 = Checkpoint::with_id("test_3", 3, 2);

    store.save(&cp1).await.unwrap();
    store.save(&cp2).await.unwrap();
    store.save(&cp3).await.unwrap();

    // List checkpoints
    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 3);

    // Delete one checkpoint
    store.delete(&"test_2".to_string()).await.unwrap();

    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 2);
    assert!(!ids.contains(&"test_2".to_string()));

    // Clear all checkpoints
    store.clear().await.unwrap();
    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 0);
}

#[tokio::test]
async fn test_get_latest_checkpoint() {
    let store = MemoryCheckpointStore::new();

    // Create checkpoints with different timestamps
    let mut cp1 = Checkpoint::with_id("old", 1, 0);
    cp1.timestamp = 1000;

    let mut cp2 = Checkpoint::with_id("newer", 2, 1);
    cp2.timestamp = 2000;

    let mut cp3 = Checkpoint::with_id("newest", 3, 2);
    cp3.timestamp = 3000;

    store.save(&cp1).await.unwrap();
    store.save(&cp3).await.unwrap();
    store.save(&cp2).await.unwrap();

    // Get latest should return cp3
    let latest = store.latest().await.unwrap().unwrap();
    assert_eq!(latest.id, "newest");
    assert_eq!(latest.pc, 3);
}

#[tokio::test]
async fn test_checkpoint_with_node_states() {
    use dagrs::NodeState;
    let mut checkpoint = Checkpoint::new(5, 2);

    // Add node states using the new API
    checkpoint.add_node_state(NodeState::completed(1, true));
    checkpoint.add_node_state(NodeState::completed(2, false));
    checkpoint.add_node_state(NodeState::pending(3));

    assert_eq!(checkpoint.node_states.len(), 3);
    assert_eq!(
        checkpoint.node_states.get(&1).unwrap().status,
        NodeExecStatus::Succeeded
    );
    assert_eq!(
        checkpoint.node_states.get(&2).unwrap().status,
        NodeExecStatus::Failed
    );
    assert_eq!(
        checkpoint.node_states.get(&3).unwrap().status,
        NodeExecStatus::Pending
    );
}

#[tokio::test]
async fn test_checkpoint_metadata() {
    let mut checkpoint = Checkpoint::new(0, 0);

    checkpoint.add_metadata("graph_name", "test_graph");
    checkpoint.add_metadata("version", "1.0");
    checkpoint.add_metadata("environment", "test");

    assert_eq!(
        checkpoint.metadata.get("graph_name"),
        Some(&"test_graph".to_string())
    );
    assert_eq!(checkpoint.metadata.get("version"), Some(&"1.0".to_string()));
    assert_eq!(
        checkpoint.metadata.get("environment"),
        Some(&"test".to_string())
    );
}

#[tokio::test]
async fn test_checkpoint_events() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let count = Arc::new(Mutex::new(0));
    let node_a = DefaultNode::with_action(
        "A".to_string(),
        CounterAction {
            count: count.clone(),
        },
        &mut table,
    );
    let node_b = DefaultNode::with_action(
        "B".to_string(),
        CounterAction {
            count: count.clone(),
        },
        &mut table,
    );
    let id_a = node_a.id();
    let id_b = node_b.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_a, vec![id_b]).unwrap();

    graph.set_checkpoint_store(Box::new(MemoryCheckpointStore::new()));

    // Enable checkpointing on every node
    let config = CheckpointConfig::enabled()
        .with_node_interval(1)
        .with_max_checkpoints(10);
    graph.set_checkpoint_config(config);

    let receiver = graph.subscribe();
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let collector = tokio::spawn(collect_events_until_terminated(receiver));

    // Run graph
    graph.async_start().await.unwrap();

    tokio::time::sleep(Duration::from_millis(50)).await;

    let collected = collector.await.unwrap();
    *events_clone.lock().unwrap() = collected;

    let events_list = events.lock().unwrap();

    // Should have node events
    let has_node_start = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::NodeStart { .. }));
    let has_node_success = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::NodeSuccess { .. }));
    let has_terminated = events_list.iter().any(|e| {
        matches!(
            e,
            GraphEvent::ExecutionTerminated {
                status: TerminationStatus::Succeeded,
                error: None,
            }
        )
    });

    assert!(has_node_start, "Should have NodeStart events");
    assert!(has_node_success, "Should have NodeSuccess events");
    assert!(has_terminated, "Should have a successful termination event");
}

#[tokio::test]
async fn test_checkpoint_config_builder() {
    // Test default config
    let default_config = CheckpointConfig::default();
    assert!(!default_config.enabled);
    assert_eq!(default_config.interval_nodes, Some(10));
    assert!(default_config.on_loop_iteration);
    assert_eq!(default_config.max_checkpoints, 5);

    // Test builder pattern
    let custom_config = CheckpointConfig::enabled()
        .with_node_interval(20)
        .with_time_interval(300)
        .with_loop_checkpoint(false)
        .with_max_checkpoints(10);

    assert!(custom_config.enabled);
    assert_eq!(custom_config.interval_nodes, Some(20));
    assert_eq!(custom_config.interval_seconds, Some(300));
    assert!(!custom_config.on_loop_iteration);
    assert_eq!(custom_config.max_checkpoints, 10);
}

#[tokio::test]
async fn test_checkpoint_serialization() {
    use dagrs::NodeState;
    let mut checkpoint = Checkpoint::new(5, 3);
    checkpoint.active_nodes.insert(1);
    checkpoint.active_nodes.insert(2);
    checkpoint.active_nodes.insert(3);
    checkpoint.add_metadata("key", "value");
    checkpoint.add_node_state(NodeState::completed(1, true));

    // Serialize to JSON
    let json = serde_json::to_string(&checkpoint).unwrap();
    assert!(json.contains("\"pc\":5"));
    assert!(json.contains("\"loop_count\":3"));

    // Deserialize back
    let restored: Checkpoint = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.pc, 5);
    assert_eq!(restored.loop_count, 3);
    assert_eq!(restored.active_nodes.len(), 3);
    assert_eq!(restored.metadata.get("key"), Some(&"value".to_string()));
}

#[tokio::test]
async fn test_resume_execution_basic() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let producer_runs = Arc::new(Mutex::new(0));
    let seen = Arc::new(Mutex::new(Vec::new()));

    let node_a = DefaultNode::with_action(
        "A".to_string(),
        ProduceCheckpointedValue {
            runs: producer_runs.clone(),
        },
        &mut table,
    );
    let id_a = node_a.id();
    let gate = ConditionalNode::with_condition("Gate".to_string(), AlwaysTrueCondition, &mut table);
    let id_gate = gate.id();
    let node_b = DefaultNode::with_action(
        "B".to_string(),
        CaptureCheckpointedValue {
            source_id: id_a,
            seen: seen.clone(),
        },
        &mut table,
    );
    let id_b = node_b.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(gate).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_a, vec![id_gate]).unwrap();
    graph.add_edge(id_gate, vec![id_b]).unwrap();
    graph.add_edge(id_a, vec![id_b]).unwrap();

    let store = MemoryCheckpointStore::new();
    let mut checkpoint = Checkpoint::with_id("resume_basic", 1, 0);
    checkpoint.active_nodes.insert(id_a.as_usize());
    checkpoint.active_nodes.insert(id_gate.as_usize());
    checkpoint.active_nodes.insert(id_b.as_usize());
    checkpoint.add_node_state(
        NodeState::succeeded(id_a.as_usize())
            .with_output_kind(StoredOutputKind::String)
            .with_output_data(serde_json::to_vec(&"checkpointed").unwrap())
            .with_summary("String(checkpointed)"),
    );
    checkpoint.add_node_state(NodeState::succeeded(id_gate.as_usize()));
    checkpoint.add_node_state(NodeState::pending(id_b.as_usize()));
    store.save(&checkpoint).await.unwrap();
    graph.set_checkpoint_store(Box::new(store));

    let report = tokio::time::timeout(
        Duration::from_secs(1),
        graph.resume_from_checkpoint("resume_basic"),
    )
    .await
    .expect("resume should not hang")
    .expect("resume should succeed");

    assert_eq!(*producer_runs.lock().unwrap(), 0, "A should not rerun");
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["checkpointed"],
        "B should receive the checkpointed output",
    );
    assert_eq!(report.node_succeeded, 3);
}

#[tokio::test]
async fn test_resume_reruns_nodes_with_unserializable_outputs() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let producer_runs = Arc::new(Mutex::new(0usize));
    let seen = Arc::new(Mutex::new(Vec::new()));
    let fail_first = Arc::new(AtomicBool::new(true));

    let producer = DefaultNode::with_action(
        "Producer".to_string(),
        ProduceNonSerializableValue {
            runs: producer_runs.clone(),
        },
        &mut table,
    );
    let id_producer = producer.id();
    let gate = ConditionalNode::with_condition("Gate".to_string(), AlwaysTrueCondition, &mut table);
    let id_gate = gate.id();
    let consumer = DefaultNode::with_action(
        "Consumer".to_string(),
        FailOnceAfterReceivingNonSerializable {
            source_id: id_producer,
            seen: seen.clone(),
            fail_first: fail_first.clone(),
        },
        &mut table,
    );
    let id_consumer = consumer.id();

    graph.add_node(producer).unwrap();
    graph.add_node(gate).unwrap();
    graph.add_node(consumer).unwrap();
    graph.add_edge(id_producer, vec![id_gate]).unwrap();
    graph.add_edge(id_gate, vec![id_consumer]).unwrap();
    graph.add_edge(id_producer, vec![id_consumer]).unwrap();

    graph.set_checkpoint_store(Box::new(MemoryCheckpointStore::new()));
    graph.set_checkpoint_config(
        CheckpointConfig::enabled()
            .with_node_interval(1)
            .with_max_checkpoints(10),
    );

    let err = graph
        .async_start()
        .await
        .expect_err("first run should fail");
    assert_eq!(err.code, dagrs::ErrorCode::DgRun0006NodeExecutionFailed);

    let checkpoint = graph
        .get_latest_checkpoint()
        .await
        .unwrap()
        .expect("checkpoint should exist before the failing block");

    let report = tokio::time::timeout(
        Duration::from_secs(1),
        graph.resume_from_checkpoint(&checkpoint.id),
    )
    .await
    .expect("resume should not hang")
    .expect("resume should succeed");

    assert_eq!(
        *producer_runs.lock().unwrap(),
        2,
        "producer should rerun because its output could not be checkpointed",
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["custom".to_string(), "custom".to_string()],
    );
    assert_eq!(report.node_succeeded, 3);
    assert_eq!(report.node_failed, 0);
}

#[tokio::test]
async fn test_resume_reruns_legacy_succeeded_node_without_checkpointed_output() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let producer_runs = Arc::new(Mutex::new(0usize));
    let seen = Arc::new(Mutex::new(Vec::new()));

    let producer = DefaultNode::with_action(
        "Producer".to_string(),
        ProduceNonSerializableValue {
            runs: producer_runs.clone(),
        },
        &mut table,
    );
    let id_producer = producer.id();
    let gate = ConditionalNode::with_condition("Gate".to_string(), AlwaysTrueCondition, &mut table);
    let id_gate = gate.id();
    let consumer = DefaultNode::with_action(
        "Consumer".to_string(),
        FailOnceAfterReceivingNonSerializable {
            source_id: id_producer,
            seen: seen.clone(),
            fail_first: Arc::new(AtomicBool::new(false)),
        },
        &mut table,
    );
    let id_consumer = consumer.id();

    graph.add_node(producer).unwrap();
    graph.add_node(gate).unwrap();
    graph.add_node(consumer).unwrap();
    graph.add_edge(id_producer, vec![id_gate]).unwrap();
    graph.add_edge(id_gate, vec![id_consumer]).unwrap();
    graph.add_edge(id_producer, vec![id_consumer]).unwrap();

    let store = MemoryCheckpointStore::new();
    let mut checkpoint = Checkpoint::with_id("legacy_unserializable_success", 1, 0);
    checkpoint.active_nodes.extend([
        id_producer.as_usize(),
        id_gate.as_usize(),
        id_consumer.as_usize(),
    ]);
    checkpoint.add_node_state(
        NodeState::succeeded(id_producer.as_usize())
            .with_summary("legacy checkpoint without replayable output"),
    );
    checkpoint.add_node_state(NodeState::succeeded(id_gate.as_usize()));
    checkpoint.add_node_state(NodeState::pending(id_consumer.as_usize()));
    store.save(&checkpoint).await.unwrap();
    graph.set_checkpoint_store(Box::new(store));

    let report = tokio::time::timeout(
        Duration::from_secs(1),
        graph.resume_from_checkpoint("legacy_unserializable_success"),
    )
    .await
    .expect("resume should not hang")
    .expect("legacy checkpoint should be recoverable");

    assert_eq!(
        *producer_runs.lock().unwrap(),
        1,
        "producer should rerun when a legacy checkpoint cannot replay its output",
    );
    assert_eq!(seen.lock().unwrap().as_slice(), ["custom".to_string()]);
    assert_eq!(report.node_succeeded, 3);
    assert_eq!(report.node_failed, 0);
}

#[tokio::test]
async fn test_resume_restores_effective_active_upstreams() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let observed_senders = Arc::new(Mutex::new(Vec::new()));

    let node_a = DefaultNode::new("A".to_string(), &mut table);
    let id_a = node_a.id();
    let node_b = DefaultNode::new("B".to_string(), &mut table);
    let id_b = node_b.id();
    let gate = ConditionalNode::with_condition("Gate".to_string(), AlwaysTrueCondition, &mut table);
    let id_gate = gate.id();
    let node_d = DefaultNode::with_action(
        "D".to_string(),
        ObserveActiveSendersAction {
            expected_sender: id_a,
            observed_senders: observed_senders.clone(),
        },
        &mut table,
    );
    let id_d = node_d.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_node(gate).unwrap();
    graph.add_node(node_d).unwrap();
    graph.add_edge(id_a, vec![id_gate]).unwrap();
    graph.add_edge(id_b, vec![id_gate]).unwrap();
    graph.add_edge(id_gate, vec![id_d]).unwrap();
    graph.add_edge(id_a, vec![id_d]).unwrap();
    graph.add_edge(id_b, vec![id_d]).unwrap();

    let store = MemoryCheckpointStore::new();
    let mut checkpoint = Checkpoint::with_id("resume_skip_merge", 1, 0);
    checkpoint.active_nodes.insert(id_a.as_usize());
    checkpoint.active_nodes.insert(id_gate.as_usize());
    checkpoint.active_nodes.insert(id_d.as_usize());
    checkpoint.add_node_state(
        NodeState::succeeded(id_a.as_usize())
            .with_output_kind(StoredOutputKind::String)
            .with_output_data(serde_json::to_vec(&"from-a").unwrap())
            .with_summary("String(from-a)"),
    );
    checkpoint.add_node_state(NodeState::skipped(id_b.as_usize()));
    checkpoint.add_node_state(NodeState::succeeded(id_gate.as_usize()));
    checkpoint.add_node_state(NodeState::pending(id_d.as_usize()));
    store.save(&checkpoint).await.unwrap();
    graph.set_checkpoint_store(Box::new(store));

    let report = graph
        .resume_from_checkpoint("resume_skip_merge")
        .await
        .unwrap();
    assert_eq!(
        observed_senders.lock().unwrap().as_slice(),
        [vec![id_a.as_usize(), id_gate.as_usize()]],
        "only the active upstream sender should remain visible after resume",
    );
    assert_eq!(report.node_skipped, 1);
}

#[tokio::test]
async fn test_resume_from_failed_checkpoint_keeps_successful_upstreams_frozen() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let producer_runs = Arc::new(Mutex::new(1usize));
    let seen = Arc::new(Mutex::new(Vec::new()));

    let producer = DefaultNode::with_action(
        "Producer".to_string(),
        ProduceCheckpointedValue {
            runs: producer_runs.clone(),
        },
        &mut table,
    );
    let id_producer = producer.id();
    let gate = ConditionalNode::with_condition("Gate".to_string(), AlwaysTrueCondition, &mut table);
    let id_gate = gate.id();
    let consumer = DefaultNode::with_action(
        "Consumer".to_string(),
        CaptureCheckpointedValue {
            source_id: id_producer,
            seen: seen.clone(),
        },
        &mut table,
    );
    let id_consumer = consumer.id();

    graph.add_node(producer).unwrap();
    graph.add_node(gate).unwrap();
    graph.add_node(consumer).unwrap();
    graph
        .add_edge(id_producer, vec![id_gate, id_consumer])
        .unwrap();
    graph.add_edge(id_gate, vec![id_consumer]).unwrap();

    let store = MemoryCheckpointStore::new();
    let mut checkpoint = Checkpoint::with_id("failed_resume", 1, 0);
    checkpoint.active_nodes.extend([
        id_producer.as_usize(),
        id_gate.as_usize(),
        id_consumer.as_usize(),
    ]);
    checkpoint.add_node_state(
        NodeState::succeeded(id_producer.as_usize())
            .with_output_kind(StoredOutputKind::String)
            .with_output_data(serde_json::to_vec(&"checkpointed".to_string()).unwrap())
            .with_summary("String(checkpointed)"),
    );
    checkpoint.add_node_state(NodeState::succeeded(id_gate.as_usize()));
    checkpoint.add_node_state(
        NodeState::failed(id_consumer.as_usize())
            .with_summary("Error: consumer failed before checkpoint"),
    );
    store.save(&checkpoint).await.unwrap();
    graph.set_checkpoint_store(Box::new(store));

    let report = graph.resume_from_checkpoint("failed_resume").await.unwrap();

    assert_eq!(
        *producer_runs.lock().unwrap(),
        1,
        "successful upstream nodes should not rerun when resuming a failed downstream block",
    );
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["checkpointed".to_string()],
    );
    assert_eq!(report.node_succeeded, 3);
    assert_eq!(report.node_failed, 0);
}

#[tokio::test]
async fn test_loop_checkpoint_idempotency() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let counter = Arc::new(Mutex::new(1usize));

    let node_a = DefaultNode::with_action(
        "A".to_string(),
        CounterAction {
            count: counter.clone(),
        },
        &mut table,
    );
    let id_a = node_a.id();
    let loop_node = dagrs::LoopNode::new(
        "Loop".to_string(),
        id_a,
        CountLoopCondition::new(2),
        &mut table,
    );
    let id_loop = loop_node.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(loop_node).unwrap();
    graph.add_edge(id_a, vec![id_loop]).unwrap();

    let store = MemoryCheckpointStore::new();
    let mut checkpoint = Checkpoint::with_id("loop_idempotent", 0, 1);
    checkpoint.active_nodes.insert(id_a.as_usize());
    checkpoint.active_nodes.insert(id_loop.as_usize());
    checkpoint.add_node_state(NodeState::succeeded(id_a.as_usize()));
    checkpoint.add_node_state(NodeState::succeeded(id_loop.as_usize()));
    store.save(&checkpoint).await.unwrap();
    graph.set_checkpoint_store(Box::new(store));

    let report = graph
        .resume_from_checkpoint("loop_idempotent")
        .await
        .unwrap();

    assert_eq!(
        *counter.lock().unwrap(),
        3,
        "resuming from loop checkpoint should only execute the remaining iterations",
    );
    assert_eq!(report.node_succeeded, 2);
}
#[tokio::test]
async fn test_file_checkpoint_store_basic() {
    use dagrs::FileCheckpointStore;
    use std::env::temp_dir;

    // Create a temporary directory for test
    let test_dir = temp_dir().join("dagrs_checkpoint_test");
    let store = FileCheckpointStore::new(&test_dir);

    // Clean up any existing checkpoints
    let _ = store.clear().await;

    // Create and save checkpoint
    let mut checkpoint = Checkpoint::with_id("test_file_cp", 5, 2);
    checkpoint.add_metadata("test_key", "test_value");
    checkpoint.active_nodes.insert(1);

    store.save(&checkpoint).await.unwrap();

    // Load checkpoint
    let loaded = store.load(&"test_file_cp".to_string()).await.unwrap();
    assert_eq!(loaded.pc, 5);
    assert_eq!(loaded.loop_count, 2);
    assert!(loaded.active_nodes.contains(&1));

    // List checkpoints
    let ids = store.list().await.unwrap();
    assert!(ids.contains(&"test_file_cp".to_string()));

    // Get latest
    let latest = store.latest().await.unwrap();
    assert!(latest.is_some());

    // Clean up
    store.clear().await.unwrap();
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[tokio::test]
async fn test_file_checkpoint_store_path_traversal_prevention() {
    use dagrs::FileCheckpointStore;
    use std::env::temp_dir;

    let test_dir = temp_dir().join("dagrs_checkpoint_security_test");
    let store = FileCheckpointStore::new(&test_dir);

    // Attempt path traversal attack with ..
    let malicious_checkpoint = Checkpoint::with_id("../../../etc/passwd", 0, 0);
    let result = store.save(&malicious_checkpoint).await;
    assert!(
        result.is_err(),
        "Should reject path traversal attempts with .."
    );

    // Attempt with forward slash
    let malicious_checkpoint2 = Checkpoint::with_id("foo/bar", 0, 0);
    let result2 = store.save(&malicious_checkpoint2).await;
    assert!(result2.is_err(), "Should reject checkpoint IDs with /");

    // Attempt to load with malicious ID
    let result3 = store.load(&"../secret".to_string()).await;
    assert!(result3.is_err(), "Should reject path traversal in load");

    // Clean up
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[tokio::test]
async fn test_checkpoint_id_generation() {
    // Test that auto-generated IDs are safe
    let cp1 = Checkpoint::new(0, 0);
    let cp2 = Checkpoint::new(0, 1);
    let cp3 = Checkpoint::new(10, 5);

    // IDs should start with "ckpt_" and not contain path separators
    assert!(cp1.id.starts_with("ckpt_"));
    assert!(cp2.id.starts_with("ckpt_"));
    assert!(cp3.id.starts_with("ckpt_"));
    assert!(!cp1.id.contains('/'));
    assert!(!cp1.id.contains('\\'));
    assert!(!cp1.id.contains(".."));

    assert_ne!(cp1.id, cp2.id);
    assert_ne!(cp2.id, cp3.id);

    let store = MemoryCheckpointStore::new();
    store.save(&cp1).await.unwrap();
    store.save(&cp2).await.unwrap();
    store.save(&cp3).await.unwrap();

    let ids = store.list().await.unwrap();
    assert_eq!(
        ids.len(),
        3,
        "generated checkpoint IDs must not overwrite each other"
    );

    let latest = store.latest().await.unwrap().unwrap();
    assert_eq!(latest.id, cp3.id);
}

#[tokio::test]
async fn test_node_state_builder_api() {
    use dagrs::NodeState;

    // Test completed with success
    let state = NodeState::completed(1, true)
        .with_summary("String(hello)")
        .with_output_kind(StoredOutputKind::String)
        .with_output_data(b"\"hello\"".to_vec());

    assert_eq!(state.node_id, 1);
    assert_eq!(state.status, NodeExecStatus::Succeeded);
    assert_eq!(state.output_kind, Some(StoredOutputKind::String));
    assert_eq!(state.output_summary, Some("String(hello)".to_string()));
    assert_eq!(state.output_data, Some(b"\"hello\"".to_vec()));

    // Test completed with failure
    let state = NodeState::completed(2, false).with_summary("Error: connection timeout");

    assert_eq!(state.node_id, 2);
    assert_eq!(state.status, NodeExecStatus::Failed);
    assert_eq!(
        state.output_summary,
        Some("Error: connection timeout".to_string())
    );

    // Test pending
    let state = NodeState::pending(3);
    assert_eq!(state.node_id, 3);
    assert_eq!(state.status, NodeExecStatus::Pending);

    let state = NodeState::running(4);
    assert_eq!(state.status, NodeExecStatus::Running);

    let state = NodeState::skipped(5);
    assert_eq!(state.status, NodeExecStatus::Skipped);
}

#[tokio::test]
async fn test_checkpoint_output_data_serialization() {
    use dagrs::NodeState;

    let mut checkpoint = Checkpoint::new(0, 0);

    // Add node with serialized output
    let output_json = serde_json::to_vec(&"test_output").unwrap();
    let state = NodeState::completed(1, true)
        .with_output_kind(StoredOutputKind::String)
        .with_output_data(output_json.clone())
        .with_summary("String(test_output)");
    checkpoint.add_node_state(state);

    // Serialize and deserialize checkpoint
    let json = serde_json::to_string(&checkpoint).unwrap();
    let restored: Checkpoint = serde_json::from_str(&json).unwrap();

    // Verify output data was preserved
    let restored_state = restored.node_states.get(&1).unwrap();
    assert_eq!(restored_state.output_kind, Some(StoredOutputKind::String));
    assert_eq!(restored_state.output_data, Some(output_json));
    assert_eq!(
        restored_state.output_summary,
        Some("String(test_output)".to_string())
    );

    // Deserialize the output data
    let output: String =
        serde_json::from_slice(restored_state.output_data.as_ref().unwrap()).unwrap();
    assert_eq!(output, "test_output");
}
