//! RKL overlay rootfs configuration.
//!
//! Controls whether persistent overlayfs mounts are used for container rootfs,
//! and which overlay backend (libfuse or Linux native) to use.
//!
//! Environment variables:
//! - `RKL_OVERLAY_ROOTFS`: set to `0` to disable overlay mode and fall back to traditional cp.
//!   Defaults to `1` (enabled).
//! - `RKL_USE_LIBFUSE`: set to `1` to use libfuse overlay backend, `0` for Linux native.
//!   Defaults to `0` (native). Both modes require root privileges.

use common::RegistryCredential;
use std::sync::{LazyLock, RwLock};

/// Global overlay configuration for RKL container rootfs.
pub struct OverlayConfig {
    /// Container rootfs mount mode: `true` = persistent overlay mount, `false` = traditional cp mode.
    pub use_overlay_rootfs: bool,
    /// Overlay backend: `true` = libfuse, `false` = Linux native.
    /// Both modes require root privileges.
    pub use_libfuse_overlay: bool,
}

/// Globally initialized overlay configuration, read from environment variables at first access.
pub static OVERLAY_CONFIG: LazyLock<OverlayConfig> = LazyLock::new(|| OverlayConfig {
    use_overlay_rootfs: std::env::var("RKL_OVERLAY_ROOTFS")
        .map(|v| v != "0")
        .unwrap_or(true),
    use_libfuse_overlay: std::env::var("RKL_USE_LIBFUSE")
        .map(|v| v == "1")
        .unwrap_or(false),
});

/// Registry credentials received from rks at node registration.
/// Used by `RkforgeImagePuller` to authenticate against OCI registries.
static REGISTRY_CREDENTIALS: RwLock<Vec<RegistryCredential>> = RwLock::new(Vec::new());

/// Replace the stored registry credentials with a new set received from rks.
pub fn set_registry_credentials(creds: Vec<RegistryCredential>) {
    if let Ok(mut store) = REGISTRY_CREDENTIALS.write() {
        *store = creds;
    }
}

/// Return a snapshot of all stored credentials.
pub fn get_all_registry_credentials() -> Vec<RegistryCredential> {
    REGISTRY_CREDENTIALS
        .read()
        .map(|store| store.clone())
        .unwrap_or_default()
}
