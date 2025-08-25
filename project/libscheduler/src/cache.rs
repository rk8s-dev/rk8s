use std::collections::HashMap;

use crate::models::{NodeInfo, PodInfo, PodNameWithPriority};

/// A shortcut of the node status.
pub struct Cache {
    pods: HashMap<String, PodInfo>,
    nodes: HashMap<String, NodeInfo>,
}

/// Cache stores the cluster state in Xline.
/// Please remember to update the data in the cache
/// whenever you receive status updates about pods running on nodes.
impl Cache {
    pub fn new() -> Self {
        Cache {
            pods: HashMap::new(),
            nodes: HashMap::new(),
        }
    }

    pub fn add_fail(&mut self, pod_name: &str) -> bool {
        if !self.pods.contains_key(pod_name) {
            return false;
        }
        self.pods
            .entry(pod_name.to_string())
            .and_modify(|p| p.queued_info.attempts += 1);
        true
    }

    pub fn assign(&mut self, pod_name: &str, node_name: &str) -> bool {
        let pod_info = if let Some(pod) = self.pods.get_mut(pod_name) {
            pod
        } else {
            return false;
        };
        let node = if let Some(node) = self.nodes.get_mut(node_name) {
            node
        } else {
            return false;
        };
        pod_info.scheduled = Some(node_name.to_owned());
        node.cpu -= pod_info.spec.resources.cpu;
        node.memory -= pod_info.spec.resources.memory;
        true
    }

    pub fn update_pod(&mut self, pod: PodInfo) -> Option<PodInfo> {
        self.pods.insert(pod.name.clone(), pod)
    }

    pub fn remove_pod(&mut self, pod_name: &str) {
        if let Some(p) = self.pods.get(pod_name) {
            if let Some(n) = &p.scheduled {
                let node = self.nodes.get_mut(n);
                if let Some(node) = node {
                    node.cpu += p.spec.resources.cpu;
                    node.memory += p.spec.resources.memory;
                }
            }
        }
        self.pods.remove(pod_name);
    }

    pub fn pop_pod_on_node(&mut self, node_name: &str) -> Vec<PodNameWithPriority> {
        let mut res = Vec::new();
        self.pods
            .values_mut()
            .filter(|p| matches!(&p.scheduled, Some(name) if name == node_name))
            .for_each(|p| {
                p.scheduled = None;
                p.queued_info.attempts = 0;
                res.push((p.spec.priority, p.name.clone()));
            });
        res
    }

    pub fn update_node(&mut self, node: NodeInfo) {
        self.nodes.insert(node.name.clone(), node);
    }

    pub fn remove_node(&mut self, node_name: &str) {
        self.nodes.remove(node_name);
    }

    pub fn get_nodes(&self) -> Vec<NodeInfo> {
        self.nodes.values().cloned().collect()
    }

    pub fn get_pod(&self, pod_name: &str) -> Option<PodInfo> {
        self.pods.get(pod_name).cloned()
    }
}
