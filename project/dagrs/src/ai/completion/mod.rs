pub mod message;
pub mod request;

pub use message::{AssistantContent, Message, MessageError, UserContent};
pub use request::{CompletionRequest, CompletionResponse};

use std::future::Future;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompletionError {
    #[error("HttpError: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("JsonError: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("RequestError: {0}")]
    RequestError(#[from] Box<dyn std::error::Error + Send + Sync + 'static>),

    #[error("ProviderError: {0}")]
    ProviderError(String),

    #[error("ResponseError: {0}")]
    ResponseError(String),
}

pub trait CompletionModel: Clone + Send + Sync {
    type Response: Send + Sync;

    fn completion(
        &self,
        request: CompletionRequest,
    ) -> impl Future<Output = Result<CompletionResponse<Self::Response>, CompletionError>> + Send;
}

pub trait Prompt: Send + Sync {
    fn prompt(
        &self,
        prompt: impl Into<Message> + Send,
    ) -> impl Future<Output = Result<String, CompletionError>> + Send;
}

pub trait Chat: Send + Sync {
    fn chat(
        &self,
        prompt: impl Into<Message> + Send,
        chat_history: Vec<Message>,
    ) -> impl Future<Output = Result<String, CompletionError>> + Send;
}
