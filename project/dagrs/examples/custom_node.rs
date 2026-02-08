//! # Example: custom_node
//! Creates a custom implementation of [`Node`] that returns a [`String`],
//! then create a new [`Graph`] with this node and run.

use std::sync::Arc;

use async_trait::async_trait;
use dagrs::{
    Content, EnvVar, Graph, InChannels, Node, NodeId, NodeName, NodeTable, OutChannels, Output,
};

struct MessageNode {
    id: NodeId,
    name: NodeName,
    in_channels: InChannels,
    out_channels: OutChannels,
    /*Put your custom fields here.*/
    message: String,
}

#[async_trait]
impl Node for MessageNode {
    fn id(&self) -> NodeId {
        self.id
    }

    fn name(&self) -> NodeName {
        self.name.clone()
    }

    fn input_channels(&mut self) -> &mut InChannels {
        &mut self.in_channels
    }

    fn output_channels(&mut self) -> &mut OutChannels {
        &mut self.out_channels
    }

    async fn run(&mut self, _: Arc<EnvVar>) -> Output {
        Output::Out(Some(Content::new(self.message.clone())))
    }
}

impl MessageNode {
    fn new(name: String, node_table: &mut NodeTable) -> Self {
        Self {
            id: node_table.alloc_id_for(&name),
            name,
            in_channels: InChannels::default(),
            out_channels: OutChannels::default(),
            message: "hello dagrs".to_string(),
        }
    }
}

fn main() {
    // create an empty `NodeTable`
    let mut node_table = NodeTable::new();
    // create a `MessageNode`
    let node = MessageNode::new("message node".to_string(), &mut node_table);
    let id: &dagrs::NodeId = &node.id();

    // create a graph with this node and run
    let mut graph = Graph::new();
    graph.add_node(node);
    match graph.start() {
        Ok(_) => {
            // verify the output of this node
            let outputs = graph.get_outputs();
            assert_eq!(outputs.len(), 1);

            let content = outputs.get(id).unwrap().get_out().unwrap();
            let node_output = content.get::<String>().unwrap();
            assert_eq!(node_output, "hello dagrs")
        }
        Err(e) => {
            eprintln!("Graph execution failed: {:?}", e);
        }
    }
}
