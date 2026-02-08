use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};

/// Action that marks execution
#[derive(Clone)]
struct MarkAction {
    executed: Arc<Mutex<bool>>,
}

#[async_trait]
impl Action for MarkAction {
    /// Mark that this action has been executed.
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        *self.executed.lock().unwrap() = true;
        Output::empty()
    }
}

/// Simple router that routes to a fixed target
struct SimpleRouter {
    target: usize,
}

#[async_trait]
impl Router for SimpleRouter {
    /// Route data to the fixed target node
    async fn route(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        vec![self.target]
    }
}

#[test]
fn test_router_merge() {
    // A -> Router -> B
    //             -> C
    // B -> D
    // C -> D
    // We want to execute A -> Router -> B -> D.
    // C is skipped. D should be executed because B executed.

    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let exec_b = Arc::new(Mutex::new(false));
    let exec_c = Arc::new(Mutex::new(false));
    let exec_d = Arc::new(Mutex::new(false));

    let node_b = DefaultNode::with_action(
        "B".to_string(),
        MarkAction {
            executed: exec_b.clone(),
        },
        &mut table,
    );
    let id_b = node_b.id();

    let node_c = DefaultNode::with_action(
        "C".to_string(),
        MarkAction {
            executed: exec_c.clone(),
        },
        &mut table,
    );
    let id_c = node_c.id();

    let node_d = DefaultNode::with_action(
        "D".to_string(),
        MarkAction {
            executed: exec_d.clone(),
        },
        &mut table,
    );
    let id_d = node_d.id();

    let router = RouterNode::new(
        "Router".to_string(),
        SimpleRouter {
            target: id_b.as_usize(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router);
    graph.add_node(node_b);
    graph.add_node(node_c);
    graph.add_node(node_d);

    graph.add_edge(id_router, vec![id_b, id_c]);
    graph.add_edge(id_b, vec![id_d]);
    graph.add_edge(id_c, vec![id_d]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        match graph.async_start().await {
            Ok(_) => {}
            Err(e) => panic!("Graph failed: {:?}", e),
        }
    });

    assert!(*exec_b.lock().unwrap(), "B should run");
    assert!(!*exec_c.lock().unwrap(), "C should not run");
    assert!(*exec_d.lock().unwrap(), "D should run");
}
