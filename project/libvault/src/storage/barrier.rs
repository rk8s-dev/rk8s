//! Defines the `SecurityBarrier` trait for different barrier types.
//!
//! Specific barriers in RustyVault need to implement this trait and the `Storage` trait.
//!
//! It usually means a different symmetric encryption algorithm is going to be supported,
//! if a new barrier is under development.

use async_trait::async_trait;
use zeroize::Zeroizing;

use super::Storage;
use crate::errors::RvError;

pub const BARRIER_INIT_PATH: &str = "barrier/init";

#[async_trait]
pub trait SecurityBarrier: Storage + Send + Sync {
    async fn inited(&self) -> Result<bool, RvError>;
    async fn init(&self, key: &[u8]) -> Result<(), RvError>;
    fn generate_key(&self) -> Result<Zeroizing<Vec<u8>>, RvError>;
    fn key_length_range(&self) -> (usize, usize);
    fn sealed(&self) -> Result<bool, RvError>;
    async fn unseal(&self, key: &[u8]) -> Result<(), RvError>;
    fn seal(&self) -> Result<(), RvError>;
    fn derive_hmac_key(&self) -> Result<Vec<u8>, RvError>;
    fn as_storage(&self) -> &dyn Storage;
}
