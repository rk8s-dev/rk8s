use crate::ai::completion::{
    Chat, CompletionError, CompletionModel, CompletionRequest, Message, Prompt,
};
use crate::ai::tools::ToolSet;

pub mod builder;
pub use builder::AgentBuilder;

/// An AI Agent that manages interactions with a CompletionModel.
///
/// The Agent is responsible for:
/// - Maintaining configuration (temperature, preamble/system prompt).
/// - Managing tools (optional).
/// - Constructing requests to the underlying model.
///
/// It implements the `Prompt` and `Chat` traits for easy interaction.
pub struct Agent<M: CompletionModel> {
    /// The underlying completion model (e.g., Gemini, OpenAI).
    pub model: M,
    /// System prompt or preamble to set the agent's behavior context.
    pub preamble: Option<String>,
    /// Sampling temperature (0.0 to 1.0). Higher values mean more creativity.
    pub temperature: Option<f64>,
    /// Set of tools available to the agent.
    pub tools: ToolSet,
}

impl<M: CompletionModel> Agent<M> {
    /// Creates a new Agent with the given model.
    ///
    /// # Arguments
    /// * `model` - The completion model instance.
    pub fn new(model: M) -> Self {
        Self {
            model,
            preamble: None,
            temperature: None,
            tools: ToolSet::default(),
        }
    }
}

impl<M: CompletionModel> Prompt for Agent<M> {
    async fn prompt(&self, prompt: impl Into<Message> + Send) -> Result<String, CompletionError> {
        let msg = prompt.into();
        let request = CompletionRequest {
            preamble: self.preamble.clone(),
            chat_history: vec![msg],
            temperature: self.temperature,
            ..Default::default()
        };

        let response = self.model.completion(request).await?;
        Ok(response.choice)
    }
}

impl<M: CompletionModel> Chat for Agent<M> {
    async fn chat(
        &self,
        prompt: impl Into<Message> + Send,
        mut chat_history: Vec<Message>,
    ) -> Result<String, CompletionError> {
        let msg = prompt.into();
        chat_history.push(msg);

        let request = CompletionRequest {
            preamble: self.preamble.clone(),
            chat_history,
            temperature: self.temperature,
            ..Default::default()
        };

        let response = self.model.completion(request).await?;
        Ok(response.choice)
    }
}
