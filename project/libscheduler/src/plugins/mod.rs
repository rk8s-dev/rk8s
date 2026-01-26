//! Scheduler plugins.
//!
//! The functionality of each plugin corresponds to its namesake in Kubernetes.
//! Some comments are also quoted from the Kubernetes codebase.

// Since more detailed features and plugins in k8s have not yet been implemented, the presence of deadcode is permitted.
#![allow(dead_code)]

use crate::cycle_state::CycleState;
use crate::models::{NodeInfo, PodInfo};
use crate::plugins::balanced_allocation::BalancedAllocation;
use crate::plugins::node_name::NodeName;
use crate::plugins::node_resources_fit::Fit;
use crate::plugins::node_unschedulable::NodeUnschedulable;
use crate::plugins::scheduling_gates::SchedulingGates;
use crate::plugins::taint_toleration::TaintToleration;
use bitflags::bitflags;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub mod balanced_allocation;
pub mod node_affinity;
pub mod node_name;
pub mod node_resources_fit;
pub mod node_unschedulable;
pub mod pod_affinity;
pub mod scheduling_gates;
pub mod taint_toleration;

/// Plugin specifies a plugin name and its weight when applicable. Weight is used only for Score plugins.
#[derive(Clone)]
pub struct PluginInfo {
    pub name: String,
    pub weight: i64,
}

/// List of enabled plugins
pub struct Plugins {
    pub pre_enqueue: Vec<PluginInfo>,
    pub queue_sort: PluginInfo,
    pub pre_filter: Vec<PluginInfo>,
    pub filter: Vec<PluginInfo>,
    pub post_filter: Vec<PluginInfo>,
    pub score: Vec<PluginInfo>,
    pub pre_score: Vec<PluginInfo>,
    pub reserve: Vec<PluginInfo>,
    pub permit: Vec<PluginInfo>,
    pub pre_bind: Vec<PluginInfo>,
    pub bind: Vec<PluginInfo>,
    pub post_bind: Vec<PluginInfo>,
    pub enqueue_extensions: Vec<PluginInfo>,
}

impl PluginInfo {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            weight: 0,
        }
    }

    fn with_weight(name: &str, weight: i64) -> Self {
        Self {
            name: name.to_string(),
            weight,
        }
    }
}

pub trait Plugin {
    fn name(&self) -> &str;
}

impl Default for Plugins {
    fn default() -> Self {
        let node_affinity = PluginInfo::with_weight("NodeAffinity", 2);
        let node_name = PluginInfo::new("NodeName");
        let fit = PluginInfo::with_weight("NodeResourcesFit", 1);
        let node_unschedulable = PluginInfo::new("NodeUnschedulable");
        let scheduling_gates = PluginInfo::new("SchedulingGates");
        let taint_toleration = PluginInfo::with_weight("TaintToleration", 3);
        let balanced_allocation = PluginInfo::with_weight("NodeResourcesBalancedAllocation", 1);
        let pod_affinity = PluginInfo::with_weight("PodAffinity", 2);

        Self {
            pre_enqueue: vec![scheduling_gates.clone()],
            queue_sort: PluginInfo::new("PrioritySort"),
            pre_filter: vec![node_affinity.clone(), fit.clone(), pod_affinity.clone()],
            filter: vec![
                node_affinity.clone(),
                fit.clone(),
                taint_toleration.clone(),
                node_name.clone(),
                node_unschedulable.clone(),
                pod_affinity.clone(),
            ],
            post_filter: vec![],
            pre_score: vec![
                node_affinity.clone(),
                fit.clone(),
                balanced_allocation.clone(),
                taint_toleration.clone(),
                pod_affinity.clone(),
            ],
            score: vec![
                node_affinity.clone(),
                fit.clone(),
                balanced_allocation.clone(),
                taint_toleration.clone(),
                pod_affinity.clone(),
            ],
            reserve: vec![],
            permit: vec![],
            pre_bind: vec![],
            bind: vec![],
            post_bind: vec![],
            enqueue_extensions: vec![
                balanced_allocation.clone(),
                node_affinity.clone(),
                node_name.clone(),
                fit.clone(),
                taint_toleration.clone(),
                pod_affinity.clone(),
            ],
        }
    }
}

/// Plugin called before adding pods to active queue.
/// Should be lightweight (avoid expensive operations like external endpoint calls).
pub trait PreEnqueuePlugin: Plugin + Send + Sync {
    fn pre_enqueue(&self, pod: &PodInfo) -> Status;
}

/// Plugin for sorting pods in the scheduling queue.
/// Only one queue sort plugin can be enabled at a time.
///
/// # Note
/// now it's unimplemented
pub trait _QueueSortPlugin: Plugin + Send + Sync {
    fn less(&self, a: PodInfo, b: PodInfo) -> Ordering;
}

type QueueingHintFn =
    Option<Box<dyn Fn(PodInfo, EventInner) -> Result<QueueingHint, String> + Send + Sync>>;

pub struct ClusterEventWithHint {
    pub event: ClusterEvent,
    /// QueueingHintFn returns a hint that signals whether the event can make a Pod,
    /// which was rejected by this plugin in the past scheduling cycle, schedulable or not.
    /// It's called before a Pod gets moved from unschedulableQ to backoffQ or activeQ.
    /// If it returns an error, we'll take the returned QueueingHint as `Queue` at the caller whatever we returned here so that
    /// we can prevent the Pod from being stuck in the unschedulable pod pool.
    pub queueing_hint_fn: QueueingHintFn,
}

pub struct ClusterEvent {
    pub resource: EventResource,
    pub action_type: ActionType,
}

bitflags! {
    pub struct ActionType: u32 {
        const Add = 1;
        const Delete = 1 << 1;
        const UpdateNodeLabel = 1 << 2;
        const UpdateNodeTaint = 1 << 3;
        const UpdatePodLabel = 1 << 4;
        const UpdatePodToleration = 1 << 5;
        const UpdateNodeAllocatable = 1 << 6;
    }
}

pub enum EventResource {
    Pod,
    Node,
}

/// Updated info of pod or node.
///
/// In kubernetes, it use `oldObj, newObj interface{}` pass updated pod or node,
/// but in rust, it is not graceful to use `Box<dyn Any>` anywhere, so
/// we use enum.
#[derive(Debug, Clone)]
pub enum EventInner {
    Pod(Box<Option<PodInfo>>, Box<Option<PodInfo>>),
    Node(Box<Option<NodeInfo>>, Box<NodeInfo>),
}

pub enum QueueingHint {
    Skip,
    Queue,
}

pub trait EnqueueExtension: Plugin + Send + Sync {
    fn events_to_register(&self) -> Vec<ClusterEventWithHint>;
}

pub trait PreFilterPlugin: Plugin + Send + Sync {
    /// Executes at scheduling cycle start. All plugins must return success or pod is rejected.
    /// Optionally returns filtered nodes to evaluate downstream.
    /// Returns Skip to bypass associated Filter plugin/extensions.
    fn pre_filter(
        &self,
        state: &mut CycleState,
        pod: &PodInfo,
        nodes: Vec<NodeInfo>,
    ) -> (PreFilterResult, Status);
}

/// Result type for PreFilterPlugin::pre_filter
pub struct PreFilterResult {
    pub node_names: Vec<String>,
}

/// Evaluates if a node can run a pod. Returns Success, Unschedulable, or Error.
/// Use provided nodeInfo rather than snapshot (may differ during preemption).
pub trait FilterPlugin: Plugin + Send + Sync {
    fn filter(&self, state: &mut CycleState, pod: &PodInfo, node_info: NodeInfo) -> Status;
}

pub struct NodeToStatus {
    node_to_status: HashMap<String, Status>,
}

impl NodeToStatus {
    fn get(&self, node_name: String) -> Option<Status> {
        self.node_to_status.get(&node_name).cloned()
    }

    fn nodes_for_status_code(&self, _code: Code) -> Vec<NodeInfo> {
        unimplemented!()
    }
}

/// Executes after scheduling failure in PreFilter/Filter phases.
pub trait PostFilterPlugin: Plugin + Send + Sync {
    /// Returns:
    /// - Unschedulable: pod remains unschedulable
    /// - Success: pod can be made schedulable (optionally with PostFilterResult)
    /// - Error: plugin encountered internal error
    fn post_filter(
        &self,
        state: &mut CycleState,
        pod: &PodInfo,
        filtered_node_status_map: NodeToStatus,
    ) -> (PostFilterResult, Status);
}

/// Result type for PostFilterPlugin::post_filter
pub struct PostFilterResult;

/// Informational plugin called after filtering phase with list of viable nodes
pub trait PreScorePlugin: Plugin + Send + Sync {
    /// Executes with nodes that passed filtering. All must return success or pod is rejected.
    /// Returns Skip to bypass associated Score plugin.
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, nodes: Vec<NodeInfo>) -> Status;
}

/// Plugin that ranks nodes passing the filtering phase
pub trait ScorePlugin: Plugin + Send + Sync {
    /// Assigns a score to a node (higher = better fit). Must return success.
    fn score(&self, state: &mut CycleState, pod: &PodInfo, node_info: NodeInfo) -> (i64, Status);

    fn score_extension(&self) -> Box<dyn ScoreExtension>;
}

pub struct NodeScore {
    name: String,
    score: i64,
}

pub trait ScoreExtension {
    fn normalize_score(&self, _: &CycleState, _: &PodInfo, _: &mut Vec<i64>) -> Status;
}

pub struct DefaultNormalizeScore {
    pub max_score: i64,
    pub reverse: bool,
}

impl ScoreExtension for DefaultNormalizeScore {
    fn normalize_score(&self, _: &CycleState, _: &PodInfo, scores: &mut Vec<i64>) -> Status {
        let mut max = 0_i64;
        for node_score in scores.iter_mut() {
            if *node_score > max {
                max = *node_score;
            }
        }

        if max == 0 {
            if self.reverse {
                for node_score in scores.iter_mut() {
                    *node_score = self.max_score;
                }
            }
            return Status::default();
        }

        for node_score in scores.iter_mut() {
            let score = node_score;
            *score = self.max_score * (*score) / max;
        }
        Status::default()
    }
}

/// Plugin that manages state updates when pods are reserved/unreserved
pub trait ReservePlugin: Plugin + Send + Sync {
    /// Called when scheduler cache is updated. Failure triggers Unreserve for all plugins.
    fn reserve(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> &Status;

    /// Called when a reserved pod is rejected or fails later. Must be idempotent.
    fn unreserve(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str);
}

/// Plugin called before a pod is scheduled
pub trait PreBindPlugin: Plugin + Send + Sync {
    /// Lightweight check before PreBind. Returns:
    /// - Success: plugin will handle this pod
    /// - Skip: no action needed for this pod
    fn pre_bind_pre_flight(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str)
    -> Status;

    /// Executes before pod binding. All must succeed or pod is rejected.
    fn pre_bind(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> Status;
}

/// Plugin called after a pod is successfully bound to a node
pub trait PostBindPlugin: Plugin + Send + Sync {
    /// Executes after successful pod binding. Typically used for cleanup.
    fn post_bind(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str);
}

/// Plugin that can prevent or delay pod binding
pub trait PermitPlugin: Plugin + Send + Sync {
    /// Executes before binding. Returns success, wait with timeout, or rejection.
    /// Waiting only occurs if no other plugin rejects the pod.
    fn permit(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> (Status, Duration);
}

/// Plugin responsible for binding a pod to a node
pub trait BindPlugin: Plugin + Send + Sync {
    /// Executes after all PreBind plugins. Handles pod binding or returns Skip.
    /// First handling plugin skips remaining bind plugins.
    fn bind(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> Status;
}

#[derive(Clone, Default)]
pub struct EnabledPlugins {
    pub pre_enqueue: Vec<(Arc<dyn PreEnqueuePlugin>, i64)>,
    pub pre_filter: Vec<(Arc<dyn PreFilterPlugin>, i64)>,
    pub filter: Vec<(Arc<dyn FilterPlugin>, i64)>,
    pub post_filter: Vec<(Arc<dyn PostFilterPlugin>, i64)>,
    pub pre_score: Vec<(Arc<dyn PreScorePlugin>, i64)>,
    pub score: Vec<(Arc<dyn ScorePlugin>, i64)>,
    pub reserve: Vec<(Arc<dyn ReservePlugin>, i64)>,
    pub permit: Vec<(Arc<dyn PermitPlugin>, i64)>,
    pub pre_bind: Vec<(Arc<dyn PreBindPlugin>, i64)>,
    pub bind: Vec<(Arc<dyn BindPlugin>, i64)>,
    pub post_bind: Vec<(Arc<dyn PostBindPlugin>, i64)>,
}

#[derive(Clone)]
/// Registry of all avaliable plugins.
///
/// # Note
/// Not support configure QueueSort method now.
pub struct Registry {
    pub pre_enqueue: Vec<Arc<dyn PreEnqueuePlugin>>,
    pub pre_filter: Vec<Arc<dyn PreFilterPlugin>>,
    pub filter: Vec<Arc<dyn FilterPlugin>>,
    pub post_filter: Vec<Arc<dyn PostFilterPlugin>>,
    pub pre_score: Vec<Arc<dyn PreScorePlugin>>,
    pub score: Vec<Arc<dyn ScorePlugin>>,
    pub reserve: Vec<Arc<dyn ReservePlugin>>,
    pub permit: Vec<Arc<dyn PermitPlugin>>,
    pub pre_bind: Vec<Arc<dyn PreBindPlugin>>,
    pub bind: Vec<Arc<dyn BindPlugin>>,
    pub post_bind: Vec<Arc<dyn PostBindPlugin>>,
    pub enqueue_extensions: Vec<Arc<dyn EnqueueExtension>>,
}

impl Default for Registry {
    fn default() -> Self {
        let node_affinity = Arc::new(node_affinity::NodeAffinity {});
        let node_name = Arc::new(NodeName {});
        let fit = Arc::new(Fit {});
        let node_unschedulable = Arc::new(NodeUnschedulable {});
        let scheduling_gates = Arc::new(SchedulingGates {});
        let taint_toleration = Arc::new(TaintToleration {});
        let balanced_allocation = Arc::new(BalancedAllocation::default());
        let pod_affinity = Arc::new(pod_affinity::PodAffinityPlugin);

        Self {
            pre_enqueue: vec![scheduling_gates.clone()],
            pre_filter: vec![node_affinity.clone(), fit.clone(), pod_affinity.clone()],
            filter: vec![
                node_affinity.clone(),
                fit.clone(),
                taint_toleration.clone(),
                node_name.clone(),
                node_unschedulable.clone(),
                pod_affinity.clone(),
            ],
            post_filter: vec![],
            pre_score: vec![
                node_affinity.clone(),
                fit.clone(),
                balanced_allocation.clone(),
                taint_toleration.clone(),
                pod_affinity.clone(),
            ],
            score: vec![
                node_affinity.clone(),
                fit.clone(),
                balanced_allocation.clone(),
                taint_toleration.clone(),
                pod_affinity.clone(),
            ],
            // below features are unimplemented
            reserve: vec![],
            permit: vec![],
            pre_bind: vec![],
            bind: vec![],
            post_bind: vec![],
            enqueue_extensions: vec![
                balanced_allocation.clone(),
                node_affinity.clone(),
                node_name.clone(),
                fit.clone(),
                taint_toleration.clone(),
                pod_affinity.clone(),
            ],
        }
    }
}

#[derive(Clone)]
pub struct Status {
    pub code: Code,
    // Prompting the reason for scheduling failure has not yet been implemented.
    pub reasons: Vec<String>,
    pub err: String,
    pub plugin: String,
}

impl Default for Status {
    /// Default success status
    fn default() -> Self {
        Self {
            code: Code::Success,
            reasons: vec![],
            err: String::new(),
            plugin: String::new(),
        }
    }
}

impl Status {
    pub fn new(code: Code, reasons: Vec<String>) -> Self {
        Status {
            code,
            reasons,
            err: String::new(),
            plugin: String::new(),
        }
    }

    pub fn error(error: &str) -> Self {
        Self {
            code: Code::Error,
            err: error.to_string(),
            ..Default::default()
        }
    }
}

/// Code is the Status code/type which is returned from plugins.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum Code {
    /// Success means that plugin ran correctly and found pod schedulable.
    Success,
    /// Error is one of the failures, used for internal plugin errors, unexpected input, etc.
    /// Plugin shouldn't return this code for expected failures, like Unschedulable.
    /// Since it's the unexpected failure, the scheduling queue registers the pod without unschedulable plugins.
    /// Meaning, the Pod will be requeued to activeQ/backoffQ soon.
    Error,
    /// Unschedulable is one of the failures, used when a plugin finds a pod unschedulable.
    /// If it's returned from PreFilter or Filter, the scheduler might attempt to
    /// run other postFilter plugins like preemption to get this pod scheduled.
    Unschedulable,
    /// UnschedulableAndUnresolvable is used when a plugin finds a pod unschedulable and
    /// other postFilter plugins like preemption would not change anything.
    UnschedulableAndUnresolvable,
    /// Wait is used when a Permit plugin finds a pod scheduling should wait.
    _Wait,
    /// Skip is used in the following scenarios:
    /// - when a Bind plugin chooses to skip binding.
    /// - when a PreFilter plugin returns Skip so that coupled Filter plugin/PreFilterExtensions() will be skipped.
    /// - when a PreScore plugin returns Skip so that coupled Score plugin will be skipped.
    Skip,
    /// Pending means that the scheduling process is finished successfully,
    /// but the plugin wants to stop the scheduling cycle/binding cycle here.
    ///
    /// Pods rejected by such reasons don't need to suffer a penalty (backoff).
    /// When the scheduling queue requeues Pods, which was rejected with Pending in the last scheduling,
    /// the Pod goes to activeQ directly ignoring backoff.
    Pending,
}
