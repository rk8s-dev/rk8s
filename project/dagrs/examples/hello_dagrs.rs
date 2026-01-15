//! # Example: hello_dagrs
//! Creates a `DefaultNode` that returns with "Hello Dagrs",
//! then create a new `Graph` with this node and run.

use std::sync::Arc;

use async_trait::async_trait;
use dagrs::{
    Action, Content, DefaultNode, EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output,
};

/// An implementation of [`Action`] that returns [`Output::Out`] containing a String "Hello world".
#[derive(Default)]
pub struct HelloAction;
#[async_trait]
impl Action for HelloAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::Out(Some(Content::new("Hello Dagrs".to_string())))
    }
}

fn main() {
    // create an empty `NodeTable`
    let mut node_table = NodeTable::new();
    // create a `DefaultNode` with action `HelloAction`
    let hello_node = DefaultNode::with_action(
        "Hello Dagrs".to_string(),
        HelloAction::default(),
        &mut node_table,
    );
    let id: &dagrs::NodeId = &hello_node.id();

    // create a graph with this node and run
    let mut graph = Graph::new();
    graph.add_node(hello_node);

    match graph.start() {
        Ok(_) => {
            // verify the output of this node
            let outputs = graph.get_outputs();
            assert_eq!(outputs.len(), 1);

            let content = outputs.get(id).unwrap().get_out().unwrap();
            let node_output = content.get::<String>().unwrap();
            assert_eq!(node_output, "Hello Dagrs")
        }
        Err(e) => {
            eprintln!("Graph execution failed: {:?}", e);
        }
    }
}
