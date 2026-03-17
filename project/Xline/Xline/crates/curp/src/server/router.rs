use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;

use crate::rpc::CurpError;

use super::{CurpServer, CurpServiceRegistry};

/// Curp request router
/// Used to route requests to the appropriate service based on request type
pub struct CurpRouter {
    /// Service Registry
    registry: Arc<CurpServiceRegistry>,
}

impl CurpRouter {
    /// Create a new router
    pub fn new(registry: Arc<CurpServiceRegistry>) -> Self {
        Self {
            registry,
        }
    }
    

    
    /// Get service registry
    pub fn registry(&self) -> &Arc<CurpServiceRegistry> {
        &self.registry
    }
}