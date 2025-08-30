use crate::domain::user_model::User;
use crate::error::{AppError, BusinessError, InternalError, MapToAppError};
use crate::utils::jwt::gen_token;
use crate::utils::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_extra::TypedHeader;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::Basic;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct UserReq {
    username: String,
    password: String,
}

pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UserReq>,
) -> Result<impl IntoResponse, AppError> {
    if req.username == "anonymous" {
        return Err(BusinessError::BadRequest(
            "`anonymous` is a reserved username, please change another one".to_string(),
        )
        .into());
    }
    match state.user_storage.query_user_by_name(&req.username).await {
        Ok(_) => Err(BusinessError::Conflict("username is already taken".to_string()).into()),
        Err(_) => {
            let password = hash_password(&state.config.password_salt, &req.password)?;
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
    auth: Option<TypedHeader<Authorization<Basic>>>,
) -> Result<impl IntoResponse, AppError> {
    let token = match auth {
        Some(auth) => {
            let username = auth.username();
            let user = state.user_storage.query_user_by_name(username).await?;
            let token = gen_token(&state.config.jwt_secret, &user.username);
            {
                let state = state.clone();
                // Check password is a rather time-consuming operation. So it should be executed in `spawn_blocking`
                tokio::task::spawn_blocking(move || {
                    check_password(&state.config.password_salt, &user, auth.password())
                })
                .await
                .map_err(|e| InternalError::Others(e.to_string()))??;
            }
            token
        }
        None => gen_token(&state.config.jwt_secret, "anonymous"),
    };
    Ok(Json(AuthRes {
        token: token.clone(),
        access_token: token,
        expires_in: state.config.jwt_lifetime_secs,
        issued_at: Utc::now().to_rfc3339(),
    }))
}

fn hash_password(salt: &str, password: &str) -> Result<String, AppError> {
    Ok(bcrypt::hash_with_salt(
        password,
        bcrypt::DEFAULT_COST,
        salt.as_bytes().try_into().unwrap(),
    )
    .map_to_internal()?
    .to_string())
}

fn check_password(salt: &str, user: &User, password: &str) -> Result<(), AppError> {
    let hash = hash_password(salt, password)?;
    if hash == user.password {
        return Ok(());
    }
    Err(BusinessError::BadRequest("invalid password".to_string()).into())
}
