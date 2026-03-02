use std::collections::{BinaryHeap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, RwLock, watch};
use tokio::time::interval;
use tokio::time::{Duration, Instant};

use crate::cache::Cache;
use crate::cycle_state::CycleState;
use crate::models::{Assignment, BackOffPod, PodNameWithPriority};
use crate::models::{NodeInfo, PodInfo};
use crate::plugins::node_resources_fit::ScoringStrategy;
use crate::plugins::{
    ClusterEventWithHint, Code, EnabledPlugins, EventInner, EventResource, FilterPlugin, Plugins,
    PreFilterPlugin, PreScorePlugin, QueueingHint, Registry, ScorePlugin, Status,
};

pub struct Scheduler {
    cache: Arc<RwLock<Cache>>,
    queue: Arc<SchedulingQueue>,
    // Differ to k8s, we don't have profile cofig now
    strategy: ScoringStrategy,
    enabled_plugins: EnabledPlugins,
}

type ActiveQueue = Arc<Mutex<BinaryHeap<PodNameWithPriority>>>;
type BackoffQueue = Arc<Mutex<BinaryHeap<BackOffPod>>>;
type UnschedulableQueue = Arc<Mutex<Vec<(BackOffPod, Instant)>>>;

pub struct SchedulingQueue {
    active_queue: ActiveQueue,
    backoff_queue: BackoffQueue,
    unschedulable_queue: UnschedulableQueue,
    pod_events_hint: Arc<Vec<ClusterEventWithHint>>,
    node_events_hint: Arc<Vec<ClusterEventWithHint>>,
    /// Used for waiting for state changes when no Pods are schedulable.
    /// Each Pod addition increments the state change counter.
    status_count: Mutex<watch::Receiver<usize>>,
    status_sx: watch::Sender<usize>,
}

impl Default for SchedulingQueue {
    fn default() -> Self {
        Self::new(vec![])
    }
}

impl SchedulingQueue {
    pub fn new(queueing_hints: Vec<ClusterEventWithHint>) -> Self {
        let (node_hints, pod_hints): (Vec<_>, Vec<_>) = queueing_hints
            .into_iter()
            .partition(|e| matches!(e.event.resource, EventResource::Node));

        let (sx, rx) = watch::channel(0);
        Self {
            active_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            backoff_queue: Arc::new(Mutex::new(BinaryHeap::new())),
            unschedulable_queue: Arc::new(Mutex::new(Vec::new())),
            status_count: Mutex::new(rx),
            status_sx: sx,
            node_events_hint: Arc::new(node_hints),
            pod_events_hint: Arc::new(pod_hints),
        }
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

    async fn push_unschedulable(&self, mut pod: PodInfo) {
        pod.queued_info.attempts += 1;
        let expire =
            Instant::now() + Duration::from_secs(2_u64.pow(pod.queued_info.attempts as u32));
        let backoff_pod = BackOffPod {
            pod: (pod.spec.priority, pod.name.clone()),
            expire,
        };
        let mut guard = self.unschedulable_queue.lock().await;
        guard.push((backoff_pod, Instant::now()));
    }

    async fn push_backoff(&self, mut pod: PodInfo) {
        pod.queued_info.attempts += 1;
        let expire =
            Instant::now() + Duration::from_secs(2_u64.pow(pod.queued_info.attempts as u32));
        let backoff_pod = BackOffPod {
            pod: (pod.spec.priority, pod.name.clone()),
            expire,
        };
        if pod.queued_info.attempts > 8 {
            let mut guard = self.unschedulable_queue.lock().await;
            guard.push((backoff_pod, Instant::now()));
        } else {
            let mut guard = self.backoff_queue.lock().await;
            guard.push(backoff_pod);
        }
    }

    // For now, we will not distinguish between the specific more granular changes corresponding to each type of modification.
    // TODO: improve performance by distinguishing modifications in detail.
    async fn hint(&self, event: EventInner, pods_snapshot: HashMap<String, PodInfo>) {
        let hint_fn = match event.clone() {
            EventInner::Node(_, _) => self.node_events_hint.clone(),
            EventInner::Pod(_, _) => self.pod_events_hint.clone(),
        };
        let mut backoff_guard = self.backoff_queue.lock().await;
        let (backoff_to_active, remain_backoff): (Vec<_>, Vec<_>) =
            (*backoff_guard).drain().partition(|p| {
                hint_fn.iter().any(|f| {
                    if let Some(func) = &f.queueing_hint_fn {
                        if let Some(pod_info) = pods_snapshot.get(&p.pod.1) {
                            matches!(
                                func(pod_info.clone(), event.clone()),
                                Ok(QueueingHint::Queue)
                            )
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                })
            });
        *backoff_guard = remain_backoff.into();
        drop(backoff_guard);

        let mut unschedulable_guard = self.unschedulable_queue.lock().await;
        let (unschedulable_to_active, remain_unschedulable): (Vec<_>, Vec<_>) =
            (*unschedulable_guard).drain(..).partition(|p| {
                hint_fn.iter().any(|f| {
                    if let Some(func) = &f.queueing_hint_fn {
                        if let Some(pod_info) = pods_snapshot.get(&p.0.pod.1) {
                            matches!(
                                func(pod_info.clone(), event.clone(),),
                                Ok(QueueingHint::Queue)
                            )
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                })
            });
        *unschedulable_guard = remain_unschedulable;
        drop(unschedulable_guard);

        for bp in backoff_to_active {
            self.push(bp.pod.1, bp.pod.0).await;
        }
        for (bp, _) in unschedulable_to_active {
            self.push(bp.pod.1, bp.pod.0).await;
        }
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new(ScoringStrategy::LeastAllocated, Plugins::default())
    }
}

impl Scheduler {
    pub fn new(strategy: ScoringStrategy, plugins: Plugins) -> Self {
        let registry = Registry::default();
        let mut enabled = EnabledPlugins::default();

        macro_rules! enable_plugins {
            ($plugin_type:ident) => {
                for p in plugins.$plugin_type.iter() {
                    let plugin_struct = registry
                        .$plugin_type
                        .iter()
                        .find(|item| item.name() == p.name)
                        .cloned();
                    if let Some(plugin) = plugin_struct {
                        enabled.$plugin_type.push((plugin, p.weight));
                    }
                }
            };
        }

        enable_plugins!(pre_enqueue);
        enable_plugins!(pre_filter);
        enable_plugins!(filter);
        enable_plugins!(post_filter);
        enable_plugins!(pre_score);
        enable_plugins!(score);
        enable_plugins!(reserve);
        enable_plugins!(permit);
        enable_plugins!(pre_bind);
        enable_plugins!(bind);
        enable_plugins!(post_bind);

        let mut queueing_hints = Vec::new();
        for p in plugins.enqueue_extensions.iter() {
            let plugin = registry
                .enqueue_extensions
                .iter()
                .find(|item| item.name() == p.name)
                .cloned();
            if let Some(plugin) = plugin {
                queueing_hints.append(&mut plugin.events_to_register());
            }
        }

        Self {
            cache: Arc::new(RwLock::new(Cache::new())),
            queue: Arc::new(SchedulingQueue::new(queueing_hints)),
            strategy,
            enabled_plugins: enabled,
        }
    }

    fn run_prefilter_plugin(
        plugins: &Vec<(Arc<dyn PreFilterPlugin>, i64)>,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: &[NodeInfo],
    ) -> (Vec<NodeInfo>, Status) {
        let mut filtered_set = HashSet::new();
        for (pl, _) in plugins {
            let (res, sta) = pl.pre_filter(state, pod, nodes.to_owned());
            if let Code::Error = sta.code {
                continue;
            } else if let Code::Skip = sta.code {
                state.skip_filter_plugins.insert(pl.name().to_string());
                continue;
            }
            res.node_names.into_iter().for_each(|n| {
                filtered_set.insert(n);
            });
            if matches!(
                sta.code,
                Code::Unschedulable | Code::UnschedulableAndUnresolvable | Code::Pending
            ) {
                return (vec![], sta);
            }
        }
        let passed_nodes = nodes
            .iter()
            .filter(|n| !filtered_set.contains(&n.name))
            .cloned()
            .collect();
        (passed_nodes, Status::default())
    }

    fn run_filter_plugin(
        plugins: &Vec<(Arc<dyn FilterPlugin>, i64)>,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: &[NodeInfo],
    ) -> Vec<NodeInfo> {
        let mut nodes = nodes.to_owned();
        for (pl, _) in plugins {
            if state.skip_filter_plugins.contains(pl.name()) {
                continue;
            }
            nodes.retain(|n| {
                matches!(
                    pl.filter(state, pod, n.clone()).code,
                    Code::Success | Code::Skip
                )
            });
        }
        nodes
    }

    fn run_pre_score_plugin(
        plugins: &Vec<(Arc<dyn PreScorePlugin>, i64)>,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: &[NodeInfo],
    ) -> Status {
        for (pl, _) in plugins {
            let sta = pl.pre_score(state, pod, nodes.to_owned());
            if let Code::Skip = sta.code {
                state.skip_score_plugins.insert(pl.name().to_string());
            } else if !matches!(sta.code, Code::Success) {
                return sta;
            }
        }
        Status::default()
    }

    fn run_score_plugin(
        plugins: &Vec<(Arc<dyn ScorePlugin>, i64)>,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: &[NodeInfo],
    ) -> Vec<(i64, NodeInfo)> {
        let mut score = vec![0_i64; nodes.len()];
        for (pl, w) in plugins {
            let mut cur_score: Vec<i64> = nodes
                .iter()
                .map(|n| pl.score(state, pod, n.clone()).0)
                .collect();
            let normalizer = pl.score_extension();
            normalizer.normalize_score(state, pod, &mut cur_score);
            for i in 0..score.len() {
                score[i] += cur_score[i] * w;
            }
        }
        score.into_iter().zip(nodes.to_owned()).collect()
    }

    async fn schedule_one(
        enabled_plugins: EnabledPlugins,
        cache: Arc<RwLock<Cache>>,
        queue: Arc<SchedulingQueue>,
        res_sx: UnboundedSender<Result<Assignment, anyhow::Error>>,
        strategy: ScoringStrategy,
    ) {
        let (pod_priority, pod_name) = queue.next_pod().await;
        let cache_read = cache.read().await;
        let pod_info = cache_read.get_pod(&pod_name);
        let nodes_snapshot = cache_read.get_nodes();
        drop(cache_read);
        if let Some(pod_info) = pod_info {
            if pod_info.spec.priority != pod_priority {
                // The pod priority is already updated.
                return;
            }

            macro_rules! break_cycle {
                ($v: ident) => {
                    let mut cache_write = cache.write().await;
                    if cache_write.add_fail(&pod_name) {
                        queue.$v(pod_info).await;
                    }
                    return;
                };
            }

            const SCORING_STRATEGY_CONFIG_KEY: &str = "ScoringStrategyConfig";
            let mut cycle_state = CycleState::default();
            cycle_state.write(SCORING_STRATEGY_CONFIG_KEY, Box::new(strategy));

            // Get all scheduled pods for pod affinity plugins
            let cache_read = cache.read().await;
            let all_scheduled_pods: Vec<PodInfo> = cache_read
                .get_pods()
                .into_values()
                .filter(|p| p.scheduled.is_some() || p.spec.node_name.is_some())
                .collect();
            drop(cache_read);
            cycle_state.write("AllScheduledPods", Box::new(all_scheduled_pods));

            let (passed_prefilter, sta) = Self::run_prefilter_plugin(
                &enabled_plugins.pre_filter,
                &mut cycle_state,
                &pod_info,
                &nodes_snapshot,
            );
            match sta.code {
                Code::Pending => {
                    queue.push(pod_name, pod_priority).await;
                    return;
                }
                Code::Unschedulable => {
                    break_cycle!(push_backoff);
                }
                Code::UnschedulableAndUnresolvable => {
                    break_cycle!(push_unschedulable);
                }
                _ => {}
            }

            let filtered = Self::run_filter_plugin(
                &enabled_plugins.filter,
                &mut cycle_state,
                &pod_info,
                &passed_prefilter,
            );
            let sta = Self::run_pre_score_plugin(
                &enabled_plugins.pre_score,
                &mut cycle_state,
                &pod_info,
                &filtered,
            );
            if filtered.is_empty() || !matches!(sta.code, Code::Success) {
                break_cycle!(push_backoff);
            }

            let mut scores = Self::run_score_plugin(
                &enabled_plugins.score,
                &mut cycle_state,
                &pod_info,
                &filtered,
            );
            scores.sort_by_key(|b| std::cmp::Reverse(b.0));
            let mut cache_write = cache.write().await;
            if cache_write.assume(&pod_name, &scores[0].1.name) {
                res_sx
                    .send(Ok(Assignment {
                        pod_name,
                        node_name: scores[0].1.name.clone(),
                    }))
                    .expect("scheduling result rx closed before scheduler closed");
            }
        }
    }

    /// Un assume a pod, if the pod is not scheduled, do nothing
    pub async fn unassume(&mut self, pod_name: &str) {
        let mut cache_write = self.cache.write().await;
        let pod = cache_write.unassume(pod_name);
        if let Some(pod_info) = pod {
            self.queue.push(pod_info.name, pod_info.spec.priority).await;
        }
    }

    pub fn run(&self) -> UnboundedReceiver<Result<Assignment, anyhow::Error>> {
        self.queue.run();
        let queue = self.queue.clone();
        let cache = self.cache.clone();
        let enabled_plugins = self.enabled_plugins.clone();
        let (sx, rx) = unbounded_channel();
        let strategy = self.strategy.clone();
        tokio::spawn(async move {
            loop {
                Self::schedule_one(
                    enabled_plugins.clone(),
                    cache.clone(),
                    queue.clone(),
                    sx.clone(),
                    strategy.clone(),
                )
                .await;
            }
        });
        rx
    }

    pub async fn enqueue(&self, pod: PodInfo) {
        for (p, _) in &self.enabled_plugins.pre_enqueue {
            let sta = p.pre_enqueue(&pod);
            if !matches!(
                sta.code,
                Code::Error | Code::Skip | Code::Success | Code::Pending
            ) {
                return;
            }
        }
        self.queue.push(pod.name, pod.spec.priority).await;
    }

    /// Only need to call update when an unusual update occurred.
    /// There is no need to update scheduled updates;
    /// cache will update automatically.
    pub async fn update_cache_pod(&mut self, pod: PodInfo) {
        // TODO: automatic reassume
        let mut write_lock = self.cache.write().await;
        let ori = (*write_lock).update_pod(pod.clone());
        drop(write_lock);

        if pod.scheduled.is_none() {
            let read_lock = self.cache.read().await;
            let pod_snapshot = read_lock.get_pods();
            self.queue
                .hint(
                    EventInner::Pod(Box::new(ori.clone()), Box::new(Some(pod.clone()))),
                    pod_snapshot,
                )
                .await;

            if let Some(o) = &ori {
                if o.scheduled.is_some() {
                    self.enqueue(pod).await;
                }
            } else {
                self.enqueue(pod).await;
            }
        } else {
            let pod_name = pod.name.clone();
            let node_name = pod.scheduled.clone().unwrap();
            let mut cache = self.cache.write().await;
            let _ = cache.assume(&pod_name, &node_name);
            drop(cache);
        }
    }

    pub async fn remove_cache_pod(&mut self, pod_name: &str) {
        let mut write_lock = self.cache.write().await;
        let ori = (*write_lock).remove_pod(pod_name);
        drop(write_lock);

        let read_lock = self.cache.read().await;
        let pod_snapshot = read_lock.get_pods();
        self.queue
            .hint(
                EventInner::Pod(Box::new(ori.clone()), Box::new(None)),
                pod_snapshot,
            )
            .await;
    }

    pub async fn set_cache_node(&mut self, nodes: Vec<NodeInfo>) {
        let mut write_lock = self.cache.write().await;
        write_lock.set_nodes(nodes);
    }

    pub async fn update_cache_node(&mut self, node: NodeInfo) {
        let mut write_lock = self.cache.write().await;
        let ori = (*write_lock).update_node(node.clone());
        drop(write_lock);
        self.queue.add_count().await;

        let read_lock = self.cache.read().await;
        let pod_snapshot = read_lock.get_pods();
        self.queue
            .hint(
                EventInner::Node(Box::new(ori), Box::new(node)),
                pod_snapshot,
            )
            .await;
    }

    pub async fn remove_cache_node(&mut self, node_name: &str) {
        let mut write_lock = self.cache.write().await;
        let pod_on_to_delete_node = write_lock.pop_pod_on_node(node_name);
        for (priority, name) in pod_on_to_delete_node {
            self.queue.push(name, priority).await;
        }
        (*write_lock).remove_node(node_name);
        drop(write_lock);
    }
}

#[cfg(test)]
mod tests {
    use tokio::time::timeout;

    use super::*;
    use crate::models::{NodeSpec, PodSpec, QueuedInfo, ResourcesRequirements};

    #[test]
    fn test_plugins_enabled() {
        let plugins = Plugins::default();
        let scheduler = Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());

        macro_rules! test_plugin {
            ($plugin: ident) => {{
                let mut enabled_names = Vec::new();
                let mut expected_names = Vec::new();
                for (pl, _) in scheduler.enabled_plugins.$plugin {
                    enabled_names.push(pl.name().to_string());
                }
                for pl in plugins.$plugin.iter() {
                    expected_names.push(pl.name.clone());
                }
                enabled_names.sort();
                expected_names.sort();
                assert_eq!(enabled_names, expected_names);
            }};
        }

        test_plugin!(pre_enqueue);
        test_plugin!(pre_filter);
        test_plugin!(filter);
        test_plugin!(post_filter);
        test_plugin!(pre_score);
        test_plugin!(score);
        test_plugin!(reserve);
        test_plugin!(permit);
        test_plugin!(pre_bind);
        test_plugin!(bind);
        test_plugin!(post_bind);
    }

    #[tokio::test]
    async fn test_push_and_next_pod() {
        let queue = Arc::new(SchedulingQueue::new(vec![]));
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
            labels: std::collections::HashMap::new(),
            spec: PodSpec {
                resources: ResourcesRequirements { cpu: 1, memory: 1 },
                priority,
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        }
    }

    #[tokio::test]
    async fn test_push_backoff_and_unschedulable() {
        let queue = SchedulingQueue::new(vec![]);
        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "pod".to_owned(),
            spec: PodSpec {
                resources: ResourcesRequirements { cpu: 1, memory: 1 },
                priority: 1,
                ..Default::default()
            },
            queued_info: QueuedInfo {
                attempts: 9,
                ..Default::default()
            },
            scheduled: None,
        };
        queue.push_backoff(pod).await;
        let unschedulable = queue.unschedulable_queue.lock().await;
        assert_eq!(unschedulable.len(), 1);
    }

    #[tokio::test]
    async fn test_backoff_queue_flush() {
        let queue = SchedulingQueue::new(vec![]);
        let pod = PodInfo {
            labels: std::collections::HashMap::new(),
            name: "pod".to_string(),
            spec: PodSpec {
                resources: ResourcesRequirements { cpu: 1, memory: 1 },
                priority: 1,
                ..Default::default()
            },
            queued_info: QueuedInfo::default(),
            scheduled: None,
        };
        queue.run();
        queue.push_backoff(pod).await;
        let res = timeout(Duration::from_secs(3), queue.next_pod()).await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn test_scheduler_update_cache_pod() {
        let mut scheduler: Scheduler =
            Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
        let pod = make_pod("pod3", 7);
        scheduler.update_cache_pod(pod).await;
        let cache = scheduler.cache.read().await;
        assert!(cache.get_pod("pod3").is_some());
    }

    #[tokio::test]
    async fn test_scheduler_remove_cache_pod() {
        let mut scheduler: Scheduler =
            Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
        let pod = make_pod("pod4", 8);
        scheduler.update_cache_pod(pod).await;
        scheduler.remove_cache_pod("pod4").await;
        let cache = scheduler.cache.read().await;
        assert!(cache.get_pod("pod4").is_none());
    }

    #[tokio::test]
    async fn test_scheduler_add_and_remove_node() {
        let mut scheduler: Scheduler =
            Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
        let node = NodeInfo {
            name: "node1".to_string(),
            allocatable: ResourcesRequirements { cpu: 2, memory: 10 },
            requested: ResourcesRequirements { cpu: 0, memory: 0 },
            spec: NodeSpec::default(),
            ..Default::default()
        };
        scheduler.update_cache_node(node).await;
        let cache = scheduler.cache.read().await;
        assert!(!cache.get_nodes().is_empty());
        drop(cache);
        scheduler.remove_cache_node("node1").await;
        let cache = scheduler.cache.read().await;
        assert!(cache.get_nodes().is_empty());
    }

    #[tokio::test]
    async fn test_schedule_one_assigns_pod() {
        let scheduler: Scheduler =
            Scheduler::new(ScoringStrategy::LeastAllocated, Plugins::default());
        let mut cache = scheduler.cache.write().await;

        let node = NodeInfo {
            name: "node".to_string(),
            allocatable: ResourcesRequirements { cpu: 2, memory: 10 },
            requested: ResourcesRequirements { cpu: 0, memory: 0 },
            spec: NodeSpec::default(),
            ..Default::default()
        };
        cache.update_node(node);
        let node = NodeInfo {
            name: "node2".to_string(),
            allocatable: ResourcesRequirements { cpu: 1, memory: 8 },
            requested: ResourcesRequirements { cpu: 0, memory: 0 },
            spec: NodeSpec::default(),
            ..Default::default()
        };
        cache.update_node(node);

        cache.update_pod(PodInfo {
            name: "pod".to_string(),
            labels: std::collections::HashMap::new(),
            spec: PodSpec {
                resources: ResourcesRequirements { cpu: 1, memory: 3 },
                priority: 1,
                ..Default::default()
            },
            queued_info: QueuedInfo {
                attempts: 1,
                ..Default::default()
            },
            scheduled: None,
        });
        drop(cache);

        scheduler.queue.push("pod".to_string(), 1).await;
        let (sx, mut rx) = unbounded_channel();
        Scheduler::schedule_one(
            scheduler.enabled_plugins,
            scheduler.cache.clone(),
            scheduler.queue.clone(),
            sx,
            scheduler.strategy,
        )
        .await;
        let res = rx.recv().await.unwrap();
        assert!(res.is_ok());
        let assignment = res.unwrap();
        assert_eq!(assignment.pod_name, "pod");
        assert_eq!(assignment.node_name, "node");
    }
}
