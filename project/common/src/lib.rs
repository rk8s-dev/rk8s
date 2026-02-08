use chrono::{DateTime, Utc};
use libcni::ip::route::{Interface, Route};
use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};
use std::fmt::{Display, Formatter};
use std::{
    collections::HashMap,
    fmt,
    net::{Ipv4Addr, Ipv6Addr},
    time::Duration,
};
use uuid::Uuid;

pub mod _private {
    pub use log::error;
}

pub mod lease;
pub mod quic;

use libcontainer::oci_spec::runtime::Capability;
pub use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TypeMeta {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ObjectMeta {
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default = "Uuid::new_v4")]
    pub uid: Uuid,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
    #[serde(default)]
    pub owner_references: Option<Vec<OwnerReference>>,
    #[serde(default)]
    pub creation_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deletion_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finalizers: Option<Vec<Finalizer>>,
    #[serde(default)]
    pub generation: Option<i64>,
}

impl Default for ObjectMeta {
    fn default() -> Self {
        Self {
            name: String::new(),
            namespace: default_namespace(),
            uid: Uuid::new_v4(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
            owner_references: None,
            creation_timestamp: Some(Utc::now()),
            deletion_timestamp: None,
            finalizers: None,
            generation: Some(0),
        }
    }
}

/// A lightweight reference to another object (similar to Kubernetes' ObjectReference).
/// Used for optional cross-references (e.g. EndpointAddress.targetRef).
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct ObjectReference {
    #[serde(rename = "apiVersion", default)]
    pub api_version: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub namespace: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(rename = "resourceVersion", default)]
    pub resource_version: Option<String>,
    #[serde(rename = "fieldPath", default)]
    pub field_path: Option<String>,
}

fn default_namespace() -> String {
    "default".to_string()
}

#[derive(Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Copy, Clone, Default)]
pub enum ResourceKind {
    Pod,
    Service,
    Deployment,
    ReplicaSet,
    Endpoint,
    #[default]
    Unknown,
}

impl fmt::Display for ResourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            ResourceKind::Pod => "Pod",
            ResourceKind::Service => "Service",
            ResourceKind::Deployment => "Deployment",
            ResourceKind::ReplicaSet => "ReplicaSet",
            ResourceKind::Endpoint => "Endpoint",
            ResourceKind::Unknown => "Unknown",
        };
        write!(f, "{}", kind)
    }
}
impl From<&str> for ResourceKind {
    fn from(input: &str) -> Self {
        match input {
            "Pod" => ResourceKind::Pod,
            "Service" => ResourceKind::Service,
            "Deployment" => ResourceKind::Deployment,
            "ReplicaSet" => ResourceKind::ReplicaSet,
            "Endpoint" => ResourceKind::Endpoint,
            _ => ResourceKind::Unknown, // Default to Unknown for unknown kinds
        }
    }
}

/// Owner reference that establishes ownership relationships between resources.
///
/// This is a core mechanism for managing resource dependencies. The `GarbageCollector`
/// uses `OwnerReference` to implement cascading deletion: when an owner resource is deleted,
/// the garbage collector automatically handles the deletion of dependent resources based on
/// the deletion propagation policy. It tracks owner-dependant relationships through these
/// references.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize, Clone, Default)]
pub struct OwnerReference {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    pub kind: ResourceKind,
    pub name: String,
    pub uid: Uuid,
    pub controller: bool,
    #[serde(default)]
    pub block_owner_deletion: Option<bool>,
}

/// Finalizer used to perform cleanup operations when a resource is deleted.
///
/// When a resource contains finalizers, it will not be immediately deleted even if a delete
/// request is received. Instead, it waits until all finalizers are removed before actual deletion.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Finalizer {
    DeletingDependents,
    OrphanDependents,
    Custom(String),
}

impl From<&str> for Finalizer {
    fn from(s: &str) -> Self {
        match s {
            "DeletingDependents" => Finalizer::DeletingDependents,
            "OrphanDependents" => Finalizer::OrphanDependents,
            other => Finalizer::Custom(other.to_string()),
        }
    }
}

impl fmt::Display for Finalizer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Finalizer::DeletingDependents => write!(f, "DeletingDependents"),
            Finalizer::OrphanDependents => write!(f, "OrphanDependents"),
            Finalizer::Custom(s) => write!(f, "{}", s),
        }
    }
}

/// Delete propagation policy for cascading deletion.
///
/// When deleting an owner resource, you can specify how its dependents should be handled:
///
/// - **Foreground**: Mark the owner for deletion, but don't delete it until all its dependents
///   are deleted. The owner gets a `DeletingDependents` finalizer and waits for all blocking
///   dependents to be removed.
/// - **Background**: Delete the owner immediately, and let the garbage collector delete its
///   dependents in the background. This is the fastest deletion method.
/// - **Orphan**: Delete the owner, but leave dependents as orphaned objects (remove owner
///   references from dependents). The owner gets an `OrphanDependents` finalizer and
///   will be deleted after all dependents' owner references are removed.
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum DeletePropagationPolicy {
    Foreground,
    Background,
    Orphan,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Affinity {
    #[serde(default)]
    pub node_affinity: Option<NodeAffinity>,
    #[serde(default)]
    pub pod_affinity: Option<PodAffinity>,
    #[serde(default)]
    pub pod_anti_affinity: Option<PodAntiAffinity>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct NodeAffinity {
    #[serde(default, rename = "requiredDuringSchedulingIgnoredDuringExecution")]
    pub required_during_scheduling_ignored_during_execution: Option<NodeSelector>,
    #[serde(default, rename = "preferredDuringSchedulingIgnoredDuringExecution")]
    pub preferred_during_scheduling_ignored_during_execution: Option<PreferredSchedulingTerms>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PodAffinityTerm {
    #[serde(default, rename = "labelSelector")]
    pub label_selector: Option<LabelSelector>,
    #[serde(rename = "topologyKey")]
    pub topology_key: String,
    #[serde(default)]
    pub namespaces: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct WeightedPodAffinityTerm {
    pub weight: i32,
    #[serde(rename = "podAffinityTerm")]
    pub pod_affinity_term: PodAffinityTerm,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PodAffinity {
    #[serde(default, rename = "requiredDuringSchedulingIgnoredDuringExecution")]
    pub required_during_scheduling_ignored_during_execution: Option<Vec<PodAffinityTerm>>,
    #[serde(default, rename = "preferredDuringSchedulingIgnoredDuringExecution")]
    pub preferred_during_scheduling_ignored_during_execution: Option<Vec<WeightedPodAffinityTerm>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PodAntiAffinity {
    #[serde(default, rename = "requiredDuringSchedulingIgnoredDuringExecution")]
    pub required_during_scheduling_ignored_during_execution: Option<Vec<PodAffinityTerm>>,
    #[serde(default, rename = "preferredDuringSchedulingIgnoredDuringExecution")]
    pub preferred_during_scheduling_ignored_during_execution: Option<Vec<WeightedPodAffinityTerm>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PreferredSchedulingTerms {
    #[serde(default)]
    pub terms: Vec<PreferredSchedulingTerm>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PreferredSchedulingTerm {
    pub weight: i64,
    #[serde(rename = "preference")]
    pub preference: NodeSelectorTerm,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct NodeSelector {
    #[serde(rename = "nodeSelectorTerms")]
    pub node_selector_terms: Vec<NodeSelectorTerm>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct NodeSelectorTerm {
    #[serde(rename = "matchExpressions")]
    pub match_expressions: Vec<NodeSelectorRequirement>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct NodeSelectorRequirement {
    pub key: String,
    pub operator: NodeSelectorOperator,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum NodeSelectorOperator {
    In,
    NotIn,
    #[default]
    Exists,
    DoesNotExist,
    Gt,
    Lt,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct PodSpec {
    //if pod is distributed to a node ,then this field should be filled with node-id
    #[serde(default)]
    pub node_name: Option<String>,
    #[serde(default)]
    pub containers: Vec<ContainerSpec>,
    #[serde(default)]
    pub init_containers: Vec<ContainerSpec>,
    #[serde(default)]
    pub tolerations: Vec<Toleration>,
    #[serde(default)]
    pub affinity: Option<Affinity>,
    #[serde(default)]
    pub restart_policy: RestartPolicy,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum RestartPolicy {
    Always,
    OnFailure,
    #[default]
    Never,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ContainerRes {
    pub limits: Option<Resource>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Resource {
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct SecurityContext {
    #[serde(rename = "runAsUser")]
    pub run_as_user: Option<i64>,

    #[serde(rename = "runAsGroup")]
    pub run_as_group: Option<i64>,

    #[serde(default)]
    pub privileged: Option<bool>,

    #[serde(rename = "allowPrivilegeEscalation", default)]
    pub allow_privilege_escalation: Option<bool>,

    pub capabilities: Option<Capabilities>,
}

/// The pattern should be like: "CAP_AUDIT_CONTROL" refers to  <http://man7.org/linux/man-pages/man7/capabilities.7.html>
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Capabilities {
    #[serde(default)]
    pub add: Vec<Capability>, // List of capabilities to add

    #[serde(default)]
    pub drop: Vec<Capability>, // List of capabilities to drop
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EnvVar {
    pub name: String,

    #[serde(default)]
    pub value: Option<String>,
    // #[serde(rename = "valueFrom", default)]
    // pub value_from: Option<EnvVarSource>,
}

// #[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
// pub struct EnvVarSource {
//     #[serde(rename = "secretKeyRef", default)]
//     pub secret_key_ref: Option<SecretKeySelector>, // Selects a key of a Secret

//     #[serde(rename = "configMapKeyRef", default)]
//     pub config_map_key_ref: Option<ConfigMapKeySelector>, // Selects a key of a ConfigMap

//     #[serde(rename = "fieldRef", default)]
//     pub field_ref: Option<ObjectFieldSelector>, // Selects a field of the Pod/container
// }

// // Placeholder structures for EnvVarSource fields
// pub struct SecretKeySelector { /* ... */ }
// pub struct ConfigMapKeySelector { /* ... */ }
// pub struct ObjectFieldSelector { /* ... */ }

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct VolumeMount {
    pub name: String,

    #[serde(rename = "mountPath")]
    pub mount_path: String,

    #[serde(rename = "readOnly", default)]
    pub read_only: Option<bool>,

    #[serde(rename = "subPath", default)]
    pub sub_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ContainerSpec {
    pub name: String,

    pub image: String,

    #[serde(default)]
    pub ports: Vec<Port>,

    #[serde(default)]
    pub args: Vec<String>,

    pub resources: Option<ContainerRes>,

    #[serde(rename = "livenessProbe", default)]
    pub liveness_probe: Option<Probe>,

    #[serde(rename = "readinessProbe", default)]
    pub readiness_probe: Option<Probe>,

    #[serde(rename = "startupProbe", default)]
    pub startup_probe: Option<Probe>,

    // Handle the security
    #[serde(rename = "securityContext", default)]
    pub security_context: Option<SecurityContext>,

    #[serde(default)]
    pub env: Option<Vec<EnvVar>>,

    #[serde(rename = "volumeMounts", default)]
    pub volume_mounts: Option<Vec<VolumeMount>>,

    #[serde(default)]
    pub command: Option<Vec<String>>,

    #[serde(rename = "workingDir", default)]
    pub working_dir: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ProbeAction {
    Exec(ExecAction),
    HttpGet(HttpGetAction),
    TcpSocket(TcpSocketAction),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct Probe {
    pub action: Option<ProbeAction>,

    #[serde(rename = "initialDelaySeconds", default)]
    pub initial_delay_seconds: Option<u32>,

    #[serde(rename = "periodSeconds", default)]
    pub period_seconds: Option<u32>,

    #[serde(rename = "timeoutSeconds", default)]
    pub timeout_seconds: Option<u32>,

    #[serde(rename = "successThreshold", default)]
    pub success_threshold: Option<u32>,

    #[serde(rename = "failureThreshold", default)]
    pub failure_threshold: Option<u32>,
}

impl Probe {
    /// Validates that the probe has exactly one action specified
    pub fn validate(&self) -> Result<(), String> {
        match &self.action {
            Some(_) => Ok(()),
            None => Err(
                "probe must specify exactly one action (exec, httpGet, or tcpSocket)".to_string(),
            ),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct ExecAction {
    #[serde(default)]
    pub command: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct HttpGetAction {
    #[serde(default = "default_http_path")]
    pub path: String,

    pub port: u16,

    #[serde(default)]
    pub host: Option<String>,
}

fn default_http_path() -> String {
    "/".to_string()
}

impl Default for HttpGetAction {
    fn default() -> Self {
        Self {
            path: default_http_path(),
            port: 0,
            host: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct TcpSocketAction {
    pub port: u16,

    #[serde(default)]
    pub host: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Port {
    #[serde(rename = "containerPort")]
    pub container_port: i32,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(rename = "hostPort", default)]
    pub host_port: i32,
    #[serde(rename = "hostIP", default)]
    pub host_ip: String,
}

fn default_protocol() -> String {
    "TCP".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PodTask {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: PodSpec,
    #[serde(default)]
    pub status: PodStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct PodStatus {
    #[serde(rename = "podIP")]
    pub pod_ip: Option<String>,

    #[serde(rename = "containerStatuses", default)]
    pub container_statuses: Vec<ContainerStatus>,
    /// Phase indicates the high-level summary of the pod's status.
    #[serde(default)]
    pub phase: PodPhase,
    /// Detailed conditions of the pod's status.
    #[serde(default)]
    pub conditions: Option<Vec<PodCondition>>,
    /// A message indicating details about why the pod is in this state
    #[serde(default)]
    pub message: Option<String>,
    /// A brief reason message about why the pod is in this state
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub start_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum PodPhase {
    #[default]
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct PodCondition {
    #[serde(rename = "type")]
    pub condition_type: PodConditionType,
    pub status: ConditionStatus,
    #[serde(rename = "lastProbeTime", default)]
    pub last_probe_time: Option<DateTime<Utc>>,
    #[serde(rename = "lastTransitionTime", default)]
    pub last_transition_time: Option<DateTime<Utc>>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
pub enum PodConditionType {
    #[default]
    PodScheduled,
    PodReady,
    ContainersReady,
    PodInitialized,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ContainerStatus {
    pub name: String,

    #[serde(default)]
    pub state: Option<ContainerState>,

    #[serde(default)]
    pub last_termination_state: Option<ContainerState>,

    #[serde(default)]
    pub ready: bool,

    #[serde(default)]
    pub restart_count: u32,

    #[serde(rename = "readinessProbe", default)]
    pub readiness_probe: Option<ContainerProbeStatus>,

    #[serde(rename = "livenessProbe", default)]
    pub liveness_probe: Option<ContainerProbeStatus>,

    #[serde(rename = "startupProbe", default)]
    pub startup_probe: Option<ContainerProbeStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum ContainerState {
    Waiting {
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
    Running {
        #[serde(default)]
        started_at: Option<DateTime<Utc>>,
    },
    Terminated {
        exit_code: i32,
        #[serde(default)]
        signal: Option<i32>,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        started_at: Option<DateTime<Utc>>,
        #[serde(default)]
        finished_at: Option<DateTime<Utc>>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "PascalCase")]
pub enum ProbeCondition {
    #[default]
    Pending,
    Ready,
    Failing,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct ContainerProbeStatus {
    pub state: ProbeCondition,

    #[serde(rename = "consecutiveSuccesses", default)]
    pub consecutive_successes: u32,

    #[serde(rename = "consecutiveFailures", default)]
    pub consecutive_failures: u32,

    #[serde(rename = "lastError", default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct Toleration {
    /// Empty means match all taint keys.
    pub key: Option<TaintKey>,

    /// Operator represents a key's relationship to the value.
    #[serde(default)]
    pub operator: TolerationOperator,

    /// Effect indicates the taint effect to match. None means match all.
    pub effect: Option<TaintEffect>,

    #[serde(default)]
    pub value: String,
}

impl Toleration {
    pub fn tolerate(&self, taint: &Taint) -> bool {
        if self.effect.is_some() && self.effect.as_ref().unwrap() != &taint.effect {
            return false;
        }
        if self.key.is_some() && self.key.as_ref().unwrap() != &taint.key {
            return false;
        }
        match self.operator {
            TolerationOperator::Equal => self.value == taint.value,
            TolerationOperator::Exists => true,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub enum TolerationOperator {
    Exists,
    #[default]
    Equal,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaintEffect {
    NoSchedule,
    PreferNoSchedule,
    NoExecute,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TaintKey {
    NodeNotReady,
    NodeUnreachable,
    NodeUnschedulable,
    NodeMemoryPressure,
    NodeDiskPressure,
    NodeNetworkUnavailable,
    NodeOutOfService,
}

#[macro_export]
macro_rules! invalid_rks_variant_error {
    ($message:expr, $expected:pat) => {
        RksMessage::Error(format!(
            "invalid message, expected: {}, but got: {}",
            stringify!($expected),
            $message
        ))
    };
}

#[derive(Serialize, Deserialize, Clone)]
pub enum RksMessage {
    //request
    CreatePod(Box<PodTask>),
    DeletePod(String),
    GetPodByUid(Uuid),
    GetPod(String),
    ListPod,

    CreateReplicaSet(Box<ReplicaSet>),
    UpdateReplicaSet(Box<ReplicaSet>),
    DeleteReplicaSet(String),
    GetReplicaSet(String),
    ListReplicaSet,

    // Deployment operations
    CreateDeployment(Box<Deployment>),
    UpdateDeployment(Box<Deployment>),
    DeleteDeployment(String),
    GetDeployment(String),
    ListDeployment,
    RollbackDeployment {
        name: String,
        revision: i64,
    },
    GetDeploymentHistory(String),

    // Service operations
    CreateService(Box<ServiceTask>),
    UpdateService(Box<ServiceTask>),
    DeleteService(String),
    GetService(String),
    ListService,

    GetNodeCount,
    RegisterNode(Box<Node>),
    UserRequest(String),
    Heartbeat {
        node_name: String,
        status: NodeStatus,
    },
    SetNetwork(Box<NodeNetworkConfig>),
    UpdateRoutes(String, Vec<Route>),
    SetDns(String, u16),

    CertificateSign {
        token: String,
        req: IssueCertificateRequest,
    },
    /// Set nftables rules payload (serialized nft commands) - Full Sync
    SetNftablesRules(String),
    /// Update nftables rules payload (serialized nft commands) - Incremental
    UpdateNftablesRules(String),

    UpdatePodStatus {
        pod_name: String,
        pod_namespace: String,
        status: PodStatus,
    },

    //response
    Ack,
    Error(String),
    NodeCount(usize),
    GetPodByUidRes(Box<PodTask>),
    GetPodRes(Box<PodTask>),
    ListPodRes(Vec<PodTask>),
    GetReplicaSetRes(Box<ReplicaSet>),
    ListReplicaSetRes(Vec<ReplicaSet>),
    // Deployment responses
    GetDeploymentRes(Box<Deployment>),
    ListDeploymentRes(Vec<Deployment>),
    DeploymentHistoryRes(Vec<DeploymentRevisionInfo>),
    // Service responses
    GetServiceRes(Box<ServiceTask>),
    ListServiceRes(Vec<ServiceTask>),
    // (Podname, Podip)
    SetPodip((String, String)),
    Certificate(IssueCertificateResponse),
}

impl std::fmt::Debug for RksMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            // request
            Self::CreatePod(_) => f.write_str("RksMessage::CreatePod { .. }"),
            Self::DeletePod(pod_name) => {
                write!(f, "RksMessage::DeletePod {{ pod_name: {} }}", pod_name)
            }
            Self::GetPodByUid(uid) => write!(f, "RksMessage::GetPodByUid({})", uid),
            Self::GetPod(name) => write!(f, "RksMessage::GetPod({})", name),
            Self::ListPod => f.write_str("RksMessage::ListPod"),
            Self::CreateReplicaSet(_) => f.write_str("RksMessage::CreateReplicaSet { .. }"),
            Self::UpdateReplicaSet(_) => f.write_str("RksMessage::UpdateReplicaSet { .. }"),
            Self::DeleteReplicaSet(name) => {
                write!(f, "RksMessage::DeleteReplicaSet {{ name: {} }}", name)
            }
            Self::GetReplicaSet(name) => {
                write!(f, "RksMessage::GetReplicaSet {{ name: {} }}", name)
            }
            Self::ListReplicaSet => f.write_str("RksMessage::ListReplicaSet"),
            Self::CreateDeployment(_) => f.write_str("RksMessage::CreateDeployment { .. }"),
            Self::UpdateDeployment(_) => f.write_str("RksMessage::UpdateDeployment { .. }"),
            Self::DeleteDeployment(name) => {
                write!(f, "RksMessage::DeleteDeployment {{ name: {} }}", name)
            }
            Self::GetDeployment(name) => {
                write!(f, "RksMessage::GetDeployment {{ name: {} }}", name)
            }
            Self::ListDeployment => f.write_str("RksMessage::ListDeployment"),
            Self::RollbackDeployment { name, revision } => {
                write!(
                    f,
                    "RksMessage::RollbackDeployment {{ name: {}, revision: {} }}",
                    name, revision
                )
            }
            Self::GetDeploymentHistory(name) => {
                write!(f, "RksMessage::GetDeploymentHistory {{ name: {} }}", name)
            }
            Self::CreateService(_) => f.write_str("RksMessage::CreateService { .. }"),
            Self::UpdateService(_) => f.write_str("RksMessage::UpdateService { .. }"),
            Self::DeleteService(name) => {
                write!(f, "RksMessage::DeleteService {{ name: {} }}", name)
            }
            Self::GetService(name) => write!(f, "RksMessage::GetService {{ name: {} }}", name),
            Self::ListService => f.write_str("RksMessage::ListService"),
            Self::GetNodeCount => f.write_str("RksMessage::GetNodeCount"),
            Self::RegisterNode(_) => f.write_str("RksMessage::RegisterNode { .. }"),
            Self::UserRequest(_) => f.write_str("RksMessage::UserRequest { .. }"),
            Self::Heartbeat { node_name, status } => {
                write!(
                    f,
                    "RksMessage::Heartbeat {{ node_name: {}, status: {:?} }}",
                    node_name, status
                )
            }
            Self::SetNetwork(_) => f.write_str("RksMessage::SetNetwork { .. }"),
            Self::UpdateRoutes(node_name, routes) => {
                write!(
                    f,
                    "RksMessage::UpdateRoutes {{ node_name: {}, routes_count: {} }}",
                    node_name,
                    routes.len()
                )
            }
            Self::CertificateSign { .. } => f.write_str("RksMessage::CertificateSign { .. }"),
            Self::UpdatePodStatus {
                pod_name,
                pod_namespace,
                status: _,
            } => write!(
                f,
                "RksMessage::UpdatePodStatus {{ pod_name: {}, pod_namespace: {} }}",
                pod_name, pod_namespace
            ),
            Self::SetNftablesRules(rules) => {
                write!(f, "RksMessage::SetNftablesRules (len={})", rules.len())
            }
            Self::UpdateNftablesRules(rules) => {
                write!(f, "RksMessage::UpdateNftablesRules (len={})", rules.len())
            }
            // response
            Self::Ack => f.write_str("RksMessage::Ack"),
            Self::Error(err_msg) => write!(f, "RksMessage::Error({})", err_msg),
            Self::NodeCount(count) => write!(f, "RksMessage::NodeCount({})", count),
            Self::GetPodByUidRes(_) => f.write_str("RksMessage::GetPodByUidRes { .. }"),
            Self::GetPodRes(_) => f.write_str("RksMessage::GetPodRes { .. }"),
            Self::ListPodRes(pods) => {
                write!(f, "RksMessage::ListPodRes {{ count: {} }}", pods.len())
            }
            Self::GetReplicaSetRes(_) => f.write_str("RksMessage::GetReplicaSetRes { .. }"),
            Self::ListReplicaSetRes(rss) => {
                write!(
                    f,
                    "RksMessage::ListReplicaSetRes {{ count: {} }}",
                    rss.len()
                )
            }
            Self::GetDeploymentRes(_) => f.write_str("RksMessage::GetDeploymentRes { .. }"),
            Self::ListDeploymentRes(deps) => {
                write!(
                    f,
                    "RksMessage::ListDeploymentRes {{ count: {} }}",
                    deps.len()
                )
            }
            Self::DeploymentHistoryRes(history) => {
                write!(
                    f,
                    "RksMessage::DeploymentHistoryRes {{ count: {} }}",
                    history.len()
                )
            }
            Self::GetServiceRes(_) => f.write_str("RksMessage::GetServiceRes { .. }"),
            Self::ListServiceRes(services) => {
                write!(
                    f,
                    "RksMessage::ListServiceRes {{ count: {} }}",
                    services.len()
                )
            }
            Self::SetPodip((pod_name, pod_ip)) => {
                write!(
                    f,
                    "RksMessage::SetPodip {{ pod_name: {}, pod_ip: {} }}",
                    pod_name, pod_ip
                )
            }
            Self::SetDns(ip, dns_port) => write!(
                f,
                "RksMessage::SetDns {{ ip: {}, dns_port: {} }}",
                ip, dns_port,
            ),
            Self::Certificate(_) => f.write_str("RksMessage::Certificate"),
        }
    }
}

impl Display for RksMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            // request
            Self::CreatePod(pod) => write!(
                f,
                "Create pod '{}' in namespace '{}'",
                pod.metadata.name, pod.metadata.namespace
            ),
            Self::DeletePod(pod_name) => write!(f, "Delete pod '{}'", pod_name),
            Self::GetPodByUid(uid) => write!(f, "Get pod by UID '{}'", uid),
            Self::GetPod(name) => write!(f, "Get pod '{}'", name),
            Self::ListPod => f.write_str("List pods"),
            Self::CreateReplicaSet(rs) => write!(f, "Create replicaset '{}'", rs.metadata.name),
            Self::UpdateReplicaSet(rs) => write!(f, "Update replicaset '{}'", rs.metadata.name),
            Self::DeleteReplicaSet(name) => write!(f, "Delete replicaset '{}'", name),
            Self::GetReplicaSet(name) => write!(f, "Get replicaset '{}'", name),
            Self::ListReplicaSet => f.write_str("List replicasets"),
            Self::CreateDeployment(d) => write!(f, "Create deployment '{}'", d.metadata.name),
            Self::UpdateDeployment(d) => write!(f, "Update deployment '{}'", d.metadata.name),
            Self::DeleteDeployment(name) => write!(f, "Delete deployment '{}'", name),
            Self::GetDeployment(name) => write!(f, "Get deployment '{}'", name),
            Self::ListDeployment => f.write_str("List deployments"),
            Self::RollbackDeployment { name, revision } => {
                if *revision == 0 {
                    write!(f, "Rollback deployment '{}' to previous revision", name)
                } else {
                    write!(f, "Rollback deployment '{}' to revision {}", name, revision)
                }
            }
            Self::GetDeploymentHistory(name) => {
                write!(f, "Get deployment '{}' revision history", name)
            }
            Self::CreateService(svc) => write!(f, "Create service '{}'", svc.metadata.name),
            Self::UpdateService(svc) => write!(f, "Update service '{}'", svc.metadata.name),
            Self::DeleteService(name) => write!(f, "Delete service '{}'", name),
            Self::GetService(name) => write!(f, "Get service '{}'", name),
            Self::ListService => f.write_str("List services"),
            Self::GetNodeCount => f.write_str("Get node count"),
            Self::RegisterNode(node) => write!(f, "Register node '{}'", node.metadata.name),
            Self::UserRequest(payload) => write!(f, "User request: {}", payload),
            Self::SetNftablesRules(rules) => write!(f, "SetNftablesRules (len={})", rules.len()),
            Self::UpdateNftablesRules(rules) => {
                write!(f, "UpdateNftablesRules (len={})", rules.len())
            }
            Self::Heartbeat { node_name, status } => {
                let ready_state = status
                    .conditions
                    .iter()
                    .find(|cond| matches!(cond.condition_type, NodeConditionType::Ready))
                    .map(|cond| match &cond.status {
                        ConditionStatus::True => "ready",
                        ConditionStatus::False => "not ready",
                        ConditionStatus::Unknown => "status unknown",
                    });

                let msg = ready_state
                    .map(|state| format!("Heartbeat from node '{}' ({})", node_name, state))
                    .unwrap_or(format!("Heartbeat from node '{}'", node_name));
                write!(f, "{}", msg)
            }
            Self::SetNetwork(config) => write!(
                f,
                "Apply network settings for node '{}' (subnet: {})",
                config.node_id, config.subnet_env
            ),
            Self::UpdateRoutes(node_name, routes) => write!(
                f,
                "Update {} route(s) on node '{}'",
                routes.len(),
                node_name
            ),
            Self::SetDns(ip, dns_port) => {
                write!(f, "Configure DNS server {}:{}", ip, dns_port)
            }
            Self::CertificateSign { token, .. } => {
                write!(f, "Submit certificate signing request (token: {})", token)
            }
            Self::UpdatePodStatus {
                pod_name,
                pod_namespace,
                status: _,
            } => write!(
                f,
                "Update status for pod '{}' in namespace '{}'",
                pod_name, pod_namespace
            ),

            // response
            Self::Ack => f.write_str("Acknowledge message receipt"),
            Self::Error(err_msg) => write!(f, "Error: {}", err_msg),
            Self::NodeCount(count) => write!(f, "Reported node count: {}", count),
            Self::GetPodByUidRes(pod) => {
                write!(f, "Get pod by UID response: '{}'", pod.metadata.name)
            }
            Self::GetPodRes(pod) => {
                write!(f, "Get pod response: '{}'", pod.metadata.name)
            }
            Self::ListPodRes(pods) => {
                if pods.is_empty() {
                    return f.write_str("List pods response: no pods found");
                }

                let preview = pods
                    .iter()
                    .take(3)
                    .map(|pod| pod.metadata.name.as_str())
                    .collect::<Vec<_>>();

                if pods.len() > preview.len() {
                    return write!(
                        f,
                        "List pods response: {} (+{} more)",
                        preview.join(", "),
                        pods.len() - preview.len()
                    );
                }
                write!(f, "List pods response: {}", preview.join(", "))
            }
            Self::GetReplicaSetRes(rs) => {
                write!(f, "Get replicaset '{}' response", rs.metadata.name)
            }
            Self::ListReplicaSetRes(rss) => {
                if rss.is_empty() {
                    return f.write_str("List replicasets response: no replicasets found");
                }

                let preview = rss
                    .iter()
                    .take(3)
                    .map(|rs| rs.metadata.name.as_str())
                    .collect::<Vec<_>>();

                if rss.len() > preview.len() {
                    return write!(
                        f,
                        "List replicasets response: {} (+{} more)",
                        preview.join(", "),
                        rss.len() - preview.len()
                    );
                }
                write!(f, "List replicasets response: {}", preview.join(", "))
            }
            Self::GetDeploymentRes(d) => {
                write!(f, "Get deployment '{}' response", d.metadata.name)
            }
            Self::ListDeploymentRes(deps) => {
                if deps.is_empty() {
                    return f.write_str("List deployments response: no deployments found");
                }
                let preview = deps
                    .iter()
                    .take(3)
                    .map(|d| d.metadata.name.as_str())
                    .collect::<Vec<_>>();
                if deps.len() > preview.len() {
                    return write!(
                        f,
                        "List deployments response: {} (+{} more)",
                        preview.join(", "),
                        deps.len() - preview.len()
                    );
                }
                write!(f, "List deployments response: {}", preview.join(", "))
            }
            Self::DeploymentHistoryRes(history) => {
                write!(
                    f,
                    "Deployment history response: {} revision(s)",
                    history.len()
                )
            }
            Self::GetServiceRes(svc) => {
                write!(f, "Get service '{}' response", svc.metadata.name)
            }
            Self::ListServiceRes(services) => {
                if services.is_empty() {
                    return f.write_str("List services response: no services found");
                }
                let preview = services
                    .iter()
                    .take(3)
                    .map(|svc| svc.metadata.name.as_str())
                    .collect::<Vec<_>>();
                if services.len() > preview.len() {
                    return write!(
                        f,
                        "List services response: {} (+{} more)",
                        preview.join(", "),
                        services.len() - preview.len()
                    );
                }
                write!(f, "List services response: {}", preview.join(", "))
            }
            Self::SetPodip((pod_name, pod_ip)) => {
                write!(f, "Set pod '{}' IP address to {}", pod_name, pod_ip)
            }
            Self::Certificate(_) => f.write_str("Certificate response received"),
        }
    }
}

/// Deployment revision information for rollback history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRevisionInfo {
    pub revision: i64,
    pub revision_history: Vec<i64>,
    pub replicaset_name: String,
    pub created_at: Option<String>,
    pub replicas: i32,
    pub image: Option<String>,
    pub is_current: bool,
}

/// Node spec
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeSpec {
    #[serde(rename = "podCIDR")]
    pub pod_cidr: String, // Pod network CIDR assigned to this node
    #[serde(default)]
    pub taints: Vec<Taint>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Taint {
    pub key: TaintKey,
    #[serde(default)]
    pub value: String,
    pub effect: TaintEffect,
}
impl Taint {
    pub fn new(key: TaintKey, effect: TaintEffect) -> Self {
        Self {
            key,
            effect,
            value: String::new(),
        }
    }
}

impl TryFrom<&NodeCondition> for Taint {
    type Error = ();

    fn try_from(condition: &NodeCondition) -> Result<Self, Self::Error> {
        match condition.condition_type {
            NodeConditionType::Ready
                if matches!(
                    condition.status,
                    ConditionStatus::False | ConditionStatus::Unknown
                ) =>
            {
                Ok(Taint::new(TaintKey::NodeNotReady, TaintEffect::NoExecute))
            }
            NodeConditionType::MemoryPressure
                if matches!(condition.status, ConditionStatus::True) =>
            {
                Ok(Taint::new(
                    TaintKey::NodeMemoryPressure,
                    TaintEffect::NoSchedule,
                ))
            }
            NodeConditionType::DiskPressure
                if matches!(condition.status, ConditionStatus::True) =>
            {
                Ok(Taint::new(
                    TaintKey::NodeDiskPressure,
                    TaintEffect::NoSchedule,
                ))
            }
            NodeConditionType::PIDPressure if matches!(condition.status, ConditionStatus::True) => {
                Ok(Taint::new(
                    TaintKey::NodeUnschedulable,
                    TaintEffect::NoSchedule,
                ))
            }
            NodeConditionType::NetworkUnavailable
                if matches!(condition.status, ConditionStatus::True) =>
            {
                Ok(Taint::new(
                    TaintKey::NodeNetworkUnavailable,
                    TaintEffect::NoSchedule,
                ))
            }
            _ => Err(()),
        }
    }
}

impl TryFrom<NodeCondition> for Taint {
    type Error = ();

    fn try_from(value: NodeCondition) -> Result<Self, Self::Error> {
        (&value).try_into()
    }
}

/// Node status
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeStatus {
    pub capacity: HashMap<String, String>, // Total resource capacity
    pub allocatable: HashMap<String, String>, // Available for scheduling
    #[serde(default)]
    pub addresses: Vec<NodeAddress>, // Node IPs, hostnames, etc.
    #[serde(default)]
    pub conditions: Vec<NodeCondition>, // Health and status flags
}

/// Node address entry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeAddress {
    #[serde(rename = "type")]
    pub address_type: String, // e.g., "InternalIP", "Hostname"
    pub address: String, // IP or hostname value
}

/// Node condition entry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeCondition {
    #[serde(rename = "type")]
    pub condition_type: NodeConditionType, // e.g., "Ready", "MemoryPressure"
    pub status: ConditionStatus, // "True" | "False" | "Unknown"
    #[serde(rename = "lastHeartbeatTime", default)]
    pub last_heartbeat_time: Option<String>, // Last heartbeat timestamp
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub enum NodeConditionType {
    Ready,
    DiskPressure,
    MemoryPressure,
    PIDPressure,
    NetworkUnavailable,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionStatus {
    True,
    False,
    #[default]
    Unknown,
}

impl NodeCondition {
    pub fn is_ready(&self) -> bool {
        matches!(self.condition_type, NodeConditionType::Ready)
    }

    pub fn heartbeat_age(&self, now: DateTime<Utc>) -> Option<Duration> {
        let timestamp = self.last_heartbeat_time.as_ref()?;
        let heartbeat = chrono::DateTime::parse_from_rfc3339(timestamp)
            .ok()?
            .with_timezone(&Utc);
        now.signed_duration_since(heartbeat).to_std().ok()
    }

    pub fn is_expired_at(&self, grace: Duration, now: DateTime<Utc>) -> bool {
        self.heartbeat_age(now)
            .map(|elapsed| elapsed > grace)
            .unwrap_or(true)
    }

    pub fn is_expired(&self, grace: Duration) -> bool {
        self.is_expired_at(grace, Utc::now())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: NodeSpec,
    pub status: NodeStatus,
}

impl Node {
    pub fn ready_condition(&self) -> Option<&NodeCondition> {
        self.status.conditions.iter().find(|cond| cond.is_ready())
    }

    pub fn ready_condition_mut(&mut self) -> Option<&mut NodeCondition> {
        self.status
            .conditions
            .iter_mut()
            .find(|cond| cond.is_ready())
    }

    pub fn set_last_heartbeat_time(&mut self, now: DateTime<Utc>) {
        if let Some(cond) = self.ready_condition_mut() {
            cond.last_heartbeat_time = Some(now.to_rfc3339());
        }
    }

    pub fn last_heartbeat_time(&self) -> Option<DateTime<Utc>> {
        self.ready_condition().and_then(|cond| {
            cond.last_heartbeat_time
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc))
        })
    }

    pub fn update_ready_status_on_timeout(&mut self, grace: Duration) -> bool {
        if let Some(cond) = self.ready_condition_mut()
            && cond.is_expired(grace)
            && !matches!(cond.status, ConditionStatus::Unknown)
        {
            cond.status = ConditionStatus::Unknown;
            cond.last_heartbeat_time = Some(Utc::now().to_rfc3339());
            return true;
        }
        false
    }

    pub fn derive_taints_from_conditions(conditions: &[NodeCondition]) -> Vec<Taint> {
        conditions
            .iter()
            .filter_map(|condition| condition.try_into().ok())
            .collect()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeNetworkConfig {
    pub node_id: String,
    pub subnet_env: String,
}

#[derive(Debug, Clone)]
pub struct ExternalInterface {
    pub iface: Interface,
    pub iface_addr: Option<Ipv4Addr>,
    pub iface_v6_addr: Option<Ipv6Addr>,
    pub ext_addr: Option<Ipv4Addr>,
    pub ext_v6_addr: Option<Ipv6Addr>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    #[serde(rename = "type", default = "default_service_type")]
    pub service_type: String, // ClusterIP, NodePort, LoadBalancer
    /// **BREAKING CHANGE**: The `selector` field type changed from `HashMap<String, String>` to `Option<LabelSelector>`.
    /// This aligns with Kubernetes API semantics and allows for match expressions.
    /// All code accessing `svc.spec.selector` must be updated to handle `Option<LabelSelector>`.
    /// See release notes for migration details.
    #[serde(default)]
    pub selector: Option<LabelSelector>,
    #[serde(default)]
    pub ports: Vec<ServicePort>,
    #[serde(rename = "clusterIP", default)]
    pub cluster_ip: Option<String>,
}

fn default_service_type() -> String {
    "ClusterIP".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ServicePort {
    #[serde(rename = "port")]
    pub port: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "targetPort", default)]
    pub target_port: Option<i32>,
    #[serde(rename = "protocol", default = "default_protocol")]
    pub protocol: String, // TCP/UDP
    #[serde(rename = "nodePort", default)]
    pub node_port: Option<i32>, // NodePort
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceTask {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: ServiceSpec,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
pub struct LabelSelector {
    #[serde(rename = "matchLabels", default)]
    pub match_labels: HashMap<String, String>,

    #[serde(rename = "matchExpressions", default)]
    pub match_expressions: Vec<LabelSelectorRequirement>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct LabelSelectorRequirement {
    pub key: String,
    pub operator: LabelSelectorOperator,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum LabelSelectorOperator {
    In,
    NotIn,
    Exists,
    DoesNotExist,
}

fn default_replicas() -> i32 {
    1
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct PodTemplateSpec {
    pub metadata: ObjectMeta,
    pub spec: PodSpec,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ReplicaSetSpec {
    #[serde(default = "default_replicas")]
    pub replicas: i32,
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ReplicaSetStatus {
    #[serde(default)]
    pub replicas: i32,
    #[serde(rename = "fullyLabeledReplicas", default)]
    pub fully_labeled_replicas: i32,
    #[serde(rename = "readyReplicas", default)]
    pub ready_replicas: i32,
    #[serde(rename = "availableReplicas", default)]
    pub available_replicas: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ReplicaSet {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: ReplicaSetSpec,
    #[serde(default)]
    pub status: ReplicaSetStatus,
}

/// Endpoint related types (similar to Kubernetes Endpoints)
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EndpointPort {
    pub port: i32,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "appProtocol")]
    pub app_protocol: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EndpointAddress {
    pub ip: String,
    #[serde(rename = "nodeName", default)]
    pub node_name: Option<String>,
    /// Optional reference to the target object (keeps shape simple - use ObjectReference)
    #[serde(rename = "targetRef", default)]
    pub target_ref: Option<ObjectReference>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EndpointSubset {
    #[serde(default)]
    pub addresses: Vec<EndpointAddress>,
    #[serde(rename = "notReadyAddresses", default)]
    pub not_ready_addresses: Vec<EndpointAddress>,
    #[serde(default)]
    pub ports: Vec<EndpointPort>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Endpoint {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    #[serde(default)]
    pub subsets: Vec<EndpointSubset>,
}

/// Support absolute values or percentages
/// rkl needs to check positive value and percentage format
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum IntOrPercentage {
    Int(i32),
    String(String),
}

impl IntOrPercentage {
    /// Resolves the value based on the total. For percentages, returns ceil(total * percent / 100).
    pub fn resolve(&self, total: i32) -> i32 {
        match self {
            IntOrPercentage::Int(n) => *n,
            IntOrPercentage::String(s) => {
                if let Some(percent_str) = s.strip_suffix('%') {
                    let percent: f64 = percent_str.parse().unwrap();
                    ((total as f64 * percent / 100.0).ceil() as i32).max(0)
                } else {
                    s.parse().unwrap()
                }
            }
        }
    }
}

/// According to k8s default value, use 25%
fn default_max_surge() -> IntOrPercentage {
    IntOrPercentage::String("25%".to_string())
}

fn default_max_unavailable() -> IntOrPercentage {
    IntOrPercentage::String("25%".to_string())
}

/// Rolling update strategy parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RollingUpdateStrategy {
    // this parameter means the max number of pods that can be created over the desired number of pods
    #[serde(default = "default_max_surge")]
    pub max_surge: IntOrPercentage,
    // this parameter means the max number of pods that can be unavailable during the update process
    #[serde(default = "default_max_unavailable")]
    pub max_unavailable: IntOrPercentage,
}

impl Default for RollingUpdateStrategy {
    fn default() -> Self {
        Self {
            max_surge: default_max_surge(),
            max_unavailable: default_max_unavailable(),
        }
    }
}

/// Deployment update strategy
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum DeploymentStrategy {
    Recreate,
    RollingUpdate {
        #[serde(rename = "rollingUpdate", default)]
        rolling_update: RollingUpdateStrategy,
    },
}

fn default_deployment_strategy() -> DeploymentStrategy {
    DeploymentStrategy::RollingUpdate {
        rolling_update: RollingUpdateStrategy::default(),
    }
}

fn default_progress_deadline() -> i64 {
    600
}

fn default_revision_history_limit() -> i32 {
    10
}

/// Deployment condition
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentCondition {
    // "Progressing" | "Available" | "ReplicaFailure" ...
    #[serde(rename = "type")]
    pub condition_type: String,
    // "True" | "False" | "Unknown"
    pub status: String,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    pub last_transition_time: String,
    #[serde(default)]
    pub last_update_time: Option<String>,
}

/// The field of deployment is almost same with replicaset
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentSpec {
    #[serde(default = "default_replicas")]
    pub replicas: i32,
    pub selector: LabelSelector,
    pub template: PodTemplateSpec,
    #[serde(default = "default_deployment_strategy")]
    pub strategy: DeploymentStrategy,
    #[serde(default = "default_progress_deadline")]
    pub progress_deadline_seconds: i64,
    #[serde(default = "default_revision_history_limit")]
    pub revision_history_limit: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentStatus {
    #[serde(default)]
    pub replicas: i32,
    // Total number of pods using the desired pod template.
    #[serde(default)]
    pub updated_replicas: i32,
    #[serde(default)]
    pub ready_replicas: i32,
    // Total number of available pods, which is ready for at least minReadySeconds.
    #[serde(default)]
    pub available_replicas: i32,
    #[serde(default)]
    pub unavailable_replicas: i32,
    // Collision count for hash collision resolution
    #[serde(default)]
    pub collision_count: i32,
    #[serde(default)]
    pub observed_generation: Option<i64>,
    #[serde(default)]
    pub conditions: Vec<DeploymentCondition>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Deployment {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
    pub metadata: ObjectMeta,
    pub spec: DeploymentSpec,
    #[serde(default)]
    pub status: DeploymentStatus,
}
