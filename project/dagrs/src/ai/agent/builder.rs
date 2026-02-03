use super::Agent;
use crate::ai::completion::CompletionModel;
use crate::ai::tools::ToolSet;

/// A builder for configuring and creating AI Agent instances.
pub struct AgentBuilder<M: CompletionModel> {
    model: M,
    preamble: Option<String>,
    temperature: Option<f64>,
    tools: ToolSet,
}

impl<M: CompletionModel> AgentBuilder<M> {
    /// Creates a new AgentBuilder with the specified CompletionModel.
    pub fn new(model: M) -> Self {
        Self {
            model,
            preamble: None,
            temperature: None,
            tools: ToolSet::default(),
        }
    }

    /// Sets the preamble (system prompt) for the agent.
    pub fn preamble(mut self, preamble: impl Into<String>) -> Self {
        self.preamble = Some(preamble.into());
        self
    }

    /// Sets the temperature for the agent's responses.
    pub fn temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Builds and returns the configured Agent instance.
    pub fn build(self) -> Agent<M> {
        Agent {
            model: self.model,
            preamble: self.preamble,
            temperature: self.temperature,
            tools: self.tools,
        }
    }
}
