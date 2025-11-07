//! This module manages all storage related code by defining a 'barrier' concept and a 'backend'
//! concept.
//!
//! Each different storage type needs to implement the `backend` trait to complete the support.
//!
//! Each barrier represents a specific cryptography method for encrypting or decrypting data before
//! the data connects to a specific backend. A barrier is defined by implementing the `SecurityBarrier`
//! trait.
//!
//! So one example of a whole data path could be something like this:
//!
//! HTTP API -> some module (e.g. KV) -> barrier -> backend -> real storage (file, MySQL...)
//!
//! Typical storage types may be direct file, databases, remote network filesystem and etc.
//! Different storage types are all as sub-module of this module.

use std::{any::Any, collections::HashMap, sync::Arc};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::errors::RvError;

pub mod barrier;
pub mod barrier_aes_gcm;
pub mod barrier_view;
pub mod physical;
pub mod xline;

/// A trait that abstracts core methods for all storage barrier types.
#[async_trait]
pub trait Storage: Send + Sync {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError>;
    async fn get(&self, key: &str) -> Result<Option<StorageEntry>, RvError>;
    async fn put(&self, entry: &StorageEntry) -> Result<(), RvError>;
    async fn delete(&self, key: &str) -> Result<(), RvError>;
    async fn lock(&self, _lock_name: &str) -> Result<Box<dyn Any>, RvError> {
        Ok(Box::new(true))
    }
}

/// This struct is used to describe a specific storage entry
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageEntry {
    pub key: String,
    pub value: Vec<u8>,
}

impl StorageEntry {
    pub fn new(k: &str, v: &impl Serialize) -> Result<StorageEntry, RvError> {
        let data = serde_json::to_string(v)?;

        Ok(StorageEntry {
            key: k.to_string(),
            value: data.into_bytes(),
        })
    }
}

#[async_trait]
pub trait Backend: Send + Sync {
    //! This trait describes the generic methods that a storage backend needs to implement.
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError>;
    async fn get(&self, key: &str) -> Result<Option<BackendEntry>, RvError>;
    async fn put(&self, entry: &BackendEntry) -> Result<(), RvError>;
    async fn delete(&self, key: &str) -> Result<(), RvError>;
    async fn lock(&self, _lock_name: &str) -> Result<Box<dyn Any>, RvError> {
        Ok(Box::new(true))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEntry {
    pub key: String,
    pub value: Vec<u8>,
}

/// this is a generic function that instantiates different storage backends.
pub fn new_backend(t: &str, conf: &HashMap<String, Value>) -> Result<Arc<dyn Backend>, RvError> {
    match t {
        "file" => {
            let backend = physical::file::FileBackend::new(conf)?;
            Ok(Arc::new(backend))
        }
        "xline" => {
            let backend = xline::XlineBackend::new(conf)?;
            Ok(Arc::new(backend))
        }
        "mock" => Ok(Arc::new(physical::mock::MockBackend::new())),
        _ => Err(RvError::ErrPhysicalTypeInvalid),
    }
}
