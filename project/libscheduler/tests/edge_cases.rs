use common::{Taint, TaintEffect, TaintKey, Toleration, TolerationOperator};
use libscheduler::models::{
    Affinity, NodeAffinity, NodeInfo, NodeSelector, NodeSelectorOperator, NodeSelectorRequirement,
    NodeSelectorTerm, NodeSpec, PodInfo, PodSpec, QueuedInfo, ResourcesRequirements,
};
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::scheduler::Scheduler;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
fn make_pod(name: &str, priority: u64, cpu: u64, memory: u64) -> PodInfo {
    PodInfo {
        name: name.to_string(),
        spec: PodSpec {
            resources: ResourcesRequirements { cpu, memory },
            priority,
            ..Default::default()
        },
        queued_info: QueuedInfo::default(),
        scheduled: None,
    }
}

fn make_node(name: &str, cpu: u64, memory: u64) -> NodeInfo {
    NodeInfo {
        name: name.to_string(),
        allocatable: ResourcesRequirements { cpu, memory },
        requested: ResourcesRequirements { cpu: 0, memory: 0 },
        spec: NodeSpec::default(),
        labels: HashMap::new(),
    }
}

#[tokio::test]
async fn test_scheduler_zero_resource_pods() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;

    scheduler
        .update_cache_pod(make_pod("zero-cpu", 10, 0, 1000))
        .await;
    scheduler
        .update_cache_pod(make_pod("zero-memory", 10, 1, 0))
        .await;
    scheduler
        .update_cache_pod(make_pod("zero-both", 10, 0, 0))
        .await;

    let mut rx = scheduler.run();
    let mut assignments = Vec::new();
    for _ in 0..3 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push(assignment.pod_name);
    }
    assignments.sort();

    let expected = vec!["zero-both", "zero-cpu", "zero-memory"];
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_exact_resource_match() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("exact-node", 100, 1000))
        .await;

    scheduler
        .update_cache_pod(make_pod("exact-pod", 10, 100, 1000))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "exact-pod");
    assert_eq!(assignment.node_name, "exact-node");
}

#[tokio::test]
async fn test_scheduler_multiple_taints_and_tolerations() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut multi_tainted_node = make_node("multi-tainted", 10, 10000);
    multi_tainted_node.spec.taints = vec![
        Taint {
            key: TaintKey::NodeNotReady,
            value: "true".to_string(),
            effect: TaintEffect::NoSchedule,
        },
        Taint {
            key: TaintKey::NodeMemoryPressure,
            value: "high".to_string(),
            effect: TaintEffect::NoSchedule,
        },
    ];

    scheduler.update_cache_node(multi_tainted_node).await;
    scheduler
        .update_cache_node(make_node("clean-node", 10, 10000))
        .await;

    let mut fully_tolerant_pod = make_pod("fully-tolerant", 10, 1, 1000);
    fully_tolerant_pod.spec.tolerations = vec![
        Toleration {
            key: Some(TaintKey::NodeNotReady),
            operator: TolerationOperator::Equal,
            value: "true".to_string(),
            effect: Some(TaintEffect::NoSchedule),
        },
        Toleration {
            key: Some(TaintKey::NodeMemoryPressure),
            operator: TolerationOperator::Equal,
            value: "high".to_string(),
            effect: Some(TaintEffect::NoSchedule),
        },
    ];

    let mut partially_tolerant_pod = make_pod("partially-tolerant", 10, 1, 1000);
    partially_tolerant_pod.spec.tolerations = vec![Toleration {
        key: Some(TaintKey::NodeNotReady),
        operator: TolerationOperator::Equal,
        value: "true".to_string(),
        effect: Some(TaintEffect::NoSchedule),
    }];

    scheduler.update_cache_pod(fully_tolerant_pod).await;
    scheduler.update_cache_pod(partially_tolerant_pod).await;

    let mut rx = scheduler.run();
    let mut assignments = Vec::new();
    for _ in 0..2 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push((assignment.pod_name, assignment.node_name));
    }
    assignments.sort();

    let expected = vec![
        ("fully-tolerant".to_string(), "multi-tainted".to_string()),
        ("partially-tolerant".to_string(), "clean-node".to_string()),
    ];
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_node_selector_no_match() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("env".to_string(), "production".to_string());

    scheduler.update_cache_node(node1).await;

    let mut pod = make_pod("mismatched-pod", 10, 1, 1000);
    pod.spec
        .node_selector
        .insert("env".to_string(), "staging".to_string());

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());

    let mut matching_node = make_node("staging-node", 10, 10000);
    matching_node
        .labels
        .insert("env".to_string(), "staging".to_string());
    scheduler.update_cache_node(matching_node).await;

    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "mismatched-pod");
    assert_eq!(assignment.node_name, "staging-node");
}

#[tokio::test]
async fn test_scheduler_complex_node_affinity() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "us-west-1".to_string());
    node1
        .labels
        .insert("instance-type".to_string(), "m5.large".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "us-west-2".to_string());
    node2
        .labels
        .insert("instance-type".to_string(), "m5.xlarge".to_string());

    let mut node3 = make_node("node3", 10, 10000);
    node3
        .labels
        .insert("zone".to_string(), "us-east-1".to_string());
    node3
        .labels
        .insert("instance-type".to_string(), "m5.large".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;
    scheduler.update_cache_node(node3).await;

    let mut pod = make_pod("complex-affinity", 10, 1, 1000);
    pod.spec.affinity = Some(Affinity {
        node_affinity: Some(NodeAffinity {
            required_during_scheduling_ignored_during_execution: Some(NodeSelector {
                node_selector_terms: vec![NodeSelectorTerm {
                    match_expressions: vec![
                        NodeSelectorRequirement {
                            key: "zone".to_string(),
                            operator: NodeSelectorOperator::NodeSelectorOpIn,
                            values: vec!["us-west-1".to_string(), "us-west-2".to_string()],
                        },
                        NodeSelectorRequirement {
                            key: "instance-type".to_string(),
                            operator: NodeSelectorOperator::NodeSelectorOpIn,
                            values: vec!["m5.large".to_string()],
                        },
                    ],
                }],
            }),
            ..Default::default()
        }),
    });

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "complex-affinity");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_node_affinity_gt_lt_operators() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("cpu-cores".to_string(), "8".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("cpu-cores".to_string(), "16".to_string());

    let mut node3 = make_node("node3", 10, 10000);
    node3
        .labels
        .insert("cpu-cores".to_string(), "4".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;
    scheduler.update_cache_node(node3).await;

    let mut pod = make_pod("gt-pod", 10, 1, 1000);
    pod.spec.affinity = Some(Affinity {
        node_affinity: Some(NodeAffinity {
            required_during_scheduling_ignored_during_execution: Some(NodeSelector {
                node_selector_terms: vec![NodeSelectorTerm {
                    match_expressions: vec![NodeSelectorRequirement {
                        key: "cpu-cores".to_string(),
                        operator: NodeSelectorOperator::NodeSelectorOpGt,
                        values: vec!["6".to_string()],
                    }],
                }],
            }),
            ..Default::default()
        }),
    });

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "gt-pod");
    assert!(["node1", "node2"].contains(&assignment.node_name.as_str()));
}

#[tokio::test]
async fn test_scheduler_toleration_exists_operator() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut tainted_node = make_node("tainted-node", 10, 10000);
    tainted_node.spec.taints = vec![Taint {
        key: TaintKey::NodeDiskPressure,
        value: "any-value".to_string(),
        effect: TaintEffect::NoSchedule,
    }];

    scheduler.update_cache_node(tainted_node).await;

    let mut tolerant_pod = make_pod("exists-tolerant", 10, 1, 1000);
    tolerant_pod.spec.tolerations = vec![Toleration {
        key: Some(TaintKey::NodeDiskPressure),
        operator: TolerationOperator::Exists,
        value: String::new(),
        effect: Some(TaintEffect::NoSchedule),
    }];

    scheduler.update_cache_pod(tolerant_pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "exists-tolerant");
    assert_eq!(assignment.node_name, "tainted-node");
}

#[tokio::test]
async fn test_scheduler_high_priority_preemption_order() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;

    for i in (1..=100).rev() {
        scheduler
            .update_cache_pod(make_pod(&format!("pod-{i}"), i, 1, 100))
            .await;
    }

    let mut rx = scheduler.run();
    let mut assignments = Vec::new();
    for _ in 0..10 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push(assignment.pod_name);
    }

    let expected_start = vec!["pod-100", "pod-99", "pod-98", "pod-97", "pod-96"];
    assert_eq!(&assignments[0..5], &expected_start);
}

#[tokio::test]
async fn test_scheduler_node_removal_pod_rescheduling() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;
    scheduler
        .update_cache_node(make_node("node2", 10, 10000))
        .await;

    let mut pod1 = make_pod("pod1", 10, 1, 1000);
    pod1.spec.node_name = Some("node1".to_string());
    let mut pod2 = make_pod("pod2", 10, 1, 1000);
    pod2.spec.node_name = Some("node1".to_string());

    scheduler.update_cache_pod(pod1).await;
    scheduler.update_cache_pod(pod2).await;

    let mut rx = scheduler.run();

    for _ in 0..2 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assert_eq!(assignment.node_name, "node1");
    }

    scheduler.remove_cache_node("node1").await;

    let res = timeout(Duration::from_secs(2), rx.recv()).await;
    assert!(res.is_err());

    let mut pod2_2 = make_pod("pod3", 10, 1, 1000);
    pod2_2.spec.node_name = Some("node2".to_string());
    scheduler.update_cache_pod(pod2_2).await;
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        res,
        Ok(v) if v.node_name == "node2"
    ));
}

#[tokio::test]
async fn test_scheduler_empty_cluster() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_pod(make_pod("orphan-pod", 10, 1, 1000))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());
}

#[tokio::test]
async fn test_scheduler_mixed_scheduling_constraints() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut constrained_node = make_node("constrained-node", 20, 20000);
    constrained_node
        .labels
        .insert("workload".to_string(), "batch".to_string());
    constrained_node
        .labels
        .insert("zone".to_string(), "us-west".to_string());
    constrained_node.spec.taints = vec![Taint {
        key: TaintKey::NodeOutOfService,
        value: "maintenance".to_string(),
        effect: TaintEffect::NoSchedule,
    }];

    let regular_node = make_node("regular-node", 10, 10000);

    scheduler.update_cache_node(constrained_node).await;
    scheduler.update_cache_node(regular_node).await;

    let mut constrained_pod = make_pod("constrained-pod", 50, 5, 5000);
    constrained_pod
        .spec
        .node_selector
        .insert("workload".to_string(), "batch".to_string());
    constrained_pod.spec.tolerations = vec![Toleration {
        key: Some(TaintKey::NodeOutOfService),
        operator: TolerationOperator::Equal,
        value: "maintenance".to_string(),
        effect: Some(TaintEffect::NoSchedule),
    }];
    constrained_pod.spec.affinity = Some(Affinity {
        node_affinity: Some(NodeAffinity {
            required_during_scheduling_ignored_during_execution: Some(NodeSelector {
                node_selector_terms: vec![NodeSelectorTerm {
                    match_expressions: vec![NodeSelectorRequirement {
                        key: "zone".to_string(),
                        operator: NodeSelectorOperator::NodeSelectorOpIn,
                        values: vec!["us-west".to_string()],
                    }],
                }],
            }),
            ..Default::default()
        }),
    });

    let regular_pod = make_pod("regular-pod", 10, 2, 2000);

    scheduler.update_cache_pod(constrained_pod).await;
    scheduler.update_cache_pod(regular_pod).await;

    let mut rx = scheduler.run();
    let mut assignments = Vec::new();
    for _ in 0..2 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push((assignment.pod_name, assignment.node_name));
    }
    assignments.sort();

    let expected = vec![
        (
            "constrained-pod".to_string(),
            "constrained-node".to_string(),
        ),
        ("regular-pod".to_string(), "regular-node".to_string()),
    ];
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_concurrent_modifications() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;

    let mut rx = scheduler.run();

    for i in 0..5 {
        scheduler
            .update_cache_pod(make_pod(&format!("pod-{i}"), 10, 1, 1000))
            .await;
        if i == 2 {
            scheduler
                .update_cache_node(make_node("node2", 10, 10000))
                .await;
        }
    }

    let mut assignments = Vec::new();
    for _ in 0..5 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push(assignment.pod_name);
    }
    assignments.sort();

    let expected = vec!["pod-0", "pod-1", "pod-2", "pod-3", "pod-4"];
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_invalid_node_name() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_node(make_node("real-node", 10, 10000))
        .await;

    let mut pod = make_pod("invalid-node-pod", 10, 1, 1000);
    pod.spec.node_name = Some("non-existent-node".to_string());

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());
}
