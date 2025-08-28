use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo, ResourcesRequirements},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, Plugin, PreScorePlugin, QueueingHint,
        ScoreExtension, ScorePlugin, Status,
    },
};

pub struct BalancedAllocation {
    resources: Vec<ResourceName>,
}

impl Default for BalancedAllocation {
    fn default() -> Self {
        Self {
            // now we don't offer resources config
            resources: vec![ResourceName::Cpu, ResourceName::Memory],
        }
    }
}

#[derive(Clone, Debug)]
pub enum ResourceName {
    Cpu,
    Memory,
}

impl Plugin for BalancedAllocation {
    fn name(&self) -> &str {
        "NodeResourcesBalancedAllocation"
    }
}

impl EnqueueExtension for BalancedAllocation {
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

struct BalancedAllocationPreScoreState {
    pod_requests: ResourcesRequirements,
}

const BALANCED_ALLOCATION_PRE_SCORE_KEY: &str = "PreScoreNodeResourcesBalancedAllocation";

impl PreScorePlugin for BalancedAllocation {
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, _nodes: Vec<NodeInfo>) -> Status {
        let pod_requests = pod.spec.resources.clone();

        if self.is_best_effort_pod(&pod_requests) {
            log::trace!(
                "Skipping BalancedAllocation scoring for best-effort pod {:?}",
                pod
            );
            return Status::new(Code::Skip, vec![]);
        }

        state.write(
            BALANCED_ALLOCATION_PRE_SCORE_KEY,
            Box::new(BalancedAllocationPreScoreState { pod_requests }),
        );
        Status::default()
    }
}

impl BalancedAllocation {
    fn is_best_effort_pod(&self, pod_requests: &ResourcesRequirements) -> bool {
        pod_requests.cpu == 0 && pod_requests.memory == 0
    }

    fn calculate_pod_resource_request_list(
        &self,
        pod_requests: &ResourcesRequirements,
    ) -> Vec<u64> {
        let mut requests = Vec::new();

        for resource in &self.resources {
            match resource {
                ResourceName::Cpu => requests.push(pod_requests.cpu),
                ResourceName::Memory => requests.push(pod_requests.memory),
            }
        }

        requests
    }

    fn calculate_node_allocatable_list(&self, node_info: &NodeInfo) -> Vec<u64> {
        let mut allocatable = Vec::new();

        for resource in &self.resources {
            match resource {
                ResourceName::Cpu => allocatable.push(node_info.allocatable.cpu),
                ResourceName::Memory => allocatable.push(node_info.allocatable.memory),
            }
        }

        allocatable
    }

    fn calculate_node_requested_list(&self, node_info: &NodeInfo) -> Vec<u64> {
        let mut requested = Vec::new();

        for resource in &self.resources {
            match resource {
                ResourceName::Cpu => requested.push(node_info.requested.cpu),
                ResourceName::Memory => requested.push(node_info.requested.memory),
            }
        }

        requested
    }

    fn balanced_resource_scorer(&self, requested: &[u64], allocatable: &[u64]) -> u64 {
        let mut resource_fractions = Vec::new();
        let mut total_fraction = 0.0;

        for i in 0..requested.len() {
            if allocatable[i] == 0 {
                continue;
            }

            let mut fraction = requested[i] as f64 / allocatable[i] as f64;
            if fraction > 1.0 {
                fraction = 1.0;
            }

            total_fraction += fraction;
            resource_fractions.push(fraction);
        }

        let std = if resource_fractions.len() == 2 {
            (resource_fractions[0] - resource_fractions[1]).abs() / 2.0
        } else if resource_fractions.len() > 2 {
            let mean = total_fraction / resource_fractions.len() as f64;
            let variance = resource_fractions
                .iter()
                .map(|&f| (f - mean).powi(2))
                .sum::<f64>()
                / resource_fractions.len() as f64;
            variance.sqrt()
        } else {
            0.0
        };

        ((1.0 - std) * 100.0) as u64
    }
}

impl ScorePlugin for BalancedAllocation {
    fn score(&self, state: &mut CycleState, _pod: &PodInfo, node_info: NodeInfo) -> (i64, Status) {
        let s = state.read::<BalancedAllocationPreScoreState>(BALANCED_ALLOCATION_PRE_SCORE_KEY);

        if let Some(sta) = s {
            let pod_requests_list = self.calculate_pod_resource_request_list(&sta.pod_requests);
            let node_allocatable_list = self.calculate_node_allocatable_list(&node_info);
            let node_requested_list = self.calculate_node_requested_list(&node_info);
            let mut total_requested = Vec::new();
            for i in 0..pod_requests_list.len() {
                total_requested.push(node_requested_list[i] + pod_requests_list[i]);
            }

            let score = self.balanced_resource_scorer(&total_requested, &node_allocatable_list);

            (score as i64, Status::default())
        } else {
            (
                0,
                Status::new(Code::Error, vec!["can't read state".to_string()]),
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
