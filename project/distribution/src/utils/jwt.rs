use crate::error::{AppError, OciError};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
}

pub fn encode(secret: impl AsRef<str>, claims: &Claims) -> String {
    jsonwebtoken::encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_ref().as_bytes()),
    )
    .unwrap()
}

pub fn decode(secret: impl AsRef<str>, token: impl AsRef<str>) -> Result<Claims, AppError> {
    Ok(jsonwebtoken::decode::<Claims>(
        token.as_ref(),
        &DecodingKey::from_secret(secret.as_ref().as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| OciError::Unauthorized {
        msg: e.to_string(),
        auth_url: None,
    })?
    .claims)
}

pub fn gen_token(lifetime_secs: i64, secret: impl AsRef<str>, id: impl AsRef<str>) -> String {
    let claims = Claims {
        sub: id.as_ref().to_string(),
        exp: (Utc::now() + Duration::seconds(lifetime_secs)).timestamp(),
    };
    encode(secret, &claims)
}
