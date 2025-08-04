use std::cmp::Ordering;

use tokio::time::Instant;

#[derive(Clone)]
pub struct PodInfo {
    pub name: String,
    /// CPU resource limits, measured in millicores.
    pub cpu: u64,
    /// Memory resource limits, measured in bytes.
    pub memory: u64,
    /// Priority to the scheduler.
    pub priority: u64,
    /// Scheduling failed attempts.
    pub attempts: usize,
    pub scheduled: Option<String>,
}

impl PartialEq for PodInfo {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority
    }
}

impl Eq for PodInfo {}

impl PartialOrd for PodInfo {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PodInfo {
    fn cmp(&self, other: &Self) -> Ordering {
        self.priority.cmp(&other.priority)
    }
}

#[derive(Clone)]
pub struct NodeInfo {
    pub name: String,
    /// Available CPU resources on the node, measured in millicores.
    pub cpu: u64,
    /// Available memory resources on the node, measured in bytes.
    pub memory: u64,
}

pub type PodNameWithPriority = (u64, String);
pub struct BackOffPod {
    pub pod: PodNameWithPriority,
    pub expire: Instant,
}

impl PartialEq for BackOffPod {
    fn eq(&self, other: &Self) -> bool {
        self.expire == other.expire
    }
}

impl Eq for BackOffPod {}

impl PartialOrd for BackOffPod {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BackOffPod {
    fn cmp(&self, other: &Self) -> Ordering {
        other.expire.cmp(&self.expire)
    }
}

pub struct Assignment {
    pub pod_name: String,
    pub node_name: String,
}
