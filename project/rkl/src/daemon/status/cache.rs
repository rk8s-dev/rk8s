use common::PodStatus;
use dashmap::DashMap;
use uuid::Uuid;

#[derive(Debug)]
pub struct CacheRecord {
    pub status: PodStatus,
    pub modified_time: std::time::Instant,
}

#[derive(Debug)]
pub struct PodStatusCache {
    map: DashMap<Uuid, CacheRecord>,
    global_time: std::time::Instant,
}

impl PodStatusCache {
    pub fn new() -> Self {
        Self {
            map: DashMap::new(),
            global_time: std::time::Instant::now(),
        }
    }

    pub async fn get(&self, pod_uid: Uuid) -> anyhow::Result<Option<PodStatus>> {
        // Implementation goes here
        todo!()
    }

    pub async fn set(&self, pod_uid: Uuid, status: PodStatus) -> anyhow::Result<bool> {
        // Implementation goes here
        todo!()
    }

    pub async fn remove(&self, pod_uid: Uuid) -> anyhow::Result<()> {
        // Implementation goes here
        todo!()
    }

    pub async fn get_newer_than(
        &self,
        pod_uid: Uuid,
        threshold: std::time::Duration,
    ) -> anyhow::Result<Option<PodStatus>> {
        // Implementation goes here
        todo!()
    }

    pub fn update_time(&mut self, time: std::time::Instant) {
        self.global_time = time;
    }
}
