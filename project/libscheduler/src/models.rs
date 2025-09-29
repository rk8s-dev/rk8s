use std::{cmp::Ordering, collections::HashMap};

use tokio::time::Instant;

use common::*;
#[derive(Clone, Default, Debug)]
pub struct ResourcesRequirements {
    /// CPU resource limits, measured in millicores.
    pub cpu: u64,
    /// Memory resource limits, measured in bytes.
    pub memory: u64,
}

#[derive(Clone, Default, Debug)]
pub struct PodSpec {
    pub resources: ResourcesRequirements,
    /// Priority to the scheduler.
    pub priority: u64,
    pub scheduling_gates: Vec<String>,
    pub tolerations: Vec<Toleration>,
    pub node_name: Option<String>,
    pub node_selector: HashMap<String, String>,
    pub affinity: Option<Affinity>,
}

#[derive(Clone, Default, Debug)]
pub struct Affinity {
    pub node_affinity: Option<NodeAffinity>,
}

#[derive(Clone, Default, Debug)]
pub struct NodeAffinity {
    pub required_during_scheduling_ignored_during_execution: Option<NodeSelector>,
    pub preferred_during_scheduling_ignored_during_execution: Option<PreferredSchedulingTerms>,
}

#[derive(Clone, Default, Debug)]
pub struct PreferredSchedulingTerms {
    pub terms: Vec<PreferredSchedulingTerm>,
}

impl PreferredSchedulingTerms {
    pub fn score(&self, node: &NodeInfo) -> i64 {
        let mut count = 0;
        for t in self.terms.iter() {
            if t.match_label.matches(node) {
                count += t.weight;
            }
        }
        count
    }
}

/// Represents the OR of the selectors represented by the node selector terms.
#[derive(Clone, Default, Debug)]
pub struct NodeSelector {
    pub node_selector_terms: Vec<NodeSelectorTerm>,
}

impl NodeSelector {
    pub fn matches(&self, node: &NodeInfo) -> bool {
        if self.node_selector_terms.is_empty() {
            return true;
        }
        self.node_selector_terms.iter().any(|t| t.matches(node))
    }
}

#[derive(Clone, Default, Debug)]
pub struct NodeSelectorTerm {
    pub match_expressions: Vec<NodeSelectorRequirement>, // Differ to k8s, we only support match_expressions now
                                                         // TODO: add match_fields support
}

impl NodeSelectorTerm {
    pub fn matches(&self, node: &NodeInfo) -> bool {
        self.match_expressions.iter().all(|m| m.matches(node))
    }
}

#[derive(Clone, Default, Debug)]
pub struct NodeSelectorRequirement {
    pub key: String,
    pub operator: NodeSelectorOperator,
    pub values: Vec<String>,
}

impl NodeSelectorRequirement {
    pub fn matches(&self, node: &NodeInfo) -> bool {
        match self.operator {
            NodeSelectorOperator::NodeSelectorOpDoesNotExist => {
                !node.labels.contains_key(&self.key)
            }
            NodeSelectorOperator::NodeSelectorOpExists => node.labels.contains_key(&self.key),
            NodeSelectorOperator::NodeSelectorOpGt => {
                let label = node.labels.get(&self.key);
                if let Some(v) = label {
                    if let Ok(value) = v.parse::<i64>() {
                        if self.values.len() != 1 {
                            return false;
                        }
                        let limit = self.values[0].parse::<i64>();
                        if let Ok(limit_value) = limit {
                            value > limit_value
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            NodeSelectorOperator::NodeSelectorOpLt => {
                let label = node.labels.get(&self.key);
                if let Some(v) = label {
                    if let Ok(value) = v.parse::<i64>() {
                        if self.values.len() != 1 {
                            return false;
                        }
                        let limit = self.values[0].parse::<i64>();
                        if let Ok(limit_value) = limit {
                            value < limit_value
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            NodeSelectorOperator::NodeSelectorOpIn => {
                let label = node.labels.get(&self.key);
                if let Some(v) = label {
                    self.values.iter().any(|va| v == va)
                } else {
                    false
                }
            }
            NodeSelectorOperator::NodeSelectorOpNotIn => {
                let label = node.labels.get(&self.key);
                if let Some(v) = label {
                    !self.values.iter().any(|va| v == va)
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub enum NodeSelectorOperator {
    NodeSelectorOpIn,
    NodeSelectorOpNotIn,
    NodeSelectorOpExists,
    NodeSelectorOpDoesNotExist,
    NodeSelectorOpGt,
    NodeSelectorOpLt,
}

impl Default for NodeSelectorOperator {
    fn default() -> Self {
        Self::NodeSelectorOpExists
    }
}

#[derive(Clone, Default, Debug)]
pub struct PreferredSchedulingTerm {
    pub match_label: NodeSelectorRequirement,
    pub weight: i64,
}

#[derive(Clone, Debug)]
pub struct QueuedInfo {
    /// Scheduling failed attempts.
    pub attempts: usize,
    pub timestamp: Instant,
}

impl Default for QueuedInfo {
    fn default() -> Self {
        Self {
            attempts: 0,
            timestamp: Instant::now(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PodInfo {
    pub name: String,
    pub spec: PodSpec,
    pub queued_info: QueuedInfo,
    pub scheduled: Option<String>,
}
impl PartialEq for PodInfo {
    fn eq(&self, other: &Self) -> bool {
        self.spec.priority == other.spec.priority
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
        self.spec.priority.cmp(&other.spec.priority)
    }
}

#[derive(Clone, Default, Debug)]
pub struct NodeSpec {
    pub unschedulable: bool,
    pub taints: Vec<Taint>,
}

#[derive(Clone, Debug, Default)]
pub struct NodeInfo {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub spec: NodeSpec,
    pub requested: ResourcesRequirements,
    pub allocatable: ResourcesRequirements,
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

#[derive(Debug)]
pub struct Assignment {
    pub pod_name: String,
    pub node_name: String,
}
