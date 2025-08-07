pub mod basic;

use crate::models::{NodeInfo, PodInfo};

pub trait Algorithm: Clone {
    fn filter(nodes: Vec<NodeInfo>, pod: &PodInfo) -> Vec<NodeInfo>;
    fn grader(nodes: Vec<NodeInfo>, pod: &PodInfo) -> Vec<(NodeInfo, u64)>;
}
