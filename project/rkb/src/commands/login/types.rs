use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct CallbackResponse {
    pub pat: String,
}

#[derive(Deserialize)]
pub struct RequestCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum PollTokenResponse {
    Ok(PollTokenOk),
    Err {
        error: PollTokenErrorKind,
        error_description: String,
        interval: Option<u64>,
    },
}

#[derive(Serialize, Deserialize)]
pub struct PollTokenOk {
    pub access_token: String,
    pub token_type: String,
    pub scope: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PollTokenErrorKind {
    AuthorizationPending,
    SlowDown,
    ExpiredToken,
    UnsupportedGrantType,
    IncorrectClientCredentials,
    IncorrectDeviceCode,
    AccessDenied,
    DeviceFlowDisabled,
}

#[derive(Deserialize)]
pub struct RequestClientIdResponse {
    pub client_id: String,
}
