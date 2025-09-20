use crate::login::client_ref;
use crate::login::types::{
    PollTokenErrorKind, PollTokenOk, PollTokenResponse, RequestCodeResponse,
};
use crate::utils::cli::RequestBuilderExt;
use qrcode::QrCode;
use qrcode::render::unicode;
use serde_json::json;
use std::time::Duration;

#[derive(Default)]
pub struct OAuthFlow {
    client_id: String,
}

impl OAuthFlow {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
        }
    }

    fn display_qr_and_code(uri: impl AsRef<str>, user_code: impl AsRef<str>) {
        let code = QrCode::new(uri.as_ref().as_bytes()).unwrap();
        let image = code.render::<unicode::Dense1x2>().build();

        println!("Scan the QR code below with your phone to open the authentication page:");
        println!("{image}");
        println!("Or visit this URL in your browser: {}", uri.as_ref());
        println!("Your one-time code is: {}", user_code.as_ref());
    }

    async fn request_code(&self) -> anyhow::Result<RequestCodeResponse> {
        let url = "https://github.com/login/device/code";
        let scope = "read:user";

        client_ref()
            .post(url)
            .form(&json!({
                "client_id": self.client_id,
                "scope": scope,
            }))
            .send_and_json()
            .await
    }

    async fn poll_token(
        &self,
        interval: u64,
        device_code: impl AsRef<str>,
    ) -> anyhow::Result<PollTokenOk> {
        let url = "https://github.com/login/oauth/access_token";

        let mut sleep_secs = interval;
        loop {
            tokio::time::sleep(Duration::from_secs(sleep_secs)).await;

            let res = client_ref()
                .post(url)
                .form(&json!({
                    "client_id": self.client_id,
                    "device_code": device_code.as_ref(),
                    "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                }))
                .send_and_json::<PollTokenResponse>()
                .await?;

            match res {
                PollTokenResponse::Ok(res) => return Ok(res),
                PollTokenResponse::Err {
                    error,
                    error_description,
                    interval,
                    ..
                } => match error {
                    PollTokenErrorKind::AuthorizationPending => {}
                    PollTokenErrorKind::SlowDown => {
                        let interval = interval.unwrap();
                        sleep_secs = interval;
                    }
                    _ => anyhow::bail!("{error_description}"),
                },
            }
        }
    }

    pub async fn request_token(&self) -> anyhow::Result<PollTokenOk> {
        let res = self.request_code().await?;
        Self::display_qr_and_code(res.verification_uri, res.user_code);
        self.poll_token(res.interval, res.device_code).await
    }
}
