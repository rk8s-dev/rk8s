use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::router_node::{Router, RouterNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeId, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct MarkAction {
    name: String,
    executed: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl Action for MarkAction {
    async fn run(&self, _: &mut InChannels, out: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        self.executed.lock().unwrap().push(self.name.clone());
        out.broadcast(dagrs::Content::new(self.name.clone())).await;
        Output::empty()
    }
}

struct StaticRouter {
    target: Arc<Mutex<NodeId>>,
}

#[async_trait]
impl Router for StaticRouter {
    async fn route(&self, _: &mut InChannels, out: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
        let t = *self.target.lock().unwrap();
        let _ = out
            .send_to(&t, dagrs::Content::new("ping".to_string()))
            .await;
        vec![t.as_usize()]
    }
}

#[test]
fn test_branch_pruning() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let executed = Arc::new(Mutex::new(Vec::new()));

    // Topology:
    // Router -> A -> B
    //        -> C -> D
    //
    // If Router chooses A, then C and D should NOT run.

    let action_a = MarkAction {
        name: "A".to_string(),
        executed: executed.clone(),
    };
    let node_a = DefaultNode::with_action("A".to_string(), action_a, &mut table);
    let id_a = node_a.id();

    let action_b = MarkAction {
        name: "B".to_string(),
        executed: executed.clone(),
    };
    let node_b = DefaultNode::with_action("B".to_string(), action_b, &mut table);
    let id_b = node_b.id();

    let action_c = MarkAction {
        name: "C".to_string(),
        executed: executed.clone(),
    };
    let node_c = DefaultNode::with_action("C".to_string(), action_c, &mut table);
    let id_c = node_c.id();

    let action_d = MarkAction {
        name: "D".to_string(),
        executed: executed.clone(),
    };
    let node_d = DefaultNode::with_action("D".to_string(), action_d, &mut table);
    let id_d = node_d.id();

    let target = Arc::new(Mutex::new(id_a));
    let router = RouterNode::new(
        "Router".to_string(),
        StaticRouter {
            target: target.clone(),
        },
        &mut table,
    );
    let id_router = router.id();

    graph.add_node(router);
    graph.add_node(node_a);
    graph.add_node(node_b);
    graph.add_node(node_c);
    graph.add_node(node_d);

    // Edges
    graph.add_edge(id_router, vec![id_a, id_c]); // Router connects to A and C
    graph.add_edge(id_a, vec![id_b]); // A -> B
    graph.add_edge(id_c, vec![id_d]); // C -> D

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        graph.async_start().await.unwrap();
    });

    let exec_log = executed.lock().unwrap();
    println!("Executed nodes: {:?}", *exec_log);

    // Expect: A, B.
    // Should NOT contain: C, D.
    assert!(exec_log.contains(&"A".to_string()));
    assert!(exec_log.contains(&"B".to_string()));
    assert!(
        !exec_log.contains(&"C".to_string()),
        "Node C should be pruned"
    );
    assert!(
        !exec_log.contains(&"D".to_string()),
        "Node D should be pruned (descendant of C)"
    );
}

/// Test case for diamond pattern where pruned node's descendant has another active parent.
///
/// Topology:
/// ```text
///   Router -> A -> B -> D
///          -> C        ^
///                      |
///   E -----------------+
/// ```
///
/// Scenario:
/// - Router prunes A (only selects C)
/// - A and B are pruned (B has no other active parent)
/// - D has two parents: B (pruned) and E (active, independent node)
/// - D should still execute because E is active and reaches D
///
/// This tests that the pruning logic correctly handles the case where a node
/// has multiple parents and at least one parent remains active.
#[test]
fn test_branch_pruning_diamond_with_active_alternate_parent() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let executed = Arc::new(Mutex::new(Vec::new()));

    // Create nodes
    let action_a = MarkAction {
        name: "A".to_string(),
        executed: executed.clone(),
    };
    let node_a = DefaultNode::with_action("A".to_string(), action_a, &mut table);
    let id_a = node_a.id();

    let action_b = MarkAction {
        name: "B".to_string(),
        executed: executed.clone(),
    };
    let node_b = DefaultNode::with_action("B".to_string(), action_b, &mut table);
    let id_b = node_b.id();

    let action_c = MarkAction {
        name: "C".to_string(),
        executed: executed.clone(),
    };
    let node_c = DefaultNode::with_action("C".to_string(), action_c, &mut table);
    let id_c = node_c.id();

    let action_d = MarkAction {
        name: "D".to_string(),
        executed: executed.clone(),
    };
    let node_d = DefaultNode::with_action("D".to_string(), action_d, &mut table);
    let id_d = node_d.id();

    let action_e = MarkAction {
        name: "E".to_string(),
        executed: executed.clone(),
    };
    let node_e = DefaultNode::with_action("E".to_string(), action_e, &mut table);
    let id_e = node_e.id();

    // Router that only selects C (prunes A)
    let target = Arc::new(Mutex::new(id_c));
    let router = RouterNode::new(
        "Router".to_string(),
        StaticRouter {
            target: target.clone(),
        },
        &mut table,
    );
    let id_router = router.id();

    // Add all nodes
    graph.add_node(router);
    graph.add_node(node_a);
    graph.add_node(node_b);
    graph.add_node(node_c);
    graph.add_node(node_d);
    graph.add_node(node_e);

    // Build topology:
    // Router -> A, C
    // A -> B
    // B -> D
    // E -> D (independent path to D)
    graph.add_edge(id_router, vec![id_a, id_c]);
    graph.add_edge(id_a, vec![id_b]);
    graph.add_edge(id_b, vec![id_d]);
    graph.add_edge(id_e, vec![id_d]); // E is an independent node that also connects to D

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        graph.async_start().await.unwrap();
    });

    let exec_log = executed.lock().unwrap();
    println!("Executed nodes (diamond test): {:?}", *exec_log);

    // Expected behavior:
    // - Router executes and selects only C (prunes A)
    // - A should NOT execute (pruned by router)
    // - B should NOT execute (its only parent A is pruned)
    // - C should execute (selected by router)
    // - E should execute (independent node, no parents)
    // - D should execute (has active parent E, even though B is pruned)

    assert!(
        !exec_log.contains(&"A".to_string()),
        "Node A should be pruned by router"
    );
    assert!(
        !exec_log.contains(&"B".to_string()),
        "Node B should be pruned (parent A is pruned)"
    );
    assert!(
        exec_log.contains(&"C".to_string()),
        "Node C should execute (selected by router)"
    );
    assert!(
        exec_log.contains(&"E".to_string()),
        "Node E should execute (independent node)"
    );
    assert!(
        exec_log.contains(&"D".to_string()),
        "Node D should execute (has active parent E)"
    );
}

/// Test case for dynamic routing inside a loop that alternates between branches.
///
/// Topology:
/// ```text
///   Start -> Router -> A -> LoopNode
///                   -> B
/// ```
///
/// Scenario:
/// - Router alternates between selecting A and B across iterations
/// - First iteration: selects A, prunes B
/// - Second iteration: should select B (after loop jumps back)
/// - Without proper active_nodes reset on loop, B would remain pruned forever
///
/// This tests that the pruning state is reset on loop iteration to allow
/// dynamic routing to select different branches in subsequent iterations.
#[test]
fn test_router_in_loop_alternating_branches() {
    use dagrs::node::loop_node::{LoopCondition, LoopNode};

    let mut graph = Graph::new();
    let mut table = NodeTable::new();
    let executed = Arc::new(Mutex::new(Vec::new()));

    // Track which iteration we're in
    let iteration = Arc::new(Mutex::new(0usize));

    // Create nodes
    let action_a = MarkAction {
        name: "A".to_string(),
        executed: executed.clone(),
    };
    let node_a = DefaultNode::with_action("A".to_string(), action_a, &mut table);
    let id_a = node_a.id();

    let action_b = MarkAction {
        name: "B".to_string(),
        executed: executed.clone(),
    };
    let node_b = DefaultNode::with_action("B".to_string(), action_b, &mut table);
    let id_b = node_b.id();

    // Router that alternates between A and B
    struct AlternatingRouter {
        iteration: Arc<Mutex<usize>>,
        id_a: NodeId,
        id_b: NodeId,
    }

    #[async_trait]
    impl Router for AlternatingRouter {
        async fn route(&self, _: &mut InChannels, out: &OutChannels, _: Arc<EnvVar>) -> Vec<usize> {
            let target = {
                let mut iter = self.iteration.lock().unwrap();
                let target = if *iter % 2 == 0 { self.id_a } else { self.id_b };
                *iter += 1;
                target
            };
            let _ = out
                .send_to(&target, dagrs::Content::new("ping".to_string()))
                .await;
            vec![target.as_usize()]
        }
    }

    let router = RouterNode::new(
        "Router".to_string(),
        AlternatingRouter {
            iteration: iteration.clone(),
            id_a,
            id_b,
        },
        &mut table,
    );
    let id_router = router.id();

    // Loop condition that runs 4 times (to test 2 full alternations)
    struct CountCondition {
        count: Mutex<usize>,
        max: usize,
    }

    impl LoopCondition for CountCondition {
        fn should_continue(
            &mut self,
            _: &dagrs::InChannels,
            _: &dagrs::OutChannels,
            _: Arc<EnvVar>,
        ) -> bool {
            let mut c = self.count.lock().unwrap();
            *c += 1;
            *c < self.max
        }

        fn reset(&mut self) {
            *self.count.lock().unwrap() = 0;
        }
    }

    let loop_node = LoopNode::new(
        "Loop".to_string(),
        id_router,
        CountCondition {
            count: Mutex::new(0),
            max: 4, // Run 4 iterations
        },
        &mut table,
    );
    let id_loop = loop_node.id();

    // Add nodes
    graph.add_node(router);
    graph.add_node(node_a);
    graph.add_node(node_b);
    graph.add_node(loop_node);

    // Build topology:
    // Router -> A, B
    // A -> Loop
    // B -> Loop
    graph.add_edge(id_router, vec![id_a, id_b]);
    graph.add_edge(id_a, vec![id_loop]);
    graph.add_edge(id_b, vec![id_loop]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        graph.async_start().await.unwrap();
    });

    let exec_log = executed.lock().unwrap();
    println!(
        "Executed nodes (alternating router in loop): {:?}",
        *exec_log
    );

    // Count how many times A and B were executed
    let a_count = exec_log.iter().filter(|s| *s == "A").count();
    let b_count = exec_log.iter().filter(|s| *s == "B").count();

    println!("A executed {} times, B executed {} times", a_count, b_count);

    // With proper active_nodes reset on loop:
    // - Iteration 0: Router selects A (B pruned), A runs
    // - Iteration 1: Router selects B (A pruned), B runs
    // - Iteration 2: Router selects A (B pruned), A runs
    // - Iteration 3: Router selects B (A pruned), B runs
    // Both A and B should execute 2 times each
    assert!(
        a_count >= 2,
        "A should execute at least 2 times (got {})",
        a_count
    );
    assert!(
        b_count >= 2,
        "B should execute at least 2 times (got {}). Without active_nodes reset, B would be permanently pruned after first iteration.",
        b_count
    );
}
