pub(crate) mod endpoint;
pub(crate) mod h3wrapper;
pub(crate) mod server;

pub(crate) type Router = axum::Router;
pub(crate) type StateRouter<T> = axum::Router<T>;
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

pub(crate) use http::HeaderValue;
pub(crate) use http_body::Body;
pub(crate) use server::{RouterBuilder, Server};
