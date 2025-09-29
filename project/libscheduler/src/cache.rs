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

    pub fn set_nodes(&mut self, nodes: Vec<NodeInfo>) {
        for n in nodes {
            self.nodes.insert(n.name.clone(), n);
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

    pub fn assume(&mut self, pod_name: &str, node_name: &str) -> bool {
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
        node.requested.cpu += pod_info.spec.resources.cpu;
        node.requested.memory += pod_info.spec.resources.memory;

        true
    }

    pub fn unassume(&mut self, pod_name: &str) -> Option<PodInfo> {
        let pod_info = self.pods.get_mut(pod_name)?;
        let node_name_opt = pod_info.scheduled.clone();
        let node = if let Some(node_name) = node_name_opt {
            self.nodes.get_mut(&node_name)?
        } else {
            return None;
        };

        pod_info.scheduled = None;
        node.requested.cpu -= pod_info.spec.resources.cpu;
        node.requested.memory -= pod_info.spec.resources.memory;

        Some(pod_info.clone())
    }

    pub fn update_pod(&mut self, pod: PodInfo) -> Option<PodInfo> {
        self.pods.insert(pod.name.clone(), pod)
    }

    pub fn remove_pod(&mut self, pod_name: &str) -> Option<PodInfo> {
        if let Some(p) = self.pods.get(pod_name)
            && let Some(n) = &p.scheduled
            && let Some(node) = self.nodes.get_mut(n)
        {
            node.requested.cpu -= p.spec.resources.cpu;
            node.requested.memory -= p.spec.resources.memory;
        }
        self.pods.remove(pod_name)
    }

    pub fn update_node(&mut self, new_node: NodeInfo) -> Option<NodeInfo> {
        let name = new_node.name.clone();
        if let Some(old_node) = self.nodes.get_mut(&name) {
            let requested = old_node.requested.clone();
            old_node.labels = new_node.labels;
            old_node.spec = new_node.spec;
            old_node.allocatable = new_node.allocatable;
            old_node.requested = requested;
            Some(old_node.clone())
        } else {
            self.nodes.insert(name.clone(), new_node)
        }
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

    pub fn remove_node(&mut self, node_name: &str) {
        self.nodes.remove(node_name);
    }

    pub fn get_nodes(&self) -> Vec<NodeInfo> {
        self.nodes.values().cloned().collect()
    }

    pub fn get_pods(&self) -> HashMap<String, PodInfo> {
        self.pods.clone()
    }

    pub fn get_pod(&self, pod_name: &str) -> Option<PodInfo> {
        self.pods.get(pod_name).cloned()
    }
}
