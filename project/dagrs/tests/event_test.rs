//! Tests for state subscription and event broadcasting.

use async_trait::async_trait;
use dagrs::graph::event::{GraphEvent, SkipReason, TerminationStatus};
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::loop_node::{CountLoopCondition, LoopNode};
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{
    DagrsError, EnvVar, ErrorCode, Graph, InChannels, Node, NodeTable, OutChannels, Output,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

struct NoOpAction;

#[async_trait]
impl Action for NoOpAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::empty()
    }
}

struct FailingAction;

#[async_trait]
impl Action for FailingAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::execution_failed("Intentional failure")
    }
}

struct FirstBranchRouter {
    target_id: usize,
}

#[async_trait]
impl Router for FirstBranchRouter {
    async fn route(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        vec![self.target_id]
    }
}

async fn collect_events_until_terminated(
    mut receiver: broadcast::Receiver<GraphEvent>,
) -> Vec<GraphEvent> {
    let mut events = Vec::new();
    while let Ok(Ok(event)) =
        tokio::time::timeout(Duration::from_millis(250), receiver.recv()).await
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
async fn test_event_node_start_and_success() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let receiver = graph.subscribe();

    let node = DefaultNode::with_action("TestNode".to_string(), NoOpAction, &mut table);
    let node_id = node.id();
    graph.add_node(node).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    graph.async_start().await.expect("graph should succeed");
    let events = collector.await.unwrap();

    assert!(
        events
            .iter()
            .any(|event| matches!(event, GraphEvent::NodeStart { id, .. } if *id == node_id))
    );
    assert!(events.iter().any(|event| {
        matches!(event, GraphEvent::NodeSuccess { id, duration_ms } if *id == node_id && *duration_ms <= 1_000)
    }));
    let progress_events = events
        .iter()
        .filter(|event| matches!(event, GraphEvent::Progress { total, .. } if *total == 1))
        .count();
    assert_eq!(
        progress_events, 1,
        "single-block execution should emit exactly one progress event"
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
async fn test_event_node_failed() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let receiver = graph.subscribe();

    let node = DefaultNode::with_action("FailingNode".to_string(), FailingAction, &mut table);
    let node_id = node.id();
    graph.add_node(node).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    let err = graph.async_start().await.expect_err("graph should fail");
    let events = collector.await.unwrap();

    assert_eq!(err.code, ErrorCode::DgRun0006NodeExecutionFailed);
    assert!(events.iter().any(|event| {
        matches!(event, GraphEvent::NodeFailed { id, error }
            if *id == node_id
                && error.code == ErrorCode::DgRun0006NodeExecutionFailed
                && error.message.contains("Intentional failure"))
    }));
    assert!(matches!(
        events.last(),
        Some(GraphEvent::ExecutionTerminated {
            status: TerminationStatus::Failed,
            error: Some(error),
        }) if error.code == ErrorCode::DgRun0006NodeExecutionFailed
    ));
}

#[tokio::test]
async fn test_event_node_skipped() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let receiver = graph.subscribe();

    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();
    let node_b = DefaultNode::with_action("NodeB".to_string(), NoOpAction, &mut table);
    let id_b = node_b.id();
    let router = RouterNode::new(
        "Router".to_string(),
        FirstBranchRouter {
            target_id: id_a.as_usize(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router).unwrap();
    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_router, vec![id_a, id_b]).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    graph.async_start().await.expect("graph should succeed");
    let events = collector.await.unwrap();

    assert!(events.iter().any(|event| {
        matches!(event, GraphEvent::NodeSkipped { id, reason }
            if *id == id_b && *reason == SkipReason::PrunedByControlFlow)
    }));
}

#[tokio::test]
async fn test_event_loop_execution() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let receiver = graph.subscribe();

    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();
    let loop_node = LoopNode::new(
        "Loop".to_string(),
        id_a,
        CountLoopCondition::new(2),
        &mut table,
    );
    let id_loop = loop_node.id();

    graph.add_node(node_a).unwrap();
    graph.add_node(loop_node).unwrap();
    graph.add_edge(id_a, vec![id_loop]).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    graph.async_start().await.expect("graph should succeed");
    let events = collector.await.unwrap();

    let node_a_starts = events
        .iter()
        .filter(|event| matches!(event, GraphEvent::NodeStart { id, .. } if *id == id_a))
        .count();
    assert_eq!(node_a_starts, 3);
    assert!(matches!(
        events.last(),
        Some(GraphEvent::ExecutionTerminated {
            status: TerminationStatus::Succeeded,
            error: None,
        })
    ));
}

#[tokio::test]
async fn test_event_branch_selected() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let receiver = graph.subscribe();

    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();
    let node_b = DefaultNode::with_action("NodeB".to_string(), NoOpAction, &mut table);
    let id_b = node_b.id();
    let router = RouterNode::new(
        "Router".to_string(),
        FirstBranchRouter {
            target_id: id_a.as_usize(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router).unwrap();
    graph.add_node(node_a).unwrap();
    graph.add_node(node_b).unwrap();
    graph.add_edge(id_router, vec![id_a, id_b]).unwrap();

    let collector = tokio::spawn(collect_events_until_terminated(receiver));
    graph.async_start().await.expect("graph should succeed");
    let events = collector.await.unwrap();

    assert!(events.iter().any(|event| {
        matches!(event, GraphEvent::BranchSelected { node_id, selected_branches }
            if *node_id == id_router && selected_branches == &vec![id_a.as_usize()])
    }));
}

#[tokio::test]
async fn test_graph_event_clone_and_debug() {
    let mut table = NodeTable::new();
    let node = DefaultNode::new("TestNode".to_string(), &mut table);
    let node_id = node.id();
    let error = DagrsError::new(ErrorCode::DgRun0006NodeExecutionFailed, "test error")
        .with_node(node_id.as_usize(), "TestNode");

    let events = vec![
        GraphEvent::NodeStart {
            id: node_id,
            timestamp: 12345,
        },
        GraphEvent::NodeSuccess {
            id: node_id,
            duration_ms: 42,
        },
        GraphEvent::NodeFailed {
            id: node_id,
            error: error.clone(),
        },
        GraphEvent::NodeSkipped {
            id: node_id,
            reason: SkipReason::PrunedByControlFlow,
        },
        GraphEvent::NodeRetry {
            id: node_id,
            attempt: 1,
            max_retries: 3,
            error: error.clone(),
        },
        GraphEvent::LoopIteration {
            iteration: 0,
            block_index: 1,
        },
        GraphEvent::BranchSelected {
            node_id,
            selected_branches: vec![1, 2],
        },
        GraphEvent::Progress {
            completed: 5,
            total: 10,
        },
        GraphEvent::CheckpointSaved {
            checkpoint_id: "ckpt_1".to_string(),
            pc: 1,
            completed_nodes: 2,
        },
        GraphEvent::CheckpointRestored {
            checkpoint_id: "ckpt_1".to_string(),
            pc: 1,
        },
        GraphEvent::ExecutionTerminated {
            status: TerminationStatus::Failed,
            error: Some(error),
        },
    ];

    for event in &events {
        let cloned = event.clone();
        let debug_str = format!("{cloned:?}");
        assert!(!debug_str.is_empty());
    }
}
