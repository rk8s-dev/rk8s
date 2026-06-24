use crate::{
    cycle_state::CycleState,
    models::{NodeInfo, PodInfo},
    plugins::{
        ActionType, ClusterEvent, ClusterEventWithHint, Code, DefaultNormalizeScore,
        EnqueueExtension, EventInner, EventResource, FilterPlugin, Plugin, PreFilterPlugin,
        PreFilterResult, PreScorePlugin, QueueingHint, ScoreExtension, ScorePlugin, Status,
    },
};

pub struct NodeGpuResourcesFit;

impl Plugin for NodeGpuResourcesFit {
    fn name(&self) -> &str {
        "NodeGpuResourcesFit"
    }
}

const PRE_FILTER_KEY: &str = "PreFilterNodeGpuResourcesFit";
const PRE_SCORE_KEY: &str = "PreScoreNodeGpuResourcesFit";

struct GpuPreFilterState {
    gpu_request: u32,
    gpu_memory_request: Option<u64>,
}

struct GpuPreScoreState {
    gpu_request: u32,
}

fn is_gpu_fit(pod: &PodInfo, node: &NodeInfo) -> bool {
    if pod.spec.gpu_request == 0 {
        return true;
    }
    let Some(gpu) = &node.gpu_resources else {
        return false;
    };
    let avail = gpu.total.saturating_sub(gpu.requested);
    if avail < pod.spec.gpu_request {
        return false;
    }
    if let Some(req_mem) = pod.spec.gpu_memory_request
        && req_mem > gpu.memory_per_gpu
    {
        return false;
    }
    true
}

fn is_schedulable_after_pod_event(pod: PodInfo, event: EventInner) -> Result<QueueingHint, String> {
    match event {
        EventInner::Pod(_, modified) => {
            if modified.is_none() {
                log::trace!("pod was deleted, may free GPU for unscheduled pod. pod {pod:?}");
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
        EventInner::Node(_, modified) => {
            if is_gpu_fit(&pod, &modified) {
                Ok(QueueingHint::Queue)
            } else {
                Ok(QueueingHint::Skip)
            }
        }
        _ => Err(format!(
            "event inner {event:?} not match event resource node"
        )),
    }
}

impl EnqueueExtension for NodeGpuResourcesFit {
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

impl PreFilterPlugin for NodeGpuResourcesFit {
    fn pre_filter(
        &self,
        state: &mut CycleState,
        pod: &PodInfo,
        _nodes: Vec<NodeInfo>,
    ) -> (PreFilterResult, Status) {
        if pod.spec.gpu_request == 0 {
            return (
                PreFilterResult { node_names: vec![] },
                Status::new(Code::Skip, vec![]),
            );
        }
        state.write(
            PRE_FILTER_KEY,
            Box::new(GpuPreFilterState {
                gpu_request: pod.spec.gpu_request,
                gpu_memory_request: pod.spec.gpu_memory_request,
            }),
        );
        (PreFilterResult { node_names: vec![] }, Status::default())
    }
}

const ERR_REASON_GPU_COUNT: &str = "node(s) didn't have enough GPU(s)";
const ERR_REASON_GPU_MEMORY: &str = "node(s) didn't have enough GPU memory per card";
const ERR_REASON_NO_GPU: &str = "node has no GPU resources";

impl FilterPlugin for NodeGpuResourcesFit {
    fn filter(&self, state: &mut CycleState, _pod: &PodInfo, node: NodeInfo) -> Status {
        let Some(s) = state.read::<GpuPreFilterState>(PRE_FILTER_KEY) else {
            return Status::default();
        };
        let Some(gpu) = &node.gpu_resources else {
            return Status::new(Code::Unschedulable, vec![ERR_REASON_NO_GPU.to_string()]);
        };
        let avail = gpu.total.saturating_sub(gpu.requested);
        if avail < s.gpu_request {
            return Status::new(Code::Unschedulable, vec![ERR_REASON_GPU_COUNT.to_string()]);
        }
        if let Some(req_mem) = s.gpu_memory_request
            && req_mem > gpu.memory_per_gpu
        {
            return Status::new(Code::Unschedulable, vec![ERR_REASON_GPU_MEMORY.to_string()]);
        }
        Status::default()
    }
}

impl PreScorePlugin for NodeGpuResourcesFit {
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, _nodes: Vec<NodeInfo>) -> Status {
        if pod.spec.gpu_request == 0 {
            return Status::new(Code::Skip, vec![]);
        }
        state.write(
            PRE_SCORE_KEY,
            Box::new(GpuPreScoreState {
                gpu_request: pod.spec.gpu_request,
            }),
        );
        Status::default()
    }
}

impl ScorePlugin for NodeGpuResourcesFit {
    fn score(&self, state: &mut CycleState, _pod: &PodInfo, node: NodeInfo) -> (i64, Status) {
        let Some(s) = state.read::<GpuPreScoreState>(PRE_SCORE_KEY) else {
            return (0, Status::error("missing GPU prescore state"));
        };
        let Some(gpu) = &node.gpu_resources else {
            return (0, Status::default());
        };
        if gpu.total == 0 {
            return (0, Status::default());
        }
        let used_after = gpu.requested.saturating_add(s.gpu_request).min(gpu.total);
        // LeastAllocated: prefer nodes with more free GPUs after assignment.
        let free_after = gpu.total.saturating_sub(used_after);
        let score = (free_after as i64) * 100 / (gpu.total as i64);
        (score, Status::default())
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
    use crate::models::{GpuResources, PodSpec, QueuedInfo};
    use std::collections::HashMap;

    fn pod_with_gpu(gpu: u32, gpu_mem: Option<u64>) -> PodInfo {
        PodInfo {
            name: "p".into(),
            labels: HashMap::new(),
            spec: PodSpec {
                gpu_request: gpu,
                gpu_memory_request: gpu_mem,
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        }
    }

    fn node_with_gpu(name: &str, total: u32, requested: u32, mem_per_gpu: u64) -> NodeInfo {
        NodeInfo {
            name: name.into(),
            gpu_resources: Some(GpuResources {
                total,
                requested,
                memory_per_gpu: mem_per_gpu,
                model: String::new(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn pre_filter_skips_when_no_gpu_request() {
        let plugin = NodeGpuResourcesFit;
        let mut state = CycleState::default();
        let pod = pod_with_gpu(0, None);
        let (_, sta) = plugin.pre_filter(&mut state, &pod, vec![]);
        assert_eq!(sta.code, Code::Skip);
    }

    #[test]
    fn filter_rejects_insufficient_gpu_count() {
        let plugin = NodeGpuResourcesFit;
        let mut state = CycleState::default();
        let pod = pod_with_gpu(4, None);
        plugin.pre_filter(&mut state, &pod, vec![]);
        let node = node_with_gpu("n", 2, 0, 0);
        let sta = plugin.filter(&mut state, &pod, node);
        assert_eq!(sta.code, Code::Unschedulable);
    }

    #[test]
    fn filter_passes_when_enough_gpu() {
        let plugin = NodeGpuResourcesFit;
        let mut state = CycleState::default();
        let pod = pod_with_gpu(2, None);
        plugin.pre_filter(&mut state, &pod, vec![]);
        let node = node_with_gpu("n", 8, 4, 0);
        let sta = plugin.filter(&mut state, &pod, node);
        assert_eq!(sta.code, Code::Success);
    }

    #[test]
    fn filter_rejects_when_gpu_memory_too_small() {
        let plugin = NodeGpuResourcesFit;
        let mut state = CycleState::default();
        let pod = pod_with_gpu(1, Some(80 * 1024 * 1024 * 1024));
        plugin.pre_filter(&mut state, &pod, vec![]);
        let node = node_with_gpu("n", 8, 0, 40 * 1024 * 1024 * 1024);
        let sta = plugin.filter(&mut state, &pod, node);
        assert_eq!(sta.code, Code::Unschedulable);
    }

    #[test]
    fn filter_rejects_node_without_gpu_resources() {
        let plugin = NodeGpuResourcesFit;
        let mut state = CycleState::default();
        let pod = pod_with_gpu(1, None);
        plugin.pre_filter(&mut state, &pod, vec![]);
        let node = NodeInfo {
            name: "n".into(),
            ..Default::default()
        };
        let sta = plugin.filter(&mut state, &pod, node);
        assert_eq!(sta.code, Code::Unschedulable);
    }

    #[test]
    fn score_prefers_more_free_gpus() {
        let plugin = NodeGpuResourcesFit;
        let mut state = CycleState::default();
        let pod = pod_with_gpu(2, None);
        plugin.pre_score(&mut state, &pod, vec![]);
        let node_a = node_with_gpu("a", 8, 0, 0);
        let node_b = node_with_gpu("b", 8, 4, 0);
        let (sa, _) = plugin.score(&mut state, &pod, node_a);
        let (sb, _) = plugin.score(&mut state, &pod, node_b);
        assert!(sa > sb);
    }
}
