//! [`CsiNode`] trait implementation for the RKL worker node.
//!
//! Delegates all operations to [`SlayerFsOperator`], providing idempotent
//! semantics as required by the CSI specification.

use std::sync::Arc;

use async_trait::async_trait;

use libcsi::{
    CsiError, CsiNode, NodeInfo, NodePublishVolumeRequest, NodeStageVolumeRequest, VolumeId,
};

use super::operator::SlayerFsOperator;

/// Maximum number of volumes a single node can host.
const MAX_VOLUMES_PER_NODE: u64 = 256;

/// CSI Node service backed by [`SlayerFsOperator`].
pub struct CsiNodeService {
    operator: Arc<SlayerFsOperator>,
}

impl CsiNodeService {
    pub fn new(operator: Arc<SlayerFsOperator>) -> Self {
        Self { operator }
    }
}

#[async_trait]
impl CsiNode for CsiNodeService {
    async fn stage_volume(&self, req: NodeStageVolumeRequest) -> Result<(), CsiError> {
        self.operator.stage_volume(req).await
    }

    async fn unstage_volume(
        &self,
        volume_id: &VolumeId,
        staging_target_path: &str,
    ) -> Result<(), CsiError> {
        self.operator
            .unstage_volume(volume_id, staging_target_path)
            .await
    }

    async fn publish_volume(&self, req: NodePublishVolumeRequest) -> Result<(), CsiError> {
        self.operator.publish_volume(req).await
    }

    async fn unpublish_volume(
        &self,
        volume_id: &VolumeId,
        target_path: &str,
    ) -> Result<(), CsiError> {
        self.operator.unpublish_volume(volume_id, target_path).await
    }

    async fn get_info(&self) -> Result<NodeInfo, CsiError> {
        Ok(NodeInfo {
            node_id: self.operator.node_id().to_owned(),
            max_volumes: MAX_VOLUMES_PER_NODE,
            accessible_topology: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slayerfs::ChunkLayout;

    #[tokio::test]
    async fn get_info_returns_node_id() {
        let tmp = tempfile::tempdir().unwrap();
        let op = Arc::new(
            SlayerFsOperator::new(
                tmp.path().join("data"),
                tmp.path().join("state"),
                ChunkLayout::default(),
                "test-node".to_owned(),
            )
            .unwrap(),
        );
        let svc = CsiNodeService::new(op);
        let info = svc.get_info().await.unwrap();
        assert_eq!(info.node_id, "test-node");
        assert_eq!(info.max_volumes, MAX_VOLUMES_PER_NODE);
    }
}
