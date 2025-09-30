//! This module handles almost everything related to RustyVault's HTTP(S) server, including basic
//! connection, HTTP request reading, HTTP response writing, data encoding/decoding, TLS stuffs, etc.
//! This module utilize `actix_web` crate as the underlying provider.

use std::{any::Any, net::SocketAddr, sync::Arc};

use actix_tls::accept::openssl::TlsStream;
use actix_web::{
    cookie::Cookie,
    dev::Extensions,
    http::{header, StatusCode},
    rt::net::TcpStream,
    web, HttpRequest, HttpResponse, ResponseError,
};
use openssl::x509::{X509Ref, X509VerifyResult, X509};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::{core::Core, errors::RvError, logical::Request};

pub mod logical;
pub mod metrics;
pub mod sys;

pub const AUTH_COOKIE_NAME: &str = "token";
pub const AUTH_HEADER_NAME: &str = "X-RustyVault-Token";
pub const VAULT_AUTH_HEADER_NAME: &str = "X-Vault-Token";

#[derive(Debug, Clone)]
pub struct TlsClientInfo {
    pub client_cert_chain: Option<Vec<X509>>,
    pub client_verify_result: X509VerifyResult,
}

impl TlsClientInfo {
    pub fn new() -> Self {
        TlsClientInfo { client_cert_chain: None, client_verify_result: X509VerifyResult::OK }
    }
}

#[derive(Debug, Clone)]
pub struct Connection {
    pub bind: SocketAddr,
    pub peer: SocketAddr,
    pub ttl: Option<u32>,
    pub tls: Option<TlsClientInfo>,
}

impl Connection {
    pub fn new() -> Self {
        Self {
            bind: SocketAddr::from(([0, 0, 0, 0], 8080)),
            peer: SocketAddr::from(([127, 0, 0, 1], 8888)),
            ttl: None,
            tls: None,
        }
    }
}

pub fn request_on_connect_handler(conn: &dyn Any, ext: &mut Extensions) {
    if let Some(tls_stream) = conn.downcast_ref::<TlsStream<TcpStream>>() {
        let socket = tls_stream.get_ref();
        let mut cert_chain = None;

        let peer_addr = socket.peer_addr();
        if peer_addr.is_err() {
            return;
        }

        if let Some(cert_stack) = tls_stream.ssl().peer_cert_chain() {
            let certs: Vec<X509> = cert_stack.iter().map(X509Ref::to_owned).collect();
            cert_chain = Some(certs);
        }

        if let Some(cert) = tls_stream.ssl().peer_certificate() {
            if let Some(ref mut chain) = cert_chain {
                chain.push(cert);
            } else {
                cert_chain = Some(vec![cert]);
            }
        }

        ext.insert(Connection {
            bind: socket.local_addr().unwrap(),
            peer: peer_addr.unwrap(),
            ttl: socket.ttl().ok(),
            tls: Some(TlsClientInfo {
                client_cert_chain: cert_chain,
                client_verify_result: tls_stream.ssl().verify_result(),
            }),
        });
    } else if let Some(socket) = conn.downcast_ref::<TcpStream>() {
        let peer_addr = socket.peer_addr();
        if peer_addr.is_err() {
            return;
        }

        ext.insert(Connection {
            bind: socket.local_addr().unwrap(),
            peer: peer_addr.unwrap(),
            ttl: socket.ttl().ok(),
            tls: None,
        });
    } else {
        unreachable!("socket should be TLS or plaintext");
    }
}

pub fn init_service(cfg: &mut web::ServiceConfig) {
    sys::init_sys_service(cfg);
    logical::init_logical_service(cfg);
    metrics::init_metrics_service(cfg);
}

impl ResponseError for RvError {
    // builds the actual response to send back when an error occurs
    fn error_response(&self) -> HttpResponse {
        let mut status = self.response_status();
        let text: String;
        if let RvError::ErrResponse(resp_text) = self {
            status = StatusCode::from_u16(400).unwrap();
            text = resp_text.clone();
        } else if let RvError::ErrResponseStatus(status_code, resp_text) = self {
            status = StatusCode::from_u16(*status_code).unwrap();
            text = resp_text.clone();
        } else {
            text = self.to_string();
        }
        HttpResponse::build(status).json(json!({ "error": text }))
    }
}

pub fn get_token_from_req(req: &HttpRequest) -> Result<String, RvError> {
    if let Some(token) = req.headers().get(AUTH_HEADER_NAME) {
        return Ok(token.to_str()?.to_string());
    } else if let Some(vault_token) = req.headers().get(VAULT_AUTH_HEADER_NAME) {
        return Ok(vault_token.to_str()?.to_string());
    } else if let Some(auth) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Ok(auth_str.trim_start_matches("Bearer ").to_string());
            }
        }
    } else if let Some(cookie_token) = req.cookie(AUTH_COOKIE_NAME) {
        return Ok(cookie_token.value().to_string());
    }

    Err(RvError::ErrResponse("missing client token".to_string()))
}

pub fn request_auth(req: &HttpRequest) -> Request {
    let mut r = Request::default();
    if let Ok(token) = get_token_from_req(req) {
        r.client_token = token;
    }
    r
}

pub fn response_error(status: StatusCode, msg: &str) -> HttpResponse {
    if msg.is_empty() {
        HttpResponse::build(status).finish()
    } else {
        let err_json = json!({ "error": msg.to_string() });
        HttpResponse::build(status).json(err_json)
    }
}

pub fn response_ok(cookie: Option<Cookie>, body: Option<&Map<String, Value>>) -> HttpResponse {
    if body.is_none() {
        let mut resp = HttpResponse::NoContent();
        if cookie.is_some() {
            resp.cookie(cookie.unwrap());
        }
        resp.finish()
    } else {
        let mut resp = HttpResponse::Ok();
        if cookie.is_some() {
            resp.cookie(cookie.unwrap());
        }
        resp.json(body.as_ref().unwrap())
    }
}

pub fn response_json<T: Serialize>(status: StatusCode, cookie: Option<Cookie>, body: T) -> HttpResponse {
    let mut resp = HttpResponse::build(status);
    if cookie.is_some() {
        resp.cookie(cookie.unwrap());
    }
    resp.json(body)
}

pub fn response_json_ok<T: Serialize>(cookie: Option<Cookie>, body: T) -> HttpResponse {
    response_json(StatusCode::OK, cookie, body)
}

pub async fn handle_request(core: web::Data<Arc<Core>>, req: &mut Request) -> Result<HttpResponse, RvError> {
    #[cfg(feature = "sync_handler")]
    let resp = core.handle_request(req)?;
    #[cfg(not(feature = "sync_handler"))]
    let resp = core.handle_request(req).await?;
    if resp.is_none() {
        Ok(response_ok(None, None))
    } else {
        let data = resp.unwrap().data;
        if data.is_none() {
            Ok(response_ok(None, None))
        } else {
            Ok(response_ok(None, data.as_ref()))
        }
    }
}
