//! Tests for execution hook functionality.

use async_trait::async_trait;
use dagrs::graph::event::{GraphEvent, TerminationStatus};
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::utils::hook::{ExecutionHook, RetryDecision};
use dagrs::{EnvVar, ErrorCode, Graph, InChannels, Node, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;

struct ComprehensiveHook {
    before_runs: Arc<Mutex<Vec<String>>>,
    after_runs: Arc<Mutex<Vec<String>>>,
    skips: Arc<Mutex<Vec<String>>>,
    retries: Arc<Mutex<Vec<(String, u32)>>>,
    retry_decision: RetryDecision,
}

#[async_trait]
impl ExecutionHook for ComprehensiveHook {
    async fn before_node_run(&self, node: &dyn Node, _env: &Arc<EnvVar>) {
        self.before_runs
            .lock()
            .unwrap()
            .push(node.name().to_string());
    }

    async fn after_node_run(&self, node: &dyn Node, _output: &Output, _env: &Arc<EnvVar>) {
        self.after_runs
            .lock()
            .unwrap()
            .push(node.name().to_string());
    }

    async fn on_skip(&self, node: &dyn Node, _env: &Arc<EnvVar>) {
        self.skips.lock().unwrap().push(node.name().to_string());
    }

    async fn on_retry(
        &self,
        node: &dyn Node,
        _error: &dagrs::DagrsError,
        attempt: u32,
        _max_retries: u32,
        _env: &Arc<EnvVar>,
    ) -> RetryDecision {
        self.retries
            .lock()
            .unwrap()
            .push((node.name().to_string(), attempt));
        self.retry_decision.clone()
    }
}

struct NoOpAction;

#[async_trait]
impl Action for NoOpAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::empty()
    }
}

struct FailingAction {
    message: String,
}

#[async_trait]
impl Action for FailingAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::execution_failed(self.message.clone())
    }
}

struct SelectARouter {
    target_id: usize,
}

#[async_trait]
impl Router for SelectARouter {
    async fn route(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        vec![self.target_id]
    }
}

async fn collect_events_until_terminated(
    mut receiver: broadcast::Receiver<GraphEvent>,
) -> Vec<GraphEvent> {
    let mut events = Vec::new();
    while let Ok(Ok(event)) =
        tokio::time::timeout(Duration::from_millis(500), receiver.recv()).await
    {
        let terminated = matches!(event, GraphEvent::ExecutionTerminated { .. });
        events.push(event);
        if terminated {
            break;
        }
    }
    events
}

#[tokio::test]
async fn test_hook_before_and_after() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let before_runs = Arc::new(Mutex::new(Vec::new()));
    let after_runs = Arc::new(Mutex::new(Vec::new()));

    graph
        .add_hook(Box::new(ComprehensiveHook {
            before_runs: before_runs.clone(),
            after_runs: after_runs.clone(),
            skips: Arc::new(Mutex::new(Vec::new())),
            retries: Arc::new(Mutex::new(Vec::new())),
            retry_decision: RetryDecision::Retry,
        }))
        .await;

    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let node_b = DefaultNode::with_action("NodeB".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();
    let id_b = node_b.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_a, vec![id_b]).unwrap();

    graph.async_start().await.expect("graph should succeed");

    let before = before_runs.lock().unwrap();
    let after = after_runs.lock().unwrap();
    assert_eq!(before.len(), 2);
    assert_eq!(after.len(), 2);
    assert!(before.contains(&"NodeA".to_string()));
    assert!(before.contains(&"NodeB".to_string()));
    assert!(after.contains(&"NodeA".to_string()));
    assert!(after.contains(&"NodeB".to_string()));
}

#[tokio::test]
async fn test_hook_failure_does_not_call_after_hook() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let before_runs = Arc::new(Mutex::new(Vec::new()));
    let after_runs = Arc::new(Mutex::new(Vec::new()));

    graph
        .add_hook(Box::new(ComprehensiveHook {
            before_runs: before_runs.clone(),
            after_runs: after_runs.clone(),
            skips: Arc::new(Mutex::new(Vec::new())),
            retries: Arc::new(Mutex::new(Vec::new())),
            retry_decision: RetryDecision::Retry,
        }))
        .await;

    let failing_node = DefaultNode::with_action(
        "FailingNode".to_string(),
        FailingAction {
            message: "Test error message".to_string(),
        },
        &mut table,
    );

    graph.add_node(failing_node).unwrap();

    let err = graph.async_start().await.expect_err("graph should fail");
    assert_eq!(err.code, ErrorCode::DgRun0006NodeExecutionFailed);
    assert_eq!(before_runs.lock().unwrap().as_slice(), ["FailingNode"]);
    assert!(after_runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn test_hook_on_skip() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let skips = Arc::new(Mutex::new(Vec::new()));

    graph
        .add_hook(Box::new(ComprehensiveHook {
            before_runs: Arc::new(Mutex::new(Vec::new())),
            after_runs: Arc::new(Mutex::new(Vec::new())),
            skips: skips.clone(),
            retries: Arc::new(Mutex::new(Vec::new())),
            retry_decision: RetryDecision::Retry,
        }))
        .await;

    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();
    let node_b = DefaultNode::with_action("NodeB".to_string(), NoOpAction, &mut table);
    let id_b = node_b.id();
    let router = RouterNode::new(
        "Router".to_string(),
        SelectARouter {
            target_id: id_a.as_usize(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router).unwrap();
    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_router, vec![id_a, id_b]).unwrap();

    graph.async_start().await.expect("graph should succeed");

    let skips_list = skips.lock().unwrap();
    assert!(skips_list.contains(&"NodeB".to_string()));
}

#[tokio::test]
async fn test_retry_decision_enum() {
    assert_eq!(RetryDecision::Retry, RetryDecision::Retry);
    assert_eq!(RetryDecision::Fail, RetryDecision::Fail);
    assert_ne!(RetryDecision::Retry, RetryDecision::Fail);
    assert_eq!(format!("{:?}", RetryDecision::Retry), "Retry");
    assert_eq!(format!("{:?}", RetryDecision::Fail), "Fail");

    let decision = RetryDecision::Retry;
    let cloned = decision.clone();
    assert_eq!(decision, cloned);
}

struct RetryableNode {
    id: dagrs::node::NodeId,
    name: dagrs::node::NodeName,
    in_channels: InChannels,
    out_channels: OutChannels,
    fail_count: Arc<Mutex<usize>>,
    failures_before_success: usize,
    max_retries_config: u32,
}

impl RetryableNode {
    fn new(
        name: String,
        failures_before_success: usize,
        max_retries: u32,
        fail_count: Arc<Mutex<usize>>,
        table: &mut NodeTable,
    ) -> Self {
        Self {
            id: table.alloc_id_for(&name),
            name,
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
            fail_count,
            failures_before_success,
            max_retries_config: max_retries,
        }
    }
}

#[async_trait]
impl Node for RetryableNode {
    fn id(&self) -> dagrs::node::NodeId {
        self.id
    }

    fn name(&self) -> dagrs::node::NodeName {
        self.name.clone()
    }

    fn input_channels(&mut self) -> &mut InChannels {
        &mut self.in_channels
    }

    fn output_channels(&mut self) -> &mut OutChannels {
        &mut self.out_channels
    }

    async fn run(&mut self, _env: Arc<EnvVar>) -> Output {
        let mut count = self.fail_count.lock().unwrap();
        *count += 1;
        if *count <= self.failures_before_success {
            Output::execution_failed(format!("Intentional failure #{}", *count))
        } else {
            Output::new(format!("Success after {} failures", *count - 1))
        }
    }

    fn max_retries(&self) -> u32 {
        self.max_retries_config
    }

    fn retry_delay_ms(&self, _attempt: u32) -> u64 {
        10
    }
}

#[tokio::test]
async fn test_automatic_retry_success() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let fail_count = Arc::new(Mutex::new(0));
    let retries = Arc::new(Mutex::new(Vec::new()));
    let receiver = graph.subscribe();

    graph
        .add_hook(Box::new(ComprehensiveHook {
            before_runs: Arc::new(Mutex::new(Vec::new())),
            after_runs: Arc::new(Mutex::new(Vec::new())),
            skips: Arc::new(Mutex::new(Vec::new())),
            retries: retries.clone(),
            retry_decision: RetryDecision::Retry,
        }))
        .await;

    let node = RetryableNode::new(
        "RetryNode".to_string(),
        2,
        3,
        fail_count.clone(),
        &mut table,
    );
    graph.add_node(node).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    graph.async_start().await.expect("graph should succeed");
    let events = collector.await.unwrap();

    assert_eq!(*fail_count.lock().unwrap(), 3);
    assert_eq!(retries.lock().unwrap().len(), 2);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, GraphEvent::NodeRetry { .. }))
            .count(),
        2
    );
    assert!(matches!(
        events.last(),
        Some(GraphEvent::ExecutionTerminated {
            status: TerminationStatus::Succeeded,
            error: None,
        })
    ));
}

#[tokio::test]
async fn test_retry_hook_can_stop_retries() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let fail_count = Arc::new(Mutex::new(0));
    let retries = Arc::new(Mutex::new(Vec::new()));
    let receiver = graph.subscribe();

    graph
        .add_hook(Box::new(ComprehensiveHook {
            before_runs: Arc::new(Mutex::new(Vec::new())),
            after_runs: Arc::new(Mutex::new(Vec::new())),
            skips: Arc::new(Mutex::new(Vec::new())),
            retries: retries.clone(),
            retry_decision: RetryDecision::Fail,
        }))
        .await;

    let node = RetryableNode::new(
        "AlwaysFail".to_string(),
        100,
        3,
        fail_count.clone(),
        &mut table,
    );
    graph.add_node(node).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    let err = graph.async_start().await.expect_err("graph should fail");
    let events = collector.await.unwrap();

    assert_eq!(err.code, ErrorCode::DgRun0006NodeExecutionFailed);
    assert_eq!(*fail_count.lock().unwrap(), 1);
    assert_eq!(retries.lock().unwrap().len(), 1);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, GraphEvent::NodeRetry { .. }))
            .count(),
        1
    );
    assert!(matches!(
        events.last(),
        Some(GraphEvent::ExecutionTerminated {
            status: TerminationStatus::Failed,
            error: Some(error),
        }) if error.code == ErrorCode::DgRun0006NodeExecutionFailed
    ));
}
