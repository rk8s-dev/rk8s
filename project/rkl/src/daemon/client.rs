use anyhow::Result;
use std::collections::HashMap;
use std::{env, fs, net::SocketAddr, path::Path, sync::Arc, time::Duration};
// use tokio::sync::OnceCell;

use tokio::time;

use crate::commands::pod;
use crate::daemon::status::probe::probe_manager::PROBE_MANAGER;
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

use crate::commands::pod::TLSConnectionArgs;
use crate::quic::client::{Daemon as ClientDaemon, QUICClient};
use sysinfo::{Disks, System};
use tracing::{error, info, warn};

// pub static DAEMON_CLIENT: OnceCell<Arc<QUICClient<ClientDaemon>>> = OnceCell::const_new();

fn get_subnet_file_path() -> String {
    if let Ok(path) = env::var("SUBNET_FILE_PATH") {
        info!("Using custom subnet file path: {path}");
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
        info!("Using CNI standard path: {cni_path}");
        return cni_path.to_string();
    }

    if let Ok(home) = env::var("HOME") {
        let user_dir = format!("{home}/.rkl");
        let user_path = format!("{user_dir}/subnet.env");

        if fs::create_dir_all(&user_dir).is_ok() {
            info!("Using user directory: {user_path}");
            return user_path;
        }
    }

    let default_path = "/tmp/subnet.env";
    info!("Using default temporary path: {default_path}");
    default_path.to_string()
}

/// Run worker loop based on environment variables.
/// This function will keep reconnecting if errors occur.
pub async fn run_forever(tls_cfg: TLSConnectionArgs) -> Result<()> {
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
        if let Err(e) = run_once(
            server_addr,
            node.clone(),
            ext_iface.clone(),
            tls_cfg.clone(),
        )
        .await
        {
            error!("[rkl_worker] error: {e:?}, retrying in 3s");
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
    tls_cfg: TLSConnectionArgs,
) -> Result<()> {
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

    info!("Network receiver created for node: {}", node.metadata.name);

    let client = QUICClient::<ClientDaemon>::connect(server_addr.to_string(), &tls_cfg).await?;
    info!("[worker] connected to RKS at {server_addr}");
    // DAEMON_CLIENT
    //     .set(Arc::new(client.clone()))
    //     .map_err(|_| anyhow::anyhow!("Failed to set DAEMON_CLIENT"))?;

    // register to rks by sending RegisterNode(Box<Node>)
    let register_msg = RksMessage::RegisterNode(Box::new(node.clone()));
    client.send_msg(&register_msg).await?;
    info!("[worker] sent RegisterNode({})", node.metadata.name);

    // heartbeat
    let hb_conn = client.clone();
    let node_name = node.metadata.name.clone();
    let heartbeat_iface = ext_iface.clone();
    let hb_handle = tokio::spawn(async move {
        loop {
            time::sleep(Duration::from_secs(5)).await;
            // Generate fresh status but reuse the same node identity
            let status = match generate_node_status(&heartbeat_iface).await {
                Ok(status) => status,
                Err(e) => {
                    error!("[worker heartbeat] generate_node_status failed: {e}");
                    continue;
                }
            };

            let hb = RksMessage::Heartbeat {
                node_name: node_name.clone(),
                status,
            };

            if let Err(e) = hb_conn.send_msg(&hb).await {
                error!("[worker heartbeat] send failed: {e}");
            } else {
                info!("[worker] heartbeat sent");
            }
        }
    });

    //Main receive loop: handle CreatePod/DeletePod/Network...
    loop {
        match client.accept_uni().await {
            Ok(mut recv) => {
                let mut buf = vec![0u8; 4096];
                match recv.read(&mut buf).await {
                    Ok(Some(n)) => match serde_json::from_slice::<RksMessage>(&buf[..n]) {
                        Ok(RksMessage::Ack) => {
                            info!("[worker] got register Ack");
                        }
                        Ok(RksMessage::Error(e)) => {
                            error!("[worker] register error: {e}");
                        }
                        Ok(RksMessage::SetNetwork(cfg)) => {
                            info!("[worker] received network config: {cfg:?}");

                            if let Err(e) = handle_network_config(&network_receiver, &cfg).await {
                                error!("[worker] failed to apply network config: {e}");
                                let _ = client
                                    .send_msg(&RksMessage::Error(format!(
                                        "network config failed: {e}"
                                    )))
                                    .await;
                            } else {
                                info!("[worker] network config applied successfully");
                                let _ = client.send_msg(&RksMessage::Ack).await;
                            }
                        }
                        Ok(RksMessage::UpdateRoutes(_id, routes)) => {
                            info!("[worker] received routes update: {routes:?}");
                            let route_msg = NetworkConfigMessage::Route { routes };
                            if let Err(e) = network_receiver.handle_network_config(route_msg).await
                            {
                                error!("[worker] failed to apply routes: {e}");
                                let _ = client
                                    .send_msg(&RksMessage::Error(format!(
                                        "routes update failed: {e}"
                                    )))
                                    .await;
                            } else {
                                info!("[worker] routes applied successfully");
                                let _ = client.send_msg(&RksMessage::Ack).await;
                            }
                        }
                        Ok(RksMessage::CreatePod(pod_box)) => {
                            let pod: PodTask = (*pod_box).clone();

                            // validate target node
                            let target_opt = pod.spec.node_name.as_deref();
                            if let Some(target) = target_opt
                                && target != node.metadata.name
                            {
                                warn!(
                                    "[worker] CreatePod skipped: target={} self={}",
                                    target, node.metadata.name
                                );
                                let _ = client
                                    .send_msg(&RksMessage::Error(format!(
                                        "pod {} target node mismatch: target={}, self={}",
                                        pod.metadata.name, target, node.metadata.name
                                    )))
                                    .await;
                                continue;
                            }

                            info!(
                                "[worker] CreatePod name={} assigned_to={}",
                                pod.metadata.name,
                                target_opt.unwrap_or("<unspecified>")
                            );

                            // Create and run task
                            let runner = match TaskRunner::from_task(pod.clone()) {
                                Ok(r) => r,
                                Err(e) => {
                                    error!("[worker] TaskRunner::from_task failed: {e:?}");
                                    let _ = client
                                        .send_msg(&RksMessage::Error(format!(
                                            "create {} failed: {e}",
                                            pod.metadata.name
                                        )))
                                        .await;
                                    continue;
                                }
                            };

                            match pod::run_pod_from_taskrunner(runner).await {
                                Ok(result) => {
                                    let pod_name = result.pod_task.metadata.name.clone();

                                    if let Some(pm) = PROBE_MANAGER.get() {
                                        if let Err(e) =
                                            pm.add_pod(&result.pod_task, &result.pod_ip).await
                                        {
                                            error!(
                                                error = e.to_string(),
                                                "[worker] failed to add probes for pod"
                                            );
                                        } else {
                                            info!("[worker] probes added for pod {}", pod_name);
                                        }
                                    } else {
                                        error!("[worker] PROBE_MANAGER not initialized");
                                    }

                                    let pod_ip = result
                                        .pod_ip
                                        .split('/')
                                        .next()
                                        .unwrap_or(&result.pod_ip)
                                        .to_string();

                                    info!("[worker] SetPodip {} -> {}", pod_name, pod_ip);
                                    if let Err(e) = client
                                        .send_msg(&RksMessage::SetPodip((pod_name, pod_ip)))
                                        .await
                                    {
                                        error!("[worker] SetPodip send failed: {e}");
                                    }
                                }

                                Err(e) => {
                                    error!("[worker] run_pod_from_taskrunner failed: {e:?}");
                                    let _ = client
                                        .send_msg(&RksMessage::Error(format!(
                                            "create {} failed: {e}",
                                            pod.metadata.name
                                        )))
                                        .await;
                                }
                            }
                        }
                        Ok(RksMessage::DeletePod(name)) => {
                            info!("[worker] DeletePod {name}");
                            match pod::standalone::delete_pod(&name) {
                                Ok(_) => {
                                    // Ensure probe deregistration completes before sending the Ack.
                                    // Previously this was spawned as a detached task which could
                                    // panic or fail silently. Awaiting here surfaces errors and
                                    // ensures cleanup has finished when the controller receives
                                    // the acknowledgement.
                                    info!(pod = %name, "removing probes for pod");
                                    if let Some(pm) = PROBE_MANAGER.get() {
                                        pm.remove_pod(&name).await;
                                        info!(pod = %name, "probes removed for pod");
                                        let _ = client.send_msg(&RksMessage::Ack).await;
                                    } else {
                                        error!("[worker] PROBE_MANAGER not initialized");
                                        let _ = client
                                            .send_msg(&RksMessage::Error(format!(
                                                "delete probe for pod {name} failed: PROBE_MANAGER not initialized"
                                            )))
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    error!("[worker] delete_pod failed: {e:?}");
                                    let _ = client
                                        .send_msg(&RksMessage::Error(format!(
                                            "delete {name} failed: {e}"
                                        )))
                                        .await;
                                }
                            }
                        }
                        Ok(RksMessage::SetDns(ip, dns_port)) => {
                            info!("[worker] received dns config: {ip}:{dns_port}");

                            if let Err(e) = handle_dns_config(ip, dns_port).await {
                                error!("[worker] failed to apply dns config: {e}");
                                let _ = client
                                    .send_msg(&RksMessage::Error(format!("dns config failed: {e}")))
                                    .await;
                            } else {
                                info!("[worker] dns config applied successfully");
                                let _ = client.send_msg(&RksMessage::Ack).await;
                            }
                        }
                        Ok(RksMessage::SetNftablesRules(rules)) => {
                            info!(
                                "[worker] received nftables rules (len={}):{}",
                                rules.len(),
                                rules
                            );
                            match network_receiver.apply_nft_rules(rules).await {
                                Ok(()) => {
                                    info!("[worker] nftables rules applied");
                                    let _ = client.send_msg(&RksMessage::Ack).await;
                                }
                                Err(e) => {
                                    error!("[worker] failed to apply nftables rules: {e}");
                                    let _ = client
                                        .send_msg(&RksMessage::Error(format!(
                                            "apply nftables failed: {e}"
                                        )))
                                        .await;
                                }
                            }
                        }
                        Ok(RksMessage::UpdateNftablesRules(rules)) => {
                            info!(
                                "[worker] received nftables rules update (len={}):{}",
                                rules.len(),
                                rules,
                            );
                            match network_receiver.apply_nft_rules(rules).await {
                                Ok(()) => {
                                    info!("[worker] nftables rules updated");
                                    let _ = client.send_msg(&RksMessage::Ack).await;
                                }
                                Err(e) => {
                                    error!("[worker] failed to update nftables rules: {e}");
                                    let _ = client
                                        .send_msg(&RksMessage::Error(format!(
                                            "update nftables failed: {e}"
                                        )))
                                        .await;
                                }
                            }
                        }
                        Ok(other) => {
                            warn!("[worker] unexpected message: {other:?}");
                        }
                        Err(err) => {
                            error!("[worker] deserialize failed: {err}");
                            error!("[worker] raw: {:?}", &buf[..n]);
                        }
                    },
                    Ok(None) => {
                        warn!("[worker] uni stream closed early");
                    }
                    Err(e) => {
                        error!("[worker] read error: {e}");
                    }
                }
            }
            Err(e) => {
                error!("[worker] accept_uni error: {e}, breaking to reconnect");
                hb_handle.abort();
                return Err(anyhow::anyhow!("accept_uni failed: {e}"));
            }
        }
    }
}

async fn handle_network_config(
    network_receiver: &NetworkReceiver,
    node_cfg: &NodeNetworkConfig,
) -> Result<()> {
    info!(
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

    info!("[worker] Network configuration processed successfully");
    Ok(())
}

/// Generate NodeStatus for heartbeat
pub async fn generate_node_status(ext_iface: &ExternalInterface) -> Result<NodeStatus> {
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
        address: hostname,
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

    // conditions - include all condition types
    let conditions = vec![
        ready_condition(),
        memory_condition(0.9),
        disk_condition(1.0),
        pid_condition(0.9),
        network_condition(),
    ];

    Ok(NodeStatus {
        capacity,
        allocatable,
        addresses,
        conditions,
    })
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
            ..Default::default()
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

async fn handle_dns_config(_dns_ip: String, _dns_port: u16) -> Result<()> {
    Ok(())
}
