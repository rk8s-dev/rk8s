use super::client::Client;
use super::gemini_api_types::{
    Content, GenerateContentRequest, GenerateContentResponse, GenerationConfig, Part,
};
use crate::ai::client::Provider;
use crate::ai::completion::{
    AssistantContent, CompletionError, CompletionModel as CompletionModelTrait, CompletionRequest,
    CompletionResponse, Message, UserContent,
};

/// A completion model implementation for Google Gemini.
///
/// This struct handles the interaction with the Gemini API, including:
/// - Constructing requests from the generic `CompletionRequest`.
/// - Parsing responses from the Gemini API.
/// - Handling errors and status codes.
#[derive(Clone, Debug)]
pub struct CompletionModel {
    /// The client instance used to make HTTP requests.
    client: Client,
    /// The name of the Gemini model to use (e.g., "gemini-1.5-flash").
    model: String,
}

impl CompletionModel {
    /// Creates a new Gemini CompletionModel.
    ///
    /// # Arguments
    /// * `client` - The configured Gemini client.
    /// * `model` - The model identifier.
    pub fn new(client: Client, model: impl Into<String>) -> Self {
        Self {
            client,
            model: model.into(),
        }
    }
}

impl CompletionModelTrait for CompletionModel {
    type Response = GenerateContentResponse;

    async fn completion(
        &self,
        request: CompletionRequest,
    ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
        // Use generic Provider to customize request if needed, though Gemini usually uses query param or header.
        // Our new Client structure puts auth in header via on_request.

        let url = format!(
            "{}/v1beta/models/{}:generateContent",
            self.client.base_url, self.model
        );

        // Convert messages
        let mut contents = Vec::new();
        for msg in request.chat_history {
            match msg {
                Message::User { content } => {
                    let mut parts = Vec::new();
                    for item in content.into_iter() {
                        match item {
                            UserContent::Text(t) => parts.push(Part { text: Some(t.text) }),
                            UserContent::Image(_) => {
                                // Reserved for future Image support
                                return Err(CompletionError::RequestError(
                                    "Image support not implemented yet for Gemini provider".into(),
                                ));
                            }
                            UserContent::ToolResult(_) => {
                                // Reserved for future Tool support
                                return Err(CompletionError::RequestError(
                                    "Tool result support not implemented yet for Gemini provider"
                                        .into(),
                                ));
                            }
                        }
                    }
                    contents.push(Content {
                        role: Some("user".to_string()),
                        parts,
                    });
                }
                Message::Assistant { content, .. } => {
                    let mut parts = Vec::new();
                    for item in content.into_iter() {
                        match item {
                            AssistantContent::Text(t) => parts.push(Part { text: Some(t.text) }),
                            AssistantContent::ToolCall(_) => {
                                // Reserved for future Tool support
                                return Err(CompletionError::RequestError(
                                    "Tool call support not implemented yet for Gemini provider"
                                        .into(),
                                ));
                            }
                        }
                    }
                    contents.push(Content {
                        role: Some("model".to_string()),
                        parts,
                    });
                }
                Message::System { content } => {
                    // System messages in chat history might need to be merged or handled as 'user' role for Gemini
                    // or ignored if preamble is preferred.
                    // For now, treat as user text.
                    let mut parts = Vec::new();
                    for item in content.into_iter() {
                        if let UserContent::Text(t) = item {
                            parts.push(Part { text: Some(t.text) })
                        }
                    }
                    contents.push(Content {
                        role: Some("user".to_string()),
                        parts,
                    });
                }
            }
        }

        // Handle Preamble (System Prompt)
        let system_instruction = request.preamble.map(|preamble| Content {
            role: Some("user".to_string()),
            parts: vec![Part {
                text: Some(preamble),
            }],
        });

        let body = GenerateContentRequest {
            contents,
            system_instruction,
            generation_config: Some(GenerationConfig {
                temperature: request.temperature,
            }),
        };

        // Build request using generic client logic
        let mut req_builder = self.client.http_client.post(&url).json(&body);

        // Apply Provider customizations (Auth headers)
        req_builder = self.client.provider.on_request(req_builder);

        log::info!("Sending request to Gemini API: {}", url);
        let resp = req_builder
            .send()
            .await
            .map_err(CompletionError::HttpError)?;

        log::info!("Received response status: {}", resp.status());

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(CompletionError::ProviderError(format!(
                "Gemini API Error: {}",
                text
            )));
        }

        let api_resp: GenerateContentResponse =
            resp.json().await.map_err(CompletionError::HttpError)?;

        // Extract text
        let text = api_resp
            .candidates
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.content.as_ref())
            .and_then(|c| c.parts.first())
            .and_then(|p| p.text.clone())
            .unwrap_or_default();

        Ok(CompletionResponse {
            choice: text,
            raw_response: api_resp,
        })
    }
}
