use common::RksMessage;
use libcontainer::{container::Container, syscall::syscall::create_syscall};
use libruntime::rootpath;
use tracing::warn;
use uuid::Uuid;

use crate::{
    commands::pod::{PodInfo, TLSConnectionArgs},
    quic::client::{Cli, QUICClient},
};

#[derive(Debug)]
#[allow(unused)]
pub struct Pod {
    pub id: Uuid,
    pub name: String,
    pub namespace: String,
    pub containers: Vec<Container>,
    pub sandboxes: Vec<Container>,
}

impl Pod {
    pub fn get_container_by_id(&self, cid: &str) -> Option<&Container> {
        self.containers
            .iter()
            .chain(self.sandboxes.iter())
            .find(|container| container.state.id == cid)
            .map(|c| c as _)
    }
}

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
