use common::{PodTask, RksMessage};
use uuid::Uuid;

use crate::quic::client::{Daemon, QUICClient};

pub mod cache;
pub mod pleg;
pub mod pod;
pub mod status_manager;

pub async fn get_pod_by_uid(
    client: &QUICClient<Daemon>,
    uid: &Uuid,
) -> anyhow::Result<Option<PodTask>> {
    client.send_msg(&RksMessage::GetPodByUid(*uid)).await?;
    let pod = match client.fetch_msg().await? {
        RksMessage::GetPodByUidRes(res) => Some(*res),
        _ => None,
    };
    Ok(pod)
}
