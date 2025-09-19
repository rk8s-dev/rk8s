pub mod middleware;
pub mod v2;

use crate::api::middleware::{
    authorize_repository_access, populate_oci_claims, require_authentication,
};
use crate::api::v2::probe;
use crate::domain::user::UserRepository;
use crate::error::{AppError, OciError};
use crate::service::auth::{auth, client_id, oauth_callback};
use crate::service::repo::{change_visibility, list_visible_repos};
use crate::utils::jwt::{Claims, decode};
use crate::utils::password::check_password;
use crate::utils::state::AppState;
use axum::Router;
use axum::extract::OptionalFromRequestParts;
use axum::http::request::Parts;
use axum::routing::{get, post, put};
use axum_extra::TypedHeader;
use axum_extra::headers::Authorization;
use axum_extra::headers::authorization::{Basic, Bearer};
use std::sync::Arc;

pub fn create_router(state: Arc<AppState>) -> Router<()> {
    // we need to handle both /v2 and /v2/
    let mut router = Router::new()
        .route("/v2/", get(probe))
        .nest("/v2", v2::create_v2_router(state.clone()))
        .nest("/api/v1", custom_v1_router(state.clone()))
        .route("/auth/token", get(auth));

    #[cfg(debug_assertions)]
    {
        router = router.nest("/debug", debug_router(state.clone()));
    }
    router.with_state(state)
}

fn custom_v1_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/{provider}/callback", post(oauth_callback))
        .route("/auth/{provider}/client_id", get(client_id))
        .merge(v1_router_with_auth(state))
}

fn v1_router_with_auth(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route(
            "/repo",
            get(list_visible_repos).layer(axum::middleware::from_fn_with_state(
                state.clone(),
                require_authentication,
            )),
        )
        .route(
            "/{*tail}",
            put(change_visibility)
                .layer(axum::middleware::from_fn_with_state(
                    state.clone(),
                    authorize_repository_access,
                ))
                .layer(axum::middleware::from_fn_with_state(
                    state,
                    populate_oci_claims,
                )),
        )
}

#[cfg(debug_assertions)]
fn debug_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    use crate::service::auth::create_user;

    Router::new()
        .route("/users", post(create_user))
        .with_state(state)
}

pub enum AuthHeader {
    Bearer(TypedHeader<Authorization<Bearer>>),
    Basic(TypedHeader<Authorization<Basic>>),
}

impl<S> OptionalFromRequestParts<S> for AuthHeader
where
    S: Send + Sync,
{
    type Rejection = ();

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Option<Self>, Self::Rejection> {
        if let Ok(header) =
            <TypedHeader<_> as OptionalFromRequestParts<_>>::from_request_parts(parts, state).await
            && let Some(header) = header
        {
            return Ok(Some(Self::Bearer(header)));
        };
        if let Ok(header) =
            <TypedHeader<_> as OptionalFromRequestParts<_>>::from_request_parts(parts, state).await
            && let Some(header) = header
        {
            return Ok(Some(Self::Basic(header)));
        };
        Ok(None)
    }
}

async fn extract_claims(
    auth: Option<AuthHeader>,
    jwt_secret: impl AsRef<str>,
    user_storage: &dyn UserRepository,
    auth_url: impl AsRef<str>,
) -> Result<Claims, AppError> {
    match auth {
        Some(auth) => match auth {
            AuthHeader::Bearer(bearer) => decode(jwt_secret, bearer.token()),
            AuthHeader::Basic(basic) => {
                let user = user_storage.query_user_by_name(basic.username()).await?;
                check_password(&user.salt, &user.password, basic.password())?;
                Ok(Claims {
                    sub: basic.username().to_string(),
                    exp: 0,
                })
            }
        },
        None => Err(OciError::Unauthorized {
            msg: "Missing `authorization` header or invalid `authorization` header".to_string(),
            auth_url: Some(auth_url.as_ref().to_string()),
        }
        .into()),
    }
}
