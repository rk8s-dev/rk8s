//! Authentication service.

use crate::lock::RwLockExt;
use http::{header::AUTHORIZATION, HeaderValue, Request};
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};
use tower_service::Service;

#[derive(Debug, Clone)]
pub struct AuthService<S> {
    inner: S,
    token: Arc<RwLock<Option<HeaderValue>>>,
}

impl<S> AuthService<S> {
    #[inline]
    pub fn new(inner: S, token: Arc<RwLock<Option<HeaderValue>>>) -> Self {
        Self { inner, token }
    }
}

impl<S, Body, Response> Service<Request<Body>> for AuthService<S>
where
    S: Service<Request<Body>, Response = Response>,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    #[inline]
    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        if let Some(token) = self.token.read_unpoisoned().as_ref() {
            request.headers_mut().insert(AUTHORIZATION, token.clone());
        }

        self.inner.call(request)
    }
}
