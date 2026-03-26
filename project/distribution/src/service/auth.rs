use crate::error::{AppError, InternalError};
use crate::utils::jwt::gen_token;
use crate::utils::password::check_password;
use crate::utils::state::AppState;
use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use axum_extra::TypedHeader;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::Basic;
use chrono::Utc;
use serde::Serialize;
use std::sync::Arc;

fn canonical_namespace(value: &str) -> String {
    value.to_ascii_lowercase()
}

#[derive(Serialize)]
pub struct AuthResponse {
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
            let canonical_username = canonical_namespace(&user.username);
            let token = gen_token(
                state.config.jwt_lifetime_secs,
                &state.config.jwt_secret,
                &canonical_username,
            );
            {
                // Check password is a rather time-consuming operation. So it should be executed in `spawn_blocking`.
                tokio::task::spawn_blocking(move || {
                    check_password(&user.salt, &user.password, auth.password())
                })
                .await
                .map_err(|e| InternalError::Others(e.to_string()))??;
            }
            token
        }
        None => gen_token(
            state.config.jwt_lifetime_secs,
            &state.config.jwt_secret,
            "anonymous",
        ),
    };
    Ok(Json(AuthResponse {
        token: token.clone(),
        access_token: token,
        expires_in: state.config.jwt_lifetime_secs,
        issued_at: Utc::now().to_rfc3339(),
    }))
}

#[cfg(debug_assertions)]
#[derive(serde::Deserialize)]
pub struct CreateUserRequest {
    username: String,
    password: String,
}

#[cfg(debug_assertions)]
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    use crate::domain::user::User;
    use crate::utils::password::{gen_salt, hash_password};
    use rand::{Rng, SeedableRng};

    let mut rng = rand::rngs::StdRng::from_os_rng();

    let salt = gen_salt();
    let password = hash_password(&salt, &req.password)?;
    let canonical_username = canonical_namespace(&req.username);
    let user = User::new(rng.random(), canonical_username, password, salt);
    state.user_storage.create_user(user).await?;
    Ok(reqwest::StatusCode::CREATED)
}

#[cfg(test)]
mod tests {
    use super::canonical_namespace;

    #[test]
    fn canonical_namespace_is_lowercase() {
        assert_eq!(canonical_namespace("LingBou"), "lingbou");
    }
}
