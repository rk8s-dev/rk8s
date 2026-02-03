use super::message::Message;
use serde_json::Value;

/// Represents a request for AI completion, including chat history and optional parameters.
#[derive(Debug, Clone, Default)]
pub struct CompletionRequest {
    pub preamble: Option<String>,   // Future-proof: Preamble support
    pub chat_history: Vec<Message>, // Conversation messages
    pub temperature: Option<f64>,   // Sampling temperature
    // Future-proof: Tools support
    pub tools: Vec<Value>, // Placeholder for generic ToolDefinition
    // Future-proof: RAG support
    pub documents: Vec<Value>, // Placeholder for Document
}

/// Represents a response from the AI completion service.
#[derive(Debug)]
pub struct CompletionResponse<T> {
    pub choice: String,  // Simplified for basic text
    pub raw_response: T, // Raw response from the AI service
}

impl CompletionRequest {
    /// Create a new CompletionRequest with the given chat history.
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            chat_history: messages,
            ..Default::default()
        }
    }
}
