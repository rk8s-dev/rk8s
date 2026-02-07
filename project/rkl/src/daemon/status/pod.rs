//! Local pod representation used by PLEG for container state diffing.
//!
//! This module bridges the gap between the rks API server's [`common::PodTask`]
//! view and the container runtime's on-disk state. [`get_pods`] fetches the pod
//! list from rks, then resolves each pod's actual containers and sandbox from
//! the local runtime root path, producing [`Pod`] values that PLEG can diff.

use common::RksMessage;
use libcontainer::{container::Container, syscall::syscall::create_syscall};
use libruntime::rootpath;
use tracing::warn;
use uuid::Uuid;

use crate::{
    commands::pod::{PodInfo, TLSConnectionArgs},
    quic::client::{Cli, QUICClient},
};

/// A snapshot of a pod on the local node, including its runtime containers and
/// sandbox(es).
///
/// Used exclusively by [`super::pleg::PLEG`] to compare successive snapshots
/// and detect container state transitions.
#[derive(Debug)]
#[allow(unused)]
pub struct Pod {
    /// Pod UID from the API server.
    pub id: Uuid,
    /// Pod name.
    pub name: String,
    /// Pod namespace.
    pub namespace: String,
    /// Application containers belonging to this pod.
    pub containers: Vec<Container>,
    /// Sandbox (pause) containers for this pod.
    pub sandboxes: Vec<Container>,
}

impl Pod {
    /// Returns a reference to the container (application or sandbox) with the
    /// given runtime ID, or `None` if not found.
    pub fn get_container_by_id(&self, cid: &str) -> Option<&Container> {
        self.containers
            .iter()
            .chain(self.sandboxes.iter())
            .find(|container| container.state.id == cid)
            .map(|c| c as _)
    }
}

/// Fetches all pods from the rks API server and resolves their local container
/// state from the runtime root path.
///
/// Pods whose local [`PodInfo`] cannot be loaded (e.g. not yet scheduled to
/// this node) are skipped with a warning log.
pub async fn get_pods(server_addr: &str, tls_cfg: &TLSConnectionArgs) -> anyhow::Result<Vec<Pod>> {
    // Implementation goes here
    let root_path = rootpath::determine(None, &*create_syscall())?;

    // get pod list from rks server
    let client = QUICClient::<Cli>::connect(server_addr, tls_cfg).await?;
    client.send_msg(&RksMessage::ListPod).await?;
    let server_pods = match client.fetch_msg().await? {
        RksMessage::ListPodRes(pods) => pods,
        msg => anyhow::bail!("unexpected response {:?} ", msg),
    };

    // convert to local Pod structs
    let mut pods = Vec::new();
    for server_pod in server_pods {
        let pod_info = PodInfo::load(&root_path, &server_pod.metadata.name).ok();
        if pod_info.is_none() {
            warn!("pod {} not found in local", server_pod.metadata.name);
            continue;
        }
        let pod_info = pod_info.unwrap();
        let containers = pod_info.get_pod_containers(&root_path)?;
        let sandbox = pod_info.get_pod_sandbox(&root_path)?;

        let pod = Pod {
            id: server_pod.metadata.uid,
            name: server_pod.metadata.name,
            namespace: server_pod.metadata.namespace,
            containers,
            sandboxes: vec![sandbox],
        };
        pods.push(pod);
    }

    Ok(pods)
}
