//! `rusty_vault::modules` contains a set of real RustyVault modules. Each sub module needs to
//! implement the `rusty_vault::modules::Module` trait defined here and then the module
//! could be added to module manager.
//!
//! It's important for the developers who want to implement a new RustyVault module themselves to
//! get the `trait Module` implemented correctly.

use std::{any::Any, sync::Arc};

use crate::{core::Core, errors::RvError};

pub mod auth;
pub mod credential;
pub mod crypto;
pub mod kv;
pub mod pki;
pub mod policy;
pub mod system;

#[maybe_async::maybe_async]
pub trait Module: Any + Send + Sync {
    //! Description for a trait itself.
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
