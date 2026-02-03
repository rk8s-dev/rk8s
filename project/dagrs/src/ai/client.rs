use reqwest::Client as HttpClient;

use crate::ai::completion::CompletionModel;

/// A generic client for AI providers.
///
/// It holds the shared HTTP client, base URL, and provider-specific extension.
/// This client handles common HTTP logic like timeouts and proxy configuration.
#[derive(Clone, Debug)]
pub struct Client<P> {
    /// The base URL of the AI provider's API.
    pub base_url: String,
    /// The shared HTTP client (reqwest).
    pub http_client: HttpClient,
    /// Provider-specific logic (e.g., authentication).
    pub provider: P,
}

impl<P> Client<P> {
    /// Creates a new generic Client.
    ///
    /// # Arguments
    /// * `base_url` - The base API URL.
    /// * `provider` - The provider-specific implementation.
    ///
    /// This constructor automatically configures the HTTP client with:
    /// - 30 seconds timeout
    /// - System proxy support (from environment variables)
    pub fn new(base_url: &str, provider: P) -> Self {
        // Build client with timeout and system proxy support
        let http_client = HttpClient::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| HttpClient::new());

        Self {
            base_url: base_url.to_string(),
            http_client,
            provider,
        }
    }
}

/// Trait defining provider-specific behavior.
pub trait Provider: Send + Sync {
    /// Allows the provider to customize the HTTP request (e.g., adding headers).
    ///
    /// # Arguments
    /// * `request` - The pending HTTP request builder.
    ///
    /// # Returns
    /// The modified request builder.
    fn on_request(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
    }
}

/// Trait for clients that support text completion.
pub trait CompletionClient {
    /// The concrete CompletionModel type returned by this client.
    type Model: CompletionModel;

    /// Creates a completion model instance for the given model name.
    fn completion_model(&self, model: impl Into<String>) -> Self::Model;
}
