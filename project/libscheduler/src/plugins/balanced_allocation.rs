use std::cmp::Ordering;

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
            log::trace!("Skipping BalancedAllocation scoring for best-effort pod {pod:?}");
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

        let std = match resource_fractions.len().cmp(&2) {
            Ordering::Equal => (resource_fractions[0] - resource_fractions[1]).abs() / 2.0,
            Ordering::Greater => {
                let mean = total_fraction / resource_fractions.len() as f64;
                let variance = resource_fractions
                    .iter()
                    .map(|&f| (f - mean).powi(2))
                    .sum::<f64>()
                    / resource_fractions.len() as f64;
                variance.sqrt()
            }
            Ordering::Less => 0.0,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle_state::CycleState;
    use crate::models::{PodSpec, QueuedInfo};

    #[test]
    fn test_balanced_allocation_pre_score_best_effort_pod() {
        let plugin = BalancedAllocation::default();
        let mut state = CycleState::default();

        let pod = PodInfo {
            name: "test-pod".to_string(),
            labels: std::collections::HashMap::new(),
            spec: PodSpec {
                resources: ResourcesRequirements { cpu: 0, memory: 0 },
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        // Should skip for best-effort pods
        let status = plugin.pre_score(&mut state, &pod, vec![]);
        assert_eq!(status.code, Code::Skip);
    }

    #[test]
    fn test_balanced_allocation_pre_score_normal_pod() {
        let plugin = BalancedAllocation::default();
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

        let status = plugin.pre_score(&mut state, &pod, vec![]);
        assert_eq!(status.code, Code::Success);

        // Verify state was written
        let state_data =
            state.read::<BalancedAllocationPreScoreState>(BALANCED_ALLOCATION_PRE_SCORE_KEY);
        assert!(state_data.is_some());
        let state_data = state_data.unwrap();
        assert_eq!(state_data.pod_requests.cpu, 1000);
        assert_eq!(state_data.pod_requests.memory, 1024 * 1024 * 1024);
    }

    #[test]
    fn test_balanced_allocation_score() {
        let plugin = BalancedAllocation::default();
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

        // Set up pre-score state
        state.write(
            BALANCED_ALLOCATION_PRE_SCORE_KEY,
            Box::new(BalancedAllocationPreScoreState {
                pod_requests: ResourcesRequirements {
                    cpu: 1000,
                    memory: 1024 * 1024 * 1024,
                },
            }),
        );

        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Success);
        // Score should be positive for balanced allocation
        assert!(score > 0);
        assert!(score <= 100); // Should be normalized
    }

    #[test]
    fn test_balanced_allocation_score_no_state() {
        let plugin = BalancedAllocation::default();
        let mut state = CycleState::default();

        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "test-pod".to_string(),
            spec: PodSpec::default(),
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };

        let node = NodeInfo::default();

        // No pre-score state set up
        let (score, status) = plugin.score(&mut state, &pod, node);
        assert_eq!(status.code, Code::Error);
        assert_eq!(score, 0);
    }

    #[test]
    fn test_balanced_allocation_balanced_resource_scorer() {
        let plugin = BalancedAllocation::default();

        // Test with balanced utilization
        let requested = vec![2000, 2 * 1024 * 1024 * 1024]; // CPU: 2000mc, Memory: 2GB
        let allocatable = vec![4000, 8 * 1024 * 1024 * 1024]; // CPU: 4000mc, Memory: 8GB
        let score = plugin.balanced_resource_scorer(&requested, &allocatable);

        // Should give high score for balanced utilization
        assert!(score > 50);
        assert!(score <= 100);

        // Test with unbalanced utilization
        let unbalanced_requested = vec![3500, 1024 * 1024 * 1024]; // CPU: 3500mc, Memory: 1GB
        let unbalanced_score = plugin.balanced_resource_scorer(&unbalanced_requested, &allocatable);

        // Should give lower score for unbalanced utilization
        assert!(unbalanced_score < score);
    }

    #[test]
    fn test_balanced_allocation_calculate_resource_lists() {
        let plugin = BalancedAllocation::default();

        let pod_requests = ResourcesRequirements {
            cpu: 1000,
            memory: 1024 * 1024 * 1024,
        };
        let pod_list = plugin.calculate_pod_resource_request_list(&pod_requests);
        assert_eq!(pod_list, vec![1000, 1024 * 1024 * 1024]);

        let node_info = NodeInfo {
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

        let allocatable_list = plugin.calculate_node_allocatable_list(&node_info);
        assert_eq!(allocatable_list, vec![4000, 8 * 1024 * 1024 * 1024]);

        let requested_list = plugin.calculate_node_requested_list(&node_info);
        assert_eq!(requested_list, vec![2000, 2 * 1024 * 1024 * 1024]);
    }

    #[test]
    fn test_balanced_allocation_is_best_effort_pod() {
        let plugin = BalancedAllocation::default();

        let best_effort = ResourcesRequirements { cpu: 0, memory: 0 };
        assert!(plugin.is_best_effort_pod(&best_effort));

        let normal_pod = ResourcesRequirements {
            cpu: 1000,
            memory: 1024 * 1024 * 1024,
        };
        assert!(!plugin.is_best_effort_pod(&normal_pod));
    }

    #[test]
    fn test_balanced_allocation_events_to_register() {
        let plugin = BalancedAllocation::default();
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
    fn test_balanced_allocation_plugin_name() {
        let plugin = BalancedAllocation::default();
        assert_eq!(plugin.name(), "NodeResourcesBalancedAllocation");
    }
}
