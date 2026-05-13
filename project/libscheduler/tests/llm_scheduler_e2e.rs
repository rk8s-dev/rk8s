use std::collections::HashMap;
use std::time::Duration;

use libscheduler::models::{
    GangSpec, GpuResources, NodeInfo, PodInfo, PodSpec, ResourcesRequirements, TopologyConstraint,
};
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::scheduler::Scheduler;
use tokio::time::timeout;

const NVLINK_KEY: &str = "topology.rk8s.io/nvlink-domain";

fn gpu_node(name: &str, total_gpu: u32, domain: &str) -> NodeInfo {
    let mut labels = HashMap::new();
    labels.insert(NVLINK_KEY.to_string(), domain.to_string());
    NodeInfo {
        name: name.to_string(),
        labels,
        allocatable: ResourcesRequirements {
            cpu: 100_000,
            memory: 1_000_000_000_000,
        },
        gpu_resources: Some(GpuResources {
            total: total_gpu,
            requested: 0,
            memory_per_gpu: 80 * 1024 * 1024 * 1024,
            model: "H100".into(),
        }),
        ..Default::default()
    }
}

fn gang_pod(name: &str, gpu: u32, gang_id: &str, gang_size: u32) -> PodInfo {
    PodInfo::new(
        name.to_string(),
        HashMap::new(),
        PodSpec {
            resources: ResourcesRequirements {
                cpu: 1_000,
                memory: 1024 * 1024 * 1024,
            },
            priority: 1,
            gpu_request: gpu,
            gang: Some(GangSpec {
                id: gang_id.into(),
                size: gang_size,
            }),
            topology_constraints: vec![TopologyConstraint {
                topology_key: NVLINK_KEY.into(),
                same_value: true,
            }],
            ..Default::default()
        },
    )
}

#[tokio::test]
async fn gang_with_topology_lands_on_same_domain() {
    // node-A: 8 GPU in domain0 (can fit 4 pods x 2 GPU = 8 GPU)
    // node-B: 4 GPU in domain1 (cannot fit a 4-pod gang of 2 GPU each)
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(gpu_node("node-A", 8, "domain0"))
        .await;
    scheduler
        .update_cache_node(gpu_node("node-B", 4, "domain1"))
        .await;

    let mut rx = scheduler.run();
    // update_cache_pod writes the pod into the cache *and* enqueues it.
    // enqueue() alone only pushes the pod name into the queue; schedule_one
    // then fetches the pod from cache and silently drops it when cache miss.
    for i in 0..4u32 {
        scheduler
            .update_cache_pod(gang_pod(&format!("p{i}"), 2, "g", 4))
            .await;
    }

    let mut got = Vec::new();
    for _ in 0..4 {
        let res = timeout(Duration::from_secs(10), rx.recv())
            .await
            .expect("timed out waiting for assignment")
            .expect("scheduler dropped sender")
            .expect("scheduling error");
        got.push(res);
    }
    // All four must land on node-A (only domain that can satisfy size 4 with 2 GPU each).
    for a in &got {
        assert_eq!(a.node_name, "node-A", "pod {} on wrong node", a.pod_name);
    }
}

#[tokio::test]
async fn gang_timeout_rolls_back_when_unable_to_satisfy() {
    // Only node-A with 4 GPU in domain0 — gang size 4, gpu_request=2 each → only 2 fit.
    // The 2 successful assumes should be rolled back after the gang times out.
    let scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default())
        .with_gang_timeout(Duration::from_millis(200), Duration::from_millis(50));
    let mut scheduler = scheduler;
    scheduler
        .update_cache_node(gpu_node("node-A", 4, "domain0"))
        .await;

    let mut rx = scheduler.run();
    for i in 0..4u32 {
        scheduler
            .update_cache_pod(gang_pod(&format!("p{i}"), 2, "g", 4))
            .await;
    }

    // No assignment should be delivered: node-A has only 4 GPU so at most 2 pods
    // (2 GPU each) can be assumed, gang-size=4 is never filled, then times out
    // and all assumes are rolled back.
    let res = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(
        res.is_err(),
        "no Assignment should be sent for an unfillable gang"
    );
}
