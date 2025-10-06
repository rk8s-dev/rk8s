//! The `libvault::logical` is a low level module that defines 'backend' and relevant data
//! structures such as `Path`, `Request`, etc and traits.
//!
//! The term 'backend' is generic in RustyVault. It represents for a module that provides real
//! features, as what you can see in the modules directory. The `Backend` trait in this module is
//! designed to represent the concept of backend. Modules of RustyVault need to implement this
//! trait for their own types.
//!
//! Since modules may have common attributes, a specific data structure named `LogicalBackend` is
//! also defined in this module. Other RustyVault modules can instantiate a `LogicalBackend`
//! object and implement `libvault::logical::Backend` trait for it. Thus, this module can be
//! included in the API routing process.

use std::sync::Arc;

use async_trait::async_trait;
use enum_map::Enum;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::{context::Context, errors::RvError};

pub mod auth;
pub mod backend;
pub mod connection;
pub mod field;
pub mod lease;
pub mod path;
pub mod request;
pub mod response;
pub mod secret;

pub use auth::Auth;
pub use backend::{
    CTX_KEY_BACKEND_PATH, IntoPathArc, IntoSecretArc, LogicalBackend, LogicalBackendBuilder,
};
pub use connection::Connection;
pub use field::{Field, FieldBuilder, FieldType, FieldsBuilder, IntoFieldArc};
pub use lease::Lease;
pub use path::{Path, PathBuilder, PathOperation};
pub use request::Request;
pub use response::Response;
pub use secret::{Secret, SecretBuilder, SecretData};

#[derive(Eq, PartialEq, Copy, Clone, Debug, EnumString, Display, Enum, Serialize, Deserialize)]
pub enum Operation {
    #[strum(to_string = "list")]
    List,
    #[strum(to_string = "read")]
    Read,
    #[strum(to_string = "write")]
    Write,
    #[strum(to_string = "delete")]
    Delete,
    #[strum(to_string = "help")]
    Help,
    #[strum(to_string = "renew")]
    Renew,
    #[strum(to_string = "revoke")]
    Revoke,
    #[strum(to_string = "rollback")]
    Rollback,
}

#[async_trait]
pub trait Backend: Send + Sync {
    fn init(&mut self) -> Result<(), RvError>;
    fn setup(&self, key: &str) -> Result<(), RvError>;
    fn cleanup(&self) -> Result<(), RvError>;
    fn get_unauth_paths(&self) -> Option<Arc<Vec<String>>>;
    fn get_root_paths(&self) -> Option<Arc<Vec<String>>>;
    fn get_ctx(&self) -> Option<Arc<Context>>;
    async fn handle_request(&self, req: &mut Request) -> Result<Option<Response>, RvError>;
    fn secret(&self, key: &str) -> Option<&Arc<secret::Secret>>;
}
