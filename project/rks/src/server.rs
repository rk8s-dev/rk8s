use crate::api::xlinestore::XlineStore;
use crate::commands::{create, delete};
use crate::protocol::{PodTask, RksMessage};
use anyhow::Result;
use quinn::{Connection, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::sync::Arc;
use log::{info, warn, error};
use rcgen;
use serde_yaml;
use etcd_client;
use futures_util::StreamExt;

// Main server function
pub async fn serve(addr: String, xline_store: Arc<XlineStore>) -> anyhow::Result<()> {
    info!("Starting server with address: {}", addr);

    // 配置 QUIC 服务器
    let (cert, key) = generate_self_signed_cert()?;
    let server_config = configure_server(cert, key)?;
    
    // 创建端点并绑定地址
    let addr: std::net::SocketAddr = addr.parse()?;
    let endpoint = Endpoint::server(server_config, addr)?;
    info!("QUIC server listening on {}", addr);

    // 接受和处理连接
    while let Some(conn) = endpoint.accept().await {
        let xline_store_clone = xline_store.clone();
        tokio::spawn(async move {
            match conn.await {
                Ok(connection) => {
                    info!("New connection from {}", connection.remote_address());
                    if let Err(e) = handle_connection(connection, xline_store_clone).await {
                        error!("Connection handling error: {}", e);
                    }
                }
                Err(e) => {
                    error!("Connection failed: {}", e);
                }
            }
        });
    }

    Ok(())
}

// Generate self-signed certificate
fn generate_self_signed_cert() -> Result<(CertificateDer<'static>, PrivatePkcs8KeyDer<'static>)> {
    use rcgen::{CertificateParams, SanType};
    use time::{Duration, OffsetDateTime};
    
    // Create certificate parameters
    let mut params = CertificateParams::default();
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(365);
    params.subject_alt_names = vec![SanType::DnsName("localhost".to_string())];
    
    // Generate certificate and key
    let cert = rcgen::Certificate::from_params(params)?;
    
    // Convert to DER format
    let cert_der = CertificateDer::from(cert.serialize_der()?);
    let key_der = PrivatePkcs8KeyDer::from(cert.serialize_private_key_der());
    
    Ok((cert_der, key_der))
}

// Configure QUIC server
fn configure_server(
    cert: CertificateDer<'static>,
    key: PrivatePkcs8KeyDer<'static>,
) -> Result<ServerConfig> {
    use rustls::pki_types::PrivateKeyDer;
    
    let mut server_config = ServerConfig::with_single_cert(vec![cert], PrivateKeyDer::Pkcs8(key))?;
    
    // Configure transport parameters
    let mut transport_config = quinn::TransportConfig::default();
    transport_config.keep_alive_interval(Some(std::time::Duration::from_secs(5)));
    transport_config.max_idle_timeout(Some(std::time::Duration::from_secs(30).try_into()?));
    
    server_config.transport_config(Arc::new(transport_config));
    Ok(server_config)
}

// Handle incoming connection
async fn handle_connection(conn: Connection, xline_store: Arc<XlineStore>) -> Result<()> {
    let mut buf = vec![0u8; 4096];
    let mut is_worker = false;
    let mut node_id = None;

    // Initial handshake to classify connection
    if let Ok(mut recv) = conn.accept_uni().await {
        match recv.read(&mut buf).await {
            Ok(Some(n)) => {
                info!("[server] received raw data: {:?}", &buf[..n]);
                if let Ok(msg) = bincode::deserialize::<RksMessage>(&buf[..n]) {
                    match msg {
                        RksMessage::RegisterNode(node) => {
                            let id = node.metadata.name.clone();
                            if id.is_empty() {
                                error!("[server] invalid node: metadata.name is empty");
                                return Ok(());
                            }
                            is_worker = true;
                            node_id = Some(id.clone());
                            let ip = conn.remote_address().ip().to_string();
                            let node_yaml = serde_yaml::to_string(&*node)?;
                            xline_store.insert_node_yaml(&id, &node_yaml).await?;
                            info!("[server] registered worker node: {id}, ip: {ip}");

                            let response = RksMessage::Ack;
                            let data = bincode::serialize(&response)?;
                            if let Ok(mut stream) = conn.open_uni().await {
                                stream.write_all(&data).await?;
                                stream.finish()?;
                            }
                        }
                        RksMessage::UserRequest(_) => {
                            is_worker = false;
                            let response = RksMessage::Ack;
                            let data = bincode::serialize(&response)?;
                            if let Ok(mut stream) = conn.open_uni().await {
                                stream.write_all(&data).await?;
                                stream.finish()?;
                            }
                        }
                        _ => {
                            warn!("[server] unknown first message, closing");
                            return Ok(());
                        }
                    }
                }
            }
            Ok(None) => {
                warn!("[server] handshake stream closed");
                return Ok(());
            }
            Err(e) => {
                error!("[server] handshake read error: {e}");
                return Ok(());
            }
        }
    }

    // Spawn watcher for workers
    if is_worker && node_id.is_some() {
        let xline_store_clone = xline_store.clone();
        let conn_clone = conn.clone();
        let node_id_clone = node_id.clone();
        tokio::spawn(async move {
            if let Err(e) = watch_pods(&xline_store_clone, &conn_clone, node_id_clone).await {
                error!("Watch pods error: {}", e);
            }
        });
    }

    // Main read loop
    loop {
        match conn.accept_uni().await {
            Ok(mut recv) => {
                let mut buf = vec![0u8; 4096];
                match recv.read(&mut buf).await {
                    Ok(Some(n)) => {
                        if let Ok(msg) = bincode::deserialize::<RksMessage>(&buf[..n]) {
                            if is_worker {
                                if let Err(e) = dispatch_worker(msg.clone(), &conn).await {
                                    error!("Error dispatching worker message: {}", e);
                                }
                            } else {
                                if let Err(e) = dispatch_user(msg.clone(), &xline_store, &conn).await {
                                    error!("Error dispatching user message: {}", e);
                                }
                            }
                        }
                    }
                    Ok(None) => info!("[server] stream closed"),
                    Err(e) => error!("[server] read error: {e}"),
                }
            }
            Err(e) => {
                error!("[server] connection error: {e}");
                break;
            }
        }
    }

    Ok(())
}

// Watch for pod changes and notify workers
async fn watch_pods(
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
    node_id: Option<String>,
) -> Result<()> {
    // Get current pods snapshot and revision
    let (pods, rev) = xline_store.pods_snapshot_with_rev().await?;
    
    // Send current pods to worker node
    for (pod_name, pod_yaml) in pods {
        // Parse from YAML to PodTask
        if let Ok(pod_task) = serde_yaml::from_str::<PodTask>(&pod_yaml) {
            if let Some(ref node_id) = node_id {
                if pod_task.nodename == *node_id {
                    let msg = RksMessage::CreatePod(Box::new(pod_task));
                    let data = bincode::serialize(&msg)?;
                    
                    if let Ok(mut stream) = conn.open_uni().await {
                        stream.write_all(&data).await?;
                        stream.finish()?;
                        info!("[watch_pods] sent existing pod to worker: {}", pod_name);
                    }
                }
            }
        } else {
            error!("Failed to parse pod YAML: {}", pod_yaml);
        }
    }
    
    // Start watching for pod changes
    let (mut watcher, mut stream) = xline_store.watch_pods(rev + 1).await?;
    
    while let Some(resp) = stream.next().await {
        match resp {
            Ok(resp) => {
                for event in resp.events() {
                    match event.event_type() {
                        etcd_client::EventType::Put => {
                            if let Some(kv) = event.kv() {
                                let pod_yaml = String::from_utf8_lossy(kv.value()).to_string();
                                if let Ok(pod_task) = serde_yaml::from_str::<PodTask>(&pod_yaml) {
                                    if let Some(ref node_id) = node_id {
                                        if pod_task.nodename == *node_id {
                                            let msg = RksMessage::CreatePod(Box::new(pod_task));
                                            let data = bincode::serialize(&msg)?;
                                            
                                            if let Ok(mut stream) = conn.open_uni().await {
                                                stream.write_all(&data).await?;
                                                stream.finish()?;
                                                info!("[watch_pods] sent new pod to worker");
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        etcd_client::EventType::Delete => {
                            if let Some(kv) = event.prev_kv() {
                                let pod_name = String::from_utf8_lossy(kv.key())
                                    .replace("/registry/pods/", "");
                                let msg = RksMessage::DeletePod(pod_name.clone());
                                let data = bincode::serialize(&msg)?;
                                
                                if let Ok(mut stream) = conn.open_uni().await {
                                    stream.write_all(&data).await?;
                                    stream.finish()?;
                                    info!("[watch_pods] sent delete pod to worker: {}", pod_name);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Watch error: {}", e);
                break;
            }
        }
    }
    
    watcher.cancel().await?;
    Ok(())
}

// Dispatch worker-originated messages
async fn dispatch_worker(msg: RksMessage, conn: &Connection) -> Result<()> {
    match msg {
        RksMessage::Heartbeat(_node_id) => {
            info!("[worker dispatch] heartbeat received");
            let response = RksMessage::Ack;
            let data = bincode::serialize(&response)?;
            if let Ok(mut stream) = conn.open_uni().await {
                stream.write_all(&data).await?;
                stream.finish()?;
            }
        }
        RksMessage::Error(err_msg) => {
            error!("[worker dispatch] reported error: {err_msg}");
        }
        RksMessage::Ack => {
            info!("[worker dispatch] received Ack");
        }
        _ => {
            warn!("[worker dispatch] unknown or unexpected message from worker");
        }
    }
    Ok(())
}

// Handle user-side commands
pub async fn dispatch_user(
    msg: RksMessage,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
) -> Result<()> {
    match msg {
        RksMessage::CreatePod(pod_task) => {
            create::user_create(pod_task, xline_store, conn).await?;
        }

        RksMessage::DeletePod(pod_name) => {
            delete::user_delete(pod_name, xline_store, conn).await?;
        }

        RksMessage::GetNodeCount => {
            info!("[user dispatch] GetNodeCount received");
        }

        _ => {
            warn!("[user dispatch] unknown message");
        }
    }

    Ok(())
}