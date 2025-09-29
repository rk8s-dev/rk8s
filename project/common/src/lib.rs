use libcni::ip::route::{Interface, Route};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{Ipv4Addr, Ipv6Addr},
};

pub mod lease;
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TypeMeta {
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(rename = "kind")]
    pub kind: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ObjectMeta {
    pub name: String,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub labels: HashMap<String, String>,
    #[serde(default)]
    pub annotations: HashMap<String, String>,
}

fn default_namespace() -> String {
    "default".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerRes {
    pub limits: Option<Resource>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Resource {
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    #[serde(default)]
    pub ports: Vec<Port>,
    #[serde(default)]
    pub args: Vec<String>,
    pub resources: Option<ContainerRes>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PodStatus {
    #[serde(rename = "podIP")]
    pub pod_ip: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
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
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TolerationOperator {
    Exists,
    Equal,
}

impl Default for TolerationOperator {
    fn default() -> Self {
        Self::Equal
    }
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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

    //response
    Ack,
    Error(String),
    NodeCount(usize),
    ListPodRes(Vec<String>),
    // (Podname, Podip)
    SetPodip((String, String)),
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
