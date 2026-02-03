use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::CompletionError;

/// Represents a message in the conversation, which can be from a user, an assistant, or the system.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    // The message is from a user.
    User {
        content: OneOrMany<UserContent>,
    },
    // The message is from an assistant.
    Assistant {
        id: Option<String>,
        content: OneOrMany<AssistantContent>,
    },
    // Future-proof: Explicit System message support
    System {
        content: OneOrMany<UserContent>, // System usually takes text
    },
}

/// Content types for User messages.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum UserContent {
    Text(Text),
    // Future-proof: Image support
    Image(Image),
    // Future-proof: Tool Result support
    ToolResult(ToolResult),
}

/// Content types for Assistant messages.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum AssistantContent {
    Text(Text),
    // Future-proof: Tool Call support
    ToolCall(ToolCall),
}

/// Text content.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Text {
    pub text: String,
}

/// Image content.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Image {
    pub data: String, // Base64 or URL
    pub mime_type: Option<String>,
}

/// Tool Call content.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Tool Result content.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ToolResult {
    pub id: String,
    pub result: serde_json::Value,
}

/// Implementations for Text
impl Text {
    pub fn text(&self) -> &str {
        &self.text
    }
}

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.text)
    }
}

// ================================================================
// Helper Types
// ================================================================

/// A type that can represent either a single item or multiple items.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(T),
    Many(Vec<T>),
}

impl<T> OneOrMany<T> {
    // Create a OneOrMany with a single item.
    pub fn one(item: T) -> Self {
        Self::One(item)
    }

    // Create a OneOrMany with multiple items, or None if the vector is empty.
    pub fn many(items: Vec<T>) -> Option<Self> {
        if items.is_empty() {
            None
        } else {
            Some(Self::Many(items))
        }
    }

    // Returns an iterator over the items.
    pub fn iter(&self) -> OneOrManyIter<'_, T> {
        match self {
            OneOrMany::One(item) => OneOrManyIter::One(Some(item)),
            OneOrMany::Many(items) => OneOrManyIter::Many(items.iter()),
        }
    }
}

impl<T> IntoIterator for OneOrMany<T> {
    type Item = T;
    type IntoIter = OneOrManyIntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            OneOrMany::One(item) => OneOrManyIntoIter::One(Some(item)),
            OneOrMany::Many(items) => OneOrManyIntoIter::Many(items.into_iter()),
        }
    }
}

/// Iterator for OneOrMany
pub enum OneOrManyIter<'a, T> {
    One(Option<&'a T>),
    Many(std::slice::Iter<'a, T>),
}

impl<'a, T> Iterator for OneOrManyIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            OneOrManyIter::One(item) => item.take(),
            OneOrManyIter::Many(iter) => iter.next(),
        }
    }
}

/// IntoIterator for OneOrMany
pub enum OneOrManyIntoIter<T> {
    One(Option<T>),
    Many(std::vec::IntoIter<T>),
}

impl<T> Iterator for OneOrManyIntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            OneOrManyIntoIter::One(item) => item.take(),
            OneOrManyIntoIter::Many(iter) => iter.next(),
        }
    }
}

impl Message {
    /// Create a user message with text content.
    pub fn user(text: impl Into<String>) -> Self {
        Message::User {
            content: OneOrMany::One(UserContent::Text(Text { text: text.into() })),
        }
    }
}

impl From<String> for Message {
    fn from(text: String) -> Self {
        Message::user(text)
    }
}

impl From<&str> for Message {
    fn from(text: &str) -> Self {
        Message::user(text)
    }
}

/// Errors related to Message operations.
#[derive(Debug, Error)]
pub enum MessageError {
    #[error("Message conversion error: {0}")]
    ConversionError(String),
}

impl From<MessageError> for CompletionError {
    fn from(error: MessageError) -> Self {
        CompletionError::RequestError(error.into())
    }
}
