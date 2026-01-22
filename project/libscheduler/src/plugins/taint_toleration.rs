use log;

use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, FilterPlugin, Plugin, PreScorePlugin,
        QueueingHint, ScoreExtension, ScorePlugin, Status,
    },
};

use common::{Taint, TaintEffect, Toleration};

pub struct TaintToleration;

impl Plugin for TaintToleration {
    fn name(&self) -> &str {
        "TaintToleration"
    }
}

impl FilterPlugin for TaintToleration {
    fn filter(
        &self,
        _: &mut crate::cycle_state::CycleState,
        pod: &PodInfo,
        node_info: NodeInfo,
    ) -> Status {
        let not_schedule_taints_filter =
            |t: &&Taint| matches!(t.effect, TaintEffect::NoSchedule | TaintEffect::NoExecute);
        let untolerated = find_untolerated_taint(
            &node_info.spec.taints,
            &pod.spec.tolerations,
            not_schedule_taints_filter,
        );

        if let Some(t) = untolerated {
            let err_reason = vec![format!(
                "node(s) had untolerated taint {{{:#?}: {}}}",
                t.key, t.value
            )];
            Status::new(Code::UnschedulableAndUnresolvable, err_reason)
        } else {
            Status::new(Code::Success, vec![])
        }
    }
}

fn tolerations_tolerate_taint(tolerations: &[Toleration], taint: &Taint) -> bool {
    tolerations.iter().any(|to| to.tolerate(taint))
}

fn find_untolerated_taint<'a>(
    taints: &'a [Taint],
    tolerations: &[Toleration],
    p: impl FnMut(&&Taint) -> bool,
) -> Option<&'a Taint> {
    taints
        .iter()
        .filter(p)
        .find(|&t| !tolerations_tolerate_taint(tolerations, t))
}

const PRE_SCORE_KEY: &str = "PreScoreTaintToleration";

impl PreScorePlugin for TaintToleration {
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, _: Vec<NodeInfo>) -> Status {
        let toleration_prefer_no_schedule: Vec<_> = pod
            .spec
            .tolerations
            .iter()
            .filter(|t| matches!(t.effect, Some(TaintEffect::PreferNoSchedule) | None))
            .cloned()
            .collect();
        state.write(PRE_SCORE_KEY, Box::new(toleration_prefer_no_schedule));
        Status::default()
    }
}

impl ScorePlugin for TaintToleration {
    fn score(&self, state: &mut CycleState, _: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let s = state.read::<Vec<Toleration>>(PRE_SCORE_KEY);
        if let Some(tolerations) = s {
            let score = node_info
                .spec
                .taints
                .iter()
                .filter(|&t| {
                    matches!(t.effect, TaintEffect::PreferNoSchedule)
                        && tolerations_tolerate_taint(tolerations, t)
                })
                .count();
            (score as i64, Status::default())
        } else {
            (0, Status::error("PreScoreState not found"))
        }
    }

    fn score_extension(&self) -> Box<dyn ScoreExtension> {
        Box::new(DefaultNormalizeScore {
            max_score: 100,
            reverse: true,
        })
    }
}

impl EnqueueExtension for TaintToleration {
    fn events_to_register(&self) -> Vec<super::ClusterEventWithHint> {
        vec![
            ClusterEventWithHint {
                event: ClusterEvent {
                    resource: EventResource::Node,
                    action_type: ActionType::Add | ActionType::UpdateNodeTaint,
                },
                queueing_hint_fn: Some(Box::new(is_schedulable_after_node_change)),
            },
            ClusterEventWithHint {
                event: ClusterEvent {
                    resource: EventResource::Pod,
                    action_type: ActionType::UpdatePodToleration,
                },
                queueing_hint_fn: Some(Box::new(is_schedulable_after_pod_toleration_change)),
            },
        ]
    }
}

fn do_not_schedule_taints_filter(t: &Taint) -> bool {
    matches!(t.effect, TaintEffect::NoSchedule | TaintEffect::NoExecute)
}

fn is_schedulable_after_node_change(
    pod: PodInfo,
    event: EventInner,
) -> Result<QueueingHint, String> {
    match event {
        EventInner::Pod(_, _) => Err(format!(
            "event inner {event:?} not match event resource node"
        )),
        EventInner::Node(old, new) => {
            let was_untolerated = old.is_none()
                || find_untolerated_taint(&old.unwrap().spec.taints, &pod.spec.tolerations, |&t| {
                    do_not_schedule_taints_filter(t)
                })
                .is_some();
            let is_untolerated =
                find_untolerated_taint(&new.spec.taints, &pod.spec.tolerations, |&t| {
                    do_not_schedule_taints_filter(t)
                })
                .is_some();
            if was_untolerated && !is_untolerated {
                log::trace!(
                    "node was created or updated, and this may make the Pod rejected by TaintToleration plugin in the previous scheduling cycle schedulable. node {new:?}, pod: {pod:?}"
                );
                Ok(QueueingHint::Queue)
            } else {
                log::trace!(
                    "node was created or updated, but it doesn't change the TaintToleration plugin's decision node {new:?}, pod: {pod:?}"
                );
                Ok(QueueingHint::Skip)
            }
        }
    }
}

fn is_schedulable_after_pod_toleration_change(
    pod: PodInfo,
    event: EventInner,
) -> Result<QueueingHint, String> {
    match event {
        EventInner::Node(_, _) => Err(format!(
            "event inner {event:?} not match event resource pod"
        )),
        EventInner::Pod(_old, new) => {
            if new.is_some() && new.unwrap().name == pod.name {
                log::trace!(
                    "a new toleration is added for the unschedulable Pod, and it may make it schedulable. pod {pod:?}"
                );
                Ok(QueueingHint::Queue)
            } else {
                Ok(QueueingHint::Skip)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_state::CycleState;
    use crate::models::{NodeSpec, PodSpec, QueuedInfo};
    use common::{Taint, TaintEffect, TaintKey, Toleration, TolerationOperator};
    #[test]
    fn test_taint_toleration_filter_no_taints() {
        let plugin = TaintToleration;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: false,
                taints: vec![],
            },
            ..Default::default()
        };

        // Should succeed when node has no taints
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_taint_toleration_filter_with_tolerated_taints() {
        let plugin = TaintToleration;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                tolerations: vec![Toleration {
                    key: Some(TaintKey::NodeNotReady),
                    operator: TolerationOperator::Exists,
                    value: "".to_string(),
                    effect: Some(TaintEffect::NoSchedule),
                }],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: false,
                taints: vec![Taint {
                    key: TaintKey::NodeNotReady,
                    effect: TaintEffect::NoSchedule,
                    value: "".to_string(),
                }],
            },
            ..Default::default()
        };

        // Should succeed when pod tolerates node taints
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_taint_toleration_filter_with_untolerated_taints() {
        let plugin = TaintToleration;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: false,
                taints: vec![Taint {
                    key: TaintKey::NodeNotReady,
                    effect: TaintEffect::NoSchedule,
                    value: "".to_string(),
                }],
            },
            ..Default::default()
        };

        // Should fail when pod doesn't tolerate node taints
        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::UnschedulableAndUnresolvable);
        assert!(result.reasons[0].contains("untolerated taint"));
    }

    #[test]
    fn test_taint_toleration_pre_score() {
        let plugin = TaintToleration;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                tolerations: vec![Toleration {
                    key: Some(TaintKey::NodeNotReady),
                    operator: TolerationOperator::Exists,
                    value: "".to_string(),
                    effect: Some(TaintEffect::PreferNoSchedule),
                }],
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let status = plugin.pre_score(&mut state, &pod, vec![]);
        assert_eq!(status.code, Code::Success);

        // Verify state was written
        let state_data = state.read::<Vec<Toleration>>(PRE_SCORE_KEY);
        assert!(state_data.is_some());
        let state_data = state_data.unwrap();
        assert_eq!(state_data.len(), 1);
        assert!(matches!(
            state_data[0].effect,
            Some(TaintEffect::PreferNoSchedule)
        ));
    }

    #[test]
    fn test_taint_toleration_score_no_prefer_no_schedule_taints() {
        let plugin = TaintToleration;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: false,
                taints: vec![],
            },
            ..Default::default()
        };

        // Set up pre-score state
        state.write(PRE_SCORE_KEY, Box::new(Vec::<Toleration>::new()));

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Success);
        assert_eq!(score, 0); // No PreferNoSchedule taints to tolerate
    }

    #[test]
    fn test_taint_toleration_score_with_prefer_no_schedule_taints() {
        let plugin = TaintToleration;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            spec: NodeSpec {
                unschedulable: false,
                taints: vec![Taint {
                    key: TaintKey::NodeNotReady,
                    effect: TaintEffect::PreferNoSchedule,
                    value: "".to_string(),
                }],
            },
            ..Default::default()
        };

        // Set up pre-score state with tolerations
        state.write(
            PRE_SCORE_KEY,
            Box::new(vec![Toleration {
                key: Some(TaintKey::NodeNotReady),
                operator: TolerationOperator::Exists,
                value: "".to_string(),
                effect: Some(TaintEffect::PreferNoSchedule),
            }]),
        );

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Success);
        assert_eq!(score, 1); // Should score 1 for tolerated PreferNoSchedule taint
    }

    #[test]
    fn test_taint_toleration_events_to_register() {
        let plugin = TaintToleration;
        let events = plugin.events_to_register();

        assert_eq!(events.len(), 2);

        let node_event = &events[0];
        assert!(matches!(node_event.event.resource, EventResource::Node));
        assert!(node_event.queueing_hint_fn.is_some());

        let pod_event = &events[1];
        assert!(matches!(pod_event.event.resource, EventResource::Pod));
        assert!(pod_event.queueing_hint_fn.is_some());
    }

    #[test]
    fn test_taint_toleration_plugin_name() {
        let plugin = TaintToleration;
        assert_eq!(plugin.name(), "TaintToleration");
    }
}
