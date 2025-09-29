use anyhow::Result;
use bincode;
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{ClientConfig as QuinnClientConfig, Endpoint};
use std::{env, fs, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::time;

use crate::commands::pod;
use crate::network::receiver::{NetworkConfigMessage, NetworkReceiver};
use crate::task::TaskRunner;
use chrono::Utc;
use common::*;
use gethostname::gethostname;
use ipnetwork::Ipv4Network;
use libnetwork::{
    config::{NetworkConfig, validate_network_config},
    ip::{IPStack, PublicIPOpts, lookup_ext_iface},
};
use rustls::DigitallySignedStruct;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig as RustlsClientConfig, RootCertStore, SignatureScheme};
use std::collections::HashMap;

use sysinfo::{Disks, System};

fn get_subnet_file_path() -> String {
    if let Ok(path) = env::var("SUBNET_FILE_PATH") {
        println!("Using custom subnet file path: {path}");
        return path;
    }

    let cni_path = "/etc/cni/net.d/subnet.env";
    if Path::new("/etc/cni/net.d").exists()
        && fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(cni_path)
            .is_ok()
    {
        println!("Using CNI standard path: {cni_path}");
        return cni_path.to_string();
    }

    if let Ok(home) = env::var("HOME") {
        let user_dir = format!("{home}/.rkl");
        let user_path = format!("{user_dir}/subnet.env");

        if fs::create_dir_all(&user_dir).is_ok() {
            println!("Using user directory: {user_path}");
            return user_path;
        }
    }

    let default_path = "/tmp/subnet.env";
    println!("Using default temporary path: {default_path}");
    default_path.to_string()
}

/// Skip certificate verification
#[derive(Debug)]
pub struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

/// Run worker loop based on environment variables.
/// This function will keep reconnecting if errors occur.
pub async fn run_forever() -> Result<()> {
    //We should give the ipaddr of rks here
    let server_addr: String =
        env::var("RKS_ADDRESS").unwrap_or_else(|_| "192.168.73.128:50051".to_string());
    let server_addr: SocketAddr = server_addr.parse()?;

    let ext_iface = lookup_ext_iface(
        None,
        None,
        None,
        IPStack::Ipv4,
        PublicIPOpts {
            public_ip: None,
            public_ipv6: None,
        },
    )
    .await?;

    let node: Node = if let Ok(node_yaml) = env::var("NODE_YAML") {
        load_node_from_yaml(&node_yaml)?
    } else {
        generate_node(&ext_iface).await?
    };
    let ext_iface = Arc::new(ext_iface);
    loop {
        if let Err(e) = run_once(server_addr, node.clone(), ext_iface.clone()).await {
            eprintln!("[rkl_worker] error: {e:?}, retrying in 3s");
            time::sleep(Duration::from_secs(3)).await;
        } else {
            time::sleep(Duration::from_secs(1)).await;
        }
    }
}

fn load_node_from_yaml(yaml_path: &str) -> Result<Node> {
    use std::fs;
    let yaml_content = fs::read_to_string(yaml_path)?;
    let node: Node = serde_yaml::from_str(&yaml_content)?;
    Ok(node)
}

/// Single connection lifecycle:
/// 1. Establish QUIC connection
/// 2. Register node
/// 3. Start heartbeat loop
/// 4. Handle CreatePod/DeletePod messages
/// 5. Handle Network Configuration
pub async fn run_once(
    server_addr: SocketAddr,
    node: Node,
    ext_iface: Arc<ExternalInterface>,
) -> Result<()> {
    // Skip certificate verification
    let mut tls = RustlsClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    tls.dangerous()
        .set_certificate_verifier(Arc::new(SkipServerVerification));

    let quic_crypto = QuicClientConfig::try_from(tls)?;
    let client_cfg: QuinnClientConfig = QuinnClientConfig::new(Arc::new(quic_crypto));
    let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
    endpoint.set_default_client_config(client_cfg);

    let subnet_file_path = get_subnet_file_path();
    let link_index = env::var("LINK_INDEX")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<u32>()
        .unwrap_or(1);
    let backend_type = env::var("BACKEND_TYPE").unwrap_or_else(|_| "hostgw".to_string());

    let network_receiver = NetworkReceiver::new(
        subnet_file_path,
        link_index,
        backend_type,
        node.metadata.name.clone(),
    );

    println!("Network receiver created for node: {}", node.metadata.name);

    // establish connection with retry
    let connection = loop {
        match endpoint.connect(server_addr, "localhost") {
            Ok(connecting) => match connecting.await {
                Ok(conn) => break conn,
                Err(e) => {
                    eprintln!("[worker] connect failed: {e}, retrying 2s");
                    time::sleep(Duration::from_secs(2)).await;
                }
            },
            Err(e) => {
                eprintln!("[worker] endpoint connect error: {e}, retrying 2s");
                time::sleep(Duration::from_secs(2)).await;
            }
        }
    };
    println!("[worker] connected to RKS at {server_addr}");

    // register to rks by sending RegisterNode(Box<Node>)
    let register_msg = RksMessage::RegisterNode(Box::new(node.clone()));
    send_uni(&connection, &register_msg).await?;
    println!("[worker] sent RegisterNode({})", node.metadata.name);

    // heartbeat
    let hb_conn = connection.clone();
    let node_name = node.metadata.name.clone();
    tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(5)).await;
            match generate_node(&ext_iface).await {
                Ok(node) => {
                    let mut status = node.status;
                    status.conditions = vec![
                        ready_condition(),
                        memory_condition(0.9),
                        disk_condition(0.9),
                        pid_condition(0.9),
                        network_condition(),
                    ];

                    let hb = RksMessage::Heartbeat {
                        node_name: node_name.clone(),
                        status,
                    };

                    if let Err(e) = send_uni(&hb_conn, &hb).await {
                        eprintln!("[worker heartbeat] send failed: {e}");
                    } else {
                        println!("[worker] heartbeat sent");
                    }
                }
                Err(e) => {
                    eprintln!("[worker heartbeat] generate_node failed: {e}");
                }
            }
        }
    });

    //Main receive loop: handle CreatePod/DeletePod/Network...
    loop {
        match connection.accept_uni().await {
            Ok(mut recv) => {
                let mut buf = vec![0u8; 4096];
                match recv.read(&mut buf).await {
                    Ok(Some(n)) => match bincode::deserialize::<RksMessage>(&buf[..n]) {
                        Ok(RksMessage::Ack) => {
                            println!("[worker] got register Ack");
                        }
                        Ok(RksMessage::Error(e)) => {
                            eprintln!("[worker] register error: {e}");
                        }
                        Ok(RksMessage::SetNetwork(cfg)) => {
                            println!("[worker] received network config: {cfg:?}");

                            if let Err(e) = handle_network_config(&network_receiver, &cfg).await {
                                eprintln!("[worker] failed to apply network config: {e}");
                                let _ = send_uni(
                                    &connection,
                                    &RksMessage::Error(format!("network config failed: {e}")),
                                )
                                .await;
                            } else {
                                println!("[worker] network config applied successfully");
                                let _ = send_uni(&connection, &RksMessage::Ack).await;
                            }
                        }
                        Ok(RksMessage::UpdateRoutes(_id, routes)) => {
                            println!("[worker] received routes update: {routes:?}");
                            let route_msg = NetworkConfigMessage::Route { routes };
                            if let Err(e) = network_receiver.handle_network_config(route_msg).await
                            {
                                eprintln!("[worker] failed to apply routes: {e}");
                                let _ = send_uni(
                                    &connection,
                                    &RksMessage::Error(format!("routes update failed: {e}")),
                                )
                                .await;
                            } else {
                                println!("[worker] routes applied successfully");
                                let _ = send_uni(&connection, &RksMessage::Ack).await;
                            }
                        }
                        Ok(RksMessage::CreatePod(pod_box)) => {
                            let pod: PodTask = (*pod_box).clone();

                            // validate target node
                            let target_opt = pod.spec.node_name.as_deref();
                            if let Some(target) = target_opt
                                && target != node.metadata.name
                            {
                                eprintln!(
                                    "[worker] CreatePod skipped: target={} self={}",
                                    target, node.metadata.name
                                );
                                let _ = send_uni(
                                    &connection,
                                    &RksMessage::Error(format!(
                                        "pod {} target node mismatch: target={}, self={}",
                                        pod.metadata.name, target, node.metadata.name
                                    )),
                                )
                                .await;
                                continue;
                            }

                            println!(
                                "[worker] CreatePod name={} assigned_to={}",
                                pod.metadata.name,
                                target_opt.unwrap_or("<unspecified>")
                            );

                            // Create and run task
                            let runner = match TaskRunner::from_task(pod.clone()) {
                                Ok(r) => r,
                                Err(e) => {
                                    eprintln!("[worker] TaskRunner::from_task failed: {e:?}");
                                    let _ = send_uni(
                                        &connection,
                                        &RksMessage::Error(format!(
                                            "create {} failed: {e}",
                                            pod.metadata.name
                                        )),
                                    )
                                    .await;
                                    continue;
                                }
                            };

                            match pod::run_pod_from_taskrunner(runner) {
                                Ok(podip) => {
                                    let _ = send_uni(
                                        &connection,
                                        &RksMessage::SetPodip((pod.metadata.name.clone(), podip)),
                                    )
                                    .await;
                                }
                                Err(e) => {
                                    eprintln!("[worker] run_pod_from_taskrunner failed: {e:?}");
                                    let _ = send_uni(
                                        &connection,
                                        &RksMessage::Error(format!(
                                            "create {} failed: {e}",
                                            pod.metadata.name
                                        )),
                                    )
                                    .await;
                                }
                            }
                        }
                        Ok(RksMessage::DeletePod(name)) => {
                            println!("[worker] DeletePod {name}");
                            match pod::standalone::delete_pod(&name) {
                                Ok(_) => {
                                    let _ = send_uni(&connection, &RksMessage::Ack).await;
                                }
                                Err(e) => {
                                    eprintln!("[worker] delete_pod failed: {e:?}");
                                    let _ = send_uni(
                                        &connection,
                                        &RksMessage::Error(format!("delete {name} failed: {e}")),
                                    )
                                    .await;
                                }
                            }
                        }
                        Ok(other) => {
                            println!("[worker] unexpected message: {other:?}");
                        }
                        Err(err) => {
                            eprintln!("[worker] deserialize failed: {err}");
                            eprintln!("[worker] raw: {:?}", &buf[..n]);
                        }
                    },
                    Ok(None) => {
                        eprintln!("[worker] uni stream closed early");
                    }
                    Err(e) => {
                        eprintln!("[worker] read error: {e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("[worker] accept_uni error: {e}, breaking to reconnect");
                break Ok(());
            }
        }
    }
}

async fn handle_network_config(
    network_receiver: &NetworkReceiver,
    node_cfg: &NodeNetworkConfig,
) -> Result<()> {
    println!(
        "[worker] Processing network configuration for node: {}",
        node_cfg.node_id
    );

    let mut network = None;
    let mut subnet = None;
    let mut ip_masq = true;
    let mut mtu = 1500;

    for line in node_cfg.subnet_env.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "RKL_NETWORK" => network = Some(value.parse::<Ipv4Network>()?),
                "RKL_SUBNET" => subnet = Some(value.parse::<Ipv4Network>()?),
                "RKL_MTU" => mtu = value.parse().unwrap_or(1500),
                "RKL_IPMASQ" => ip_masq = value.parse().unwrap_or(true),
                _ => {}
            }
        }
    }

    let mut cfg = NetworkConfig {
        enable_ipv4: true,
        enable_ipv6: false,
        enable_nftables: false,
        network,
        ipv6_network: None,
        subnet_min: None,
        subnet_max: None,
        ipv6_subnet_min: None,
        ipv6_subnet_max: None,
        subnet_len: 24,
        ipv6_subnet_len: 64,
        backend_type: env::var("BACKEND_TYPE").unwrap_or_else(|_| "hostgw".to_string()),
        backend: None,
    };

    validate_network_config(&mut cfg)?;

    let config_msg = NetworkConfigMessage::SubnetConfig {
        network_config: cfg,
        ip_masq,
        ipv4_subnet: subnet,
        ipv6_subnet: None,
        mtu,
    };

    network_receiver.handle_network_config(config_msg).await?;

    println!("[worker] Network configuration processed successfully");
    Ok(())
}

/// Send a message over a unidirectional stream
async fn send_uni(conn: &quinn::Connection, msg: &RksMessage) -> Result<()> {
    let mut uni = conn.open_uni().await?;
    let data = bincode::serialize(msg)?;
    uni.write_all(&data).await?;
    uni.finish()?;
    Ok(())
}

pub fn init_crypto() {
    CryptoProvider::install_default(rustls::crypto::ring::default_provider())
        .expect("failed to install default CryptoProvider");
}

pub async fn generate_node(ext_iface: &ExternalInterface) -> Result<Node> {
    let mut sys = System::new_all();
    sys.refresh_all();

    // hostname
    let hostname = gethostname().to_string_lossy().into_owned();

    // IP addr
    let mut addresses = vec![];

    if let Some(ip) = ext_iface.iface_addr {
        addresses.push(NodeAddress {
            address_type: "InternalIP".to_string(),
            address: ip.to_string(),
        });
    }
    addresses.push(NodeAddress {
        address_type: "Hostname".to_string(),
        address: hostname.clone(),
    });

    // CPU / memory
    let total_cpu = sys.cpus().len().to_string();
    let total_mem = format!("{}Mi", sys.total_memory() / 1024);

    let mut capacity = HashMap::new();
    capacity.insert("cpu".to_string(), total_cpu.clone());
    capacity.insert("memory".to_string(), total_mem.clone());
    capacity.insert("pods".to_string(), "110".to_string());

    let mut allocatable = capacity.clone();
    allocatable.insert("cpu".to_string(), (sys.cpus().len() - 1).to_string());

    // conditions
    let now = Utc::now().to_rfc3339();
    let conditions = vec![NodeCondition {
        condition_type: NodeConditionType::Ready,
        status: ConditionStatus::True,
        last_heartbeat_time: Some(now.clone()),
    }];

    Ok(Node {
        api_version: "v1".to_string(),
        kind: "Node".to_string(),
        metadata: ObjectMeta {
            name: hostname,
            namespace: "default".to_string(),
            labels: HashMap::new(),
            annotations: HashMap::new(),
        },
        spec: NodeSpec {
            pod_cidr: "0".to_string(),
            taints: vec![],
        },
        status: NodeStatus {
            capacity,
            allocatable,
            addresses,
            conditions,
        },
    })
}

fn ready_condition() -> NodeCondition {
    NodeCondition {
        condition_type: NodeConditionType::Ready,
        status: ConditionStatus::True,
        last_heartbeat_time: Some(Utc::now().to_rfc3339()),
    }
}
fn memory_condition(threshold: f64) -> NodeCondition {
    let mut sys = System::new_all();
    sys.refresh_memory();

    let total_mem = sys.total_memory();
    let avail_mem = sys.available_memory();
    let used_ratio = if total_mem > 0 {
        (total_mem - avail_mem) as f64 / total_mem as f64
    } else {
        0.0
    };

    let status = if used_ratio > threshold {
        ConditionStatus::True
    } else {
        ConditionStatus::False
    };

    NodeCondition {
        condition_type: NodeConditionType::MemoryPressure,
        status,
        last_heartbeat_time: Some(Utc::now().to_rfc3339()),
    }
}

fn disk_condition(threshold: f64) -> NodeCondition {
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut pressure = false;
    let disks = Disks::new_with_refreshed_list();
    for disk in disks.list() {
        let total = disk.total_space();
        let avail = disk.available_space();

        if total > 0 {
            let used_ratio = 1.0 - (avail as f64 / total as f64);
            if used_ratio > threshold {
                pressure = true;
                break;
            }
        }
    }

    let status = if pressure {
        ConditionStatus::True
    } else {
        ConditionStatus::False
    };

    NodeCondition {
        condition_type: NodeConditionType::DiskPressure,
        status,
        last_heartbeat_time: Some(Utc::now().to_rfc3339()),
    }
}
fn pid_condition(threshold: f64) -> NodeCondition {
    let mut sys = System::new_all();
    sys.refresh_all();

    let process_count = sys.processes().len();
    let pid_max = get_pid_max().unwrap_or(32768);

    let status = if (process_count as f64) / (pid_max as f64) > threshold {
        ConditionStatus::True
    } else {
        ConditionStatus::False
    };

    NodeCondition {
        condition_type: NodeConditionType::PIDPressure,
        status,
        last_heartbeat_time: Some(Utc::now().to_rfc3339()),
    }
}

fn get_pid_max() -> Option<usize> {
    fs::read_to_string("/proc/sys/kernel/pid_max")
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
}

pub fn network_condition() -> NodeCondition {
    let status = ConditionStatus::False;
    NodeCondition {
        condition_type: NodeConditionType::NetworkUnavailable,
        status,
        last_heartbeat_time: Some(Utc::now().to_rfc3339()),
    }
}
