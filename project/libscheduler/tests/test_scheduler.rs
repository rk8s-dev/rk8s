use libscheduler::algorithms::basic::BasicAlgorithm;
use libscheduler::models::{NodeInfo, PodInfo};
use libscheduler::scheduler::Scheduler;
use std::time::Duration;
use tokio::time::timeout;

fn make_pod(name: &str, priority: u64, cpu: u64, memory: u64) -> PodInfo {
    PodInfo {
        name: name.to_string(),
        cpu,
        memory,
        priority,
        attempts: 0,
        scheduled: None,
    }
}

fn make_node(name: &str, cpu: u64, memory: u64) -> NodeInfo {
    NodeInfo {
        name: name.to_string(),
        cpu,
        memory,
    }
}

#[tokio::test]
async fn test_scheduler_basic_assign() {
    let mut scheduler: Scheduler<BasicAlgorithm> = Scheduler::new();
    scheduler.add_cache_node(make_node("node1", 4, 2048)).await;
    scheduler.add_cache_node(make_node("node2", 2, 1024)).await;
    scheduler.add_cache_node(make_node("node3", 8, 4096)).await;

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
    let mut scheduler: Scheduler<BasicAlgorithm> = Scheduler::new();
    scheduler
        .update_cache_pod(make_pod("bigpod", 1, 100, 100))
        .await;

    let mut rx = scheduler.run();
    let res = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(res.is_err() || res.unwrap().is_none());

    scheduler.add_cache_node(make_node("node1", 200, 200)).await;

    let res = timeout(Duration::from_secs(5), rx.recv()).await;
    assert!(res.is_ok());
    let assignment = res.unwrap().unwrap().unwrap();
    assert_eq!(assignment.pod_name, "bigpod");
    assert_eq!(assignment.node_name, "node1");
}
