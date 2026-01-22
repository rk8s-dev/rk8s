use std::collections::HashMap;

use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodAffinityTerm, PodInfo, WeightedPodAffinityTerm},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, FilterPlugin, Plugin, PreFilterPlugin,
        PreFilterResult, PreScorePlugin, QueueingHint, ScoreExtension, ScorePlugin, Status,
    },
};
use common::LabelSelector;

pub struct PodAffinityPlugin;

impl Plugin for PodAffinityPlugin {
    fn name(&self) -> &str {
        "PodAffinity"
    }
}

impl EnqueueExtension for PodAffinityPlugin {
    fn events_to_register(&self) -> Vec<super::ClusterEventWithHint> {
        vec![
            ClusterEventWithHint {
                event: ClusterEvent {
                    resource: EventResource::Pod,
                    action_type: ActionType::Add | ActionType::UpdatePodLabel | ActionType::Delete,
                },
                queueing_hint_fn: Some(Box::new(is_schedulable_after_pod_change)),
            },
            ClusterEventWithHint {
                event: ClusterEvent {
                    resource: EventResource::Node,
                    action_type: ActionType::Add | ActionType::UpdateNodeLabel,
                },
                queueing_hint_fn: Some(Box::new(is_schedulable_after_node_change)),
            },
        ]
    }
}

fn is_schedulable_after_pod_change(
    pod: PodInfo,
    event: EventInner,
) -> Result<QueueingHint, String> {
    match event {
        EventInner::Pod(_old_pod, _new_pod) => {
            // Check if the changed pod affects this pod's affinity/anti-affinity rules
            // For now, we assume any pod change could affect scheduling
            log::trace!(
                "pod changed, checking if it affects pod {}'s affinity rules",
                pod.name
            );
            Ok(QueueingHint::Queue)
        }
        EventInner::Node(_, _) => Err(format!(
            "event inner {event:?} not match event resource pod"
        )),
    }
}

fn is_schedulable_after_node_change(
    pod: PodInfo,
    event: EventInner,
) -> Result<QueueingHint, String> {
    match event {
        EventInner::Node(_old_node, _new_node) => {
            log::trace!(
                "node label changed, checking if it affects pod {}'s affinity rules",
                pod.name
            );
            Ok(QueueingHint::Queue)
        }
        EventInner::Pod(_, _) => Err(format!(
            "event inner {event:?} not match event resource node"
        )),
    }
}

/// Helper to group nodes by topology key
fn group_nodes_by_topology_key(
    nodes: &[NodeInfo],
    topology_key: &str,
) -> HashMap<String, Vec<NodeInfo>> {
    let mut groups = HashMap::new();
    for node in nodes {
        if let Some(value) = node.labels.get(topology_key) {
            groups
                .entry(value.clone())
                .or_insert_with(Vec::new)
                .push(node.clone());
        }
    }
    groups
}

/// Check if a pod matches label selector
fn pod_matches_label_selector(pod: &PodInfo, label_selector: &LabelSelector) -> bool {
    for (key, expected_value) in &label_selector.match_labels {
        if let Some(actual_value) = pod.labels.get(key) {
            if actual_value != expected_value {
                return false;
            }
        } else {
            return false;
        }
    }

    for req in &label_selector.match_expressions {
        let pod_value = pod.labels.get(&req.key);
        match req.operator {
            common::LabelSelectorOperator::In => {
                if let Some(v) = pod_value {
                    if !req.values.iter().any(|val| val == v) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            common::LabelSelectorOperator::NotIn => {
                if let Some(v) = pod_value
                    && req.values.iter().any(|val| val == v)
                {
                    return false;
                }
                // If pod doesn't have the label, it's considered not in the values, so pass
            }
            common::LabelSelectorOperator::Exists => {
                if pod_value.is_none() {
                    return false;
                }
            }
            common::LabelSelectorOperator::DoesNotExist => {
                if pod_value.is_some() {
                    return false;
                }
            }
        }
    }
    true
}

/// Get the node where a pod is scheduled or will be scheduled (via node_name)
fn get_scheduled_node(pod: &PodInfo) -> Option<&str> {
    pod.scheduled.as_deref().or(pod.spec.node_name.as_deref())
}

/// Get all pods running on a node from cache
/// This requires access to scheduler cache, which is not available in plugin context.
/// We need to think about how to get this information.
/// For now, we'll assume we have a way to get node's pods.
fn get_pods_on_node(_node: &NodeInfo) -> Vec<PodInfo> {
    vec![]
}

/// Extract pod affinity and anti-affinity terms from pod spec
fn extract_pod_affinity_terms(
    pod: &PodInfo,
) -> (
    Vec<PodAffinityTerm>,
    Vec<PodAffinityTerm>,
    Vec<WeightedPodAffinityTerm>,
    Vec<WeightedPodAffinityTerm>,
) {
    let mut required_affinity_terms = Vec::new();
    let mut required_anti_affinity_terms = Vec::new();
    let mut preferred_affinity_terms = Vec::new();
    let mut preferred_anti_affinity_terms = Vec::new();

    if let Some(affinity) = &pod.spec.affinity {
        if let Some(pod_affinity) = &affinity.pod_affinity {
            if let Some(terms) = &pod_affinity.required_during_scheduling_ignored_during_execution {
                required_affinity_terms.extend(terms.clone());
            }
            if let Some(terms) = &pod_affinity.preferred_during_scheduling_ignored_during_execution
            {
                preferred_affinity_terms.extend(terms.clone());
            }
        }
        if let Some(pod_anti_affinity) = &affinity.pod_anti_affinity {
            if let Some(terms) =
                &pod_anti_affinity.required_during_scheduling_ignored_during_execution
            {
                required_anti_affinity_terms.extend(terms.clone());
            }
            if let Some(terms) =
                &pod_anti_affinity.preferred_during_scheduling_ignored_during_execution
            {
                preferred_anti_affinity_terms.extend(terms.clone());
            }
        }
    }

    (
        required_affinity_terms,
        required_anti_affinity_terms,
        preferred_affinity_terms,
        preferred_anti_affinity_terms,
    )
}

/// Build node topology map from nodes
fn build_node_topology_map(nodes: &[NodeInfo]) -> HashMap<String, HashMap<String, String>> {
    let mut map = HashMap::new();
    for node in nodes {
        let mut topology = HashMap::new();
        for (key, value) in &node.labels {
            topology.insert(key.clone(), value.clone());
        }
        map.insert(node.name.clone(), topology);
    }
    map
}

/// Get pods matching label selector from a list of pods
fn get_matching_pods(pods: &[PodInfo], label_selector: &Option<LabelSelector>) -> Vec<PodInfo> {
    let mut matching_pods = Vec::new();
    if let Some(selector) = label_selector {
        for pod in pods {
            if pod_matches_label_selector(pod, selector) {
                matching_pods.push(pod.clone());
            }
        }
    }
    matching_pods
}

/// Evaluate required pod affinity terms for a node
fn evaluate_required_pod_affinity_terms(
    node: &NodeInfo,
    terms: &[PodAffinityTerm],
    all_pods: &[PodInfo],
    node_topology_map: &HashMap<String, HashMap<String, String>>,
    is_anti_affinity: bool,
) -> bool {
    if terms.is_empty() {
        return true;
    }

    // For each term, check if there's at least one matching pod in the same topology
    for term in terms {
        let matching_pods = get_matching_pods(all_pods, &term.label_selector);
        if matching_pods.is_empty() {
            // No matching pods at all, this term is satisfied for anti-affinity
            // but not satisfied for affinity
            if !is_anti_affinity {
                return false; // Affinity requires at least one matching pod
            }
            continue; // Anti-affinity is satisfied if no matching pods
        }

        // Check if any matching pod is in the same topology as this node
        let node_topology_value = node.labels.get(&term.topology_key);
        let mut found_in_same_topology = false;

        for pod in &matching_pods {
            if let Some(scheduled_node) = get_scheduled_node(pod)
                && let Some(node_topology) = node_topology_map.get(scheduled_node)
                && let Some(pod_topology_value) = node_topology.get(&term.topology_key)
                && node_topology_value == Some(pod_topology_value)
            {
                found_in_same_topology = true;
                break;
            }
        }

        if is_anti_affinity {
            // Anti-affinity: should NOT have matching pods in same topology
            if found_in_same_topology {
                return false;
            }
        } else {
            // Affinity: should have at least one matching pod in same topology
            if !found_in_same_topology {
                return false;
            }
        }
    }

    true
}

/// Evaluate preferred pod affinity terms for a node
fn evaluate_preferred_pod_affinity_terms(
    node: &NodeInfo,
    terms: &[WeightedPodAffinityTerm],
    all_pods: &[PodInfo],
    node_topology_map: &HashMap<String, HashMap<String, String>>,
    is_anti_affinity: bool,
) -> i64 {
    let mut score = 0;

    for term in terms {
        let matching_pods = get_matching_pods(all_pods, &term.pod_affinity_term.label_selector);
        if matching_pods.is_empty() {
            // No matching pods at all
            if !is_anti_affinity {
                // Affinity: no matching pods means no score
                continue;
            } else {
                // Anti-affinity: no matching pods is good, add weight
                score += term.weight as i64;
                continue;
            }
        }

        // Check if any matching pod is in the same topology as this node
        let node_topology_value = node.labels.get(&term.pod_affinity_term.topology_key);
        let mut found_in_same_topology = false;

        for pod in &matching_pods {
            if let Some(scheduled_node) = get_scheduled_node(pod)
                && let Some(node_topology) = node_topology_map.get(scheduled_node)
                && let Some(pod_topology_value) =
                    node_topology.get(&term.pod_affinity_term.topology_key)
                && node_topology_value == Some(pod_topology_value)
            {
                found_in_same_topology = true;
                break;
            }
        }

        if is_anti_affinity {
            // Anti-affinity: prefer nodes without matching pods in same topology
            if !found_in_same_topology {
                score += term.weight as i64;
            }
        } else {
            // Affinity: prefer nodes with matching pods in same topology
            if found_in_same_topology {
                score += term.weight as i64;
            }
        }
    }

    score
}

#[derive(Clone, Debug)]
struct PreFilterState {
    required_affinity_terms: Vec<PodAffinityTerm>,
    required_anti_affinity_terms: Vec<PodAffinityTerm>,
    preferred_affinity_terms: Vec<WeightedPodAffinityTerm>,
    preferred_anti_affinity_terms: Vec<WeightedPodAffinityTerm>,
    node_topology_map: HashMap<String, HashMap<String, String>>, // node_name -> (topology_key -> value)
}

#[derive(Clone, Debug)]
struct PreScoreState {
    preferred_affinity_terms: Vec<WeightedPodAffinityTerm>,
    preferred_anti_affinity_terms: Vec<WeightedPodAffinityTerm>,
    node_topology_map: HashMap<String, HashMap<String, String>>,
}

impl PreFilterPlugin for PodAffinityPlugin {
    fn pre_filter(
        &self,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: Vec<NodeInfo>,
    ) -> (PreFilterResult, Status) {
        let (
            required_affinity_terms,
            required_anti_affinity_terms,
            preferred_affinity_terms,
            preferred_anti_affinity_terms,
        ) = extract_pod_affinity_terms(pod);

        if required_affinity_terms.is_empty()
            && required_anti_affinity_terms.is_empty()
            && preferred_affinity_terms.is_empty()
            && preferred_anti_affinity_terms.is_empty()
        {
            return (
                PreFilterResult { node_names: vec![] },
                Status::new(Code::Skip, vec![]),
            );
        }

        let node_topology_map = build_node_topology_map(&nodes);

        let pre_filter_state = PreFilterState {
            required_affinity_terms,
            required_anti_affinity_terms,
            preferred_affinity_terms,
            preferred_anti_affinity_terms,
            node_topology_map,
        };

        state.write("PreFilterPodAffinity", Box::new(pre_filter_state));

        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

impl FilterPlugin for PodAffinityPlugin {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> Status {
        // Get pre-filter state
        let pre_filter_state = match state.read::<PreFilterState>("PreFilterPodAffinity") {
            Some(state) => state,
            None => return Status::default(),
        };

        let all_pods: Vec<PodInfo> = state
            .read::<Vec<PodInfo>>("AllScheduledPods")
            .cloned()
            .unwrap_or_default();

        let affinity_satisfied = evaluate_required_pod_affinity_terms(
            &node_info,
            &pre_filter_state.required_affinity_terms,
            &all_pods,
            &pre_filter_state.node_topology_map,
            false,
        );

        if !affinity_satisfied {
            return Status::new(
                Code::UnschedulableAndUnresolvable,
                vec!["node(s) didn't match pod affinity rules".to_string()],
            );
        }

        let anti_affinity_satisfied = evaluate_required_pod_affinity_terms(
            &node_info,
            &pre_filter_state.required_anti_affinity_terms,
            &all_pods,
            &pre_filter_state.node_topology_map,
            true,
        );

        if !anti_affinity_satisfied {
            return Status::new(
                Code::UnschedulableAndUnresolvable,
                vec!["node(s) didn't match pod anti-affinity rules".to_string()],
            );
        }

        Status::default()
    }
}

impl PreScorePlugin for PodAffinityPlugin {
    fn pre_score(&self, state: &mut CycleState, _pod: &PodInfo, nodes: Vec<NodeInfo>) -> Status {
        let pre_filter_state = match state.read::<PreFilterState>("PreFilterPodAffinity") {
            Some(state) => state,
            None => {
                return Status::new(Code::Skip, vec![]);
            }
        };

        if pre_filter_state.preferred_affinity_terms.is_empty()
            && pre_filter_state.preferred_anti_affinity_terms.is_empty()
        {
            return Status::new(Code::Skip, vec![]);
        }

        let node_topology_map = build_node_topology_map(&nodes);

        let pre_score_state = PreScoreState {
            preferred_affinity_terms: pre_filter_state.preferred_affinity_terms.clone(),
            preferred_anti_affinity_terms: pre_filter_state.preferred_anti_affinity_terms.clone(),
            node_topology_map,
        };

        state.write("PreScorePodAffinity", Box::new(pre_score_state));

        Status::default()
    }
}

impl ScorePlugin for PodAffinityPlugin {
    fn score(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let pre_score_state = match state.read::<PreScoreState>("PreScorePodAffinity") {
            Some(state) => state,
            None => {
                // No preferred terms, return 0 score
                return (0, Status::default());
            }
        };

        // Try to get all scheduled pods from cycle state
        // If not available, use empty list (for backward compatibility)
        let all_pods: Vec<PodInfo> = state
            .read::<Vec<PodInfo>>("AllScheduledPods")
            .cloned()
            .unwrap_or_default();

        let affinity_score = evaluate_preferred_pod_affinity_terms(
            &node_info,
            &pre_score_state.preferred_affinity_terms,
            &all_pods,
            &pre_score_state.node_topology_map,
            false,
        );

        let anti_affinity_score = evaluate_preferred_pod_affinity_terms(
            &node_info,
            &pre_score_state.preferred_anti_affinity_terms,
            &all_pods,
            &pre_score_state.node_topology_map,
            true,
        );

        let total_score = affinity_score + anti_affinity_score;

        (total_score, Status::default())
    }

    fn score_extension(&self) -> Box<dyn ScoreExtension> {
        Box::new(DefaultNormalizeScore {
            max_score: 100,
            reverse: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        Affinity, NodeInfo, NodeSpec, PodAffinity, PodAffinityTerm, PodAntiAffinity, PodSpec,
        QueuedInfo, ResourcesRequirements, WeightedPodAffinityTerm,
    };
    use common::LabelSelector;
    use std::collections::HashMap;

    fn make_pod(name: &str, labels: HashMap<String, String>, scheduled: Option<&str>) -> PodInfo {
        PodInfo {
            name: name.to_string(),
            labels,
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: scheduled.map(|s| s.to_string()),
        }
    }

    fn make_node(name: &str, labels: HashMap<String, String>) -> NodeInfo {
        NodeInfo {
            name: name.to_string(),
            labels,
            spec: NodeSpec::default(),
            requested: ResourcesRequirements::default(),
            allocatable: ResourcesRequirements::default(),
        }
    }

    fn create_label_selector(match_labels: HashMap<String, String>) -> LabelSelector {
        LabelSelector {
            match_labels,
            match_expressions: vec![],
        }
    }

    #[test]
    fn test_pod_affinity_plugin_name() {
        let plugin = PodAffinityPlugin;
        assert_eq!(plugin.name(), "PodAffinity");
    }

    #[test]
    fn test_pod_matches_label_selector() {
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());
        labels.insert("env".to_string(), "prod".to_string());

        let pod = make_pod("test-pod", labels, None);

        let mut selector_labels = HashMap::new();
        selector_labels.insert("app".to_string(), "web".to_string());
        let selector = create_label_selector(selector_labels);

        assert!(pod_matches_label_selector(&pod, &selector));

        let mut wrong_selector_labels = HashMap::new();
        wrong_selector_labels.insert("app".to_string(), "api".to_string());
        let wrong_selector = create_label_selector(wrong_selector_labels);

        assert!(!pod_matches_label_selector(&pod, &wrong_selector));
    }

    #[test]
    fn test_extract_pod_affinity_terms() {
        // Create a pod with affinity rules
        let mut labels = HashMap::new();
        labels.insert("app".to_string(), "web".to_string());

        let pod_affinity_term = PodAffinityTerm {
            label_selector: Some(create_label_selector({
                let mut map = HashMap::new();
                map.insert("app".to_string(), "web".to_string());
                map
            })),
            topology_key: "zone".to_string(),
            namespaces: None,
        };

        let weighted_term = WeightedPodAffinityTerm {
            weight: 10,
            pod_affinity_term: pod_affinity_term.clone(),
        };

        let pod_affinity = PodAffinity {
            required_during_scheduling_ignored_during_execution: Some(vec![
                pod_affinity_term.clone(),
            ]),
            preferred_during_scheduling_ignored_during_execution: Some(vec![weighted_term.clone()]),
        };

        let pod_anti_affinity = PodAntiAffinity {
            required_during_scheduling_ignored_during_execution: Some(vec![
                pod_affinity_term.clone(),
            ]),
            preferred_during_scheduling_ignored_during_execution: Some(vec![weighted_term.clone()]),
        };

        let affinity = Affinity {
            node_affinity: None,
            pod_affinity: Some(pod_affinity),
            pod_anti_affinity: Some(pod_anti_affinity),
        };

        let pod = PodInfo {
            name: "test-pod".to_string(),
            labels,
            spec: PodSpec {
                affinity: Some(affinity),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let (
            required_affinity_terms,
            required_anti_affinity_terms,
            preferred_affinity_terms,
            preferred_anti_affinity_terms,
        ) = extract_pod_affinity_terms(&pod);

        assert_eq!(required_affinity_terms.len(), 1);
        assert_eq!(required_anti_affinity_terms.len(), 1);
        assert_eq!(preferred_affinity_terms.len(), 1);
        assert_eq!(preferred_anti_affinity_terms.len(), 1);
        assert_eq!(required_affinity_terms[0].topology_key, "zone");
        assert_eq!(preferred_affinity_terms[0].weight, 10);
    }

    #[test]
    fn test_pre_filter_skip_when_no_affinity() {
        let plugin = PodAffinityPlugin;
        let mut state = crate::cycle_state::CycleState::default();
        let pod = make_pod("test-pod", HashMap::new(), None);
        let nodes = vec![make_node("node1", HashMap::new())];

        let (result, status) = plugin.pre_filter(&mut state, &pod, nodes);
        assert_eq!(status.code, Code::Skip);
        assert!(result.node_names.is_empty());
    }

    #[test]
    fn test_build_node_topology_map() {
        let mut node1_labels = HashMap::new();
        node1_labels.insert("zone".to_string(), "us-west".to_string());
        node1_labels.insert("rack".to_string(), "rack-1".to_string());

        let mut node2_labels = HashMap::new();
        node2_labels.insert("zone".to_string(), "us-east".to_string());
        node2_labels.insert("rack".to_string(), "rack-2".to_string());

        let nodes = vec![
            make_node("node1", node1_labels),
            make_node("node2", node2_labels),
        ];

        let map = build_node_topology_map(&nodes);

        assert_eq!(map.len(), 2);
        assert_eq!(map.get("node1").unwrap().get("zone").unwrap(), "us-west");
        assert_eq!(map.get("node2").unwrap().get("zone").unwrap(), "us-east");
        assert_eq!(map.get("node1").unwrap().get("rack").unwrap(), "rack-1");
    }

    #[test]
    fn test_evaluate_required_pod_affinity_terms() {
        // Create nodes with topology labels
        let mut node1_labels = HashMap::new();
        node1_labels.insert("zone".to_string(), "zone-a".to_string());
        let node1 = make_node("node1", node1_labels);

        let mut node2_labels = HashMap::new();
        node2_labels.insert("zone".to_string(), "zone-b".to_string());
        let node2 = make_node("node2", node2_labels);

        // Create a pod already scheduled on node1 with label app=web
        let mut pod_labels = HashMap::new();
        pod_labels.insert("app".to_string(), "web".to_string());
        let existing_pod = make_pod("existing-pod", pod_labels, Some("node1"));

        // Create pod affinity term: require pod with app=web in same zone
        let mut selector_labels = HashMap::new();
        selector_labels.insert("app".to_string(), "web".to_string());
        let label_selector = Some(create_label_selector(selector_labels));

        let term = PodAffinityTerm {
            label_selector,
            topology_key: "zone".to_string(),
            namespaces: None,
        };

        // Build node topology map
        let nodes = vec![node1.clone(), node2.clone()];
        let node_topology_map = build_node_topology_map(&nodes);

        // Test affinity: node1 should satisfy (has pod with app=web in same zone)
        let all_pods = vec![existing_pod.clone()];
        let result = evaluate_required_pod_affinity_terms(
            &node1,
            &[term.clone()],
            &all_pods,
            &node_topology_map,
            false,
        );
        assert!(result, "Node1 should satisfy affinity term");

        // Test affinity: node2 should not satisfy (no pod with app=web in zone-b)
        let result = evaluate_required_pod_affinity_terms(
            &node2,
            &[term.clone()],
            &all_pods,
            &node_topology_map,
            false,
        );
        assert!(!result, "Node2 should not satisfy affinity term");

        // Test anti-affinity: node1 should not satisfy (has pod with app=web in same zone)
        let result = evaluate_required_pod_affinity_terms(
            &node1,
            &[term.clone()],
            &all_pods,
            &node_topology_map,
            true,
        );
        assert!(!result, "Node1 should not satisfy anti-affinity term");

        // Test anti-affinity: node2 should satisfy (no pod with app=web in zone-b)
        let result = evaluate_required_pod_affinity_terms(
            &node2,
            &[term],
            &all_pods,
            &node_topology_map,
            true,
        );
        assert!(result, "Node2 should satisfy anti-affinity term");
    }

    #[test]
    fn test_evaluate_preferred_pod_affinity_terms() {
        // Create nodes with topology labels
        let mut node1_labels = HashMap::new();
        node1_labels.insert("zone".to_string(), "zone-a".to_string());
        let node1 = make_node("node1", node1_labels);

        let mut node2_labels = HashMap::new();
        node2_labels.insert("zone".to_string(), "zone-b".to_string());
        let node2 = make_node("node2", node2_labels);

        // Create a pod already scheduled on node1 with label app=web
        let mut pod_labels = HashMap::new();
        pod_labels.insert("app".to_string(), "web".to_string());
        let existing_pod = make_pod("existing-pod", pod_labels, Some("node1"));

        // Create weighted pod affinity term: prefer pod with app=web in same zone (weight=10)
        let mut selector_labels = HashMap::new();
        selector_labels.insert("app".to_string(), "web".to_string());
        let label_selector = Some(create_label_selector(selector_labels));

        let pod_affinity_term = PodAffinityTerm {
            label_selector,
            topology_key: "zone".to_string(),
            namespaces: None,
        };

        let weighted_term = WeightedPodAffinityTerm {
            weight: 10,
            pod_affinity_term,
        };

        // Build node topology map
        let nodes = vec![node1.clone(), node2.clone()];
        let node_topology_map = build_node_topology_map(&nodes);

        // Test affinity scoring: node1 should get weight 10 (has matching pod in same zone)
        let all_pods = vec![existing_pod.clone()];
        let score = evaluate_preferred_pod_affinity_terms(
            &node1,
            &[weighted_term.clone()],
            &all_pods,
            &node_topology_map,
            false,
        );
        assert_eq!(score, 10, "Node1 should get score 10 for affinity");

        // Test affinity scoring: node2 should get score 0 (no matching pod in same zone)
        let score = evaluate_preferred_pod_affinity_terms(
            &node2,
            &[weighted_term.clone()],
            &all_pods,
            &node_topology_map,
            false,
        );
        assert_eq!(score, 0, "Node2 should get score 0 for affinity");

        // Test anti-affinity scoring: node1 should get score 0 (has matching pod in same zone)
        let score = evaluate_preferred_pod_affinity_terms(
            &node1,
            &[weighted_term.clone()],
            &all_pods,
            &node_topology_map,
            true,
        );
        assert_eq!(score, 0, "Node1 should get score 0 for anti-affinity");

        // Test anti-affinity scoring: node2 should get weight 10 (no matching pod in same zone)
        let score = evaluate_preferred_pod_affinity_terms(
            &node2,
            &[weighted_term],
            &all_pods,
            &node_topology_map,
            true,
        );
        assert_eq!(score, 10, "Node2 should get score 10 for anti-affinity");
    }

    #[test]
    fn test_filter_pod_affinity() {
        let plugin = PodAffinityPlugin;
        let mut state = crate::cycle_state::CycleState::default();

        // Create a pod with required affinity rule
        let mut selector_labels = HashMap::new();
        selector_labels.insert("app".to_string(), "web".to_string());
        let label_selector = Some(create_label_selector(selector_labels));

        let pod_affinity_term = PodAffinityTerm {
            label_selector,
            topology_key: "zone".to_string(),
            namespaces: None,
        };

        let pod_affinity = PodAffinity {
            required_during_scheduling_ignored_during_execution: Some(vec![
                pod_affinity_term.clone(),
            ]),
            preferred_during_scheduling_ignored_during_execution: None,
        };

        let pod_anti_affinity = PodAntiAffinity {
            required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term]),
            preferred_during_scheduling_ignored_during_execution: None,
        };

        let affinity = Affinity {
            node_affinity: None,
            pod_affinity: Some(pod_affinity),
            pod_anti_affinity: Some(pod_anti_affinity),
        };

        let mut pod_labels = HashMap::new();
        pod_labels.insert("app".to_string(), "test".to_string());
        let pod = PodInfo {
            name: "test-pod".to_string(),
            labels: pod_labels,
            spec: PodSpec {
                affinity: Some(affinity),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        // Create nodes
        let mut node1_labels = HashMap::new();
        node1_labels.insert("zone".to_string(), "zone-a".to_string());
        let node1 = make_node("node1", node1_labels);

        let mut node2_labels = HashMap::new();
        node2_labels.insert("zone".to_string(), "zone-b".to_string());
        let node2 = make_node("node2", node2_labels);

        let nodes = vec![node1.clone(), node2.clone()];

        // Run pre-filter to set up state
        let (_pre_filter_result, pre_filter_status) = plugin.pre_filter(&mut state, &pod, nodes);
        assert_eq!(pre_filter_status.code, Code::Success);

        // Filter should return UnschedulableAndUnresolvable since there are no pods
        // matching the affinity/anti-affinity rules (all_pods is empty in test)
        // In real usage, the plugin would get all scheduled pods from scheduler cache
        let status = plugin.filter(&mut state, &pod, node1);
        assert_eq!(status.code, Code::UnschedulableAndUnresolvable);
    }

    #[test]
    fn test_score_pod_affinity() {
        let plugin = PodAffinityPlugin;
        let mut state = crate::cycle_state::CycleState::default();

        // Create a pod with preferred affinity rule
        let mut selector_labels = HashMap::new();
        selector_labels.insert("app".to_string(), "web".to_string());
        let label_selector = Some(create_label_selector(selector_labels));

        let pod_affinity_term = PodAffinityTerm {
            label_selector,
            topology_key: "zone".to_string(),
            namespaces: None,
        };

        let weighted_term = WeightedPodAffinityTerm {
            weight: 10,
            pod_affinity_term: pod_affinity_term.clone(),
        };

        let pod_affinity = PodAffinity {
            required_during_scheduling_ignored_during_execution: None,
            preferred_during_scheduling_ignored_during_execution: Some(vec![weighted_term]),
        };

        let affinity = Affinity {
            node_affinity: None,
            pod_affinity: Some(pod_affinity),
            pod_anti_affinity: None,
        };

        let mut pod_labels = HashMap::new();
        pod_labels.insert("app".to_string(), "test".to_string());
        let pod = PodInfo {
            name: "test-pod".to_string(),
            labels: pod_labels,
            spec: PodSpec {
                affinity: Some(affinity),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        // Create nodes
        let mut node1_labels = HashMap::new();
        node1_labels.insert("zone".to_string(), "zone-a".to_string());
        let node1 = make_node("node1", node1_labels);

        let mut node2_labels = HashMap::new();
        node2_labels.insert("zone".to_string(), "zone-b".to_string());
        let node2 = make_node("node2", node2_labels);

        let nodes = vec![node1.clone(), node2.clone()];

        // Run pre-filter and pre-score to set up state
        let (_pre_filter_result, pre_filter_status) =
            plugin.pre_filter(&mut state, &pod, nodes.clone());
        assert_eq!(pre_filter_status.code, Code::Success);

        let pre_score_status = plugin.pre_score(&mut state, &pod, nodes);
        assert_eq!(pre_score_status.code, Code::Success);

        // Score should return 0 since we can't get all pods in unit test
        // The actual logic requires access to scheduler cache
        let (score, status) = plugin.score(&mut state, &pod, node1);
        assert_eq!(status.code, Code::Success);
        assert_eq!(score, 0);
    }
}
