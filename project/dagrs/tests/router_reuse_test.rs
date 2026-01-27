use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeId, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};

/// Action that marks execution
#[derive(Clone)]
struct MarkAction {
    executed: Arc<Mutex<bool>>,
}

#[async_trait]
impl Action for MarkAction {
    /// Mark that this action has been executed.
    async fn run(&self, input: &mut InChannels, out: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        if !input.get_sender_ids().is_empty() {
            match input.recv_any().await {
                Ok(_) => {}
                Err(_) => return Output::empty(),
            }
        }
        *self.executed.lock().unwrap() = true;

        // Forward signal to downstream
        let _ = out.broadcast(dagrs::Content::new("ping".to_string())).await;

        Output::empty()
    }
}

/// Router that can switch target node dynamically
struct SwitchRouter {
    target: Arc<Mutex<NodeId>>,
}

#[async_trait]
impl Router for SwitchRouter {
    /// Route data to the selected target node
    async fn route(&self, _: &mut InChannels, out: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        let t = *self.target.lock().unwrap();
        // Send data to the selected target
        let _ = out
            .send_to(&t, dagrs::Content::new("ping".to_string()))
            .await;
        vec![t.as_usize()]
    }
}

#[test]
fn test_router_reuse() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let exec_a = Arc::new(Mutex::new(false));
    let exec_b = Arc::new(Mutex::new(false));

    let node_a = DefaultNode::with_action(
        "A".to_string(),
        MarkAction {
            executed: exec_a.clone(),
        },
        &mut table,
    );
    let id_a = node_a.id();

    let node_b = DefaultNode::with_action(
        "B".to_string(),
        MarkAction {
            executed: exec_b.clone(),
        },
        &mut table,
    );
    let id_b = node_b.id();

    let target = Arc::new(Mutex::new(id_a));
    let router = RouterNode::new(
        "Router".to_string(),
        SwitchRouter {
            target: target.clone(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router);
    graph.add_node(node_a);
    graph.add_node(node_b);

    // Add Node C downstream of B
    let exec_c = Arc::new(Mutex::new(false));
    let node_c = DefaultNode::with_action(
        "C".to_string(),
        MarkAction {
            executed: exec_c.clone(),
        },
        &mut table,
    );
    let id_c = node_c.id();
    graph.add_node(node_c);

    graph.add_edge(id_router, vec![id_a, id_b]);
    graph.add_edge(id_b, vec![id_c]);

    let rt = tokio::runtime::Runtime::new().unwrap();

    // First Run: Target A
    println!("First Run: Expect A to run");
    rt.block_on(async {
        graph.async_start().await.unwrap();
    });
    assert!(*exec_a.lock().unwrap());
    assert!(!*exec_b.lock().unwrap());
    assert!(!*exec_c.lock().unwrap());

    // Reset
    rt.block_on(async {
        graph.reset().await;
    });
    *exec_a.lock().unwrap() = false;
    *exec_b.lock().unwrap() = false;
    *exec_c.lock().unwrap() = false;
    // Switch target to B
    *target.lock().unwrap() = id_b;

    // Second Run: Target B -> C
    println!("Second Run: Expect B and C to run");
    rt.block_on(async {
        match graph.async_start().await {
            Ok(_) => {}
            Err(e) => panic!("Second run failed: {:?}", e),
        }
    });

    assert!(!*exec_a.lock().unwrap());
    assert!(*exec_b.lock().unwrap());
    assert!(*exec_c.lock().unwrap(), "C should run in second execution");
}
