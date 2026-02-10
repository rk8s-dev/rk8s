//! Pod status tracking and reporting subsystem for the rkl daemon.
//!
//! This module is the node-level status pipeline that mirrors the kubelet's
//! status machinery. It continuously detects container state changes on the
//! local node and synchronises them back to the rks API server.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────┐   lifecycle    ┌────────────┐   set_pod_status   ┌────────────────┐
//!  │   PLEG   │──  events  ──▶│ PodWorker   │──────────────────▶│ StatusManager   │
//!  └──────────┘               └────────────┘                    └───────┬────────┘
//!       ▲                          ▲                                    │
//!       │ relist                   │ probe results                     │ sync
//!       │                    ┌─────┴──────┐                            ▼
//!  local containers          │ProbeManager│                      rks API server
//!                            └────────────┘
//! ```
//!
//! * **[`pleg`]** — Pod Lifecycle Event Generator. Periodically relists local
//!   containers and compares their states to the previous snapshot, emitting
//!   [`pleg::PodLifecycleEvent`]s for every change.
//! * **[`status_manager`]** — Caches the authoritative pod status on this node,
//!   deduplicates updates, and batches them to the rks API server over QUIC.
//! * **[`probe`]** — Health-check probing subsystem (liveness / readiness /
//!   startup probes) that feeds results back into the `PodWorker`.
//! * **[`pod`]** — Local pod representation that aggregates a pod's containers
//!   and sandboxes from the container runtime.
//!
//! The upstream [`super::pod_worker::PodWorker`] consumes PLEG events and probe
//! results, applies them to the cached [`common::PodStatus`], and writes the
//! result through the [`status_manager::StatusManager`].
//!
//! # Helper
//!
//! [`get_pod_by_uid`] is a thin convenience wrapper that fetches a single
//! [`common::PodTask`] from the rks API server by UID.

use common::{PodTask, RksMessage};
use uuid::Uuid;

use crate::quic::client::{Cli, QUICClient};

pub mod pleg;
pub mod pod;
pub mod probe;
pub mod status_manager;

/// Fetches a [`PodTask`] from the rks API server by its unique identifier.
///
/// Sends a [`RksMessage::GetPodByUid`] request over the given QUIC client and
/// returns `Ok(Some(pod))` when the server responds with
/// [`RksMessage::GetPodByUidRes`], or `Ok(None)` for any other response type.
pub async fn get_pod_by_uid(
    client: &QUICClient<Cli>,
    uid: &Uuid,
) -> anyhow::Result<Option<PodTask>> {
    client.send_msg(&RksMessage::GetPodByUid(*uid)).await?;
    let pod = match client.fetch_msg().await? {
        RksMessage::GetPodByUidRes(res) => Some(*res),
        _ => None,
    };
    Ok(pod)
}
