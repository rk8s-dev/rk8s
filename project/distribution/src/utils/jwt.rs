use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use crate::error::AppError;
use crate::utils::state::Config;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    sub: String,
    exp: i64,
}

pub fn encode(config: &Config, claims: &Claims) -> String {
    jsonwebtoken::encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    ).unwrap()
}

pub fn decode(config: &Config, token: &str) -> Result<Claims, AppError> {
    Ok(jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )?.claims)
}

pub fn gen_token(config: &Config, name: &str) -> String {
    let lifetime_seconds = std::env::var("JWT_LIFETIME_SECONDS")
        .unwrap_or("3600".into())
        .parse::<i64>()
        .unwrap();
    let claims = Claims {
        sub: name.to_string(),
        exp: (Utc::now() + Duration::seconds(lifetime_seconds)).timestamp()
    };
    encode(config, &claims)
}