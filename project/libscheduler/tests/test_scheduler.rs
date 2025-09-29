use common::{Taint, TaintEffect, TaintKey, Toleration, TolerationOperator};
use libscheduler::models::{
    Affinity, NodeAffinity, NodeInfo, NodeSelector, NodeSelectorOperator, NodeSelectorRequirement,
    NodeSelectorTerm, NodeSpec, PodInfo, PodSpec, PreferredSchedulingTerm,
    PreferredSchedulingTerms, QueuedInfo, ResourcesRequirements,
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
async fn test_scheduler_basic_assign() {
    let mut scheduler = Scheduler::new(ScoringStrategy::MostAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("node1", 4, 2048))
        .await;
    scheduler
        .update_cache_node(make_node("node2", 2, 1024))
        .await;
    scheduler
        .update_cache_node(make_node("node3", 8, 4096))
        .await;

    scheduler
        .update_cache_pod(make_pod("pod1", 10, 2, 1024))
        .await;
    scheduler
        .update_cache_pod(make_pod("pod2", 20, 1, 512))
        .await;
    scheduler
        .update_cache_pod(make_pod("pod3", 5, 3, 2048))
        .await;

    let mut rx = scheduler.run();
    let mut assignments = Vec::new();
    for _ in 0..3 {
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();
        assignments.push((assignment.pod_name, assignment.node_name));
    }
    assignments.sort();
    let mut expected = vec![
        ("pod2".to_string(), "node2".to_string()),
        ("pod1".to_string(), "node1".to_string()),
        ("pod3".to_string(), "node3".to_string()),
    ];
    expected.sort();
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_backoff_and_recover() {
    let mut scheduler = Scheduler::new(ScoringStrategy::MostAllocated, Plugins::default());
    scheduler
        .update_cache_pod(make_pod("bigpod", 1, 100, 100))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());

    scheduler
        .update_cache_node(make_node("node1", 200, 200))
        .await;

    let res = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(res.is_ok());
    let assignment = res.unwrap().unwrap().unwrap();
    assert_eq!(assignment.pod_name, "bigpod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_priority_scheduling() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;

    scheduler
        .update_cache_pod(make_pod("low-priority", 1, 1, 1000))
        .await;
    scheduler
        .update_cache_pod(make_pod("high-priority", 100, 1, 1000))
        .await;
    scheduler
        .update_cache_pod(make_pod("med-priority", 50, 1, 1000))
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

    assert_eq!(
        assignments,
        vec!["high-priority", "med-priority", "low-priority"]
    );
}

#[tokio::test]
async fn test_scheduler_resource_constraints() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("small-node", 2, 1024))
        .await;
    scheduler
        .update_cache_node(make_node("large-node", 10, 10240))
        .await;

    scheduler
        .update_cache_pod(make_pod("small-pod", 1, 1, 512))
        .await;
    scheduler
        .update_cache_pod(make_pod("large-pod", 10, 8, 8192))
        .await;

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
        ("large-pod".to_string(), "large-node".to_string()),
        ("small-pod".to_string(), "small-node".to_string()),
    ];
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_node_affinity_required() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("zone".to_string(), "us-west".to_string());
    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("zone".to_string(), "us-east".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;

    let mut pod = make_pod("affinity-pod", 10, 1, 1000);
    pod.spec.affinity = Some(Affinity {
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

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "affinity-pod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_node_affinity_preferred() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("preferred".to_string(), "true".to_string());
    let node2 = make_node("node2", 10, 10000);

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;

    let mut pod = make_pod("preferred-pod", 10, 1, 1000);
    pod.spec.affinity = Some(Affinity {
        node_affinity: Some(NodeAffinity {
            preferred_during_scheduling_ignored_during_execution: Some(PreferredSchedulingTerms {
                terms: vec![PreferredSchedulingTerm {
                    weight: 100,
                    match_label: NodeSelectorRequirement {
                        key: "preferred".to_string(),
                        operator: NodeSelectorOperator::NodeSelectorOpExists,
                        values: vec![],
                    },
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
    assert_eq!(assignment.pod_name, "preferred-pod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_taint_toleration() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut tainted_node = make_node("tainted-node", 100, 100000);
    tainted_node.spec.taints = vec![Taint {
        key: TaintKey::NodeNotReady,
        value: "true".to_string(),
        effect: TaintEffect::NoSchedule,
    }];
    let clean_node = make_node("clean-node", 10, 10000);

    scheduler.update_cache_node(tainted_node).await;
    scheduler.update_cache_node(clean_node).await;

    let mut tolerant_pod = make_pod("tolerant-pod", 10, 1, 1000);
    tolerant_pod.spec.tolerations = vec![Toleration {
        key: Some(TaintKey::NodeNotReady),
        operator: TolerationOperator::Equal,
        value: "true".to_string(),
        effect: Some(TaintEffect::NoSchedule),
    }];

    let intolerant_pod = make_pod("intolerant-pod", 10, 1, 1000);

    scheduler.update_cache_pod(tolerant_pod).await;
    scheduler.update_cache_pod(intolerant_pod).await;

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
        ("intolerant-pod".to_string(), "clean-node".to_string()),
        ("tolerant-pod".to_string(), "tainted-node".to_string()),
    ];
    assert_eq!(assignments, expected);
}

#[tokio::test]
async fn test_scheduler_node_name_selector() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_node(make_node("target-node", 10, 10000))
        .await;
    scheduler
        .update_cache_node(make_node("other-node", 10, 10000))
        .await;

    let mut pod = make_pod("specific-pod", 10, 1, 1000);
    pod.spec.node_name = Some("target-node".to_string());

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "specific-pod");
    assert_eq!(assignment.node_name, "target-node");
}

#[tokio::test]
async fn test_scheduler_unschedulable_node() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut unschedulable_node = make_node("unschedulable-node", 10, 10000);
    unschedulable_node.spec.unschedulable = true;
    let schedulable_node = make_node("schedulable-node", 10, 10000);

    scheduler.update_cache_node(unschedulable_node).await;
    scheduler.update_cache_node(schedulable_node).await;

    scheduler
        .update_cache_pod(make_pod("test-pod", 10, 1, 1000))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "test-pod");
    assert_eq!(assignment.node_name, "schedulable-node");
}

#[tokio::test]
async fn test_scheduler_scheduling_gates() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;

    let mut pod_with_gates = make_pod("gated-pod", 10, 1, 1000);
    pod_with_gates.spec.scheduling_gates = vec!["gate1".to_string(), "gate2".to_string()];

    scheduler.update_cache_pod(pod_with_gates).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());

    let mut pod_without_gates = make_pod("ungated-pod", 10, 1, 1000);
    pod_without_gates.spec.scheduling_gates = vec![];

    scheduler.update_cache_pod(pod_without_gates).await;

    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "ungated-pod");
}

#[tokio::test]
async fn test_scheduler_scoring_strategies() {
    for strategy in [
        ScoringStrategy::LeastAllocated,
        ScoringStrategy::MostAllocated,
    ] {
        let mut scheduler = Scheduler::new(strategy.clone(), Plugins::default());

        let mut node1 = make_node("node1", 10, 10000);
        node1.requested = ResourcesRequirements {
            cpu: 8,
            memory: 8000,
        };
        let mut node2 = make_node("node2", 10, 10000);
        node2.requested = ResourcesRequirements {
            cpu: 2,
            memory: 2000,
        };

        scheduler.update_cache_node(node1).await;
        scheduler.update_cache_node(node2).await;

        scheduler
            .update_cache_pod(make_pod("test-pod", 10, 1, 1000))
            .await;

        let mut rx = scheduler.run();
        let res = timeout(Duration::from_secs(2), rx.recv())
            .await
            .unwrap()
            .unwrap();
        let assignment = res.unwrap();

        match strategy {
            ScoringStrategy::LeastAllocated => {
                assert_eq!(assignment.node_name, "node2");
            }
            ScoringStrategy::MostAllocated => {
                assert_eq!(assignment.node_name, "node1");
            }
            _ => {}
        }
    }
}

#[tokio::test]
async fn test_scheduler_node_selector() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 10, 10000);
    node1
        .labels
        .insert("env".to_string(), "production".to_string());
    let mut node2 = make_node("node2", 10, 10000);
    node2
        .labels
        .insert("env".to_string(), "development".to_string());

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;

    let mut pod = make_pod("selector-pod", 10, 1, 1000);
    pod.spec
        .node_selector
        .insert("env".to_string(), "production".to_string());

    scheduler.update_cache_pod(pod).await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "selector-pod");
    assert_eq!(assignment.node_name, "node1");
}

#[tokio::test]
async fn test_scheduler_multiple_nodes_balanced_allocation() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut node1 = make_node("node1", 100, 100000);
    node1.requested = ResourcesRequirements {
        cpu: 50,
        memory: 10000,
    };
    let mut node2 = make_node("node2", 100, 100000);
    node2.requested = ResourcesRequirements {
        cpu: 10,
        memory: 50000,
    };
    let node3 = make_node("node3", 100, 100000);

    scheduler.update_cache_node(node1).await;
    scheduler.update_cache_node(node2).await;
    scheduler.update_cache_node(node3).await;

    scheduler
        .update_cache_pod(make_pod("pod1", 10, 10, 10000))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();

    assert_eq!(assignment.pod_name, "pod1");
    assert_eq!(assignment.node_name, "node3");
}

#[tokio::test]
async fn test_scheduler_insufficient_resources() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_node(make_node("small-node", 1, 1000))
        .await;

    scheduler
        .update_cache_pod(make_pod("huge-pod", 10, 100, 100000))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());

    scheduler
        .update_cache_node(make_node("large-node", 200, 200000))
        .await;

    let res = timeout(Duration::from_secs(3), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "huge-pod");
    assert_eq!(assignment.node_name, "large-node");
}

#[tokio::test]
async fn test_scheduler_cache_operations() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    scheduler
        .update_cache_node(make_node("node1", 10, 10000))
        .await;

    scheduler
        .update_cache_pod(make_pod("pod1", 10, 1, 1000))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "pod1");
    assert_eq!(assignment.node_name, "node1");

    scheduler.remove_cache_pod("pod1").await;

    scheduler.remove_cache_node("node1").await;

    scheduler
        .update_cache_node(make_node("node2", 10, 10000))
        .await;
    scheduler
        .update_cache_pod(make_pod("pod2", 10, 1, 1000))
        .await;

    let res = timeout(Duration::from_secs(2), rx.recv())
        .await
        .unwrap()
        .unwrap();
    let assignment = res.unwrap();
    assert_eq!(assignment.pod_name, "pod2");
    assert_eq!(assignment.node_name, "node2");
}

#[tokio::test]
async fn test_scheduler_complex_scenario() {
    let mut scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

    let mut prod_node = make_node("prod-node", 20, 20000);
    prod_node
        .labels
        .insert("env".to_string(), "production".to_string());
    prod_node
        .labels
        .insert("zone".to_string(), "us-west".to_string());
    prod_node.spec.taints = vec![Taint {
        key: TaintKey::NodeMemoryPressure,
        value: "true".to_string(),
        effect: TaintEffect::NoSchedule,
    }];

    let mut dev_node = make_node("dev-node", 10, 10000);
    dev_node
        .labels
        .insert("env".to_string(), "development".to_string());
    dev_node
        .labels
        .insert("zone".to_string(), "us-east".to_string());

    scheduler.update_cache_node(prod_node).await;
    scheduler.update_cache_node(dev_node).await;

    let mut critical_pod = make_pod("critical-pod", 100, 5, 5000);
    critical_pod
        .spec
        .node_selector
        .insert("env".to_string(), "production".to_string());
    critical_pod.spec.tolerations = vec![Toleration {
        key: Some(TaintKey::NodeMemoryPressure),
        operator: TolerationOperator::Exists,
        value: String::new(),
        effect: Some(TaintEffect::NoSchedule),
    }];
    critical_pod.spec.affinity = Some(Affinity {
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

    scheduler.update_cache_pod(critical_pod).await;
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
        ("critical-pod".to_string(), "prod-node".to_string()),
        ("regular-pod".to_string(), "dev-node".to_string()),
    ];
    assert_eq!(assignments, expected);
}
