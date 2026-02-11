//! Root of the probe subsystem responsible for container health checking.
//!
//! This module re-exports two sub-modules:
//! - [`probe_manager`]: Orchestrates probe lifecycle and result broadcasting.
//! - [`prober`]: Defines the [`Prober`] trait and concrete probe implementations
//!   (exec, HTTP GET, TCP socket).
//!
//! Probes run periodically and feed results back to the [`crate::daemon::pod_worker::PodWorker`] which uses
//! them to update container readiness and trigger restarts on liveness failure.

pub mod probe_manager;
pub mod prober;
