use std::collections::HashMap;

use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, NodeSelector, PodInfo, PreferredSchedulingTerms},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, FilterPlugin, Plugin, PreFilterPlugin,
        PreFilterResult, PreScorePlugin, QueueingHint, ScoreExtension, ScorePlugin, Status,
    },
};

pub struct NodeAffinity;

impl Plugin for NodeAffinity {
    fn name(&self) -> &str {
        "NodeAffinity"
    }
}

impl EnqueueExtension for NodeAffinity {
    fn events_to_register(&self) -> Vec<super::ClusterEventWithHint> {
        vec![ClusterEventWithHint {
            event: ClusterEvent {
                resource: EventResource::Node,
                action_type: ActionType::Add | ActionType::UpdateNodeLabel,
            },
            queueing_hint_fn: Some(Box::new(is_schedulable_after_node_change)),
        }]
    }
}

fn is_schedulable_after_node_change(
    pod: PodInfo,
    event: EventInner,
) -> Result<QueueingHint, String> {
    match event {
        EventInner::Pod(_, _) => Err(format!(
            "event inner {event:?} not match event resource node"
        )),
        EventInner::Node(original, modeified) => {
            // Differ to kubernetes, we don't have scheduler-enforced affinity now
            // TODO: scheduler-enforced affinity

            let required_node_affinity = get_required_node_affinity(&pod);
            if required_node_affinity.matches(&modeified) {
                if let Some(old) = *original {
                    if required_node_affinity.matches(&old) {
                        log::trace!(
                            "node updated, but the pod's NodeAffinity hasn't changed. pod {pod:?} node {modeified:?}"
                        );
                        Ok(QueueingHint::Skip)
                    } else {
                        log::trace!(
                            "node was updated and the pod's NodeAffinity changed to matched. pod {pod:?} node {modeified:?}"
                        );
                        Ok(QueueingHint::Queue)
                    }
                } else {
                    log::trace!(
                        "node was created, and matches with the pod's NodeAffinity. pod {pod:?} node {modeified:?}"
                    );
                    Ok(QueueingHint::Queue)
                }
            } else {
                log::trace!(
                    "node was created or updated, but the pod's NodeAffinity doesn't match. pod {pod:?} node {modeified:?}"
                );
                Ok(QueueingHint::Skip)
            }
        }
    }
}

struct RequiredNodeAffinity {
    label_selector: HashMap<String, String>,
    node_selector: NodeSelector,
}

impl RequiredNodeAffinity {
    fn matches(&self, node: &NodeInfo) -> bool {
        let label_match = !self.label_selector.iter().any(|(key, value)| {
            let node_label = node.labels.get(key);
            !matches!(node_label, Some(v) if v == value)
        });
        label_match && self.node_selector.matches(node)
    }
}

fn get_required_node_affinity(pod: &PodInfo) -> RequiredNodeAffinity {
    let label_selector = pod.spec.node_selector.clone();
    let mut node_selector = NodeSelector::default();
    if let Some(affinity) = pod.spec.affinity.clone()
        && let Some(node_affinity) = affinity.node_affinity
        && let Some(selector) = node_affinity.required_during_scheduling_ignored_during_execution
    {
        node_selector = selector;
    }
    RequiredNodeAffinity {
        label_selector,
        node_selector,
    }
}

struct PreFilterState {
    required_node_selector_and_affinity: RequiredNodeAffinity,
}

impl PreFilterPlugin for NodeAffinity {
    fn pre_filter(
        &self,
        state: &mut crate::cycle_state::CycleState,
        pod: &PodInfo,
        _nodes: Vec<NodeInfo>,
    ) -> (PreFilterResult, Status) {
        let no_node_affinity = pod.spec.affinity.is_none()
            || pod
                .spec
                .affinity
                .to_owned()
                .unwrap()
                .node_affinity
                .is_none()
            || pod
                .spec
                .affinity
                .to_owned()
                .unwrap()
                .node_affinity
                .unwrap()
                .required_during_scheduling_ignored_during_execution
                .is_none();
        if no_node_affinity && pod.spec.node_selector.is_empty() {
            return (
                PreFilterResult { node_names: vec![] },
                Status::new(Code::Skip, vec![]),
            );
        }

        let cur_state = get_required_node_affinity(pod);
        state.write(
            "PreFilterNodeAffinity",
            Box::new(PreFilterState {
                required_node_selector_and_affinity: cur_state,
            }),
        );

        // TODO: match field meta.name

        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

const ERR_REASON_POD: &str = "node(s) didn't match Pod's node affinity/selector";

impl FilterPlugin for NodeAffinity {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> Status {
        let s = state.read::<PreFilterState>("PreFilterNodeAffinity");
        if let Some(sta) = s {
            if !sta.required_node_selector_and_affinity.matches(&node_info) {
                Status::new(
                    Code::UnschedulableAndUnresolvable,
                    vec![ERR_REASON_POD.to_string()],
                )
            } else {
                Status::default()
            }
        } else {
            Status::default()
        }
    }
}

struct PreScoreState {
    preferred_node_affinity: PreferredSchedulingTerms,
}

const PRE_SCORE_KEY: &str = "PreScoreNodeAffinity";

impl PreScorePlugin for NodeAffinity {
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, _nodes: Vec<NodeInfo>) -> Status {
        let preferred_node_affinity = get_pod_preferred_node_affinity(pod);
        if preferred_node_affinity.terms.is_empty() {
            return Status::new(Code::Skip, vec![]);
        }
        let s = PreScoreState {
            preferred_node_affinity,
        };
        state.write(PRE_SCORE_KEY, Box::new(s));
        Status::default()
    }
}

fn get_pod_preferred_node_affinity(pod: &PodInfo) -> PreferredSchedulingTerms {
    if let Some(affinity) = pod.spec.affinity.clone()
        && let Some(node_affinity) = affinity.node_affinity
        && let Some(preferred) = node_affinity.preferred_during_scheduling_ignored_during_execution
    {
        return preferred;
    }
    PreferredSchedulingTerms::default()
}

impl ScorePlugin for NodeAffinity {
    fn score(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let s = state.read::<PreScoreState>(PRE_SCORE_KEY);
        if let Some(sta) = s {
            (
                sta.preferred_node_affinity.score(&node_info),
                Status::default(),
            )
        } else {
            (
                0,
                Status::error("NodeAffinity scoring error when get pre-score state"),
            )
        }
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
        Affinity, NodeSelectorOperator, NodeSelectorRequirement, PodSpec, PreferredSchedulingTerm,
        QueuedInfo,
    };
    use std::collections::HashMap;

    #[test]
    fn test_node_affinity_filter_no_affinity() {
        let plugin = NodeAffinity;
        let mut state = CycleState::default();
        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };
        let node = NodeInfo::default();

        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_node_affinity_filter_with_node_selector() {
        let plugin = NodeAffinity;
        let mut state = CycleState::default();

        let mut node_selector = HashMap::new();
        node_selector.insert("disktype".to_string(), "ssd".to_string());

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                node_selector: node_selector.clone(),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let mut node_labels = HashMap::new();
        node_labels.insert("disktype".to_string(), "ssd".to_string());
        let matching_node = NodeInfo {
            name: "node-1".to_string(),
            labels: node_labels,
            ..Default::default()
        };

        let mut non_matching_labels = HashMap::new();
        non_matching_labels.insert("disktype".to_string(), "hdd".to_string());
        let non_matching_node = NodeInfo {
            name: "node-2".to_string(),
            labels: non_matching_labels,
            ..Default::default()
        };

        let (_pre_filter_result, pre_filter_status) = plugin.pre_filter(
            &mut state,
            &pod,
            vec![matching_node.clone(), non_matching_node.clone()],
        );
        assert_eq!(pre_filter_status.code, Code::Success);

        let empty_node_selector = NodeSelector::default();
        assert!(empty_node_selector.matches(&matching_node));

        let result = plugin.filter(&mut state, &pod, matching_node);
        assert_eq!(result.code, Code::Success);

        let result = plugin.filter(&mut state, &pod, non_matching_node);
        assert_eq!(result.code, Code::UnschedulableAndUnresolvable);
    }

    #[test]
    fn test_node_affinity_pre_filter_skip() {
        let plugin = NodeAffinity;
        let mut state = CycleState::default();
        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let (result, status) = plugin.pre_filter(&mut state, &pod, vec![]);
        assert_eq!(status.code, Code::Skip);
        assert!(result.node_names.is_empty());
    }

    #[test]
    fn test_node_affinity_score_no_preferred_affinity() {
        let plugin = NodeAffinity;
        let mut state = CycleState::default();
        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };
        let node = NodeInfo::default();

        let status = plugin.pre_score(&mut state, &pod, vec![node.clone()]);
        assert_eq!(status.code, Code::Skip);

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Error);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_node_affinity_score_with_preferred_affinity() {
        let plugin = NodeAffinity;
        let mut state = CycleState::default();

        let match_label = NodeSelectorRequirement {
            key: "zone".to_string(),
            operator: NodeSelectorOperator::NodeSelectorOpIn,
            values: vec!["us-west".to_string()],
        };
        let preferred_term = PreferredSchedulingTerm {
            weight: 10,
            match_label,
        };
        let preferred_terms = PreferredSchedulingTerms {
            terms: vec![preferred_term],
        };

        let node_affinity = crate::models::NodeAffinity {
            required_during_scheduling_ignored_during_execution: None,
            preferred_during_scheduling_ignored_during_execution: Some(preferred_terms),
        };

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                affinity: Some(Affinity {
                    node_affinity: Some(node_affinity),
                    pod_affinity: None,
                    pod_anti_affinity: None,
                }),
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let mut matching_labels = HashMap::new();
        matching_labels.insert("zone".to_string(), "us-west".to_string());
        let matching_node = NodeInfo {
            name: "node-1".to_string(),
            labels: matching_labels,
            ..Default::default()
        };

        let mut non_matching_labels = HashMap::new();
        non_matching_labels.insert("zone".to_string(), "us-east".to_string());
        let non_matching_node = NodeInfo {
            name: "node-2".to_string(),
            labels: non_matching_labels,
            ..Default::default()
        };

        let status = plugin.pre_score(
            &mut state,
            &pod,
            vec![matching_node.clone(), non_matching_node.clone()],
        );
        assert_eq!(status.code, Code::Success);

        let (score, status) = plugin.score(&mut state, &pod, matching_node);

        assert_eq!(status.code, Code::Success);
        assert_eq!(score, 10);

        let (score, status) = plugin.score(&mut state, &pod, non_matching_node);
        assert_eq!(status.code, Code::Success);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_node_affinity_events_to_register() {
        let plugin = NodeAffinity;
        let events = plugin.events_to_register();

        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert!(matches!(event.event.resource, EventResource::Node));
        assert!(event.queueing_hint_fn.is_some());
    }
}
