//! # Example: auto_relay
//! The macro `dependencies!` simplifies the construction of a `Graph`,
//! including the addition of nodes and edges.

use dagrs::{EmptyAction, InChannels, Node, NodeTable, OutChannels, auto_node, dependencies};

#[auto_node]
struct MyNode {/*Put customized fields here.*/}

impl MyNode {
    fn new(name: &str, node_table: &mut NodeTable) -> Self {
        Self {
            id: node_table.alloc_id_for(name),
            name: name.to_string(),
            input_channels: InChannels::default(),
            output_channels: OutChannels::default(),
            action: Box::new(EmptyAction),
        }
    }
}

#[allow(deprecated)]
fn main() {
    let mut node_table = NodeTable::default();

    let node_name = "auto_node";

    let s = MyNode::new(node_name, &mut node_table);
    let a = MyNode::new(node_name, &mut node_table);
    let b = MyNode::new(node_name, &mut node_table);

    let mut g = dependencies!(
        s -> a b,
        b -> a
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    g.start_with_runtime(&runtime).unwrap();
}
