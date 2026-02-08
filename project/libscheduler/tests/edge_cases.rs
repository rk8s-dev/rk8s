use common::{LabelSelector, Taint, TaintEffect, TaintKey, Toleration, TolerationOperator};
use libscheduler::models::{
    Affinity, NodeAffinity, NodeInfo, NodeSelector, NodeSelectorOperator, NodeSelectorRequirement,
    NodeSelectorTerm, NodeSpec, PodAffinity, PodAffinityTerm, PodAntiAffinity, PodInfo, PodSpec,
    ResourcesRequirements, WeightedPodAffinityTerm,
};
use libscheduler::plugins::Plugins;
use libscheduler::plugins::node_resources_fit::ScoringStrategy;
use libscheduler::scheduler::Scheduler;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::timeout;
fn make_pod(name: &str, priority: u64, cpu: u64, memory: u64) -> PodInfo {
    PodInfo::new(
        name.to_string(),
        HashMap::new(),
        PodSpec {
            resources: ResourcesRequirements { cpu, memory },
            priority,
            ..Default::default()
        },
    )
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
        pod_affinity: None,
        pod_anti_affinity: None,
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
        pod_affinity: None,
        pod_anti_affinity: None,
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
        pod_affinity: None,
        pod_anti_affinity: None,
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

#[tokio::test]
async fn test_scheduler_pod_affinity_edge_cases() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    // Create nodes with topology labels
    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node1
        .labels
        .insert("rack".to_string(), "rack-1".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "zone-b".to_string());
    node2
        .labels
        .insert("rack".to_string(), "rack-2".to_string());

    let mut node3 = make_node("node3", 10, 10000);
    node3
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node3
        .labels
        .insert("rack".to_string(), "rack-3".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;
    scheduler.update_cache_node(node3).await;

    // Create an existing pod with label app=web on node1 (zone-a, rack-1)
    let mut existing_pod_labels = HashMap::new();
    existing_pod_labels.insert("app".to_string(), "web".to_string());
    let mut existing_pod = make_pod("web-pod", 10, 1, 1000);
    existing_pod.labels = existing_pod_labels;
    existing_pod.spec.node_name = Some("node1".to_string());
    scheduler.update_cache_pod(existing_pod).await;

    // Test 1: Pod with required affinity (must run in same zone as web app)
    let mut selector_labels = HashMap::new();
    selector_labels.insert("app".to_string(), "web".to_string());
    let label_selector = Some(LabelSelector {
        match_labels: selector_labels,
        match_expressions: vec![],
    });

    let pod_affinity_term = PodAffinityTerm {
        label_selector,
        topology_key: "zone".to_string(),
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term.clone()]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: None,
    };

    let mut pod_labels = HashMap::new();
    pod_labels.insert("app".to_string(), "api".to_string());
    let mut affinity_pod = make_pod("affinity-pod", 10, 1, 1000);
    affinity_pod.labels = pod_labels;
    affinity_pod.spec.affinity = Some(affinity);
    scheduler.update_cache_pod(affinity_pod).await;

    // Test 2: Pod with required anti-affinity (must NOT run in same zone as web app)
    let pod_anti_affinity = PodAntiAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term.clone()]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let anti_affinity = Affinity {
        node_affinity: None,
        pod_affinity: None,
        pod_anti_affinity: Some(pod_anti_affinity),
    };

    let mut anti_pod_labels = HashMap::new();
    anti_pod_labels.insert("app".to_string(), "db".to_string());
    let mut anti_affinity_pod = make_pod("anti-affinity-pod", 10, 1, 1000);
    anti_affinity_pod.labels = anti_pod_labels;
    anti_affinity_pod.spec.affinity = Some(anti_affinity);
    scheduler.update_cache_pod(anti_affinity_pod).await;

    // Test 3: Pod with preferred affinity (prefer same rack, but not required)
    let weighted_term = WeightedPodAffinityTerm {
        weight: 10,
        pod_affinity_term: pod_affinity_term.clone(),
    };

    let preferred_pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: None,
        preferred_during_scheduling_ignored_during_execution: Some(vec![weighted_term]),
    };

    let preferred_affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(preferred_pod_affinity),
        pod_anti_affinity: None,
    };

    let mut preferred_pod_labels = HashMap::new();
    preferred_pod_labels.insert("app".to_string(), "cache".to_string());
    let mut preferred_pod = make_pod("preferred-pod", 10, 1, 1000);
    preferred_pod.labels = preferred_pod_labels;
    preferred_pod.spec.affinity = Some(preferred_affinity);
    scheduler.update_cache_pod(preferred_pod).await;

    let mut rx = scheduler.run();
    let mut assignments = Vec::new();
    for _ in 0..4 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push((assignment.pod_name, assignment.node_name));
    }
    assignments.sort_by_key(|(pod_name, _)| pod_name.clone());

    // Verify assignments
    let assignment_map: HashMap<String, String> = assignments.into_iter().collect();

    // affinity-pod should be scheduled in zone-a (node1 or node3)
    if let Some(affinity_node) = assignment_map.get("affinity-pod") {
        assert!(
            ["node1", "node3"].contains(&affinity_node.as_str()),
            "affinity-pod should be in zone-a, got {}",
            affinity_node
        );
    } else {
        panic!(
            "affinity-pod was not scheduled. Scheduled pods: {:?}",
            assignment_map
        );
    }

    // anti-affinity-pod should be scheduled in zone-b (node2)
    let anti_affinity_node = assignment_map.get("anti-affinity-pod").unwrap();
    assert_eq!(
        anti_affinity_node, "node2",
        "anti-affinity-pod should be in zone-b, got {}",
        anti_affinity_node
    );

    // preferred-pod can be scheduled anywhere, but likely in zone-a due to affinity
    let preferred_node = assignment_map.get("preferred-pod").unwrap();
    // Just ensure it's scheduled somewhere
    assert!(
        ["node1", "node2", "node3"].contains(&preferred_node.as_str()),
        "preferred-pod should be scheduled, got {}",
        preferred_node
    );
}

#[tokio::test]
async fn test_scheduler_pod_affinity_with_node_selector() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    // Create nodes with topology labels
    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node1.labels.insert("env".to_string(), "prod".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "zone-b".to_string());
    node2.labels.insert("env".to_string(), "prod".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;

    // Create an existing pod with label app=web on node1
    let mut existing_pod_labels = HashMap::new();
    existing_pod_labels.insert("app".to_string(), "web".to_string());
    let mut existing_pod = make_pod("web-pod", 10, 1, 1000);
    existing_pod.labels = existing_pod_labels;
    existing_pod.spec.node_name = Some("node1".to_string());

    scheduler.update_cache_pod(existing_pod).await;

    // Create a new pod with required affinity to web app pods in same zone AND node selector env=prod
    let mut selector_labels = HashMap::new();
    selector_labels.insert("app".to_string(), "web".to_string());
    let label_selector = Some(LabelSelector {
        match_labels: selector_labels,
        match_expressions: vec![],
    });

    let pod_affinity_term = PodAffinityTerm {
        label_selector,
        topology_key: "zone".to_string(),
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: None,
    };

    let mut pod_labels = HashMap::new();
    pod_labels.insert("app".to_string(), "api".to_string());
    let mut pod = make_pod("affinity-pod", 10, 1, 1000);
    pod.labels = pod_labels;
    pod.spec.affinity = Some(affinity);
    pod.spec
        .node_selector
        .insert("env".to_string(), "prod".to_string());

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let mut assignment = None;
    for _ in 0..2 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let a = res.unwrap();
        if a.pod_name == "affinity-pod" {
            assignment = Some(a);
            break;
        }
    }
    let assignment = assignment.expect("Should have received affinity-pod assignment");

    // The pod should be scheduled in zone-a (node1) because web-pod is there and node selector matches
    assert_eq!(assignment.pod_name, "affinity-pod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_pod_affinity_complex_match_expressions() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    // Create nodes with topology labels
    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node1
        .labels
        .insert("rack".to_string(), "rack-1".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "zone-b".to_string());
    node2
        .labels
        .insert("rack".to_string(), "rack-2".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;

    // Create existing pods with various labels
    let mut web_pod_labels = HashMap::new();
    web_pod_labels.insert("app".to_string(), "web".to_string());
    web_pod_labels.insert("env".to_string(), "prod".to_string());
    web_pod_labels.insert("version".to_string(), "v1".to_string());
    let mut web_pod = make_pod("web-pod", 10, 1, 1000);
    web_pod.labels = web_pod_labels;
    web_pod.spec.node_name = Some("node1".to_string());

    scheduler.update_cache_pod(web_pod).await;

    let mut api_pod_labels = HashMap::new();
    api_pod_labels.insert("app".to_string(), "api".to_string());
    api_pod_labels.insert("env".to_string(), "prod".to_string());
    api_pod_labels.insert("version".to_string(), "v2".to_string());
    let mut api_pod = make_pod("api-pod", 10, 1, 1000);
    api_pod.labels = api_pod_labels;
    api_pod.spec.node_name = Some("node2".to_string());

    scheduler.update_cache_pod(api_pod).await;

    // Create a pod with complex matchExpressions for affinity
    // Required: pod with app=web AND env=prod OR version in (v1, v2)
    let label_selector = Some(LabelSelector {
        match_labels: HashMap::new(), // No simple match_labels
        match_expressions: vec![
            // app=web
            common::LabelSelectorRequirement {
                key: "app".to_string(),
                operator: common::LabelSelectorOperator::In,
                values: vec!["web".to_string()],
            },
            // env=prod
            common::LabelSelectorRequirement {
                key: "env".to_string(),
                operator: common::LabelSelectorOperator::In,
                values: vec!["prod".to_string()],
            },
        ],
    });

    let pod_affinity_term = PodAffinityTerm {
        label_selector,
        topology_key: "zone".to_string(),
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![pod_affinity_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: None,
    };

    let mut new_pod_labels = HashMap::new();
    new_pod_labels.insert("app".to_string(), "new".to_string());
    let mut new_pod = make_pod("new-pod", 10, 1, 1000);
    new_pod.labels = new_pod_labels;
    new_pod.spec.affinity = Some(affinity);
    scheduler.update_cache_pod(new_pod).await;

    let mut rx = scheduler.run();
    let mut assignment = None;
    for _ in 0..3 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let a = res.unwrap();
        if a.pod_name == "new-pod" {
            assignment = Some(a);
            break;
        }
    }
    let assignment = assignment.expect("Should have received new-pod assignment");

    // The pod should be scheduled in zone-a (node1) because web-pod matches both app=web and env=prod
    assert_eq!(assignment.pod_name, "new-pod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_pod_affinity_multiple_topology_keys() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    // Create nodes with multiple topology labels
    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node1
        .labels
        .insert("rack".to_string(), "rack-1".to_string());
    node1
        .labels
        .insert("host".to_string(), "host-1".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "zone-a".to_string()); // Same zone as node1
    node2
        .labels
        .insert("rack".to_string(), "rack-2".to_string()); // Different rack
    node2
        .labels
        .insert("host".to_string(), "host-2".to_string());

    let mut node3 = make_node("node3", 10, 10000);
    node3
        .labels
        .insert("zone".to_string(), "zone-b".to_string()); // Different zone
    node3
        .labels
        .insert("rack".to_string(), "rack-3".to_string());
    node3
        .labels
        .insert("host".to_string(), "host-3".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;
    scheduler.update_cache_node(node3).await;

    // Create existing pods with label app=web on node1
    let mut existing_pod_labels = HashMap::new();
    existing_pod_labels.insert("app".to_string(), "web".to_string());
    let mut existing_pod = make_pod("web-pod", 10, 1, 1000);
    existing_pod.labels = existing_pod_labels;
    existing_pod.spec.node_name = Some("node1".to_string());

    scheduler.update_cache_pod(existing_pod).await;

    // Create a pod with affinity to web app pods in same zone AND same rack
    // This tests that both topology conditions must be satisfied
    let mut selector_labels = HashMap::new();
    selector_labels.insert("app".to_string(), "web".to_string());
    let label_selector = Some(LabelSelector {
        match_labels: selector_labels,
        match_expressions: vec![],
    });

    // First term: require same zone
    let zone_term = PodAffinityTerm {
        label_selector: label_selector.clone(),
        topology_key: "zone".to_string(),
    };

    // Second term: require same rack
    let rack_term = PodAffinityTerm {
        label_selector,
        topology_key: "rack".to_string(),
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![zone_term, rack_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: None,
    };

    let mut new_pod_labels = HashMap::new();
    new_pod_labels.insert("app".to_string(), "new".to_string());
    let mut new_pod = make_pod("new-pod", 10, 1, 1000);
    new_pod.labels = new_pod_labels;
    new_pod.spec.affinity = Some(affinity);
    scheduler.update_cache_pod(new_pod).await;

    let mut rx = scheduler.run();
    let mut assignment = None;
    for _ in 0..2 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let a = res.unwrap();
        if a.pod_name == "new-pod" {
            assignment = Some(a);
            break;
        }
    }
    let assignment = assignment.expect("Should have received new-pod assignment");

    // The pod should be scheduled on node1 because it's the only node with web-pod in same zone AND same rack
    // Node2 has web-pod in same zone but different rack
    // Node3 has web-pod in different zone
    assert_eq!(assignment.pod_name, "new-pod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_pod_affinity_and_anti_affinity_combined() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    // Create nodes with topology labels
    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node1
        .labels
        .insert("rack".to_string(), "rack-1".to_string());

    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "zone-a".to_string());
    node2
        .labels
        .insert("rack".to_string(), "rack-2".to_string());

    let mut node3 = make_node("node3", 10, 10000);
    node3
        .labels
        .insert("zone".to_string(), "zone-b".to_string());
    node3
        .labels
        .insert("rack".to_string(), "rack-3".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;
    scheduler.update_cache_node(node3).await;

    // Create existing pods:
    // - web-pod with app=web on node1 (zone-a, rack-1)
    // - api-pod with app=api on node2 (zone-a, rack-2)
    let mut web_pod_labels = HashMap::new();
    web_pod_labels.insert("app".to_string(), "web".to_string());
    let mut web_pod = make_pod("web-pod", 10, 1, 1000);
    web_pod.labels = web_pod_labels;
    web_pod.spec.node_name = Some("node1".to_string());

    scheduler.update_cache_pod(web_pod).await;

    let mut api_pod_labels = HashMap::new();
    api_pod_labels.insert("app".to_string(), "api".to_string());
    let mut api_pod = make_pod("api-pod", 10, 1, 1000);
    api_pod.labels = api_pod_labels;
    api_pod.spec.node_name = Some("node2".to_string());

    scheduler.update_cache_pod(api_pod).await;

    // Create a pod with:
    // - Required affinity to app=web pods in same zone
    // - Required anti-affinity to app=api pods in same rack
    let mut web_selector_labels = HashMap::new();
    web_selector_labels.insert("app".to_string(), "web".to_string());
    let web_label_selector = Some(LabelSelector {
        match_labels: web_selector_labels,
        match_expressions: vec![],
    });

    let mut api_selector_labels = HashMap::new();
    api_selector_labels.insert("app".to_string(), "api".to_string());
    let api_label_selector = Some(LabelSelector {
        match_labels: api_selector_labels,
        match_expressions: vec![],
    });

    let affinity_term = PodAffinityTerm {
        label_selector: web_label_selector,
        topology_key: "zone".to_string(),
    };

    let anti_affinity_term = PodAffinityTerm {
        label_selector: api_label_selector,
        topology_key: "rack".to_string(),
    };

    let pod_affinity = PodAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![affinity_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let pod_anti_affinity = PodAntiAffinity {
        required_during_scheduling_ignored_during_execution: Some(vec![anti_affinity_term]),
        preferred_during_scheduling_ignored_during_execution: None,
    };

    let affinity = Affinity {
        node_affinity: None,
        pod_affinity: Some(pod_affinity),
        pod_anti_affinity: Some(pod_anti_affinity),
    };

    let mut new_pod_labels = HashMap::new();
    new_pod_labels.insert("app".to_string(), "new".to_string());
    let mut new_pod = make_pod("new-pod", 10, 1, 1000);
    new_pod.labels = new_pod_labels;
    new_pod.spec.affinity = Some(affinity);
    scheduler.update_cache_pod(new_pod).await;

    let mut rx = scheduler.run();
    let mut assignment = None;
    for _ in 0..3 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let a = res.unwrap();
        if a.pod_name == "new-pod" {
            assignment = Some(a);
            break;
        }
    }
    let assignment = assignment.expect("Should have received new-pod assignment");

    // The pod should be scheduled on node1 because:
    // - Affinity: needs same zone as web-pod (zone-a) -> node1 or node2
    // - Anti-affinity: cannot be same rack as api-pod (rack-2) -> excludes node2
    // So only node1 satisfies both constraints
    assert_eq!(assignment.pod_name, "new-pod");
    assert_eq!(assignment.node_name, "node1");
}
