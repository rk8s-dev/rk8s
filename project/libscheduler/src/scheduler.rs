use std::collections::BinaryHeap;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, RwLock, watch};
use tokio::time::interval;
use tokio::time::{Duration, Instant};

use crate::algorithms::Algorithm;
use crate::cache::Cache;
use crate::models::{Assignment, BackOffPod, PodNameWithPriority};
use crate::models::{NodeInfo, PodInfo};

pub struct Scheduler<A: Algorithm> {
    cache: Arc<RwLock<Cache>>,
    queue: Arc<SchedulingQueue>,
    _algorithm: PhantomData<A>,
}

type ActiveQueue = Arc<Mutex<BinaryHeap<PodNameWithPriority>>>;
type BackoffQueue = Arc<Mutex<BinaryHeap<BackOffPod>>>;
type UnschedulableQueue = Arc<Mutex<Vec<(BackOffPod, Instant)>>>;

pub struct SchedulingQueue {
    active_queue: ActiveQueue,
    backoff_queue: BackoffQueue,
    unschedulable_queue: UnschedulableQueue,
    /// Used for waiting for state changes when no Pods are schedulable.
    /// Each Pod addition increments the state change counter.
    status_count: Mutex<watch::Receiver<usize>>,
    status_sx: watch::Sender<usize>,
}

impl Default for SchedulingQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulingQueue {
    pub fn new() -> Self {
        let (sx, rx) = watch::channel(0);
        Self {
            active_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            backoff_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            unschedulable_queue: Arc::new(Mutex::new(Vec::new())),
            status_count: Mutex::new(rx),
            status_sx: sx,
        }
    }

    async fn move_all_to_active_or_backoff(&self) {
        let now = Instant::now();
        let mut active_guard = self.active_queue.lock().await;
        let mut backoff_guard = self.backoff_queue.lock().await;
        self.unschedulable_queue
            .lock()
            .await
            .drain(..)
            .for_each(|p| {
                if p.0.expire >= now {
                    active_guard.push(p.0.pod);
                } else {
                    backoff_guard.push(p.0);
                }
            });
        self.add_count().await;
    }

    async fn next_pod(&self) -> (u64, String) {
        let mut next = self.active_queue.lock().await.pop();
        while next.is_none() {
            let mut status_guard = self.status_count.lock().await;
            status_guard
                .changed()
                .await
                .expect("status_sx closed for unknown reason");
            next = self.active_queue.lock().await.pop();
        }
        next.unwrap()
    }

    async fn flush_backoff_completed(
        active: ActiveQueue,
        backoff: BackoffQueue,
        sx: watch::Sender<usize>,
    ) {
        let now = Instant::now();
        let mut active_guard = active.lock().await;
        let mut backoff_guard = backoff.lock().await;
        while !backoff_guard.is_empty() && backoff_guard.peek().unwrap().expire <= now {
            let pod = backoff_guard.pop().unwrap();
            active_guard.push(pod.pod);
        }
        sx.send_modify(|v| (*v) += 1);
    }

    async fn flush_unschedulable_left_over(
        active: ActiveQueue,
        backoff: BackoffQueue,
        unschedulable: UnschedulableQueue,
        sx: watch::Sender<usize>,
    ) {
        let now = Instant::now();
        let mut active_guard = active.lock().await;
        let mut backoff_guard = backoff.lock().await;
        let mut unschedulable_guard = unschedulable.lock().await;
        unschedulable_guard.drain(..).for_each(|(p, t)| {
            if now - t > Duration::from_secs(5 * 60) {
                if now >= p.expire {
                    active_guard.push(p.pod);
                } else {
                    backoff_guard.push(p);
                }
            }
        });
        sx.send_modify(|v| (*v) += 1);
    }

    fn run(&self) {
        let active_queue = self.active_queue.clone();
        let backoff_queue = self.backoff_queue.clone();
        let status_sx = self.status_sx.clone();
        tokio::spawn(async move {
            let mut timer = interval(Duration::from_secs(1));
            loop {
                Self::flush_backoff_completed(
                    active_queue.clone(),
                    backoff_queue.clone(),
                    status_sx.clone(),
                )
                .await;
                timer.tick().await;
            }
        });

        let active_queue = self.active_queue.clone();
        let backoff_queue = self.backoff_queue.clone();
        let unschedulable_queue = self.unschedulable_queue.clone();
        let status_sx = self.status_sx.clone();
        tokio::spawn(async move {
            let mut timer = interval(Duration::from_secs(30));
            loop {
                Self::flush_unschedulable_left_over(
                    active_queue.clone(),
                    backoff_queue.clone(),
                    unschedulable_queue.clone(),
                    status_sx.clone(),
                )
                .await;
                timer.tick().await;
            }
        });
    }

    async fn add_count(&self) {
        self.status_sx.send_modify(|v| *v += 1);
    }

    async fn push(&self, pod_name: String, priority: u64) {
        let mut guard = self.active_queue.lock().await;
        guard.push((priority, pod_name));
        self.add_count().await;
    }

    async fn push_backoff(&self, mut pod: PodInfo) {
        pod.attempts += 1;
        let expire = Instant::now() + Duration::from_secs(2_u64.pow(pod.attempts as u32));
        let backoff_pod = BackOffPod {
            pod: (pod.priority, pod.name.clone()),
            expire,
        };
        if pod.attempts > 8 {
            let mut guard = self.unschedulable_queue.lock().await;
            guard.push((backoff_pod, Instant::now()));
        } else {
            let mut guard = self.backoff_queue.lock().await;
            guard.push(backoff_pod);
        }
    }
}

impl<A: Algorithm> Default for Scheduler<A> {
    fn default() -> Self {
        Self::new()
    }
}

impl<A: Algorithm> Scheduler<A> {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(Cache::new())),
            queue: Arc::new(SchedulingQueue::new()),
            _algorithm: PhantomData,
        }
    }

    async fn schedule_one(
        cache: Arc<RwLock<Cache>>,
        queue: Arc<SchedulingQueue>,
        res_sx: UnboundedSender<Result<Assignment, anyhow::Error>>,
    ) {
        let (pod_priority, pod_name) = queue.next_pod().await;
        let cache_read = cache.read().await;
        let pod_info = cache_read.get_pod(&pod_name);
        let nodes = cache_read.get_nodes();
        drop(cache_read);
        if let Some(pod_info) = pod_info {
            if pod_info.priority != pod_priority {
                // The pod priority is already updated.
                return;
            }

            let filtered = A::filter(nodes, &pod_info);
            let mut graded = A::grader(filtered, &pod_info);
            graded.sort_by(|a, b| b.1.cmp(&a.1));
            let mut cache_write = cache.write().await;
            if graded.is_empty() {
                if cache_write.add_fail(&pod_name) {
                    queue.push_backoff(pod_info).await;
                }
            } else if cache_write.assign(&pod_name, &graded[0].0.name) {
                res_sx
                    .send(Ok(Assignment {
                        pod_name,
                        node_name: graded[0].0.name.clone(),
                    }))
                    .expect("scheduling result rx closed before scheduler closed");
            }
        }
    }

    pub fn run(&self) -> UnboundedReceiver<Result<Assignment, anyhow::Error>> {
        self.queue.run();
        let queue = self.queue.clone();
        let cache = self.cache.clone();
        let (sx, rx) = unbounded_channel();
        tokio::spawn(async move {
            loop {
                Self::schedule_one(cache.clone(), queue.clone(), sx.clone()).await;
            }
        });
        rx
    }

    /// Only need to call update when an unusual update occurred.
    /// There is no need to update scheduled updates;
    /// cache will update automatically.
    pub async fn update_cache_pod(&mut self, pod: PodInfo) {
        let mut write_lock = self.cache.write().await;
        let ori = (*write_lock).update_pod(pod.clone());
        if pod.scheduled.is_none() {
            if let Some(o) = &ori {
                if o.scheduled.is_some() {
                    self.queue.push(pod.name, pod.priority).await;
                }
            } else {
                self.queue.push(pod.name, pod.priority).await;
            }
        }
    }

    pub async fn remove_cache_pod(&mut self, pod_name: &str) {
        let mut write_lock = self.cache.write().await;
        (*write_lock).remove_pod(pod_name);
    }

    pub async fn add_cache_node(&mut self, node: NodeInfo) {
        let mut write_lock = self.cache.write().await;
        (*write_lock).update_node(node);
        drop(write_lock);
        self.queue.move_all_to_active_or_backoff().await;
    }

    pub async fn remove_cache_node(&mut self, node_name: &str) {
        let mut write_lock = self.cache.write().await;
        let pod_on_to_delete_node = write_lock.pop_pod_on_node(node_name);
        for (priority, name) in pod_on_to_delete_node {
            self.queue.push(name, priority).await;
        }
        (*write_lock).remove_node(node_name);
        drop(write_lock);
        self.queue.move_all_to_active_or_backoff().await;
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::timeout;

    use super::*;
    use crate::algorithms::basic::BasicAlgorithm;

    #[tokio::test]
    async fn test_push_and_next_pod() {
        let queue = Arc::new(SchedulingQueue::new());
        queue.push("pod1".to_string(), 1).await;
        queue.push("pod3".to_string(), 3).await;
        queue.push("pod2".to_string(), 2).await;
        let (priority, name) = queue.next_pod().await;
        assert_eq!(priority, 3);
        assert_eq!(name, "pod3");
        let (priority, name) = queue.next_pod().await;
        assert_eq!(priority, 2);
        assert_eq!(name, "pod2");
        let (priority, name) = queue.next_pod().await;
        assert_eq!(priority, 1);
        assert_eq!(name, "pod1");

        let (pod_sx, mut pod_rx) = unbounded_channel();
        let cloned_queue = queue.clone();
        tokio::spawn(async move {
            let pod_with_prioirity = cloned_queue.next_pod().await;
            pod_sx.send(pod_with_prioirity).unwrap();
        });
        queue.push("pod1".to_string(), 1).await;
        let res = timeout(Duration::from_secs(5), pod_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(res.0, 1);
        assert_eq!(res.1, "pod1");
    }

    fn make_pod(pod_name: &str, priority: u64) -> PodInfo {
        PodInfo {
            name: pod_name.to_owned(),
            cpu: 1,
            memory: 1,
            priority,
            attempts: 0,
            scheduled: None,
        }
    }

    #[tokio::test]
    async fn test_push_backoff_and_unschedulable() {
        let queue = SchedulingQueue::new();
        let pod = PodInfo {
            name: "pod".to_owned(),
            cpu: 1,
            memory: 1,
            priority: 1,
            attempts: 9,
            scheduled: None,
        };
        queue.push_backoff(pod).await;
        let unschedulable = queue.unschedulable_queue.lock().await;
        assert_eq!(unschedulable.len(), 1);
    }

    #[tokio::test]
    async fn test_backoff_queue_flush() {
        let queue = SchedulingQueue::new();
        let pod = PodInfo {
            name: "pod".to_owned(),
            cpu: 1,
            memory: 1,
            priority: 1,
            attempts: 0,
            scheduled: None,
        };
        queue.run();
        queue.push_backoff(pod).await;
        let res = timeout(Duration::from_secs(3), queue.next_pod()).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_scheduler_update_cache_pod() {
        let mut scheduler: Scheduler<BasicAlgorithm> = Scheduler::new();
        let pod = make_pod("pod3", 7);
        scheduler.update_cache_pod(pod).await;
        let cache = scheduler.cache.read().await;
        assert!(cache.get_pod("pod3").is_some());
    }

    #[tokio::test]
    async fn test_scheduler_remove_cache_pod() {
        let mut scheduler: Scheduler<BasicAlgorithm> = Scheduler::new();
        let pod = make_pod("pod4", 8);
        scheduler.update_cache_pod(pod).await;
        scheduler.remove_cache_pod("pod4").await;
        let cache = scheduler.cache.read().await;
        assert!(cache.get_pod("pod4").is_none());
    }

    #[tokio::test]
    async fn test_scheduler_add_and_remove_node() {
        let mut scheduler: Scheduler<BasicAlgorithm> = Scheduler::new();
        let node = NodeInfo {
            name: "node1".to_string(),
            cpu: 2,
            memory: 10,
        };
        scheduler.add_cache_node(node).await;
        let cache = scheduler.cache.read().await;
        assert!(!cache.get_nodes().is_empty());
        drop(cache);
        scheduler.remove_cache_node("node1").await;
        let cache = scheduler.cache.read().await;
        assert!(cache.get_nodes().is_empty());
    }

    #[tokio::test]
    async fn test_schedule_one_assigns_pod() {
        let scheduler: Scheduler<BasicAlgorithm> = Scheduler::new();
        let mut cache = scheduler.cache.write().await;

        let node = NodeInfo {
            name: "node".to_string(),
            cpu: 2,
            memory: 10,
        };
        cache.update_node(node);
        let node = NodeInfo {
            name: "node2".to_string(),
            cpu: 1,
            memory: 8,
        };
        cache.update_node(node);

        cache.update_pod(PodInfo {
            name: "pod".to_string(),
            cpu: 2,
            memory: 3,
            priority: 1,
            attempts: 1,
            scheduled: None,
        });
        drop(cache);

        scheduler.queue.push("pod".to_string(), 1).await;
        let (sx, mut rx) = unbounded_channel();
        Scheduler::<BasicAlgorithm>::schedule_one(
            scheduler.cache.clone(),
            scheduler.queue.clone(),
            sx,
        )
        .await;
        let res = rx.recv().await.unwrap();
        assert!(res.is_ok());
        let assignment = res.unwrap();
        assert_eq!(assignment.pod_name, "pod");
        assert_eq!(assignment.node_name, "node");
    }
}
