use std::{collections::HashMap, sync::Arc, time::Duration};

use actix_web::{
    cookie::{time::OffsetDateTime, Cookie},
    http::{Method, StatusCode},
    web, HttpRequest, HttpResponse,
};
use humantime::parse_duration;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::AUTH_COOKIE_NAME;
use crate::{
    core::Core,
    errors::RvError,
    http::{request_auth, response_error, response_json_ok, response_ok, Connection},
    logical::{Connection as ReqConnection, Operation, Response},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Auth {
    client_token: String,
    policies: Vec<String>,
    metadata: HashMap<String, String>,
    lease_duration: u64,
    renewable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LogicalResponse {
    renewable: bool,
    lease_id: String,
    lease_duration: u64,
    auth: Option<Auth>,
    data: HashMap<String, Value>,
}

async fn logical_request_handler(
    req: HttpRequest,
    mut body: web::Bytes,
    method: Method,
    path: web::Path<String>,
    core: web::Data<Arc<Core>>,
) -> Result<HttpResponse, RvError> {
    let Some(conn) = req.conn_data::<Connection>() else {
        return Err(RvError::ErrRequestInvalid);
    };
    log::debug!("logical request, connection info: {conn:?}, method: {method:?}, path: {path:?}");

    let mut req_conn = ReqConnection::default();
    req_conn.peer_addr = conn.peer.to_string();
    if let Some(tls) = &conn.tls {
        req_conn.peer_tls_cert.clone_from(&tls.client_cert_chain);
    }

    let mut r = request_auth(&req);
    r.path.clone_from(&path.into_inner());
    r.connection = Some(req_conn);

    match method {
        Method::GET => {
            r.operation = Operation::Read;
        }
        Method::POST | Method::PUT => {
            r.operation = Operation::Write;
            if !body.is_empty() {
                let payload = serde_json::from_slice(&body)?;
                r.body = Some(payload);
                body.clear();
            }
        }
        Method::DELETE => {
            r.operation = Operation::Delete;
        }
        other => {
            if other.as_str() != "LIST" {
                return Ok(response_error(StatusCode::METHOD_NOT_ALLOWED, ""));
            }
            r.operation = Operation::List;
        }
    }
    #[cfg(feature = "sync_handler")]
    let ret = core.handle_request(&mut r)?;
    #[cfg(not(feature = "sync_handler"))]
    let ret = core.handle_request(&mut r).await?;

    match ret {
        Some(resp) => response_logical(&resp, &r.path),
        None => {
            if matches!(r.operation, Operation::Read | Operation::List) {
                return Ok(response_error(StatusCode::NOT_FOUND, ""));
            }
            Ok(response_ok(None, None))
        }
    }
}

fn response_logical(resp: &Response, path: &str) -> Result<HttpResponse, RvError> {
    let mut logical_resp = LogicalResponse::default();
    let mut cookie: Option<Cookie> = None;
    let mut no_content = true;

    if let Some(ref secret) = &resp.secret {
        logical_resp.lease_id.clone_from(&secret.lease_id);
        logical_resp.renewable = secret.lease.renewable;
        logical_resp.lease_duration = secret.lease.ttl.as_secs();
        no_content = false;
    }

    if let Some(ref auth) = &resp.auth {
        let mut expire_duration = parse_duration("365d")?;
        if logical_resp.lease_duration != 0 {
            expire_duration = Duration::from_secs(logical_resp.lease_duration);
        }

        if !path.starts_with("auth/token/") {
            let expire_time = OffsetDateTime::now_utc() + expire_duration;
            cookie = Some(Cookie::build(AUTH_COOKIE_NAME, &auth.client_token).path("/").expires(expire_time).finish());
        }

        logical_resp.auth = Some(Auth {
            client_token: auth.client_token.clone(),
            policies: auth.policies.clone(),
            metadata: auth.metadata.clone(),
            lease_duration: auth.ttl.as_secs(),
            renewable: auth.renewable(),
        });

        no_content = false;
    }

    if let Some(ref data) = &resp.data {
        logical_resp.data = data.iter().map(|(key, value)| (key.clone(), value.clone())).collect();

        no_content = false;
    }

    if no_content {
        Ok(response_ok(cookie, None))
    } else {
        Ok(response_json_ok(cookie, logical_resp))
    }
}

pub fn init_logical_service(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/v1").route("/{path:.*}", web::route().to(logical_request_handler)));
}
