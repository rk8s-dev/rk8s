//! IN
//! |          +----------+
//! |          ↓          |
//! +-----> INTER ----> PROC
//！
//！ A loop dag where K is the starter ("kicker") process, that emits a single packet containing a blank,
//! (imagine) INTER controls a screen. In this figure, INTER receives some data from PROC, displays it on
//! the screen, and then sends information back to PROC.
//! If the software infrastructure allows it, waiting for input need only suspend INTER, and other processes
//! could be working on their input while INTER is suspended.

use std::{env, fmt::Display, sync::Arc};

use async_trait::async_trait;
use dagrs::{
    graph::loop_subgraph::LoopSubgraph, Action, Content, DefaultNode, EnvVar, Graph, InChannels,
    Node, NodeId, NodeTable, OutChannels, Output,
};

struct InAction;

/// Send a unit to the INTER node.
#[async_trait]
impl Action for InAction {
    async fn run(
        &self,
        _in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        _env: Arc<EnvVar>,
    ) -> Output {
        log::info!("`In` send start signal to INTER node");
        out_channels.broadcast(Content::new(())).await;
        Output::Out(None)
    }
}

struct InterAction {
    in_id: NodeId,
    proc_id: NodeId,
    limit: usize,
}

#[async_trait]
impl Action for InterAction {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        _env: Arc<EnvVar>,
    ) -> Output {
        // Recv a start signal from the IN node.
        let content = in_channels.recv_from(&self.in_id).await.unwrap();
        in_channels.close_async(&self.in_id).await;
        log::info!("`Inter` Received start signal from IN node");

        let mut times = 0usize;

        out_channels.send_to(&self.proc_id, content).await.unwrap();
        log::info!("`Inter` send start signal to PROC node");

        while let Ok(content) = in_channels.recv_from(&self.proc_id).await {
            // Simulate screen display and user input
            log::info!(
                "`Inter` Displaying input: [{}]",
                content.get::<Arc<dyn Display + Send + Sync>>().unwrap()
            );
            out_channels.send_to(&self.proc_id, content).await.unwrap();
            log::info!("`Inter` send output to PROC node");

            times += 1;
            if times >= self.limit {
                log::info!("`Inter` reached iter limit {}, exit", times);
                out_channels.close(&self.proc_id);
                break;
            }
        }

        log::info!("`Inter` exit");
        Output::empty()
    }
}

struct ProcAction {
    inter_node: NodeId,
}

#[async_trait]
impl Action for ProcAction {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        _env: Arc<EnvVar>,
    ) -> Output {
        let mut times = 0usize;
        while let Ok(_) = in_channels.recv_from(&self.inter_node).await {
            log::info!("`Proc` send {} to INTER node", times);
            out_channels
                .send_to(
                    &self.inter_node,
                    Content::new(Arc::new(times) as Arc<dyn Display + Send + Sync>),
                )
                .await
                .unwrap();
            times += 1;
        }

        log::info!("`Proc` exit");
        Output::empty()
    }
}

fn main() {
    env::set_var("RUST_LOG", "info");
    env_logger::init();

    let mut node_table = NodeTable::default();

    // Create nodes
    let in_node = DefaultNode::with_action("IN".to_string(), InAction, &mut node_table);
    let in_id = in_node.id();

    let mut inter = DefaultNode::new("Inter".to_string(), &mut node_table);
    let inter_id = inter.id();

    let mut proc = DefaultNode::new("Proc".to_string(), &mut node_table);
    let proc_id = proc.id();

    inter.set_action(InterAction {
        in_id,
        proc_id,
        limit: 10,
    });
    proc.set_action(ProcAction {
        inter_node: inter_id,
    });

    let mut inter_proc = LoopSubgraph::new("inter_proc".to_string(), &mut node_table);
    inter_proc.add_node(inter);
    inter_proc.add_node(proc);

    // Create graph and add nodes
    let mut graph = Graph::new();
    graph.add_node(in_node);
    graph.add_node(inter_proc);

    // Set up dependencies to create the loop
    graph.add_edge(in_id, vec![inter_id]);
    graph.add_edge(inter_id, vec![proc_id]);
    graph.add_edge(proc_id, vec![inter_id]);

    // Execute graph
    match graph.start() {
        Ok(_) => println!("Graph executed successfully"),
        Err(e) => panic!("Graph execution failed: {:?}", e),
    }
}
