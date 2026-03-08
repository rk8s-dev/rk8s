//! [`CsiIdentity`] implementation for the RKL (Node) side.
//!
//! Reports the same plugin name as the RKS controller but advertises no
//! controller-side capabilities — the Node plugin only handles mount operations.

use async_trait::async_trait;

use libcsi::{CsiError, CsiIdentity, PluginCapability, PluginInfo};

/// Node-side CSI Identity service.
pub struct RklCsiIdentity;

#[async_trait]
impl CsiIdentity for RklCsiIdentity {
    async fn get_plugin_info(&self) -> Result<PluginInfo, CsiError> {
        Ok(PluginInfo {
            name: "rk8s.slayerfs.csi".to_owned(),
            vendor_version: env!("CARGO_PKG_VERSION").to_owned(),
        })
    }

    async fn probe(&self) -> Result<bool, CsiError> {
        Ok(true)
    }

    async fn get_plugin_capabilities(&self) -> Result<Vec<PluginCapability>, CsiError> {
        // Node side does not provide ControllerService.
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plugin_info() {
        let id = RklCsiIdentity;
        let info = id.get_plugin_info().await.unwrap();
        assert_eq!(info.name, "rk8s.slayerfs.csi");
        assert!(!info.vendor_version.is_empty());
    }

    #[tokio::test]
    async fn probe_returns_true() {
        assert!(RklCsiIdentity.probe().await.unwrap());
    }

    #[tokio::test]
    async fn no_controller_capabilities() {
        let caps = RklCsiIdentity.get_plugin_capabilities().await.unwrap();
        assert!(caps.is_empty());
    }
}
