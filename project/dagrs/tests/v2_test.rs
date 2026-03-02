use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::loop_node::{CountLoopCondition, LoopNode};
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};

/// Action that counts executions
#[derive(Clone)]
struct CounterAction {
    count: Arc<Mutex<usize>>,
}

#[async_trait]
impl Action for CounterAction {
    /// Increment the count each time this action is run.
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        let mut c = self.count.lock().unwrap();
        *c += 1;
        Output::new(*c)
    }
}

#[test]
fn test_loop_node() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let count = Arc::new(Mutex::new(0));
    let action = CounterAction {
        count: count.clone(),
    };

    let node_a = DefaultNode::with_action("A".to_string(), action.clone(), &mut table);
    let id_a = node_a.id();

    let loop_node = LoopNode::new(
        "Loop".to_string(),
        id_a,
        CountLoopCondition::new(3),
        &mut table,
    );
    let id_loop = loop_node.id();

    graph.add_node(node_a);
    graph.add_node(loop_node);

    // A -> Loop
    graph.add_edge(id_a, vec![id_loop]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        match graph.async_start().await {
            Ok(_) => {}
            Err(e) => panic!("Graph failed: {:?}", e),
        }
    });

    // A runs. Loop runs (iter 0 -> 1, true). Jumps to A.
    // A runs. Loop runs (iter 1 -> 2, true). Jumps to A.
    // A runs. Loop runs (iter 2 -> 3, true). Jumps to A.
    // A runs. Loop runs (iter 3 -> 4, false). Stop.
    // Total A runs: 4.
    assert_eq!(*count.lock().unwrap(), 4);
}

struct SimpleRouter {
    target: usize,
}

#[async_trait]
impl Router for SimpleRouter {
    async fn route(&self, _: &mut InChannels, _: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        vec![self.target]
    }
}

#[test]
fn test_router_node() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let count_b = Arc::new(Mutex::new(0));
    let count_c = Arc::new(Mutex::new(0));

    let node_b = DefaultNode::with_action(
        "B".to_string(),
        CounterAction {
            count: count_b.clone(),
        },
        &mut table,
    );
    let id_b = node_b.id();

    let node_c = DefaultNode::with_action(
        "C".to_string(),
        CounterAction {
            count: count_c.clone(),
        },
        &mut table,
    );
    let id_c = node_c.id();

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

    // Router -> B, Router -> C
    graph.add_edge(id_router, vec![id_b, id_c]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        match graph.async_start().await {
            Ok(_) => {}
            Err(e) => panic!("Graph failed: {:?}", e),
        }
    });

    assert_eq!(*count_b.lock().unwrap(), 1);
    assert_eq!(*count_c.lock().unwrap(), 0);
}
