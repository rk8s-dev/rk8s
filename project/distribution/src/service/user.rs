use std::sync::Arc;
use axum::extract::{State};
use axum::http::StatusCode;
use axum::Json;
use axum::response::IntoResponse;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::Basic;
use axum_extra::TypedHeader;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use crate::config::Config;
use crate::error::AppError;
use crate::utils::jwt::gen_token;
use crate::domain::user_model::User;
use crate::utils::state::{AppState};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserReq {
    username: String,
    password: String,
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UserReq>,
) -> Result<impl IntoResponse, AppError> {
    match state.user_storage.query_user_by_name(&req.username).await {
        Ok(_) => Err(AppError::UsernameTaken(req.username)),
        Err(_) => {
            let password = hash_password(&state.config, &req.password)?;
            let user = User::new(req.username, password);
            state.user_storage.insert_user(user).await?;
            Ok(StatusCode::CREATED)
        }
    }
}


#[derive(Serialize)]
pub struct AuthRes {
    token: String,
    #[serde(rename = "access_token")]
    access_token: String,
    #[serde(rename = "expires_in")]
    expires_in: i64,
    #[serde(rename = "issued_at")]
    issued_at: String,
}

pub(crate) async fn auth(
    State(state): State<Arc<AppState>>,
    TypedHeader(auth): TypedHeader<Authorization<Basic>>,
) -> Result<impl IntoResponse, AppError> {
    let username = auth.username();
    let user = state.user_storage
        .query_user_by_name(username)
        .await?;
    let token = gen_token(&state.config, &user.username);
    {
        let state = state.clone();
        // Check password is a rather time-consuming operation. So it should be executed in `spawn_blocking`
        tokio::task::spawn_blocking(move || check_password(&state.config, &user, auth.password()))
            .await
            .map_err(|e| AppError::Others(e.to_string()))??;
    }
    let issued_at = Utc::now().to_rfc3339();
    Ok((
        StatusCode::OK,
        Json(AuthRes {
            token: token.clone(),
            access_token: token,
            expires_in: state.config.jwt_lifetime_secs,
            issued_at,
        })
    ))
}

fn hash_password(config: &Config, password: &str) -> Result<String, AppError> {
    Ok(bcrypt::hash_with_salt(
        password,
        bcrypt::DEFAULT_COST,
        config.password_salt.as_bytes().try_into().unwrap(),
    )?.to_string())
}

fn check_password(config: &Config, user: &User, password: &str) -> Result<(), AppError> {
    let hash = hash_password(config, password)?;
    if hash == user.password {
        return Ok(());
    }
    Err(AppError::InvalidPassword)
}