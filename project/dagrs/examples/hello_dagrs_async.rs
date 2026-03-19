//! # Example: hello_dagrs_async
//!
//! This is the **recommended** async version of the hello_dagrs example.
//! It demonstrates running a `Graph` inside `#[tokio::main]` using
//! `graph.async_start().await` — no manual runtime management needed.
//!
//! For the synchronous variant (with explicit runtime), see `hello_dagrs.rs`.

use std::sync::Arc;

use async_trait::async_trait;
use dagrs::{
    Action, Content, DefaultNode, EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output,
};

/// An implementation of [`Action`] that returns [`Output::Out`] containing a String "Hello Dagrs".
#[derive(Default)]
pub struct HelloAction;

#[async_trait]
impl Action for HelloAction {
    async fn run(&self, _: &mut InChannels, _: &mut OutChannels, _: Arc<EnvVar>) -> Output {
        Output::Out(Some(Content::new("Hello Dagrs".to_string())))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // create an empty `NodeTable`
    let mut node_table = NodeTable::new();

    // create a `DefaultNode` with action `HelloAction`
    let hello_node =
        DefaultNode::with_action("Hello Dagrs".to_string(), HelloAction, &mut node_table);
    let id = hello_node.id();

    // create a graph with this node and run asynchronously
    let mut graph = Graph::new();
    graph.add_node(hello_node);

    // `async_start` is the recommended entry point — runtime is managed by the caller
    graph.async_start().await?;

    // verify the output of this node
    let outputs = graph.get_outputs();
    assert_eq!(outputs.len(), 1);

    let content = outputs.get(&id).unwrap().get_out().unwrap();
    let node_output = content.get::<String>().unwrap();
    assert_eq!(node_output, "Hello Dagrs");
    println!("Output: {node_output}");

    Ok(())
}
