use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ExchangeResponse {
    pub token: String,
    pub username: String,
    pub expires_at: String,
    #[serde(default)]
    pub registry_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: String,
    message: String,
}

/// POST /api/cli/exchange — swap a one-time auth_code for a JWT.
pub async fn exchange_auth_code(
    client: &reqwest::Client,
    server_url: &str,
    code: &str,
) -> anyhow::Result<ExchangeResponse> {
    let url = format!("{}/api/cli/exchange", server_url.trim_end_matches('/'));

    let res = client
        .post(&url)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await?;

    if res.status().is_success() {
        Ok(res.json::<ExchangeResponse>().await?)
    } else {
        match res.json::<ErrorBody>().await {
            Ok(err) => anyhow::bail!("{}: {}", err.error, err.message),
            Err(_) => anyhow::bail!("Exchange request failed"),
        }
    }
}
