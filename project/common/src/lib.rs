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
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub creation_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    #[serde(with = "chrono::serde::ts_seconds_option")]
    pub deletion_timestamp: Option<DateTime<Utc>>,
    #[serde(default)]
    pub finalizers: Option<Vec<Finalizer>>,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
pub struct ContainerSpec {
    pub name: String,

    pub image: String,

    #[serde(default)]
    pub ports: Vec<Port>,

    #[serde(default)]
    pub args: Vec<String>,

    pub resources: Option<ContainerRes>,
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
    ListPod,

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

    //response
    Ack,
    Error(String),
    NodeCount(usize),
    ListPodRes(Vec<String>),
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
            Self::ListPod => f.write_str("RksMessage::ListPod"),
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

            // response
            Self::Ack => f.write_str("RksMessage::Ack"),
            Self::Error(err_msg) => write!(f, "RksMessage::Error({})", err_msg),
            Self::NodeCount(count) => write!(f, "RksMessage::NodeCount({})", count),
            Self::ListPodRes(pods) => {
                write!(f, "RksMessage::ListPodRes {{ count: {} }}", pods.len())
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
            Self::ListPod => f.write_str("List pods"),
            Self::GetNodeCount => f.write_str("Get node count"),
            Self::RegisterNode(node) => write!(
                f,
                "Register node '{}' (namespace '{}')",
                node.metadata.name, node.metadata.namespace
            ),
            Self::UserRequest(payload) => write!(f, "User request: {}", payload),
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

            // response
            Self::Ack => f.write_str("Acknowledge message receipt"),
            Self::Error(err_msg) => write!(f, "Error: {}", err_msg),
            Self::NodeCount(count) => write!(f, "Reported node count: {}", count),
            Self::ListPodRes(pods) => {
                if pods.is_empty() {
                    return f.write_str("List pods response: no pods found");
                }

                let preview = pods
                    .iter()
                    .take(3)
                    .map(|name| name.as_str())
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
            Self::SetPodip((pod_name, pod_ip)) => {
                write!(f, "Set pod '{}' IP address to {}", pod_name, pod_ip)
            }
            Self::Certificate(_) => f.write_str("Certificate response received"),
        }
    }
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub enum ConditionStatus {
    True,
    False,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServiceSpec {
    #[serde(rename = "type", default = "default_service_type")]
    pub service_type: String, // ClusterIP, NodePort, LoadBalancer
    #[serde(default)]
    pub selector: HashMap<String, String>,
    #[serde(default)]
    pub ports: Vec<ServicePort>,
    #[serde(rename = "clusterIP", default)]
    pub cluster_ip: Option<String>,
}

fn default_service_type() -> String {
    "ClusterIP".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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
