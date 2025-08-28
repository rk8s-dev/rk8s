use std::{cmp::Ordering, collections::HashMap};

use tokio::time::Instant;

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
    pub terms: Vec<PreferredSchedulingTerm>
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
            NodeSelectorOperator::NodeSelectorOpDoesNotExist => 
                node.labels.get(&self.key).is_none(),
            NodeSelectorOperator::NodeSelectorOpExists => 
                node.labels.get(&self.key).is_some(),
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
            },
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
            },
            NodeSelectorOperator::NodeSelectorOpIn => {
                let label = node.labels.get(&self.key);
                if let Some(v) = label {
                    self.values.iter().any(|va| v == va)
                } else {
                    false
                }
            },
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
    match_label: NodeSelectorRequirement,
    weight: i64
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
    /// Available CPU resources on the node, measured in millicores.
    pub cpu: u64,
    /// Available memory resources on the node, measured in bytes.
    pub memory: u64,
    pub labels: HashMap<String, String>,
    pub spec: NodeSpec,
    pub requested: ResourcesRequirements,
    pub allocatable: ResourcesRequirements
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

/// The pod this Toleration is attached to tolerates any taint that matches
/// the triple <key,value,effect> using the matching operator <operator>.
#[derive(Default, Clone, Debug)]
pub struct Toleration {
    /// Key is the taint key that the toleration applies to. Empty means match all taint keys.
    /// If the key is empty, operator must be Exists; this combination means to match all values and all keys.
    pub key: Option<TaintKey>,
    /// Operator represents a key's relationship to the value.
    /// Valid operators are Exists and Equal. Defaults to Equal.
    operator: TolerationOperator,
    /// Effect indicates the taint effect to match. None means match all taint effects.
    /// When specified, allowed values are NoSchedule, PreferNoSchedule and NoExecute.
    pub effect: Option<TaintEffect>,
    value: String,
}

impl Toleration {
    pub fn tolerate(&self, taint: &Taint) -> bool {
        if self.effect.is_some() && self.effect.as_ref().unwrap() != &taint.effect {
            return false;
        }
        if self.key.is_some() && self.key.as_ref().unwrap() != &taint.key {
            return false;
        }
        match self.operator {
            TolerationOperator::Equal => self.value == taint.value,
            TolerationOperator::Exists => true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TolerationOperator {
    Exists,
    Equal,
}

impl Default for TolerationOperator {
    fn default() -> Self {
        Self::Equal
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

#[derive(Clone, Debug)]
pub struct Taint {
    pub key: TaintKey,
    pub value: String,
    pub effect: TaintEffect,
}

impl Taint {
    pub fn new(key: TaintKey, effect: TaintEffect) -> Self {
        Self {
            key: key,
            effect: effect,
            value: String::new(),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TaintKey {
    NodeNotReady,
    NodeUnreachable,
    NodeUnschedulable,
    NodeMemoryPressure,
    NodeDiskPressure,
    NodeNetworkUnavailable,
    NodeOutOfService,
}
