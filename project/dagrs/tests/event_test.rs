//! Tests for State Subscription and Event Broadcasting
//!
//! This module tests the event broadcasting system for monitoring graph execution.
//! The `GraphEvent` enum provides real-time status updates during execution.
//!
//! Events tested:
//! - `NodeStart`: Emitted when a node begins execution
//! - `NodeSuccess`: Emitted when a node completes successfully
//! - `NodeFailed`: Emitted when a node fails
//! - `NodeSkipped`: Emitted when a node is skipped (branch pruning)
//! - `NodeRetry`: Emitted when a node retry is attempted
//! - `LoopIteration`: Emitted when a loop iteration begins
//! - `BranchSelected`: Emitted when a router selects branches
//! - `Progress`: Reports execution progress
//! - `GraphFinished`: Final event when execution completes

use async_trait::async_trait;
use dagrs::graph::event::GraphEvent;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::loop_node::{CountLoopCondition, LoopNode};
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Simple action that does nothing.
struct NoOpAction;

#[async_trait]
impl Action for NoOpAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::empty()
    }
}

/// Action that fails with an error.
struct FailingAction;

#[async_trait]
impl Action for FailingAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::Err("Intentional failure".to_string())
    }
}

/// Router that selects only first branch.
struct FirstBranchRouter {
    target_id: usize,
}

#[async_trait]
impl Router for FirstBranchRouter {
    async fn route(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        vec![self.target_id]
    }
}

#[test]
fn test_event_node_start_and_success() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    // Subscribe to events before adding nodes
    let mut receiver = graph.subscribe();

    let node = DefaultNode::with_action("TestNode".to_string(), NoOpAction, &mut table);
    let node_id = node.id();
    graph.add_node(node);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // Spawn a task to collect events
        let collector = tokio::spawn(async move {
            let mut collected = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await {
                    Ok(Ok(event)) => {
                        let is_finished = matches!(event, GraphEvent::GraphFinished);
                        collected.push(event);
                        if is_finished {
                            break;
                        }
                    }
                    Ok(Err(_)) => break, // Channel closed
                    Err(_) => break,     // Timeout
                }
            }
            collected
        });

        // Run the graph
        graph.async_start().await.expect("Graph should succeed");

        // Wait for events to be collected
        tokio::time::sleep(Duration::from_millis(50)).await;

        let collected = collector.await.unwrap();
        *events_clone.lock().unwrap() = collected;
    });

    let events_list = events.lock().unwrap();

    // Check for NodeStart event
    let has_start = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::NodeStart { id, .. } if *id == node_id));
    assert!(has_start, "Should have NodeStart event for the node");

    // Check for NodeSuccess event
    let has_success = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::NodeSuccess { id } if *id == node_id));
    assert!(has_success, "Should have NodeSuccess event for the node");

    // Check for GraphFinished event
    let has_finished = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::GraphFinished));
    assert!(has_finished, "Should have GraphFinished event");
}

#[test]
fn test_event_node_failed() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let mut receiver = graph.subscribe();

    let node = DefaultNode::with_action("FailingNode".to_string(), FailingAction, &mut table);
    let node_id = node.id();
    graph.add_node(node);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let collector = tokio::spawn(async move {
            let mut collected = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await {
                    Ok(Ok(event)) => {
                        collected.push(event.clone());
                        if matches!(event, GraphEvent::GraphFinished) {
                            break;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            collected
        });

        // Graph should fail
        let result = graph.async_start().await;
        assert!(result.is_err(), "Graph should fail");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let collected = collector.await.unwrap();
        *events_clone.lock().unwrap() = collected;
    });

    let events_list = events.lock().unwrap();

    // Check for NodeFailed event
    let has_failed = events_list.iter().any(|e| {
        matches!(e, GraphEvent::NodeFailed { id, error } 
            if *id == node_id && error.contains("Intentional failure"))
    });
    assert!(
        has_failed,
        "Should have NodeFailed event with error message. Events: {:?}",
        *events_list
    );
}

#[test]
fn test_event_node_skipped() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let mut receiver = graph.subscribe();

    // Create nodes
    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();

    let node_b = DefaultNode::with_action("NodeB".to_string(), NoOpAction, &mut table);
    let id_b = node_b.id();

    // Router that only selects NodeA
    let router = RouterNode::new(
        "Router".to_string(),
        FirstBranchRouter {
            target_id: id_a.as_usize(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router);
    graph.add_node(node_a);
    graph.add_node(node_b);

    graph.add_edge(id_router, vec![id_a, id_b]);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let collector = tokio::spawn(async move {
            let mut collected = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await {
                    Ok(Ok(event)) => {
                        let is_finished = matches!(event, GraphEvent::GraphFinished);
                        collected.push(event);
                        if is_finished {
                            break;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            collected
        });

        graph.async_start().await.expect("Graph should succeed");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let collected = collector.await.unwrap();
        *events_clone.lock().unwrap() = collected;
    });

    let events_list = events.lock().unwrap();

    // Check for NodeSkipped event for NodeB
    let has_skipped = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::NodeSkipped { id } if *id == id_b));
    assert!(
        has_skipped,
        "Should have NodeSkipped event for NodeB. Events: {:?}",
        *events_list
    );
}

#[test]
fn test_event_loop_execution() {
    // This test verifies that loops execute correctly and events are properly broadcast.
    // The loop topology: A -> LoopNode (loops 2 times)
    // Expected behavior: A executes 3 times (initial + 2 iterations)
    //
    // Note: LoopIteration events are only emitted when pc jumps backward across blocks.
    // In simple topologies where all nodes are in the same block, the loop still works
    // but LoopIteration events may not be emitted.

    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let mut receiver = graph.subscribe();

    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();

    let loop_node = LoopNode::new(
        "Loop".to_string(),
        id_a,
        CountLoopCondition::new(2),
        &mut table,
    );
    let _id_loop = loop_node.id();

    graph.add_node(node_a);
    graph.add_node(loop_node);

    graph.add_edge(id_a, vec![_id_loop]);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let collector = tokio::spawn(async move {
            let mut collected = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_millis(200), receiver.recv()).await {
                    Ok(Ok(event)) => {
                        let is_finished = matches!(event, GraphEvent::GraphFinished);
                        collected.push(event);
                        if is_finished {
                            break;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            collected
        });

        graph.async_start().await.expect("Graph should succeed");

        tokio::time::sleep(Duration::from_millis(100)).await;

        let collected = collector.await.unwrap();
        *events_clone.lock().unwrap() = collected;
    });

    let events_list = events.lock().unwrap();

    // Verify that node A was executed multiple times (proving the loop works)
    let node_a_starts: Vec<_> = events_list
        .iter()
        .filter(|e| matches!(e, GraphEvent::NodeStart { id, .. } if *id == id_a))
        .collect();

    // Loop with 2 iterations means A runs 3 times: initial + 2 iterations
    assert_eq!(
        node_a_starts.len(),
        3,
        "NodeA should execute 3 times (initial + 2 loop iterations). Got {} executions. Events: {:?}",
        node_a_starts.len(),
        *events_list
    );

    // Verify GraphFinished event is present
    let has_finished = events_list
        .iter()
        .any(|e| matches!(e, GraphEvent::GraphFinished));
    assert!(has_finished, "Should have GraphFinished event");
}

#[test]
fn test_event_branch_selected() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let mut receiver = graph.subscribe();

    // Create nodes
    let node_a = DefaultNode::with_action("NodeA".to_string(), NoOpAction, &mut table);
    let id_a = node_a.id();

    let node_b = DefaultNode::with_action("NodeB".to_string(), NoOpAction, &mut table);
    let id_b = node_b.id();

    // Router that selects NodeA
    let router = RouterNode::new(
        "Router".to_string(),
        FirstBranchRouter {
            target_id: id_a.as_usize(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router);
    graph.add_node(node_a);
    graph.add_node(node_b);

    graph.add_edge(id_router, vec![id_a, id_b]);

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let collector = tokio::spawn(async move {
            let mut collected = Vec::new();
            loop {
                match tokio::time::timeout(Duration::from_millis(100), receiver.recv()).await {
                    Ok(Ok(event)) => {
                        let is_finished = matches!(event, GraphEvent::GraphFinished);
                        collected.push(event);
                        if is_finished {
                            break;
                        }
                    }
                    Ok(Err(_)) => break,
                    Err(_) => break,
                }
            }
            collected
        });

        graph.async_start().await.expect("Graph should succeed");

        tokio::time::sleep(Duration::from_millis(50)).await;

        let collected = collector.await.unwrap();
        *events_clone.lock().unwrap() = collected;
    });

    let events_list = events.lock().unwrap();

    // Check for BranchSelected event
    let has_branch_selected = events_list.iter().any(|e| {
        matches!(e, GraphEvent::BranchSelected { node_id, selected_branches } 
            if *node_id == id_router && selected_branches.contains(&id_a.as_usize()))
    });
    assert!(
        has_branch_selected,
        "Should have BranchSelected event from Router. Events: {:?}",
        *events_list
    );
}

#[test]
fn test_graph_event_clone_and_debug() {
    // Test that all GraphEvent variants implement Clone and Debug correctly
    // We create a simple graph to get a valid NodeId
    let mut table = NodeTable::new();
    let node = DefaultNode::new("TestNode".to_string(), &mut table);
    let node_id = node.id();

    let events = vec![
        GraphEvent::NodeStart {
            id: node_id,
            timestamp: 12345,
        },
        GraphEvent::NodeSuccess { id: node_id },
        GraphEvent::NodeFailed {
            id: node_id,
            error: "test error".to_string(),
        },
        GraphEvent::NodeSkipped { id: node_id },
        GraphEvent::NodeRetry {
            id: node_id,
            attempt: 1,
            max_retries: 3,
            error: "retry error".to_string(),
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
        GraphEvent::GraphFinished,
    ];

    for event in &events {
        // Test Clone
        let cloned = event.clone();
        // Test Debug
        let debug_str = format!("{:?}", cloned);
        assert!(!debug_str.is_empty(), "Debug output should not be empty");
    }
}
