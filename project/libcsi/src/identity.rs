//! CSI Identity service trait.
//!
//! The Identity service allows callers to discover plugin metadata and check
//! plugin health.  Every CSI plugin must implement this service.

use async_trait::async_trait;

use crate::error::CsiError;
use crate::types::{PluginCapability, PluginInfo};

/// Identity service — plugin discovery and health probing.
///
/// This maps to the standard CSI Identity service, simplified for RK8s.
#[async_trait]
pub trait CsiIdentity: Send + Sync {
    /// Return the plugin name and version.
    async fn get_plugin_info(&self) -> Result<PluginInfo, CsiError>;

    /// Liveness probe.  Returns `true` when the plugin is healthy.
    async fn probe(&self) -> Result<bool, CsiError>;

    /// Advertise the capabilities supported by this plugin.
    async fn get_plugin_capabilities(&self) -> Result<Vec<PluginCapability>, CsiError>;
}
