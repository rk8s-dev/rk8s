use async_trait::async_trait;
use dagrs::node::action::Action;
use dagrs::node::default_node::DefaultNode;
use dagrs::node::loop_node::{CountLoopCondition, LoopNode};
use dagrs::{EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output};
use std::sync::{Arc, Mutex};

/// Action that increments a counter
#[derive(Clone)]
struct IncAction {
    counter: Arc<Mutex<usize>>,
}

#[async_trait]
impl Action for IncAction {
    /// Increment the counter each time this action is run.
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        let mut c = self.counter.lock().unwrap();
        *c += 1;
        Output::empty()
    }
}

#[test]
fn test_loop_reset() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let counter = Arc::new(Mutex::new(0));

    // Node A: Counter++
    let node_a = DefaultNode::with_action(
        "A".to_string(),
        IncAction {
            counter: counter.clone(),
        },
        &mut table,
    );
    let id_a = node_a.id();

    // Loop: runs A 3 times
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

    // # Dynamic Jump Mechanism
    //
    // We do NOT add a static edge from Loop -> A (e.g., `graph.add_edge(id_loop, vec![id_a])`).
    //
    // ## Reason 1: Cycle Avoidance
    // Adding a static edge would create a cycle (A -> Loop -> A) in the dependency graph,
    // causing the topological sort to fail with a cycle detection error.
    //
    // ## Reason 2: Runtime Control Flow
    // The "jump" is handled dynamically at runtime:
    // 1. The `LoopNode` returns a `FlowControl::Loop` instruction containing the target node ID.
    // 2. The Graph executor interprets this instruction.
    // 3. It looks up the block index of the target node in `node_block_map`.
    // 4. It modifies the Program Counter (pc) to jump backwards to that block.
    //
    // This design allows for cyclic execution flows on top of an acyclic static graph structure,
    // maintaining the benefits of topological ordering while supporting loop semantics.

    let rt = tokio::runtime::Runtime::new().unwrap();

    println!("First Run");
    rt.block_on(async {
        graph.async_start().await.unwrap();
    });

    // A runs once initially + 3 times loop = 4 times?
    // Wait, let's check CountLoopCondition logic.
    // LoopNode calls should_continue. If true, it returns Loop(target).
    // Initial run: Graph executes LoopNode (if it's in topological order).
    // Actually loop logic depends on graph structure.
    // Let's assume standard loop subgraph behavior or just simple loop.
    // dagrs handles loops by expanding them or by jump instructions.
    // CountLoopCondition: new(3) -> 0, 1, 2 < 3. Returns true 3 times.

    // Check counter.
    // The exact count depends on how loop node is scheduled.
    // Assuming it works as intended in first run.
    let first_run_count = *counter.lock().unwrap();
    println!("First run count: {}", first_run_count);
    assert_eq!(
        first_run_count, 4,
        "Counter should be 4 (1 initial + 3 loops)"
    );

    // Reset
    rt.block_on(async {
        graph.reset().await;
    });
    *counter.lock().unwrap() = 0;

    println!("Second Run");
    rt.block_on(async {
        graph.async_start().await.unwrap();
    });

    let second_run_count = *counter.lock().unwrap();
    println!("Second run count: {}", second_run_count);

    // Verify both correctness AND consistency:
    // - First assertion ensures second run produces the expected count (4)
    // - This catches bugs where loops might be broken in both runs
    assert_eq!(
        second_run_count, 4,
        "Counter should be 4 after reset (1 initial + 3 loops)"
    );
    // - Second assertion ensures consistency between runs
    assert_eq!(
        first_run_count, second_run_count,
        "Loop execution count should be consistent after reset"
    );
}
