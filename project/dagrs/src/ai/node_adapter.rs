use crate::ai::agent::Agent;
use crate::ai::completion::{CompletionModel, Prompt};
use crate::{Action, Content, EnvVar, InChannels, OutChannels, Output};
use async_trait::async_trait;
use std::sync::Arc;

/// An Action adapter that wraps an AI Agent for use in a DAG node.
///
/// This adapter bridges the gap between `dagrs::Action` and the AI `Agent`.
/// It automatically handles:
/// 1. Reading input from upstream nodes (as Prompt).
/// 2. Invoking the Agent.
/// 3. Broadcasting the Agent's response to downstream nodes.
///
/// # Type Parameters
/// * `M` - The CompletionModel implementation used by the agent.
pub struct AgentAction<M: CompletionModel + 'static> {
    /// The wrapped AI Agent instance.
    agent: Agent<M>,
}

impl<M: CompletionModel> AgentAction<M> {
    /// Creates a new AgentAction adapter.
    ///
    /// # Arguments
    /// * `agent` - The configured Agent instance to wrap.
    pub fn new(agent: Agent<M>) -> Self {
        Self { agent }
    }
}

#[async_trait]
impl<M: CompletionModel> Action for AgentAction<M> {
    async fn run(
        &self,
        in_channels: &mut InChannels,
        out_channels: &mut OutChannels,
        _env: Arc<EnvVar>,
    ) -> Output {
        // 1. Get Input
        let ids = in_channels.get_sender_ids();
        let input: String = if let Some(id) = ids.first() {
            match in_channels.recv_from(id).await {
                Ok(content) => content.get::<String>().cloned().unwrap_or_else(|| {
                    log::warn!(
                        "Received content from upstream {:?} is not a String. Defaulting to empty.",
                        id
                    );
                    String::new()
                }),
                Err(e) => {
                    log::error!("Failed to receive input from upstream {:?}: {:?}", id, e);
                    String::new()
                }
            }
        } else {
            String::new()
        };

        // 2. Run Agent
        match self.agent.prompt(input).await {
            Ok(resp) => {
                let content = Content::new(resp);
                out_channels.broadcast(content.clone()).await;
                Output::Out(Some(content))
            }
            Err(e) => {
                log::error!("Agent Execution Error: {}", e);
                Output::Err(e.to_string())
            }
        }
    }
}
