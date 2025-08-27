use crate::algorithms::Algorithm;
use crate::models::NodeInfo;
use crate::models::PodInfo;
use std::cmp::Ordering::{Equal, Greater, Less};

#[derive(Clone)]
pub struct BasicAlgorithm {}

/// Basic scheduling algorithm: assign pod to nodes with smallest but enough cpu,
/// if cpu is same, prefer smaller memory.
impl Algorithm for BasicAlgorithm {
    fn filter(nodes: Vec<NodeInfo>, pod: &PodInfo) -> Vec<NodeInfo> {
        nodes
            .into_iter()
            .filter(|n| n.cpu >= pod.cpu && n.memory >= pod.memory)
            .collect()
    }

    fn grader(mut nodes: Vec<NodeInfo>, _pod: &PodInfo) -> Vec<(NodeInfo, u64)> {
        if nodes.is_empty() {
            return Vec::new();
        }

        let cmp = |a: &NodeInfo, b: &NodeInfo| match (a.cpu).cmp(&b.cpu) {
            Less => Less,
            Greater => Greater,
            Equal => a.memory.cmp(&b.memory),
        };
        nodes.sort_by(cmp);

        let mut cur = nodes.len() as u64;
        let mut res = Vec::new();
        let mut lst = None;
        nodes.into_iter().for_each(|n| {
            if let Some(l) = &lst
                && let Less | Greater = cmp(l, &n)
            {
                cur -= 1;
            }
            lst = Some(n.clone());
            res.push((n, cur));
        });
        res
    }
}
