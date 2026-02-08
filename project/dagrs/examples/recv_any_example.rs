//! # Example: recv_any_example
//! Demonstrates how to use the `recv_any` method of `InChannels` to receive data from any available channel.
//!
//!
//! # Output
//! When running this example, you will see output similar to:
//! ```
//! Received message 'Hello from Sender' from node NodeId(1)
//! Received message 'Hello from SlowSender' from node NodeId(2)
//! ```
//!
//! The first message comes from the normal sender, and the second message comes from the slow sender
//! after a 500ms delay.
//!
//! //! This example creates a graph with two senders and one receiver:
//! - A normal sender that sends messages immediately
//! - A slow sender that delays 500ms before sending messages
//! - A receiver that uses `recv_any` to receive messages from either sender
//!
//! # Output
//! When running this example, you will see output similar to:
//! ```
//! Received message 'Hello from Sender' from node NodeId(1)
//! Received message 'Hello from SlowSender' from node NodeId(2)
//! ```
//!
//! The first message comes from the normal sender, and the second message comes from the slow sender
//! after a 500ms delay.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use dagrs::{
    DefaultNode, EnvVar, Graph, Node, NodeTable, Output,
    connection::{in_channel::TypedInChannels, out_channel::TypedOutChannels},
    node::typed_action::TypedAction,
};
use tokio::time::sleep;

/// An action that sends a message to its output channel
#[derive(Default)]
pub struct SenderAction {
    message: String,
}

impl SenderAction {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

#[async_trait]
impl TypedAction for SenderAction {
    type I = ();
    type O = String;

    async fn run(
        &self,
        _: TypedInChannels<Self::I>,
        out: TypedOutChannels<Self::O>,
        _: Arc<EnvVar>,
    ) -> Output {
        // Send the message to all receivers
        out.broadcast(self.message.clone()).await;
        Output::Out(None)
    }
}

/// An action that sends a message to its output channel after a delay
#[derive(Default)]
pub struct SlowSenderAction {
    message: String,
}

impl SlowSenderAction {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

#[async_trait]
impl TypedAction for SlowSenderAction {
    type I = ();
    type O = String;

    async fn run(
        &self,
        _: TypedInChannels<Self::I>,
        out: TypedOutChannels<Self::O>,
        _: Arc<EnvVar>,
    ) -> Output {
        // Wait for 500ms before sending
        sleep(Duration::from_millis(500)).await;
        // Send the message to all receivers
        out.broadcast(self.message.clone()).await;
        Output::Out(None)
    }
}

/// An action that receives messages from any available channel
#[derive(Default)]
pub struct ReceiverAction;

#[async_trait]
impl TypedAction for ReceiverAction {
    type I = String;
    type O = ();

    async fn run(
        &self,
        mut input: TypedInChannels<Self::I>,
        _: TypedOutChannels<Self::O>,
        _: Arc<EnvVar>,
    ) -> Output {
        // Receive from any available channel
        match input.recv_any().await {
            Ok((sender_id, content)) => {
                let message = content.unwrap();
                println!("Received message '{}' from node {:?}", message, sender_id);
            }
            Err(e) => {
                eprintln!("Error receiving message: {:?}", e);
            }
        }

        match input.recv_any().await {
            Ok((sender_id, content)) => {
                let message = content.unwrap();
                println!("Received message '{}' from node {:?}", message, sender_id);
            }
            Err(e) => {
                eprintln!("Error receiving message: {:?}", e);
            }
        }

        Output::Out(None)
    }
}

fn main() {
    // Create a node table
    let mut node_table = NodeTable::new();

    // Create sender nodes
    let sender1 = DefaultNode::with_action(
        "Sender1".to_string(),
        SenderAction::new("Hello from Sender".to_string()),
        &mut node_table,
    );
    let sender2 = DefaultNode::with_action(
        "Sender2".to_string(),
        SlowSenderAction::new("Hello from SlowSender".to_string()),
        &mut node_table,
    );

    // Create receiver node
    let receiver = DefaultNode::with_action(
        "Receiver".to_string(),
        ReceiverAction::default(),
        &mut node_table,
    );

    // Get node IDs before adding nodes to the graph
    let sender1_id = sender1.id();
    let sender2_id = sender2.id();
    let receiver_id = receiver.id();

    // Create a graph
    let mut graph = Graph::new();

    // Add nodes to the graph
    graph.add_node(sender1);
    graph.add_node(sender2);
    graph.add_node(receiver);

    // Add edges: both senders connect to the receiver
    graph.add_edge(sender1_id, vec![receiver_id]);
    graph.add_edge(sender2_id, vec![receiver_id]);

    // Run the graph
    match graph.start() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Graph execution failed: {:?}", e);
        }
    }
}
