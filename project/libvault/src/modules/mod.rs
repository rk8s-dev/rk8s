//! `libvault::modules` contains a set of real RustyVault modules. Each sub module needs to
//! implement the `libvault::modules::Module` trait defined here and then the module
//! could be added to module manager.
//!
//! It's important for the developers who want to implement a new RustyVault module themselves to
//! get the `trait Module` implemented correctly.

use crate::{core::Core, errors::RvError, logical::Request};
use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use std::{any::Any, sync::Arc};

pub mod auth;
pub mod credential;
pub mod crypto;
pub mod kv;
pub mod pki;
pub mod policy;
pub mod system;

/// Utilities and helper traits consumed by modules.
///
/// `RequestExt` and `ResponseExt` provide convenience methods for modules
/// to parse request payloads and convert response types into generic maps.
pub trait RequestExt {
    /// Parse the request's JSON body or data field into type `T`.
    fn parse_json<T>(&self) -> Result<T, RvError>
    where
        T: DeserializeOwned;
}

impl RequestExt for Request {
    fn parse_json<T>(&self) -> Result<T, RvError>
    where
        T: DeserializeOwned,
    {
        let map = if let Some(body) = self.body.clone() {
            body
        } else if let Some(data) = self.data.clone() {
            data
        } else {
            Map::new()
        };

        serde_json::from_value(Value::Object(map)).map_err(Into::into)
    }
}

pub trait ResponseExt {
    fn to_map(&self) -> Result<Option<Map<String, Value>>, RvError>;
}

impl<T> ResponseExt for T
where
    T: Serialize,
{
    fn to_map(&self) -> Result<Option<Map<String, Value>>, RvError> {
        match serde_json::to_value(self)? {
            Value::Object(map) => Ok(Some(map)),
            _ => Ok(None),
        }
    }
}

#[async_trait]
pub trait Module: Any + Send + Sync {
    //! Trait that every RustyVault runtime module must implement.
    //!
    //! Implementers provide a stable `name()` and lifecycle hooks used by
    //! `ModuleManager` to setup/init/cleanup modules.
    fn name(&self) -> String;

    fn as_any_arc(self: Arc<Self>) -> Arc<dyn Any + Send + Sync>;

    async fn init(&self, _core: &Core) -> Result<(), RvError> {
        Ok(())
    }

    fn setup(&self, _core: &Core) -> Result<(), RvError> {
        Ok(())
    }

    fn cleanup(&self, _core: &Core) -> Result<(), RvError> {
        Ok(())
    }
}
