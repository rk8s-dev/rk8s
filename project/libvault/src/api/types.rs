use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;

/// Generic logical response returned by RustyVault HTTP APIs.
#[derive(Debug, Clone, Deserialize)]
#[serde(bound(deserialize = "T: DeserializeOwned"))]
pub struct ApiResponse<T> {
    #[serde(default)]
    pub renewable: bool,
    #[serde(default)]
    pub lease_id: String,
    #[serde(default)]
    pub lease_duration: u64,
    #[serde(default)]
    pub auth: Option<AuthResponse>,
    pub data: Option<T>,
}

impl<T> ApiResponse<T> {
    /// Returns a reference to the inner `data` field if present.
    pub fn data(&self) -> Option<&T> {
        self.data.as_ref()
    }

    /// Consumes the response and returns the inner `data` value.
    pub fn into_data(self) -> Option<T> {
        self.data
    }
}

pub type GenericResponse = ApiResponse<HashMap<String, Value>>;

/// Authentication payload returned by login endpoints.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct AuthResponse {
    pub client_token: String,
    #[serde(default)]
    pub policies: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub lease_duration: u64,
    #[serde(default)]
    pub renewable: bool,
}

#[cfg(test)]
mod tests {
    use super::{ApiResponse, AuthResponse, GenericResponse};
    use serde_json::json;

    #[test]
    fn deserialize_generic_response() {
        let payload = json!({
            "renewable": false,
            "lease_id": "",
            "lease_duration": 0,
            "auth": null,
            "data": {
                "certificate": "-----BEGIN CERT-----",
                "serial_number": "01:02"
            }
        });

        let resp: GenericResponse = serde_json::from_value(payload).unwrap();
        assert!(!resp.renewable);
        let data = resp.data().unwrap();
        assert_eq!(data.get("serial_number").unwrap(), "01:02");
    }

    #[test]
    fn deserialize_with_auth() {
        let payload = json!({
            "renewable": true,
            "lease_id": "abc",
            "lease_duration": 60,
            "auth": {
                "client_token": "token",
                "policies": ["default"],
                "metadata": {"role": "test"},
                "lease_duration": 30,
                "renewable": true
            },
            "data": null
        });

        let resp: ApiResponse<()> = serde_json::from_value(payload).unwrap();
        assert!(resp.auth.is_some());
        let auth: AuthResponse = resp.auth.unwrap();
        assert_eq!(auth.client_token, "token");
        assert_eq!(auth.policies, vec!["default".to_string()]);
        assert_eq!(auth.metadata.get("role").unwrap(), "test");
        assert_eq!(auth.lease_duration, 30);
        assert!(auth.renewable);
    }
}
