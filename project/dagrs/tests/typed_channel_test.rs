//! Tests for Typed Channel functionality
//!
//! This module tests the type-safe channel communication between nodes.
//! The `TypedAction` trait provides compile-time type checking for input and output channels.
//!
//! Key features tested:
//! - `TypedAction` trait with generic input/output types
//! - `TypedInChannels<T>` for type-safe input
//! - `TypedOutChannels<T>` for type-safe output
//! - Chain of typed actions with data transformation

use async_trait::async_trait;
use dagrs::{
    Content, DefaultNode, EnvVar, Graph, Node, NodeTable, Output,
    connection::{in_channel::TypedInChannels, out_channel::TypedOutChannels},
    node::typed_action::TypedAction,
};
use std::sync::Arc;

/// A typed action that receives a number, doubles it, and sends it downstream.
struct DoubleAction;

#[async_trait]
impl TypedAction for DoubleAction {
    type I = i32;
    type O = i32;

    async fn run(
        &self,
        mut in_channels: TypedInChannels<Self::I>,
        out_channels: TypedOutChannels<Self::O>,
        _env: Arc<EnvVar>,
    ) -> Output {
        // Collect all inputs
        let inputs = in_channels
            .map(|result| match result {
                Ok(Some(value)) => *value,
                _ => 0,
            })
            .await;

        let sum: i32 = inputs.iter().sum();
        let result = sum * 2;

        // Broadcast the result
        out_channels.broadcast(result).await;

        Output::Out(Some(Content::new(result)))
    }
}

/// A typed action that produces a constant value.
struct ProducerAction(i32);

#[async_trait]
impl TypedAction for ProducerAction {
    type I = (); // No input expected
    type O = i32;

    async fn run(
        &self,
        _in_channels: TypedInChannels<Self::I>,
        out_channels: TypedOutChannels<Self::O>,
        _env: Arc<EnvVar>,
    ) -> Output {
        out_channels.broadcast(self.0).await;
        Output::Out(Some(Content::new(self.0)))
    }
}

/// A typed action that consumes a value and stores it.
struct ConsumerAction {
    received: std::sync::Arc<std::sync::Mutex<Option<i32>>>,
}

#[async_trait]
impl TypedAction for ConsumerAction {
    type I = i32;
    type O = (); // No output

    async fn run(
        &self,
        mut in_channels: TypedInChannels<Self::I>,
        _out_channels: TypedOutChannels<Self::O>,
        _env: Arc<EnvVar>,
    ) -> Output {
        let inputs = in_channels
            .map(|result| match result {
                Ok(Some(value)) => Some(*value),
                _ => None,
            })
            .await;

        if let Some(Some(value)) = inputs.first() {
            *self.received.lock().unwrap() = Some(*value);
        }

        Output::empty()
    }
}

#[test]
fn test_typed_channel_chain() {
    // Test topology: Producer -> Double -> Consumer
    // Producer outputs 5, Double doubles it to 10, Consumer receives 10

    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let producer = DefaultNode::with_action("Producer".to_string(), ProducerAction(5), &mut table);
    let producer_id = producer.id();

    let double = DefaultNode::with_action("Double".to_string(), DoubleAction, &mut table);
    let double_id = double.id();

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let consumer = DefaultNode::with_action(
        "Consumer".to_string(),
        ConsumerAction {
            received: received.clone(),
        },
        &mut table,
    );
    let consumer_id = consumer.id();

    graph.add_node(producer);
    graph.add_node(double);
    graph.add_node(consumer);

    graph.add_edge(producer_id, vec![double_id]);
    graph.add_edge(double_id, vec![consumer_id]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        graph.async_start().await.expect("Graph execution failed");
    });

    // Verify the result: 5 * 2 = 10
    assert_eq!(*received.lock().unwrap(), Some(10));
}

/// Test with string types to ensure type safety works with different types.
struct StringProducer(String);

#[async_trait]
impl TypedAction for StringProducer {
    type I = ();
    type O = String;

    async fn run(
        &self,
        _in_channels: TypedInChannels<Self::I>,
        out_channels: TypedOutChannels<Self::O>,
        _env: Arc<EnvVar>,
    ) -> Output {
        out_channels.broadcast(self.0.clone()).await;
        Output::Out(Some(Content::new(self.0.clone())))
    }
}

struct StringConsumer {
    received: std::sync::Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl TypedAction for StringConsumer {
    type I = String;
    type O = ();

    async fn run(
        &self,
        mut in_channels: TypedInChannels<Self::I>,
        _out_channels: TypedOutChannels<Self::O>,
        _env: Arc<EnvVar>,
    ) -> Output {
        let inputs = in_channels
            .map(|result| match result {
                Ok(Some(value)) => Some((*value).clone()),
                _ => None,
            })
            .await;

        if let Some(Some(value)) = inputs.first() {
            *self.received.lock().unwrap() = Some(value.clone());
        }

        Output::empty()
    }
}

#[test]
fn test_typed_channel_with_string() {
    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let producer = DefaultNode::with_action(
        "StringProducer".to_string(),
        StringProducer("Hello, TypedChannel!".to_string()),
        &mut table,
    );
    let producer_id = producer.id();

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let consumer = DefaultNode::with_action(
        "StringConsumer".to_string(),
        StringConsumer {
            received: received.clone(),
        },
        &mut table,
    );
    let consumer_id = consumer.id();

    graph.add_node(producer);
    graph.add_node(consumer);

    graph.add_edge(producer_id, vec![consumer_id]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        graph.async_start().await.expect("Graph execution failed");
    });

    assert_eq!(
        *received.lock().unwrap(),
        Some("Hello, TypedChannel!".to_string())
    );
}

/// Test multiple inputs with typed channels.
#[test]
fn test_typed_channel_multiple_inputs() {
    // Topology:
    // Producer1 (3) -\
    //                 -> Adder -> Consumer
    // Producer2 (7) -/
    //
    // Adder doubles the sum: (3 + 7) * 2 = 20

    let mut graph = Graph::new();
    let mut table = NodeTable::new();

    let producer1 =
        DefaultNode::with_action("Producer1".to_string(), ProducerAction(3), &mut table);
    let producer1_id = producer1.id();

    let producer2 =
        DefaultNode::with_action("Producer2".to_string(), ProducerAction(7), &mut table);
    let producer2_id = producer2.id();

    let adder = DefaultNode::with_action("Adder".to_string(), DoubleAction, &mut table);
    let adder_id = adder.id();

    let received = std::sync::Arc::new(std::sync::Mutex::new(None));
    let consumer = DefaultNode::with_action(
        "Consumer".to_string(),
        ConsumerAction {
            received: received.clone(),
        },
        &mut table,
    );
    let consumer_id = consumer.id();

    graph.add_node(producer1);
    graph.add_node(producer2);
    graph.add_node(adder);
    graph.add_node(consumer);

    graph.add_edge(producer1_id, vec![adder_id]);
    graph.add_edge(producer2_id, vec![adder_id]);
    graph.add_edge(adder_id, vec![consumer_id]);

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        graph.async_start().await.expect("Graph execution failed");
    });

    // (3 + 7) * 2 = 20
    assert_eq!(*received.lock().unwrap(), Some(20));
}
