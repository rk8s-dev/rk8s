use crate::api::xlinestore::XlineStore;
use crate::commands::{create, delete};
use crate::protocol::{PodTask, RksMessage};
use anyhow::Result;
use quinn::{Connection, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;

/// launche the RKS server to listen for incoming QUIC connections.
/// spawn a new task for each accepted connection (either worker or user).
pub async fn serve(addr: String, xline_store: Arc<XlineStore>) -> anyhow::Result<()> {
    // Create QUIC endpoint and server certificate
    let endpoint = make_server_endpoint(addr.parse()?).await?;

    // set up a broadcast channel for distributing pod events
    let (tx, _rx) = broadcast::channel::<RksMessage>(100);

    loop {
        let connecting = endpoint.accept().await;
        let tx = tx.clone();
        let xline_store = xline_store.clone();

        match connecting {
            Some(connecting) => {
                match connecting.await {
                    Ok(conn) => {
                        let remote_addr = conn.remote_address().to_string();
                        println!("[server] connection accepted: addr={remote_addr}");

                        // spawn new task to handle this connection
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(conn, xline_store, tx).await {
                                eprintln!("[server] handle_connection error: {e:?}");
                            }
                        });
                    }
                    Err(e) => eprintln!("[server] failed to establish connection: {e:?}"),
                }
            }
            None => break,
        }
    }

    Ok(())
}

/// watches the pod state and pushes create or delete events to the target worker node.
/// The node_id ensures events are filtered per node.
async fn watch_pods(
    xline_store: &Arc<XlineStore>,
    rx: &mut broadcast::Receiver<RksMessage>,
    conn: &Connection,
    node_id: Option<String>,
) -> Result<()> {
    let node_id = match node_id {
        Some(id) => id,
        None => {
            eprintln!("[watch_pods] no node_id provided, skipping message dispatch");
            return Ok(());
        }
    };

    // send all existing pods assigned to this node
    // used for re-connection
    let pods = xline_store.list_pods().await?;
    for pod_name in pods {
        if let Ok(Some(pod_yaml)) = xline_store.get_pod_yaml(&pod_name).await {
            let pod_task: PodTask = serde_yaml::from_str(&pod_yaml)
                .map_err(|e| anyhow::anyhow!("Failed to parse pod_yaml: {}", e))?;
            if pod_task.spec.nodename.as_deref() == Some(&node_id) {
                let msg = RksMessage::CreatePod(Box::new(pod_task.clone()));
                let data = bincode::serialize(&msg)?;
                if let Ok(mut stream) = conn.open_uni().await {
                    stream.write_all(&data).await?;
                    stream.finish()?;
                    println!(
                        "[watch_pods] send CreatePod for pod: {} to node: {}",
                        pod_task.metadata.name, node_id
                    );
                }
            }
        }
    }

    // continue watching for new broadcasted pod messages
    // every worker has a receiver
    // the logic below decides whether rksmessage should be dispatched to worker
    while let Ok(msg) = rx.recv().await {
        match msg {
            RksMessage::CreatePod(pod_task) => {
                create::watch_create(&pod_task, conn, &node_id).await?;
            }
            RksMessage::DeletePod(pod_name) => {
                delete::watch_delete(pod_name, conn, xline_store, &node_id).await?;
            }
            _ => {}
        }
    }

    Ok(())
}

/// handle an individual connection (worker or user) by:
/// reading initial message to identify the client type,
/// spawning `watch_pods` if it's a worker,
/// dispatching further requests based on stream messages.
async fn handle_connection(
    conn: Connection,
    xline_store: Arc<XlineStore>,
    tx: broadcast::Sender<RksMessage>,
) -> Result<()> {
    let mut buf = vec![0u8; 4096];
    let mut is_worker = false;
    let mut node_id = None;

    // initial handshake to classify connection (RegisterNode or UserRequest)
    if let Ok(mut recv) = conn.accept_uni().await {
        match recv.read(&mut buf).await {
            Ok(Some(n)) => {
                println!("[server] received raw data: {:?}", &buf[..n]);
                if let Ok(msg) = bincode::deserialize::<RksMessage>(&buf[..n]) {
                    match msg {
                        RksMessage::RegisterNode(node) => {
                            let id = node.metadata.name.clone();
                            if id.is_empty() {
                                eprintln!("[server] invalid node: metadata.name is empty");
                                return Ok(());
                            }
                            is_worker = true;
                            node_id = Some(id.clone());
                            let ip = conn.remote_address().ip().to_string();
                            let node_yaml = serde_yaml::to_string(&*node)?;
                            xline_store.insert_node_yaml(&id, &node_yaml).await?;
                            println!("[server] registered worker node: {id}, ip: {ip}");

                            let response = RksMessage::Ack;
                            let data = bincode::serialize(&response)?;
                            if let Ok(mut stream) = conn.open_uni().await {
                                stream.write_all(&data).await?;
                                stream.finish()?;
                            }
                        }
                        RksMessage::UserRequest(_) => {
                            is_worker = false;
                            println!("[server] user connection established");
                        }
                        _ => {
                            eprintln!("[server] invalid initial message, closing connection");
                            return Ok(());
                        }
                    }
                } else {
                    eprintln!("[server] deserialize failed: {:?}", &buf[..n]);
                }
            }
            Ok(None) => eprintln!("[server] stream closed"),
            Err(e) => eprintln!("[server] read error: {e:?}"),
        }
    }

    // start watching pods if this is a registered worker node
    if is_worker && node_id.is_some() {
        let xline_store_clone = xline_store.clone();
        let conn_clone = conn.clone();
        let mut rx = tx.subscribe();
        tokio::spawn(async move {
            let _ = watch_pods(&xline_store_clone, &mut rx, &conn_clone, node_id).await;
        });
    }

    // Main loop: accept uni-directional streams for ongoing communication
    loop {
        match conn.accept_uni().await {
            Ok(mut recv_stream) => {
                println!("[server] stream accepted: {}", recv_stream.id());
                let xline_store = xline_store.clone();
                let tx = tx.clone();

                let mut buf = vec![0u8; 4096];
                match recv_stream.read(&mut buf).await {
                    Ok(Some(n)) => {
                        if let Ok(msg) = bincode::deserialize::<RksMessage>(&buf[..n]) {
                            if is_worker {
                                let _ = dispatch_worker(msg.clone(), &conn).await;
                            } else {
                                let _ = dispatch_user(msg.clone(), &xline_store, &conn, &tx).await;
                            }
                        }
                    }
                    Ok(None) => println!("[server] stream closed"),
                    Err(e) => println!("[server] read error: {e}"),
                }
            }
            Err(e) => {
                println!("[server] connection error: {e}");
                break;
            }
        }
    }

    Ok(())
}

/// acknowledges response from worker node
async fn dispatch_worker(msg: RksMessage, conn: &Connection) -> Result<()> {
    match msg {
        RksMessage::Heartbeat(node_id) => {
            println!("[worker dispatch] received heartbeat from node: {node_id}");
            let response = RksMessage::Ack;
            let data = bincode::serialize(&response)?;
            if let Ok(mut stream) = conn.open_uni().await {
                stream.write_all(&data).await?;
                stream.finish()?;
            }
        }
        RksMessage::Error(err_msg) => {
            println!("[worker dispatch] reported error: {err_msg}");
        }
        RksMessage::Ack => {
            println!("[worker dispatch] received Ack");
        }
        _ => {
            println!("[worker dispatch] unknown or unexpected message from worker");
        }
    }
    Ok(())
}

/// handle user-side commands like creating or deleting pods,
/// or querying cluster info like node count.
pub async fn dispatch_user(
    msg: RksMessage,
    xline_store: &Arc<XlineStore>,
    conn: &Connection,
    tx: &broadcast::Sender<RksMessage>,
) -> Result<()> {
    match msg {
        RksMessage::CreatePod(pod_task) => {
            create::user_create(pod_task, xline_store, conn, tx).await?;
        }

        RksMessage::DeletePod(pod_name) => {
            delete::user_delete(pod_name, xline_store, conn, tx).await?;
        }

        RksMessage::GetNodeCount => {
            let count = xline_store.list_nodes().await?.len();
            println!("[user dispatch] node count: {count}");
            let response = RksMessage::NodeCount(count);
            let data = bincode::serialize(&response)?;
            if let Ok(mut stream) = conn.open_uni().await {
                stream.write_all(&data).await?;
                stream.finish()?;
            }
        }

        _ => {
            println!("[user dispatch] unknown message");
        }
    }

    Ok(())
}

/// set up the QUIC server endpoint with TLS certificate.
async fn make_server_endpoint(bind_addr: SocketAddr) -> anyhow::Result<Endpoint> {
    let server_config = configure_server()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    Ok(endpoint)
}

/// generates a self-signed TLS certificate and constructs QUIC server config.
fn configure_server() -> anyhow::Result<ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let cert_der = CertificateDer::from(cert.serialize_der()?);
    let key = PrivatePkcs8KeyDer::from(cert.serialize_private_key_der());
    let certs = vec![cert_der];
    let server_config =
        ServerConfig::with_single_cert(certs, rustls::pki_types::PrivateKeyDer::Pkcs8(key))?;
    Ok(server_config)
}
