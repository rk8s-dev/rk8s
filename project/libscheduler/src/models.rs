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
    pub pod_affinity: Option<PodAffinity>,
    pub pod_anti_affinity: Option<PodAntiAffinity>,
}

#[derive(Clone, Default, Debug)]
pub struct NodeAffinity {
    pub required_during_scheduling_ignored_during_execution: Option<NodeSelector>,
    pub preferred_during_scheduling_ignored_during_execution: Option<PreferredSchedulingTerms>,
}

#[derive(Clone, Default, Debug)]
pub struct PodAffinityTerm {
    pub label_selector: Option<common::LabelSelector>,
    pub topology_key: String,
}

#[derive(Clone, Default, Debug)]
pub struct WeightedPodAffinityTerm {
    pub weight: i32,
    pub pod_affinity_term: PodAffinityTerm,
}

#[derive(Clone, Default, Debug)]
pub struct PodAffinity {
    pub required_during_scheduling_ignored_during_execution: Option<Vec<PodAffinityTerm>>,
    pub preferred_during_scheduling_ignored_during_execution: Option<Vec<WeightedPodAffinityTerm>>,
}

#[derive(Clone, Default, Debug)]
pub struct PodAntiAffinity {
    pub required_during_scheduling_ignored_during_execution: Option<Vec<PodAffinityTerm>>,
    pub preferred_during_scheduling_ignored_during_execution: Option<Vec<WeightedPodAffinityTerm>>,
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

#[derive(Clone, Debug, Default)]
pub enum NodeSelectorOperator {
    NodeSelectorOpIn,
    NodeSelectorOpNotIn,
    #[default]
    NodeSelectorOpExists,
    NodeSelectorOpDoesNotExist,
    NodeSelectorOpGt,
    NodeSelectorOpLt,
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

#[derive(Clone, Debug, Default)]
pub struct PodInfo {
    pub name: String,
    pub labels: HashMap<String, String>,
    pub spec: PodSpec,
    pub(crate) queued_info: QueuedInfo,
    pub(crate) scheduled: Option<String>,
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

impl PodInfo {
    pub fn new(name: String, labels: HashMap<String, String>, spec: PodSpec) -> Self {
        Self {
            name,
            labels,
            spec,
            queued_info: QueuedInfo::default(),
            scheduled: None,
        }
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

impl From<common::Affinity> for Affinity {
    fn from(affinity: common::Affinity) -> Self {
        Self {
            node_affinity: affinity.node_affinity.map(Into::into),
            pod_affinity: affinity.pod_affinity.map(Into::into),
            pod_anti_affinity: affinity.pod_anti_affinity.map(Into::into),
        }
    }
}

impl From<common::NodeAffinity> for NodeAffinity {
    fn from(affinity: common::NodeAffinity) -> Self {
        Self {
            required_during_scheduling_ignored_during_execution: affinity
                .required_during_scheduling_ignored_during_execution
                .map(Into::into),
            preferred_during_scheduling_ignored_during_execution: affinity
                .preferred_during_scheduling_ignored_during_execution
                .map(Into::into),
        }
    }
}

impl From<common::PodAffinity> for PodAffinity {
    fn from(affinity: common::PodAffinity) -> Self {
        Self {
            required_during_scheduling_ignored_during_execution: affinity
                .required_during_scheduling_ignored_during_execution
                .map(|v| v.into_iter().map(Into::into).collect()),
            preferred_during_scheduling_ignored_during_execution: affinity
                .preferred_during_scheduling_ignored_during_execution
                .map(|v| v.into_iter().map(Into::into).collect()),
        }
    }
}

impl From<common::PodAntiAffinity> for PodAntiAffinity {
    fn from(affinity: common::PodAntiAffinity) -> Self {
        Self {
            required_during_scheduling_ignored_during_execution: affinity
                .required_during_scheduling_ignored_during_execution
                .map(|v| v.into_iter().map(Into::into).collect()),
            preferred_during_scheduling_ignored_during_execution: affinity
                .preferred_during_scheduling_ignored_during_execution
                .map(|v| v.into_iter().map(Into::into).collect()),
        }
    }
}

impl From<common::PodAffinityTerm> for PodAffinityTerm {
    fn from(term: common::PodAffinityTerm) -> Self {
        Self {
            label_selector: term.label_selector,
            topology_key: term.topology_key,
        }
    }
}

impl From<common::WeightedPodAffinityTerm> for WeightedPodAffinityTerm {
    fn from(term: common::WeightedPodAffinityTerm) -> Self {
        Self {
            weight: term.weight,
            pod_affinity_term: term.pod_affinity_term.into(),
        }
    }
}

impl From<common::NodeSelector> for NodeSelector {
    fn from(selector: common::NodeSelector) -> Self {
        Self {
            node_selector_terms: selector
                .node_selector_terms
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

impl From<common::NodeSelectorTerm> for NodeSelectorTerm {
    fn from(term: common::NodeSelectorTerm) -> Self {
        Self {
            match_expressions: term.match_expressions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<common::NodeSelectorRequirement> for NodeSelectorRequirement {
    fn from(req: common::NodeSelectorRequirement) -> Self {
        Self {
            key: req.key,
            operator: req.operator.into(),
            values: req.values,
        }
    }
}

impl From<common::NodeSelectorOperator> for NodeSelectorOperator {
    fn from(op: common::NodeSelectorOperator) -> Self {
        match op {
            common::NodeSelectorOperator::In => NodeSelectorOperator::NodeSelectorOpIn,
            common::NodeSelectorOperator::NotIn => NodeSelectorOperator::NodeSelectorOpNotIn,
            common::NodeSelectorOperator::Exists => NodeSelectorOperator::NodeSelectorOpExists,
            common::NodeSelectorOperator::DoesNotExist => {
                NodeSelectorOperator::NodeSelectorOpDoesNotExist
            }
            common::NodeSelectorOperator::Gt => NodeSelectorOperator::NodeSelectorOpGt,
            common::NodeSelectorOperator::Lt => NodeSelectorOperator::NodeSelectorOpLt,
        }
    }
}

impl From<common::PreferredSchedulingTerms> for PreferredSchedulingTerms {
    fn from(terms: common::PreferredSchedulingTerms) -> Self {
        Self {
            terms: terms.terms.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<common::PreferredSchedulingTerm> for PreferredSchedulingTerm {
    fn from(term: common::PreferredSchedulingTerm) -> Self {
        // Convert NodeSelectorTerm to NodeSelectorRequirement by taking the first match expression
        // If there are no match expressions, create a default one that matches nothing
        let match_label = if term.preference.match_expressions.is_empty() {
            NodeSelectorRequirement {
                key: String::new(),
                operator: NodeSelectorOperator::NodeSelectorOpExists,
                values: vec![],
            }
        } else {
            // Take the first match expression
            term.preference
                .match_expressions
                .into_iter()
                .next()
                .unwrap()
                .into()
        };
        Self {
            match_label,
            weight: term.weight,
        }
    }
}
