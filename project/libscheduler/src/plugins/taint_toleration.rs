use log;
use std::rc::Rc;

use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo, Taint, TaintEffect, Toleration},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, FilterPlugin, Plugin, PreScorePlugin,
        QueueingHint, ScoreExtension, ScorePlugin, Status,
    },
};

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
        let not_schedule_taints_filter = |t: &&Taint| {
            return matches!(t.effect, TaintEffect::NoSchedule | TaintEffect::NoExecute);
        };
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

fn tolerations_tolerate_taint(tolerations: &Vec<Toleration>, taint: &Taint) -> bool {
    tolerations.iter().any(|to| to.tolerate(taint))
}

fn find_untolerated_taint<'a>(
    taints: &'a Vec<Taint>,
    tolerations: &Vec<Toleration>,
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
        state.write(
            PRE_SCORE_KEY,
            Rc::new(toleration_prefer_no_schedule),
        );
        Status::default()
    }
}

impl ScorePlugin for TaintToleration {
    fn score(&self, state: &mut CycleState, _: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let s = state.read::<Vec<Toleration>>(PRE_SCORE_KEY);
        if let Ok(tolerations) = s {
            let score = node_info
                .spec
                .taints
                .iter()
                .filter(|&t| {
                    matches!(t.effect, TaintEffect::PreferNoSchedule)
                        && tolerations_tolerate_taint(&tolerations, t)
                })
                .count();
            (score as i64, Status::default())
        } else {
            (0, Status::error("PreScoreState not found"))
        }
    }

    fn score_extension() -> Box<dyn ScoreExtension> {
        Box::new(DefaultNormalizeScore {
            max_score: 100,
            reverse: true,
        })
    }
}

impl EnqueueExtension for TaintToleration {
    fn events_to_register() -> Vec<super::ClusterEventWithHint> {
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
            "event inner {:?} not match event resource node",
            event
        )),
        EventInner::Node(old, new) => {
            let was_untolerated = old.is_none() ||
                find_untolerated_taint(&old.unwrap().spec.taints, &pod.spec.tolerations, |&t| {
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
                    "node was created or updated, and this may make the Pod rejected by TaintToleration plugin in the previous scheduling cycle schedulable. node {:?}, pod: {:?}",
                    new,
                    pod
                );
                Ok(QueueingHint::Queue)
            } else {
                log::trace!(
                    "node was created or updated, but it doesn't change the TaintToleration plugin's decision node {:?}, pod: {:?}",
                    new,
                    pod
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
            "event inner {:?} not match event resource pod",
            event
        )),
        EventInner::Pod(_old, new) => {
            if new.is_some() && new.unwrap().name == pod.name {
                log::trace!(
                    "a new toleration is added for the unschedulable Pod, and it may make it schedulable. pod {:?}",
                    pod
                );
                Ok(QueueingHint::Queue)
            } else {
                Ok(QueueingHint::Skip)
            }
        }
    }
}
