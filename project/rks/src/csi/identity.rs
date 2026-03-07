//! CsiIdentity trait implementation for the RKS controller side.

use async_trait::async_trait;
use libcsi::{CsiError, CsiIdentity, PluginCapability, PluginInfo};

/// RKS-side CSI Identity implementation.
///
/// Reports controller-side capabilities. The Node side (RKL) has its own
/// Identity implementation reporting node capabilities.
pub struct RksCsiIdentity;

#[async_trait]
impl CsiIdentity for RksCsiIdentity {
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
        Ok(vec![PluginCapability::ControllerService])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plugin_info() {
        let identity = RksCsiIdentity;
        let info = identity.get_plugin_info().await.unwrap();
        assert_eq!(info.name, "rk8s.slayerfs.csi");
        assert!(!info.vendor_version.is_empty());
    }

    #[tokio::test]
    async fn plugin_capabilities() {
        let identity = RksCsiIdentity;
        let caps = identity.get_plugin_capabilities().await.unwrap();
        assert!(caps.contains(&PluginCapability::ControllerService));
    }

    #[tokio::test]
    async fn probe_returns_true() {
        let identity = RksCsiIdentity;
        assert!(identity.probe().await.unwrap());
    }
}
