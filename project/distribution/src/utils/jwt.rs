use crate::error::{AppError, OciError};
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
}

pub fn encode(secret: &str, claims: &Claims) -> String {
    jsonwebtoken::encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap()
}

pub fn decode(secret: &str, token: &str) -> Result<Claims, AppError> {
    Ok(jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| OciError::Unauthorized(e.to_string(), None))?
    .claims)
}

pub fn gen_token(secret: &str, name: &str) -> String {
    let lifetime_seconds = std::env::var("JWT_LIFETIME_SECONDS")
        .unwrap_or("3600".into())
        .parse::<i64>()
        .unwrap();
    let claims = Claims {
        sub: name.to_string(),
        exp: (Utc::now() + Duration::seconds(lifetime_seconds)).timestamp(),
    };
    encode(secret, &claims)
}
