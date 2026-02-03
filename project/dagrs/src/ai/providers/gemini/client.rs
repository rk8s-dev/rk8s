use crate::ai::client::Provider;
use std::env;

/// Gemini AI Provider for interfacing with Google's Gemini models.
#[derive(Clone, Debug)]
pub struct GeminiProvider {
    pub api_key: String,
}

impl GeminiProvider {
    /// Creates a new GeminiProvider with the given API key.
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

impl Provider for GeminiProvider {
    // Gemini handles authentication via query parameter usually, but Rig uses generic Header handling.
    // For Gemini specifically, it often uses key=XYZ query param.
    // But to conform to the trait, we might not need to do anything here if the URL building handles it.
    // However, a robust Provider trait usually handles request signing.
    // Let's keep it simple: generic providers might add headers.
    fn on_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        // Gemini can also accept X-Goog-Api-Key header
        request.header("x-goog-api-key", &self.api_key)
    }
}

pub type Client = crate::ai::client::Client<GeminiProvider>;

impl Client {
    /// Creates a Gemini Client from environment variables.
    pub fn from_env() -> Result<Self, env::VarError> {
        let api_key = env::var("GEMINI_API_KEY")?;
        let provider = GeminiProvider::new(api_key);
        Ok(Self::new("https://generativelanguage.googleapis.com", provider))
    }

    /// Creates a CompletionModel for the specified Gemini model.
    pub fn completion_model(&self, model: &str) -> super::completion::CompletionModel {
        super::completion::CompletionModel::new(self.clone(), model)
    }
}
