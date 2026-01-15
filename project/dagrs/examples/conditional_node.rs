//! Variant of the example `compute_dag`.
//! Only use Dag, execute a job. The graph is as follows:
//!
//!    ┌───────────↴
//!    B ──→ E ──→ X ──→ G
//!  ↗     ↗           ↗
//! A ───→ C          ╱
//!  ↘     ↘        ╱
//!    D ───→ F ───╱
//!
//! The node `X` is a conditional node which will return false.
//!
//! Dagrs will split the graph into two blocks:     
//! - A, B, C, D, E, F, X
//! - G
//!
//! Since node X's condition (VerifyGT(128)) is not met,
//! execution stops at node X and never reaches node G.
//! Therefore G's result should be None.

use std::{env, sync::Arc};

use async_trait::async_trait;
use dagrs::{
    node::conditional_node::{Condition, ConditionalNode},
    Action, Content, DefaultNode, EnvVar, Graph, InChannels, Node, NodeTable, OutChannels, Output,
};

const BASE: &str = "base";

struct Compute(usize);

#[async_trait]
impl Action for Compute {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        env: Arc<EnvVar>,
    ) -> Output {
        let base = env.get::<usize>(BASE).unwrap();
        let mut sum = self.0;

        in_channels
            .map(|content| content.unwrap().into_inner::<usize>().unwrap())
            .await
            .into_iter()
            .for_each(|x| sum += *x * base);

        out_channels.broadcast(Content::new(sum)).await;

        Output::Out(Some(Content::new(sum)))
    }
}

struct VerifyGT(usize);
#[async_trait]
impl Condition for VerifyGT {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &OutChannels,
        _: Arc<EnvVar>,
    ) -> bool {
        let mut sum = 0;
        in_channels
            .map(|content| content.unwrap().into_inner::<usize>().unwrap())
            .await
            .into_iter()
            .for_each(|x| sum += *x);

        let verify = sum > self.0;
        if verify {
            out_channels.broadcast(Content::new(sum)).await;
        }

        verify
    }
}

fn main() {
    // Initialization log.
    env::set_var("RUST_LOG", "debug");
    env_logger::init();

    // Create a new `NodeTable`.
    let mut node_table = NodeTable::default();

    // Generate some tasks.
    let a = DefaultNode::with_action("Compute A".to_string(), Compute(1), &mut node_table);
    let a_id = a.id();

    let b = DefaultNode::with_action("Compute B".to_string(), Compute(2), &mut node_table);
    let b_id = b.id();

    let mut c = DefaultNode::new("Compute C".to_string(), &mut node_table);
    c.set_action(Compute(4));
    let c_id = c.id();

    let mut d = DefaultNode::new("Compute D".to_string(), &mut node_table);
    d.set_action(Compute(8));
    let d_id = d.id();

    let e = DefaultNode::with_action("Compute E".to_string(), Compute(16), &mut node_table);
    let e_id = e.id();
    let f = DefaultNode::with_action("Compute F".to_string(), Compute(32), &mut node_table);
    let f_id = f.id();

    let x =
        ConditionalNode::with_condition("Condition X".to_string(), VerifyGT(128), &mut node_table);
    let x_id = x.id();

    let g = DefaultNode::with_action("Compute G".to_string(), Compute(64), &mut node_table);
    let g_id = g.id();

    // Create a graph.
    let mut graph = Graph::new();
    vec![a, b, c, d, e, f, g]
        .into_iter()
        .for_each(|node| graph.add_node(node));
    graph.add_node(x);

    // Set up task dependencies.
    graph.add_edge(a_id, vec![b_id, c_id, d_id]);
    graph.add_edge(b_id, vec![e_id, x_id]);
    graph.add_edge(c_id, vec![e_id, f_id]);
    graph.add_edge(d_id, vec![f_id]);
    graph.add_edge(e_id, vec![x_id]);
    graph.add_edge(x_id, vec![g_id]);
    graph.add_edge(f_id, vec![g_id]);

    // Set a global environment variable for this dag.
    let mut env = EnvVar::new(node_table);
    env.set("base", 2usize);
    graph.set_env(env);

    // Start executing this dag.
    match graph.start() {
        Ok(_) => {
            // Since node X's condition (VerifyGT(128)) is not met,
            // execution stops at node X and never reaches node G.
            // Therefore G's result should be None.
            let res = graph.get_results::<usize>().get(&g_id).unwrap().clone();
            assert!(res.is_none());
        }
        Err(e) => {
            panic!("Graph execution failed: {:?}", e);
        }
    }
}
