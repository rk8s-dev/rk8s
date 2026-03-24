//! CSI Controller integration for RKS.
//!
//! This module implements the control-plane side of the Container Storage
//! Interface: volume provisioning/deprovisioning (`CsiController`), plugin
//! identity (`CsiIdentity`), and xline-backed volume metadata storage.

pub mod controller;
pub mod orchestrator;
pub mod volume_store;

pub use controller::RksCsiController;
pub use orchestrator::VolumeOrchestrator;
pub use volume_store::VolumeStore;
