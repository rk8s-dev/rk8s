use crate::domain::user::User;
use crate::error::{AppError, BusinessError, MapToAppError};
use crate::utils::password::{gen_password, gen_salt, hash_password};
use crate::utils::state::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use reqwest::StatusCode;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct OAuthCallbackParams {
    code: String,
}

#[tracing::instrument(skip_all)]
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(params): Query<OAuthCallbackParams>,
) -> Result<impl IntoResponse, AppError> {
    match provider.as_str() {
        "github" => {
            let req = request_access_token(
                &params.code,
                &state.config.github_client_id,
                &state.config.github_client_secret,
            )
            .await
            .map_to_internal()?;
            tracing::error!("{:#?}", req);

            let user_info = request_user_info(&req.access_token)
                .await
                .map_to_internal()?;

            match state
                .user_storage
                .query_user_by_github_id(user_info.id)
                .await
            {
                Ok(_) => Ok((
                    StatusCode::OK,
                    Json(json!({
                        "msg": "The user has been registered",
                    })),
                )),
                Err(_) => {
                    let salt = gen_salt();
                    let original_password = gen_password();
                    let hashed = hash_password(&salt, &original_password)?;

                    let user = User::new(user_info.id, user_info.login, hashed, salt);
                    state.user_storage.create_user(user).await?;
                    Ok((
                        StatusCode::CREATED,
                        Json(json!({
                            "pat": original_password,
                        })),
                    ))
                }
            }
        }
        _ => Err(BusinessError::BadRequest("Only support github provider".to_string()).into()),
    }
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct RequestAccessTokenRes {
    #[serde(rename = "access_token")]
    access_token: String,
    #[serde(rename = "token_type")]
    token_type: String,
    #[serde(rename = "scope")]
    scope: String,
}

#[tracing::instrument(skip_all)]
async fn request_access_token(
    code: &str,
    client_id: &str,
    client_secret: &str,
) -> Result<RequestAccessTokenRes, reqwest::Error> {
    let mut params = HashMap::new();
    params.insert("code", code);
    params.insert("client_id", client_id);
    params.insert("client_secret", client_secret);

    let client = reqwest::Client::new();
    let res = client
        .post("https://github.com/login/oauth/access_token")
        .form(&params)
        .header("Accept", "application/json")
        .send()
        .await?;
    res.json().await
}

#[derive(Deserialize, Debug)]
pub struct UserInfo {
    login: String,
    id: i64,
}

#[tracing::instrument(skip_all)]
async fn request_user_info(access_token: &str) -> Result<UserInfo, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .get("https://api.github.com/user")
        .header("User-Agent", "distribution")
        .header("Authorization", format!("token {access_token}"))
        .send()
        .await?;
    res.json().await
}
