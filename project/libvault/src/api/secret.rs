use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{errors::RvError, rv_error_string};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Secret {
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub lease_id: String,
    #[serde(default)]
    pub lease_duration: u32,
    #[serde(default)]
    pub renewable: bool,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default)]
    pub auth: Option<SecretAuth>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecretAuth {
    #[serde(default)]
    pub client_token: String,
    #[serde(default)]
    pub accessor: String,
    #[serde(default)]
    pub policies: Vec<String>,
    #[serde(default)]
    pub token_policies: Vec<String>,
    #[serde(default)]
    pub identity_policies: Vec<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    #[serde(default)]
    pub entity_id: String,
    #[serde(default)]
    pub lease_duration: u64,
    #[serde(default)]
    pub renewable: bool,
}

impl Secret {
    pub fn token_is_renewable(&self) -> Result<bool, RvError> {
        if let Some(auth) = &self.auth {
            return Ok(auth.renewable);
        }

        let Some(renewable_value) = self.data.get("renewable") else {
            return Ok(false);
        };

        let Some(renewable) = renewable_value.as_bool() else {
            return Err(rv_error_string!("token id found but in the wrong format"));
        };

        Ok(renewable)
    }

    pub fn token_id(&self) -> Result<String, RvError> {
        if let Some(auth) = &self.auth {
            if !auth.client_token.is_empty() {
                return Ok(auth.client_token.clone());
            }
        }

        let Some(id_value) = self.data.get("id") else {
            return Ok("".into());
        };

        let Some(id) = id_value.as_str() else {
            return Err(rv_error_string!("token id found but in the wrong format"));
        };

        Ok(id.to_string())
    }

    pub fn token_accessor(&self) -> Result<String, RvError> {
        if let Some(auth) = &self.auth {
            if !auth.accessor.is_empty() {
                return Ok(auth.accessor.clone());
            }
        }

        let Some(accessor_value) = self.data.get("accessor") else {
            return Ok("".into());
        };

        let Some(accessor) = accessor_value.as_str() else {
            return Err(rv_error_string!("token accessor found but in the wrong format"));
        };

        Ok(accessor.to_string())
    }

    pub fn token_policies(&self) -> Result<Vec<String>, RvError> {
        if let Some(auth) = &self.auth {
            if !auth.policies.is_empty() {
                return Ok(auth.policies.clone());
            }
        }

        let Some(policies_value) = self.data.get("policies") else {
            return Ok(vec![]);
        };

        let Some(policies) = policies_value.as_array() else {
            return Err(rv_error_string!("token policies found but in the wrong format"));
        };

        Ok(policies.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
    }

    pub fn token_ttl(&self) -> Result<u64, RvError> {
        if let Some(auth) = &self.auth {
            return Ok(auth.lease_duration);
        }

        let Some(ttl_value) = self.data.get("ttl") else {
            return Ok(0);
        };

        let Some(ttl) = ttl_value.as_u64() else {
            return Err(rv_error_string!("token ttl found but in the wrong format"));
        };

        Ok(ttl)
    }

    pub fn token_metadata(&self) -> Result<HashMap<String, String>, RvError> {
        if let Some(auth) = &self.auth {
            if !auth.metadata.is_empty() {
                return Ok(auth.metadata.clone());
            }
        }

        if let Some(data) = self.data.get("metadata") {
            let Some(metadata) = data.as_object() else {
                return Err(rv_error_string!("token metadata found but in the wrong format"));
            };

            return Ok(metadata.into_iter().map(|(k, v)| (k.to_string(), value_to_string(v))).collect());
        }

        if let Some(data) = self.data.get("meta") {
            let Some(meta) = data.as_object() else {
                return Err(rv_error_string!("token meta found but in the wrong format"));
            };

            return Ok(meta.into_iter().map(|(k, v)| (k.to_string(), value_to_string(v))).collect());
        }

        Ok(HashMap::new())
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => "".to_string(),
    }
}
