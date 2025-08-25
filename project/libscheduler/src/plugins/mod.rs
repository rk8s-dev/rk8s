//! Scheduler plugins. 
//! 
//! The functionality of each plugin corresponds to its namesake in Kubernetes. 
//! Some comments are also quoted from the Kubernetes codebase.

use crate::cycle_state::CycleState;
use crate::models::{NodeInfo, PodInfo};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::Duration;
use bitflags::bitflags;

mod node_name;
mod node_unschedulable;
mod priority_sort;
mod scheduling_gates;
mod taint_toleration;
mod node_affinity;
mod node_resources_fit;
mod balanced_allocation;

/// Plugin specifies a plugin name and its weight when applicable. Weight is used only for Score plugins.
struct PluginInfo {
    name: String,
    weight: i32,
}

/// List of enabled plugins
struct Plugins {
    pre_enqueue: Vec<PluginInfo>,
    queue_sort: PluginInfo,
    pre_filter: Vec<PluginInfo>,
    filter: Vec<PluginInfo>,
    post_filter: Vec<PluginInfo>,
    pre_score: Vec<PluginInfo>,
    reserve: Vec<PluginInfo>,
    permit: Vec<PluginInfo>,
    pre_bind: Vec<PluginInfo>,
    bind: Vec<PluginInfo>,
    post_bind: Vec<PluginInfo>,
}

pub trait Plugin {
    fn name(&self) -> &str;
}

/// Plugin called before adding pods to active queue.
/// Should be lightweight (avoid expensive operations like external endpoint calls).
pub trait PreEnqueuePlugin: Plugin {
    fn pre_enqueue(&self, pod: &PodInfo) -> Status;
}

/// Plugin for sorting pods in the scheduling queue.
/// Only one queue sort plugin can be enabled at a time.
pub trait QueueSortPlugin: Plugin {
    fn less(&self, a: PodInfo, b: PodInfo) -> Ordering;
}


pub struct ClusterEventWithHint {
    event: ClusterEvent,
    /// QueueingHintFn returns a hint that signals whether the event can make a Pod,
    /// which was rejected by this plugin in the past scheduling cycle, schedulable or not.
    /// It's called before a Pod gets moved from unschedulableQ to backoffQ or activeQ.
    /// If it returns an error, we'll take the returned QueueingHint as `Queue` at the caller whatever we returned here so that
    /// we can prevent the Pod from being stuck in the unschedulable pod pool.
    queueing_hint_fn: Option<Box<dyn Fn(PodInfo, EventInner) -> Result<QueueingHint, String>>>,
}

pub struct ClusterEvent {
    resource: EventResource,
    action_type: ActionType,   
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
#[derive(Debug)]
pub enum EventInner {
    Pod(Option<PodInfo>, Option<PodInfo>),
    Node(Option<NodeInfo>, NodeInfo)
}

pub enum QueueingHint {
    Skip,
    Queue,
}

pub trait EnqueueExtension: Plugin {
    fn events_to_register() -> Vec<ClusterEventWithHint>;
}

pub trait PreFilterPlugin: Plugin {
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
    pub node_names:  Vec<String>
}

/// Evaluates if a node can run a pod. Returns Success, Unschedulable, or Error.
/// Use provided nodeInfo rather than snapshot (may differ during preemption).
pub trait FilterPlugin: Plugin {
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
pub trait PostFilterPlugin: Plugin {
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
pub trait PreScorePlugin: Plugin {
    /// Executes with nodes that passed filtering. All must return success or pod is rejected.
    /// Returns Skip to bypass associated Score plugin.
    fn pre_score(&self, state: &mut CycleState, pod: &PodInfo, nodes: Vec<NodeInfo>) -> Status;
}

/// Plugin that ranks nodes passing the filtering phase
pub trait ScorePlugin: Plugin {
    /// Assigns a score to a node (higher = better fit). Must return success.
    fn score(&self, state: &mut CycleState, pod: &PodInfo, node_info: NodeInfo) -> (i64, Status);

    fn score_extension() -> Box<dyn ScoreExtension>;
}

pub struct NodeScore {
    name: String,
    score: i64,
}

pub trait ScoreExtension {
    fn normalize_score(&self, _: &CycleState, _: &PodInfo, _: &mut Vec<NodeScore>) -> Status;
}

pub struct DefaultNormalizeScore {
    pub max_score: i64,
    pub reverse: bool,
}

impl ScoreExtension for DefaultNormalizeScore {
    fn normalize_score(&self, _: &CycleState, _: &PodInfo, scores: &mut Vec<NodeScore>) -> Status {
        let mut max = 0;
        for node_score in scores.iter_mut() {
            if node_score.score > max {
                max = node_score.score;
            }
        }

        if max == 0 {
            if self.reverse {
                for node_score in scores.iter_mut() {
                    node_score.score = self.max_score;
                }
            }
            return Status::default();
        }

        for node_score in scores.iter_mut() {
            let mut score = node_score.score;
            score = self.max_score * score / max;
            node_score.score = score
        }
        return Status::default();
    }
}

/// Plugin that manages state updates when pods are reserved/unreserved
pub trait ReservePlugin: Plugin {
    /// Called when scheduler cache is updated. Failure triggers Unreserve for all plugins.
    fn reserve(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> &Status;

    /// Called when a reserved pod is rejected or fails later. Must be idempotent.
    fn unreserve(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str);
}

/// Plugin called before a pod is scheduled
pub trait PreBindPlugin: Plugin {
    /// Lightweight check before PreBind. Returns:
    /// - Success: plugin will handle this pod
    /// - Skip: no action needed for this pod
    fn pre_bind_pre_flight(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str)
    -> Status;

    /// Executes before pod binding. All must succeed or pod is rejected.
    fn pre_bind(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> Status;
}

/// Plugin called after a pod is successfully bound to a node
pub trait PostBindPlugin: Plugin {
    /// Executes after successful pod binding. Typically used for cleanup.
    fn post_bind(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str);
}

/// Plugin that can prevent or delay pod binding
pub trait PermitPlugin: Plugin {
    /// Executes before binding. Returns success, wait with timeout, or rejection.
    /// Waiting only occurs if no other plugin rejects the pod.
    fn permit(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> (Status, Duration);
}

/// Plugin responsible for binding a pod to a node
pub trait BindPlugin: Plugin {
    /// Executes after all PreBind plugins. Handles pod binding or returns Skip.
    /// First handling plugin skips remaining bind plugins.
    fn bind(&self, state: &mut CycleState, pod: &PodInfo, node_name: &str) -> Status;
}

struct Registry {
    pre_enqueue: Vec<Box<dyn PreEnqueuePlugin>>,
    queue_sort: Box<dyn QueueSortPlugin>,
    pre_filter: Vec<Box<dyn PreFilterPlugin>>,
    filter: Vec<Box<dyn FilterPlugin>>,
    post_filter: Vec<Box<dyn PostFilterPlugin>>,
    pre_score: Vec<Box<dyn PreScorePlugin>>,
    reserve: Vec<Box<dyn ReservePlugin>>,
    permit: Vec<Box<dyn PermitPlugin>>,
    pre_bind: Vec<Box<dyn PreBindPlugin>>,
    bind: Vec<Box<dyn BindPlugin>>,
    post_bind: Vec<Box<dyn PostBindPlugin>>,
}

#[derive(Clone)]
pub struct Status {
    pub code: Code,
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
#[derive(PartialEq, Eq, Clone)]
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
    Wait,
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
