
use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo, ResourcesRequirements},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, FilterPlugin, Plugin, PreFilterPlugin,
        PreFilterResult, PreScorePlugin, QueueingHint, ScoreExtension, ScorePlugin, Status,
    },
};

pub struct Fit;

const SCORING_STRATEGY_CONFIG_KEY: &str = "ScoringStrategyConfig";

#[derive(Clone)]
pub enum ScoringStrategy {
    LeastAllocated,
    MostAllocated,
    RequestedToCapacityRatio,
}

impl Default for ScoringStrategy {
    fn default() -> Self {
        Self::LeastAllocated
    }
}

impl Plugin for Fit {
    fn name(&self) -> &str {
        "NodeResourcesFit"
    }
}

impl EnqueueExtension for Fit {
    fn events_to_register(&self) -> Vec<ClusterEventWithHint> {
        vec![
            ClusterEventWithHint {
                event: ClusterEvent {
                    resource: EventResource::Pod,
                    action_type: ActionType::Delete,
                },
                queueing_hint_fn: Some(Box::new(is_schedulable_after_pod_event)),
            },
            ClusterEventWithHint {
                event: ClusterEvent {
                    resource: EventResource::Node,
                    action_type: ActionType::Add | ActionType::UpdateNodeAllocatable,
                },
                queueing_hint_fn: Some(Box::new(is_schedulable_after_node_change)),
            },
        ]
    }
}

fn is_schedulable_after_pod_event(pod: PodInfo, event: EventInner) -> Result<QueueingHint, String> {
    match event {
        EventInner::Pod(_original, modified) => {
            if modified.is_none() {
                log::trace!(
                    "pod was deleted, may make unscheduled pod schedulable. pod {:?}",
                    pod
                );
                Ok(QueueingHint::Queue)
            } else {
                Ok(QueueingHint::Skip)
            }
        }
        _ => Err(format!(
            "event inner {:?} not match event resource pod",
            event
        )),
    }
}

fn is_schedulable_after_node_change(
    pod: PodInfo,
    event: EventInner,
) -> Result<QueueingHint, String> {
    match event {
        EventInner::Node(original, modified) => {
            let pod_requests = pod.spec.resources.clone();
            if is_fit(&pod_requests, &modified) {
                if original.is_none() {
                    log::trace!(
                        "node was added and fits pod resource requests. pod {:?} node {:?}",
                        pod,
                        modified
                    );
                    Ok(QueueingHint::Queue)
                } else {
                    log::trace!(
                        "node was updated and fits pod resource requests. pod {:?} node {:?}",
                        pod,
                        modified
                    );
                    Ok(QueueingHint::Queue)
                }
            } else {
                log::trace!(
                    "node was created or updated, but doesn't have enough resources. pod {:?} node {:?}",
                    pod,
                    modified
                );
                Ok(QueueingHint::Skip)
            }
        }
        _ => Err(format!(
            "event inner {:?} not match event resource node",
            event
        )),
    }
}

struct PreFilterState {
    pod_requests: ResourcesRequirements,
}

impl PreFilterPlugin for Fit {
    fn pre_filter(
        &self,
        state: &mut CycleState,
        pod: &PodInfo,
        _nodes: Vec<NodeInfo>,
    ) -> (PreFilterResult, Status) {
        let pod_requests = pod.spec.resources.clone();
        state.write(
            "PreFilterNodeResourcesFit",
            Box::new(PreFilterState { pod_requests }),
        );
        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

fn is_fit(pod_requests: &ResourcesRequirements, node: &NodeInfo) -> bool {
    let node_allocatable = &node.allocatable;
    let node_requested = &node.requested;

    if pod_requests.cpu > 0 && pod_requests.cpu > (node_allocatable.cpu - node_requested.cpu) {
        return false;
    }

    if pod_requests.memory > 0
        && pod_requests.memory > (node_allocatable.memory - node_requested.memory)
    {
        return false;
    }

    true
}

const ERR_REASON_RESOURCES: &str = "node(s) didn't have enough resource(s)";

impl FilterPlugin for Fit {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> Status {
        let s = state.read::<PreFilterState>("PreFilterNodeResourcesFit");
        if let Some(sta) = s {
            if !is_fit(&sta.pod_requests, &node_info) {
                Status::new(Code::Unschedulable, vec![ERR_REASON_RESOURCES.to_string()])
            } else {
                Status::default()
            }
        } else {
            Status::error("Failed to read pre-filter state")
        }
    }
}

struct PreScoreState {
    pod_requests: ResourcesRequirements,
}

const PRE_SCORE_KEY: &str = "PreScoreNodeResourcesFit";

impl PreScorePlugin for Fit {
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, _nodes: Vec<NodeInfo>) -> Status {
        let pod_requests = pod.spec.resources.clone();

        state.write(PRE_SCORE_KEY, Box::new(PreScoreState { pod_requests }));
        Status::default()
    }
}

impl ScorePlugin for Fit {
    fn score(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let s = state.read::<PreScoreState>(PRE_SCORE_KEY);
        let strategy = state.read::<ScoringStrategy>(SCORING_STRATEGY_CONFIG_KEY);
        if strategy.is_none() {
            return (0, Status::error("error configuring scoring strategy"));
        }
        let strategy = strategy.unwrap();
        if let Some(sta) = s {
            // Use least allocated scoring strategy
            let score = match *strategy {
                ScoringStrategy::MostAllocated => {
                    calculate_most_allocated_score(&sta.pod_requests, &node_info)
                }
                ScoringStrategy::LeastAllocated => {
                    calculate_least_allocated_score(&sta.pod_requests, &node_info)
                }
                // now we only have one type two type of resources, so we don't implement RequestedToCapacityRatio scoring algorithm now.
                // TODO: calculate_RequestedToCapacityRatio_score
                ScoringStrategy::RequestedToCapacityRatio => {
                    calculate_most_allocated_score(&sta.pod_requests, &node_info)
                }
            };
            (score, Status::default())
        } else {
            (
                0,
                Status::error("NodeResourcesFit scoring error when get pre-score state"),
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

fn calculate_most_allocated_score(
    pod_requests: &ResourcesRequirements,
    node_info: &NodeInfo,
) -> i64 {
    let allocatable = &node_info.allocatable;
    let requested = &node_info.requested;

    let cpu_utilization = if allocatable.cpu > 0 {
        (requested.cpu + pod_requests.cpu) as f64 / allocatable.cpu as f64
    } else {
        0.0
    };

    let memory_utilization = if allocatable.memory > 0 {
        (requested.memory + pod_requests.memory) as f64 / allocatable.memory as f64
    } else {
        0.0
    };

    let avg_utilization = (cpu_utilization + memory_utilization) / 2.0;
    (avg_utilization * 100.0) as i64
}

fn calculate_least_allocated_score(
    pod_requests: &ResourcesRequirements,
    node_info: &NodeInfo,
) -> i64 {
    ((1.0 - calculate_most_allocated_score(pod_requests, node_info) as f64) * 100.0) as i64
}
