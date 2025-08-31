use anyhow::{anyhow, Result};
use bincode;
use clap::{Parser, Subcommand};
use quinn::{ClientConfig, Endpoint};
use rustls::client::{ServerCertVerifier, ServerCertVerified};
use rustls::{Certificate, ServerName};
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::{collections::HashMap, fs, net::SocketAddr, sync::Arc, time::Duration};
use tokio::time;

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
    pub nodename: Option<String>,
    #[serde(default)]
    pub containers: Vec<ContainerSpec>,
    #[serde(default)]
    pub init_containers: Vec<ContainerSpec>,
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
    #[serde(default)]
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
}

/// Node spec
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeSpec {
    #[serde(rename = "podCIDR")]
    pub pod_cidr: String, // Pod network CIDR assigned to this node
}

/// Node status
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeStatus {
    pub capacity: HashMap<String, String>,      // Total resource capacity
    pub allocatable: HashMap<String, String>,   // Available for scheduling
    #[serde(default)]
    pub addresses: Vec<NodeAddress>,            // Node IPs, hostnames, etc.
    #[serde(default)]
    pub conditions: Vec<NodeCondition>,         // Health and status flags
}

/// Node address entry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeAddress {
    #[serde(rename = "type")]
    pub address_type: String, // e.g., "InternalIP", "Hostname"
    pub address: String,      // IP or hostname value
}

/// Node condition entry
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeCondition {
    #[serde(rename = "type")]
    pub condition_type: String, // e.g., "Ready", "MemoryPressure"
    pub status: String,         // "True" | "False" | "Unknown"
    #[serde(rename = "lastHeartbeatTime", default)]
    pub last_heartbeat_time: Option<String>, // Last heartbeat timestamp
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
pub enum RksMessage {
    // request
    CreatePod(Box<PodTask>),
    DeletePod(String),
    GetNodeCount,
    RegisterNode(Box<Node>),  
    UserRequest(String),
    Heartbeat(String),

    // response
    Ack,
    Error(String),
    NodeCount(usize),
}

/// CLI
#[derive(Parser)]
#[command(name = "rkl_cli (short-conn)", about = "in a sequence create/delete")]
struct Cli {
    #[arg(short, long, default_value = "192.168.73.128:50051")]
    server: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// create <pod.yaml>
    Create { file: String },
    /// delete <pod-name>
    Delete { name: String },
}

struct NoVerify;
impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _cert: &Certificate,
        _ints: &[Certificate],
        _sn: &ServerName,
        _scts: &mut dyn Iterator<Item = &[u8]>,
        _ocsp: &[u8],
        _now: std::time::SystemTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let server_addr: SocketAddr = cli.server.parse()?;

    match cli.cmd {
        Cmd::Create { file } => run_once(&server_addr, |conn| create_pod(conn, &file)).await?,
        Cmd::Delete { name } => run_once(&server_addr, |conn| delete_pod(conn, &name)).await?,
    };
    Ok(())
}

async fn run_once<F, Fut>(addr: &SocketAddr, f: F) -> Result<()>
where
    F: FnOnce(quinn::Connection) -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let rustls_cfg = rustls::ClientConfig::builder()
        .with_safe_defaults()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth();
    let client_cfg = ClientConfig::new(Arc::new(rustls_cfg));
    let mut endpoint = Endpoint::client("[::]:0".parse()?)?;
    endpoint.set_default_client_config(client_cfg);

    let conn = endpoint
        .connect(*addr, "localhost")?
        .await
        .map_err(|e| anyhow!("connect failed: {e}"))?;
    println!("[rkl_cli] connected {}", addr);

    send_uni(&conn, &RksMessage::UserRequest("rkl-cli".into())).await?;

    f(conn.clone()).await?;

    conn.close(0u32.into(), b"done");
    Ok(())
}

/// create
async fn create_pod(conn: quinn::Connection, yaml_path: &str) -> Result<()> {
    let content = fs::read_to_string(yaml_path)?;
    let pod: PodTask = serde_yaml::from_str(&content)?;
    println!("[rkl_cli] create pod {}", pod.metadata.name);

    send_uni(&conn, &RksMessage::CreatePod(Box::new(pod))).await?;
    wait_ack(&conn, "create").await
}

/// delete 
async fn delete_pod(conn: quinn::Connection, name: &str) -> Result<()> {
    println!("[rkl_cli] delete pod {}", name);
    send_uni(&conn, &RksMessage::DeletePod(name.to_string())).await?;
    wait_ack(&conn, "delete").await
}

async fn send_uni(conn: &quinn::Connection, msg: &RksMessage) -> Result<()> {
    let mut uni = conn.open_uni().await?;
    uni.write_all(&bincode::serialize(msg)?).await?;
    uni.finish().await?;
    Ok(())
}

/// Wait Ack/Err
async fn wait_ack(conn: &quinn::Connection, op: &str) -> Result<()> {
    match time::timeout(Duration::from_secs(3), conn.accept_uni()).await {
        Ok(Ok(mut recv)) => {
            let mut buf = vec![0u8; 4096];
            if let Ok(Some(n)) = recv.read(&mut buf).await {
                match bincode::deserialize::<RksMessage>(&buf[..n]) {
                    Ok(RksMessage::Ack)        => println!("[rkl_cli] {} acknowledged", op),
                    Ok(RksMessage::Error(e))   => eprintln!("[rkl_cli] {} error: {}", op, e),
                    Ok(other)                  => println!("[rkl_cli] {} unexpected: {:?}", op, other),
                    Err(e)                     => eprintln!("[rkl_cli] resp deserialize failed: {e}"),
                }
            } else {
                eprintln!("[rkl_cli] response closed");
            }
        }
        _ => eprintln!("[rkl_cli] no response for {} (timeout)", op),
    }
    Ok(())
}
