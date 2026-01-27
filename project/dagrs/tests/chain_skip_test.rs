use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};

// Action that expects data, but if input closed, returns empty (and doesn't send anything)
#[derive(Clone)]
struct PassthroughAction {
    name: String,
}

#[async_trait]
impl Action for PassthroughAction {
    /// Process input data and pass it through to output.
    async fn run(&self, input: &mut InChannels, out: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        println!("[{}] Running", self.name);
        if !input.get_sender_ids().is_empty() {
            match input.recv_any().await {
                Ok((_, val)) => {
                    println!("[{}] Received {:?}", self.name, val);
                    // Send to downstream
                    out.broadcast(val).await;
                }
                Err(_) => {
                    println!("[{}] Input closed", self.name);
                    return Output::empty();
                }
            }
        }
        Output::empty()
    }
}

/// Router that can switch target node dynamically
struct SwitchRouter {
    target: Arc<Mutex<dagrs::NodeId>>,
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
fn test_chain_skip_deadlock() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    // A -> Router -> B -> C -> D

    let action_b = PassthroughAction {
        name: "B".to_string(),
    };
    let node_b = DefaultNode::with_action("B".to_string(), action_b, &mut table);
    let id_b = node_b.id();

    let action_c = PassthroughAction {
        name: "C".to_string(),
    };
    let node_c = DefaultNode::with_action("C".to_string(), action_c, &mut table);
    let id_c = node_c.id();

    let action_d = PassthroughAction {
        name: "D".to_string(),
    };
    let node_d = DefaultNode::with_action("D".to_string(), action_d, &mut table);
    let id_d = node_d.id();

    let dummy_node = DefaultNode::new("Dummy".to_string(), &mut table);
    let dummy_id = dummy_node.id();

    let target = Arc::new(Mutex::new(dummy_id));
    let router = RouterNode::new(
        "Router".to_string(),
        SwitchRouter {
            target: target.clone(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router);
    graph.add_node(node_b);
    graph.add_node(node_c);
    graph.add_node(node_d);

    graph.add_edge(id_router, vec![id_b]);
    graph.add_edge(id_b, vec![id_c]);
    graph.add_edge(id_c, vec![id_d]);

    let rt = tokio::runtime::Runtime::new().unwrap();

    println!("Starting Graph...");
    rt.block_on(async {
        // Set timeout to detect deadlock
        let result =
            tokio::time::timeout(std::time::Duration::from_secs(1), graph.async_start()).await;

        match result {
            Ok(Ok(_)) => println!("Graph finished successfully"),
            Ok(Err(e)) => panic!("Graph failed: {:?}", e),
            Err(_) => panic!("Graph timed out! Deadlock detected."),
        }
    });
}
