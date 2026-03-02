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

#[derive(Clone, Default)]
pub enum ScoringStrategy {
    #[default]
    LeastAllocated,
    MostAllocated,
    RequestedToCapacityRatio,
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
                log::trace!("pod was deleted, may make unscheduled pod schedulable. pod {pod:?}");
                Ok(QueueingHint::Queue)
            } else {
                Ok(QueueingHint::Skip)
            }
        }
        _ => Err(format!(
            "event inner {event:?} not match event resource pod"
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
                        "node was added and fits pod resource requests. pod {pod:?} node {modified:?}"
                    );
                    Ok(QueueingHint::Queue)
                } else {
                    log::trace!(
                        "node was updated and fits pod resource requests. pod {pod:?} node {modified:?}"
                    );
                    Ok(QueueingHint::Queue)
                }
            } else {
                log::trace!(
                    "node was created or updated, but doesn't have enough resources. pod {pod:?} node {modified:?}"
                );
                Ok(QueueingHint::Skip)
            }
        }
        _ => Err(format!(
            "event inner {event:?} not match event resource node"
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
    100 - calculate_most_allocated_score(pod_requests, node_info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_state::CycleState;
    use crate::models::{PodSpec, QueuedInfo};

    #[test]
    fn test_node_resources_fit_pre_filter() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            name: "test-pod".to_string(),
            labels: std::collections::HashMap::new(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let (result, status) = plugin.pre_filter(&mut state, &pod, vec![]);
        assert_eq!(status.code, Code::Success);
        assert!(result.node_names.is_empty());

        let state_data = state.read::<PreFilterState>("PreFilterNodeResourcesFit");
        assert!(state_data.is_some());
        let state_data = state_data.unwrap();
        assert_eq!(state_data.pod_requests.cpu, 1000);
        assert_eq!(state_data.pod_requests.memory, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_node_resources_fit_filter_sufficient_resources() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            allocatable: ResourcesRequirements {
                cpu: 4000,
                memory: 8 * 1024 * 1024 * 1024,
            },
            requested: ResourcesRequirements {
                cpu: 2000,
                memory: 2 * 1024 * 1024 * 1024,
            },
            ..Default::default()
        };

        state.write(
            "PreFilterNodeResourcesFit",
            Box::new(PreFilterState {
                pod_requests: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
            }),
        );

        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Success);
    }

    #[test]
    fn test_node_resources_fit_filter_insufficient_cpu() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 3000,
                    memory: 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            allocatable: ResourcesRequirements {
                cpu: 4000,
                memory: 8 * 1024 * 1024 * 1024,
            },
            requested: ResourcesRequirements {
                cpu: 2000,
                memory: 2 * 1024 * 1024 * 1024,
            },
            ..Default::default()
        };

        state.write(
            "PreFilterNodeResourcesFit",
            Box::new(PreFilterState {
                pod_requests: ResourcesRequirements {
                    cpu: 3000,
                    memory: 1024 * 1024 * 1024,
                },
            }),
        );

        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Unschedulable);
        assert!(
            result
                .reasons
                .contains(&"node(s) didn't have enough resource(s)".to_string())
        );
    }

    #[test]
    fn test_node_resources_fit_filter_insufficient_memory() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 1000,
                    memory: 6 * 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            allocatable: ResourcesRequirements {
                cpu: 4000,
                memory: 8 * 1024 * 1024 * 1024,
            },
            requested: ResourcesRequirements {
                cpu: 2000,
                memory: 3 * 1024 * 1024 * 1024,
            },
            ..Default::default()
        };

        state.write(
            "PreFilterNodeResourcesFit",
            Box::new(PreFilterState {
                pod_requests: ResourcesRequirements {
                    cpu: 1000,
                    memory: 6 * 1024 * 1024 * 1024,
                },
            }),
        );

        let result = plugin.filter(&mut state, &pod, node);
        assert_eq!(result.code, Code::Unschedulable);
        assert!(
            result
                .reasons
                .contains(&"node(s) didn't have enough resource(s)".to_string())
        );
    }

    #[test]
    fn test_node_resources_fit_pre_score() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let status = plugin.pre_score(&mut state, &pod, vec![]);
        assert_eq!(status.code, Code::Success);

        // Verify state was written
        let state_data = state.read::<PreScoreState>(PRE_SCORE_KEY);
        assert!(state_data.is_some());
        let state_data = state_data.unwrap();
        assert_eq!(state_data.pod_requests.cpu, 1000);
        assert_eq!(state_data.pod_requests.memory, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_node_resources_fit_score_least_allocated() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            allocatable: ResourcesRequirements {
                cpu: 4000,
                memory: 8 * 1024 * 1024 * 1024,
            },
            requested: ResourcesRequirements {
                cpu: 2000,
                memory: 2 * 1024 * 1024 * 1024,
            },
            ..Default::default()
        };

        state.write(
            PRE_SCORE_KEY,
            Box::new(PreScoreState {
                pod_requests: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
            }),
        );

        state.write(
            SCORING_STRATEGY_CONFIG_KEY,
            Box::new(ScoringStrategy::LeastAllocated),
        );

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Success);
        assert_eq!(score, 44);
    }

    #[test]
    fn test_node_resources_fit_score_most_allocated() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo {
            name: "test-node".to_string(),
            allocatable: ResourcesRequirements {
                cpu: 4000,
                memory: 8 * 1024 * 1024 * 1024,
            },
            requested: ResourcesRequirements {
                cpu: 2000,
                memory: 2 * 1024 * 1024 * 1024,
            },
            ..Default::default()
        };

        state.write(
            PRE_SCORE_KEY,
            Box::new(PreScoreState {
                pod_requests: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
            }),
        );

        state.write(
            SCORING_STRATEGY_CONFIG_KEY,
            Box::new(ScoringStrategy::MostAllocated),
        );

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Success);
        assert!(score > 0);
    }

    #[test]
    fn test_node_resources_fit_score_no_strategy() {
        let plugin = Fit;
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo::default();

        state.write(
            PRE_SCORE_KEY,
            Box::new(PreScoreState {
                pod_requests: ResourcesRequirements::default(),
            }),
        );

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Error);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_node_resources_fit_events_to_register() {
        let plugin = Fit;
        let events = plugin.events_to_register();

        assert_eq!(events.len(), 2);

        let pod_event = &events[0];
        assert!(matches!(pod_event.event.resource, EventResource::Pod));
        assert!(pod_event.queueing_hint_fn.is_some());

        let node_event = &events[1];
        assert!(matches!(node_event.event.resource, EventResource::Node));
        assert!(node_event.queueing_hint_fn.is_some());
    }

    #[test]
    fn test_node_resources_fit_plugin_name() {
        let plugin = Fit;
        assert_eq!(plugin.name(), "NodeResourcesFit");
    }
}
