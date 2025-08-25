use std::{collections::HashMap, rc::Rc};

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
    fn events_to_register() -> Vec<super::ClusterEventWithHint> {
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
            "event inner {:?} not match event resource node",
            event
        )),
        EventInner::Node(original, modeified) => {
            // Differ to kubernetes, we don't have scheduler-enforced affinity now
            // TODO: scheduler-enforced affinity

            let required_node_affinity = get_required_node_affinity(&pod);
            if required_node_affinity.matches(&modeified) {
                if let Some(old) = original {
                    if required_node_affinity.matches(&old) {
                        log::trace!(
                            "node updated, but the pod's NodeAffinity hasn't changed. pod {:?} node {:?}",
                            pod,
                            modeified
                        );
                        Ok(QueueingHint::Skip)
                    } else {
                        log::trace!(
                            "node was updated and the pod's NodeAffinity changed to matched. pod {:?} node {:?}",
                            pod,
                            modeified
                        );
                        Ok(QueueingHint::Queue)
                    }
                } else {
                    log::trace!(
                        "node was created, and matches with the pod's NodeAffinity. pod {:?} node {:?}",
                        pod,
                        modeified
                    );
                    Ok(QueueingHint::Queue)
                }
            } else {
                log::trace!(
                    "node was created or updated, but the pod's NodeAffinity doesn't match. pod {:?} node {:?}",
                    pod,
                    modeified
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
    if let Some(affinity) = pod.spec.affinity.clone() {
        if let Some(node_affinity) = affinity.node_affinity {
            if let Some(selector) =
                node_affinity.required_during_scheduling_ignored_during_execution
            {
                node_selector = selector;
            }
        }
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
            Rc::new(PreFilterState {
                required_node_selector_and_affinity: cur_state,
            }),
        );

        // TODO: match field meta.name

        return (PreFilterResult { node_names: vec![] }, Status::default());
    }
}

const ERR_REASON_POD: &str = "node(s) didn't match Pod's node affinity/selector";

impl FilterPlugin for NodeAffinity {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> Status {
        let s = state.read::<PreFilterState>("PreFilterNodeAffinity");
        if let Ok(sta) = s {
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
        state.write(PRE_SCORE_KEY, Rc::new(s));
        Status::default()
    }
}

fn get_pod_preferred_node_affinity(pod: &PodInfo) -> PreferredSchedulingTerms {
    if let Some(affinity) = pod.spec.affinity.clone() {
        if let Some(node_affinity) = affinity.node_affinity {
            if let Some(preferred) =
                node_affinity.preferred_during_scheduling_ignored_during_execution
            {
                return preferred;
            }
        }
    }
    PreferredSchedulingTerms::default()
}

impl ScorePlugin for NodeAffinity {
    fn score(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let s = state.read::<PreScoreState>(PRE_SCORE_KEY);
        match s {
            Ok(sta) => (
                sta.preferred_node_affinity.score(&node_info),
                Status::default(),
            ),
            Err(_) => (
                0,
                Status::error("NodeAffinity scoring error when get pre-score state"),
            ),
        }
    }

    fn score_extension() -> Box<dyn ScoreExtension> {
        Box::new(DefaultNormalizeScore {
            max_score: 100,
            reverse: false,
        })
    }
}
