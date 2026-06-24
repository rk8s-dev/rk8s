//! QUIC channel implementation
//!
//! This module provides `QuicChannel` which manages QUIC connections
//! and provides RPC call methods (unary, server-streaming, client-streaming).

use std::{
    net::IpAddr,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::Poll,
    time::Duration,
};

use dquic::prelude::{Connection, QuicClient, StreamReader, StreamWriter};
use dquic::qresolve::Source;
use futures::{Stream, future::BoxFuture};
use prost::Message;
use tokio::io::AsyncWriteExt;
use tokio::sync::{RwLock, oneshot};
use tokio::task::JoinHandle;

use crate::rpc::CurpError;

use super::codec::{Frame, FrameReader, FrameWriter, MethodId, status_error, status_ok};

/// Grace period for the send task to finish after receiving a cancel signal.
/// If the task doesn't finish within this window, it is forcibly aborted.
const SEND_TASK_GRACE_PERIOD: Duration = Duration::from_millis(100);

/// DNS fallback policy for QUIC connections
///
/// Controls what happens when DNS resolution fails for a hostname.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsFallback {
    /// DNS failure is a hard error (production default)
    Disabled,
    /// Fall back to 127.0.0.1 with the original hostname as SNI.
    /// Only for testing with fake hostnames like "s0.test".
    LocalhostForTest,
}

/// QUIC channel for managing connections and RPC calls
pub struct QuicChannel {
    /// QUIC client for creating connections
    client: Arc<QuicClient>,
    /// Address list for round-robin selection
    addrs: Arc<RwLock<Vec<String>>>,
    /// Round-robin index for load balancing
    index: Arc<AtomicUsize>,
    /// DNS fallback policy
    dns_fallback: DnsFallback,
}

impl std::fmt::Debug for QuicChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuicChannel")
            .field("index", &self.index)
            .finish_non_exhaustive()
    }
}

impl QuicChannel {
    /// Create a new QUIC channel (production: DNS failure = hard error)
    #[inline]
    pub fn new(client: Arc<QuicClient>) -> Self {
        Self {
            client,
            addrs: Arc::new(RwLock::new(Vec::new())),
            index: Arc::new(AtomicUsize::new(0)),
            dns_fallback: DnsFallback::Disabled,
        }
    }

    /// Create a new QUIC channel with initial addresses (no async needed)
    #[inline]
    pub fn with_addrs(
        client: Arc<QuicClient>,
        addrs: Vec<String>,
        dns_fallback: DnsFallback,
    ) -> Self {
        Self {
            client,
            addrs: Arc::new(RwLock::new(addrs)),
            index: Arc::new(AtomicUsize::new(0)),
            dns_fallback,
        }
    }

    /// Create a new QUIC channel with localhost fallback enabled (test only)
    ///
    /// When DNS resolution fails, falls back to 127.0.0.1 with the original
    /// server name as SNI. This is needed for fake hostnames like "s0.test".
    #[inline]
    pub fn new_for_test(client: Arc<QuicClient>) -> Self {
        Self {
            client,
            addrs: Arc::new(RwLock::new(Vec::new())),
            index: Arc::new(AtomicUsize::new(0)),
            dns_fallback: DnsFallback::LocalhostForTest,
        }
    }

    /// Add an address to the connection pool
    ///
    /// The address format should be "host:port" or "quic://host:port"
    pub async fn add_addr(&self, addr: &str) -> Result<(), CurpError> {
        let mut addrs = self.addrs.write().await;
        if !addrs.contains(&addr.to_owned()) {
            addrs.push(addr.to_owned());
        }
        Ok(())
    }

    /// Remove an address from the connection pool
    pub async fn remove_addr(&self, addr: &str) {
        let mut addrs = self.addrs.write().await;
        addrs.retain(|a| a != addr);
    }

    /// Update addresses in the connection pool
    pub async fn update_addrs(&self, new_addrs: Vec<String>) -> Result<(), CurpError> {
        let mut addrs = self.addrs.write().await;
        *addrs = new_addrs;
        Ok(())
    }

    /// Get a connection using round-robin selection
    async fn get_connection(&self) -> Result<Connection, CurpError> {
        let addrs = self.addrs.read().await;
        if addrs.is_empty() {
            return Err(CurpError::RpcTransport(()));
        }

        let len = addrs.len();
        let start = self.index.fetch_add(1, Ordering::Relaxed) % len;
        let snapshot: Vec<String> = addrs.iter().cloned().collect();
        drop(addrs);

        // Try all addresses starting from round-robin index before giving up
        let mut last_err = None;
        for i in 0..len {
            let idx = (start + i) % len;
            let addr = &snapshot[idx];
            let addr_str = addr
                .strip_prefix("quic://")
                .or_else(|| addr.strip_prefix("https://"))
                .or_else(|| addr.strip_prefix("http://"))
                .unwrap_or(addr);

            match self.try_connect(addr_str).await {
                Ok(conn) => return Ok(conn),
                Err(e) => {
                    tracing::debug!("QUIC connect failed for {addr_str}: {e:?}");
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or_else(|| CurpError::RpcTransport(())))
    }

    /// Try connecting to a single address.
    ///
    /// Strips the port from the address before using it as the TLS server_name.
    async fn try_connect(&self, addr_str: &str) -> Result<Connection, CurpError> {
        // Parse endpoint to get hostname (for SNI) and SocketAddr (for connection)
        let (server_name, socket_addr) = self.parse_endpoint(addr_str)?;

        match self
            .client
            .connected_to_with_source(&server_name, [(Source::System, socket_addr.into())])
            .await
        {
            Ok(conn) => Ok(conn),
            Err(e) => match self.dns_fallback {
                DnsFallback::Disabled => Err(CurpError::internal(format!(
                    "QUIC connect error for {addr_str}: {e}"
                ))),
                DnsFallback::LocalhostForTest => {
                    // Test mode: fall back to 127.0.0.1 with the original server_name as SNI
                    let port = socket_addr.port();
                    let fallback_addr =
                        std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), port);
                    tracing::warn!(
                        "connected_to_with_source failed for {server_name}:{port} ({e}), \
                         falling back to {fallback_addr} (test mode)"
                    );
                    self.client
                        .connected_to_with_source(
                            &server_name,
                            [(Source::System, fallback_addr.into())],
                        )
                        .await
                        .map_err(|e2| {
                            CurpError::internal(format!("QUIC connect error: {e2} (original: {e})"))
                        })
                }
            },
        }
    }

    /// Parse an endpoint string (e.g., "server0:46529" or "127.0.1.1:46529")
    /// into a (server_name, SocketAddr) pair.
    ///
    /// The server_name is the hostname WITHOUT the port, which is required
    /// for a valid TLS SNI when passed to `connected_to()`.
    ///
    /// Resolution order:
    /// 1. If hostname is an IP address, use it directly.
    /// 2. Try system DNS resolution (via /etc/hosts or DNS server).
    /// 3. If DNS fails and `DnsFallback::LocalhostForTest`, fall back to 127.0.0.1.
    fn parse_endpoint(&self, addr_str: &str) -> Result<(String, std::net::SocketAddr), CurpError> {
        let (host, port_str) = addr_str
            .rsplit_once(':')
            .ok_or_else(|| CurpError::internal(format!("invalid address format: {addr_str}")))?;
        let port: u16 = port_str
            .parse()
            .map_err(|_| CurpError::internal(format!("invalid port in address: {addr_str}")))?;

        // Try to parse as IP address
        if let Ok(ip) = host.parse::<IpAddr>() {
            let addr = std::net::SocketAddr::new(ip, port);
            return Ok((host.to_string(), addr));
        }

        // DNS hostname: try system DNS resolution (via /etc/hosts or DNS server)
        let dns_result = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current()
                .block_on(async { tokio::net::lookup_host((host, port)).await })
        });

        match dns_result {
            Ok(addrs) => {
                let addr = addrs.into_iter().next().ok_or_else(|| {
                    CurpError::internal(format!(
                        "DNS lookup returned no addresses for '{host}:{port}'"
                    ))
                })?;
                Ok((host.to_string(), addr))
            }
            Err(dns_err) => match self.dns_fallback {
                DnsFallback::Disabled => Err(CurpError::internal(format!(
                    "DNS lookup failed for '{host}:{port}': {dns_err}"
                ))),
                DnsFallback::LocalhostForTest => {
                    // Test mode: fall back to 127.0.0.1 with the original hostname as SNI
                    let fallback_addr =
                        std::net::SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), port);
                    tracing::warn!(
                        "DNS lookup failed for '{host}:{port}' ({dns_err}), \
                         falling back to {fallback_addr} (test mode)"
                    );
                    Ok((host.to_string(), fallback_addr))
                }
            },
        }
    }

    /// Open a bidirectional stream on the connection
    async fn open_bi_stream(conn: &Connection) -> Result<(StreamReader, StreamWriter), CurpError> {
        let result = conn
            .open_bi_stream()
            .await
            .map_err(|e| CurpError::internal(format!("open stream error: {e}")))?;

        match result {
            Some((_stream_id, (reader, writer))) => Ok((reader, writer)),
            None => Err(CurpError::internal("stream concurrency limit reached")),
        }
    }

    /// Perform a unary RPC call
    pub async fn unary_call<Req, Resp>(
        &self,
        method: MethodId,
        req: Req,
        meta: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Resp, CurpError>
    where
        Req: Message,
        Resp: Message + Default,
    {
        let conn = self.get_connection().await?;

        let call = async {
            // Open bidirectional stream
            let (recv_stream, send_stream) = Self::open_bi_stream(&conn).await?;

            let mut writer = FrameWriter::new(send_stream);
            let mut reader = FrameReader::new_unary_response(recv_stream);

            // Write request header
            writer.write_request_header(method, &meta).await?;

            // Write request data
            let req_bytes = req.encode_to_vec();
            writer.write_frame(&Frame::Data(req_bytes)).await?;
            writer.write_frame(&Frame::End).await?;
            writer.flush().await?;

            // Shutdown write side
            let mut send_stream: StreamWriter = writer.into_inner();
            send_stream
                .shutdown()
                .await
                .map_err(|e| CurpError::internal(format!("shutdown stream error: {e}")))?;

            // Read response
            let frame = reader.read_frame().await?;
            let resp_bytes = match frame {
                Frame::Data(data) => data,
                Frame::Status { code, details } if code == status_error() => {
                    return Err(Self::decode_error(&details)?);
                }
                _ => {
                    return Err(CurpError::internal("unexpected frame in unary response"));
                }
            };

            // Read status
            let status_frame = reader.read_frame().await?;
            match status_frame {
                Frame::Status { code, details } => {
                    if code != status_ok() {
                        return Err(Self::decode_error(&details)?);
                    }
                }
                _ => {
                    return Err(CurpError::internal("expected STATUS frame"));
                }
            }

            // Decode response
            Resp::decode(resp_bytes.as_slice())
                .map_err(|e| CurpError::internal(format!("decode response error: {e}")))
        };

        if timeout.is_zero() {
            return call.await;
        }

        tokio::time::timeout(timeout, call)
            .await
            .map_err(|_| CurpError::RpcTransport(()))?
    }

    /// Perform a server-streaming RPC call
    pub async fn server_streaming_call<Req, Resp>(
        &self,
        method: MethodId,
        req: Req,
        meta: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Resp, CurpError>> + Send>>, CurpError>
    where
        Req: Message,
        Resp: Message + Default + Send + Unpin + 'static,
    {
        let conn = self.get_connection().await?;

        let (recv_stream, send_stream): (StreamReader, StreamWriter) =
            tokio::time::timeout(timeout, Self::open_bi_stream(&conn))
                .await
                .map_err(|_| CurpError::RpcTransport(()))??;

        let mut writer = FrameWriter::new(send_stream);

        // Write request header and data
        writer.write_request_header(method, &meta).await?;
        let req_bytes = req.encode_to_vec();
        writer.write_frame(&Frame::Data(req_bytes)).await?;
        writer.write_frame(&Frame::End).await?;
        writer.flush().await?;

        // Shutdown write side
        let mut send_stream: StreamWriter = writer.into_inner();
        send_stream
            .shutdown()
            .await
            .map_err(|e| CurpError::internal(format!("shutdown stream error: {e}")))?;

        // Return stream that reads responses
        let reader = FrameReader::new_server_streaming(recv_stream);
        Ok(Box::pin(ServerStreamingResponse::<Resp>::new(
            reader, conn, None, None,
        )))
    }

    /// Perform a client-streaming RPC call
    ///
    /// The send task is managed with a "single-exit cleanup" pattern:
    /// the task is spawned inside the timeout, but its handle and cancel
    /// signal are captured via `Option` so that cleanup runs unconditionally
    /// after the main logic — regardless of success, `?` early-return, or
    /// timeout cancellation.
    pub async fn client_streaming_call<Req, Resp>(
        &self,
        method: MethodId,
        stream: Pin<Box<dyn Stream<Item = Req> + Send>>,
        meta: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Resp, CurpError>
    where
        Req: Message + 'static,
        Resp: Message + Default,
    {
        use futures::StreamExt;
        use tokio::sync::oneshot;

        let conn = self.get_connection().await?;

        // These are set once the send task is spawned inside the timeout
        // block, and read in the unconditional cleanup that follows.
        let cancel_tx: Arc<std::sync::Mutex<Option<oneshot::Sender<()>>>> =
            Arc::new(std::sync::Mutex::new(None));
        let send_handle: Arc<std::sync::Mutex<Option<JoinHandle<()>>>> =
            Arc::new(std::sync::Mutex::new(None));

        let cancel_tx_inner = Arc::clone(&cancel_tx);
        let send_handle_inner = Arc::clone(&send_handle);

        let result = tokio::time::timeout(timeout, async {
            let (recv_stream, send_stream) = Self::open_bi_stream(&conn).await?;

            let mut writer = FrameWriter::new(send_stream);
            let mut reader = FrameReader::new_unary_response(recv_stream);

            // Write request header
            writer.write_request_header(method, &meta).await?;

            // Spawn a task to write all request messages concurrently.
            // The server may respond before the client finishes sending
            // (e.g., lease_keep_alive returns a client_id after the first message).
            let (tx, mut cancel_rx) = oneshot::channel::<()>();
            let handle = tokio::spawn(async move {
                let mut stream = stream;
                loop {
                    tokio::select! {
                        biased;
                        _ = &mut cancel_rx => break,
                        item = stream.next() => {
                            match item {
                                Some(req) => {
                                    let req_bytes = req.encode_to_vec();
                                    if writer.write_frame(&Frame::Data(req_bytes)).await.is_err() {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
                // Graceful shutdown: write END frame and close the stream
                let _ = writer.write_frame(&Frame::End).await;
                let _ = writer.flush().await;
                let mut send_stream: StreamWriter = writer.into_inner();
                let _ = send_stream.shutdown().await;
            });

            // Store handles so the outer cleanup can always reach them.
            *cancel_tx_inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(tx);
            *send_handle_inner.lock().unwrap_or_else(|e| e.into_inner()) = Some(handle);

            // --- Main read logic (any `?` here is safe: cleanup runs after) ---

            // Read response (may arrive before sending completes)
            let frame = reader.read_frame().await?;
            let resp_bytes = match frame {
                Frame::Data(data) => data,
                Frame::Status { code, details } if code == status_error() => {
                    return Err(Self::decode_error(&details)?);
                }
                _ => {
                    return Err(CurpError::internal(
                        "unexpected frame in client-streaming response",
                    ));
                }
            };

            // Read status
            let status_frame = reader.read_frame().await?;
            match status_frame {
                Frame::Status { code, details } => {
                    if code != status_ok() {
                        return Err(Self::decode_error(&details)?);
                    }
                }
                _ => {
                    return Err(CurpError::internal("expected STATUS frame"));
                }
            }

            // Decode response
            Resp::decode(resp_bytes.as_slice())
                .map_err(|e| CurpError::internal(format!("decode response error: {e}")))
        })
        .await;

        // === Unconditional cleanup: runs on success, error, AND timeout ===
        let taken_cancel = cancel_tx.lock().unwrap_or_else(|e| e.into_inner()).take();
        let taken_handle = send_handle.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(handle) = taken_handle {
            if let Some(tx) = taken_cancel {
                let _ = tx.send(());
            }
            // Pin the handle so we retain ownership across the select.
            tokio::pin!(handle);
            tokio::select! {
                _ = &mut handle => {}
                _ = tokio::time::sleep(SEND_TASK_GRACE_PERIOD) => {
                    handle.abort();
                    let _ = (&mut handle).await;
                }
            }
        }

        result.map_err(|_| CurpError::RpcTransport(()))?
    }

    /// Perform a bidirectional-streaming RPC call.
    pub async fn bidirectional_streaming_call<Req, Resp>(
        &self,
        method: MethodId,
        stream: Pin<Box<dyn Stream<Item = Req> + Send>>,
        meta: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<Resp, CurpError>> + Send>>, CurpError>
    where
        Req: Message + Send + 'static,
        Resp: Message + Default + Send + Unpin + 'static,
    {
        use futures::StreamExt;

        let conn = self.get_connection().await?;

        let (recv_stream, send_stream): (StreamReader, StreamWriter) = if timeout.is_zero() {
            Self::open_bi_stream(&conn).await?
        } else {
            tokio::time::timeout(timeout, Self::open_bi_stream(&conn))
                .await
                .map_err(|_| CurpError::RpcTransport(()))??
        };

        let mut writer = FrameWriter::new(send_stream);
        writer.write_request_header(method, &meta).await?;

        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

        let send_task = tokio::spawn(async move {
            let mut stream = stream;
            loop {
                tokio::select! {
                    _ = &mut cancel_rx => {
                        break;
                    }
                    maybe_req = stream.next() => {
                        let Some(req) = maybe_req else {
                            break;
                        };
                        let req_bytes = req.encode_to_vec();
                        if writer.write_frame(&Frame::Data(req_bytes)).await.is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = writer.write_frame(&Frame::End).await;
            let _ = writer.flush().await;
            let mut send_stream: StreamWriter = writer.into_inner();
            let _ = send_stream.shutdown().await;
        });

        let reader = FrameReader::new_server_streaming(recv_stream);
        Ok(Box::pin(ServerStreamingResponse::<Resp>::new(
            reader,
            conn,
            Some(cancel_tx),
            Some(send_task),
        )))
    }

    /// Connect to a single address (for discovery)
    pub async fn connect_single(addr: &str, client: Arc<QuicClient>) -> Result<Self, CurpError> {
        let channel = Self::new(client);
        channel.add_addr(addr).await?;
        Ok(channel)
    }

    /// Connect to a single address with localhost fallback (test only)
    pub async fn connect_single_for_test(
        addr: &str,
        client: Arc<QuicClient>,
    ) -> Result<Self, CurpError> {
        let channel = Self::new_for_test(client);
        channel.add_addr(addr).await?;
        Ok(channel)
    }

    /// Decode error from STATUS frame details
    fn decode_error(details: &[u8]) -> Result<CurpError, CurpError> {
        use crate::rpc::CurpErrorWrapper;

        if details.is_empty() {
            return Ok(CurpError::internal("unknown error"));
        }

        let wrapper = CurpErrorWrapper::decode(details)
            .map_err(|e| CurpError::internal(format!("decode error details: {e}")))?;

        Ok(wrapper
            .err
            .unwrap_or_else(|| CurpError::internal("missing error in wrapper")))
    }
}

/// Test-only: send a raw method ID to test unknown-method error path.
#[doc(hidden)]
impl QuicChannel {
    /// Send a raw u16 method ID (bypasses `MethodId` type safety).
    /// Response validation is identical to `unary_call`.
    #[doc(hidden)]
    #[allow(unreachable_pub)]
    pub async fn raw_unary_call<Resp>(
        &self,
        raw_method_id: u16,
        req_bytes: Vec<u8>,
        meta: Vec<(String, String)>,
        timeout: Duration,
    ) -> Result<Resp, CurpError>
    where
        Resp: Message + Default,
    {
        let conn = self.get_connection().await?;

        tokio::time::timeout(timeout, async {
            let (recv_stream, send_stream) = Self::open_bi_stream(&conn).await?;
            let mut writer = FrameWriter::new(send_stream);
            let mut reader = FrameReader::new_unary_response(recv_stream);

            writer.write_raw_method_header(raw_method_id, &meta).await?;
            writer.write_frame(&Frame::Data(req_bytes)).await?;
            writer.write_frame(&Frame::End).await?;
            writer.flush().await?;

            let mut send_stream: StreamWriter = writer.into_inner();
            send_stream
                .shutdown()
                .await
                .map_err(|e| CurpError::internal(format!("shutdown error: {e}")))?;

            // Read response — same validation as unary_call
            let frame = reader.read_frame().await?;
            let resp_bytes = match frame {
                Frame::Data(data) => data,
                Frame::Status { code, details } if code == status_error() => {
                    return Err(Self::decode_error(&details)?);
                }
                _ => {
                    return Err(CurpError::internal(
                        "unexpected frame in raw unary response",
                    ));
                }
            };

            let status_frame = reader.read_frame().await?;
            match status_frame {
                Frame::Status { code, details } => {
                    if code != status_ok() {
                        return Err(Self::decode_error(&details)?);
                    }
                }
                _ => {
                    return Err(CurpError::internal("expected STATUS frame"));
                }
            }

            Resp::decode(resp_bytes.as_slice())
                .map_err(|e| CurpError::internal(format!("decode response error: {e}")))
        })
        .await
        .map_err(|_| CurpError::RpcTransport(()))?
    }
}

/// Server-streaming response wrapper
///
/// Stores the in-flight `read_frame()` future across polls so that progress
/// is not lost when `poll_next` returns `Pending`.
///
/// The future takes ownership of the `FrameReader` and returns it alongside
/// the result, so we can store it back for the next read.
///
/// Also holds the QUIC `Connection` to keep it alive for the duration of the
/// stream. gm-quic's `Connection` closes on drop, so we must prevent that.
struct ServerStreamingResponse<Resp> {
    /// State: either we hold the reader (idle) or a future (reading)
    state: StreamResponseState,
    /// Whether stream has ended
    ended: bool,
    /// Keep the QUIC connection alive while the stream is being consumed
    _conn: Connection,
    /// Cancel signal for the client->server send task in bidi mode
    cancel_tx: Option<oneshot::Sender<()>>,
    /// Send task handle for cleanup when stream is dropped early
    send_task: Option<JoinHandle<()>>,
    /// Phantom for response type
    _phantom: std::marker::PhantomData<Resp>,
}

/// Internal state for `ServerStreamingResponse`
enum StreamResponseState {
    /// Idle — reader is available for the next read
    Idle(FrameReader<StreamReader>),
    /// Reading — a `read_frame` future is in flight
    Reading(BoxFuture<'static, (Result<Frame, CurpError>, FrameReader<StreamReader>)>),
    /// Poisoned — state was taken and not restored (should not happen)
    Poisoned,
}

impl<Resp> ServerStreamingResponse<Resp> {
    /// Create a new server-streaming response
    fn new(
        reader: FrameReader<StreamReader>,
        conn: Connection,
        cancel_tx: Option<oneshot::Sender<()>>,
        send_task: Option<JoinHandle<()>>,
    ) -> Self {
        Self {
            state: StreamResponseState::Idle(reader),
            ended: false,
            _conn: conn,
            cancel_tx,
            send_task,
            _phantom: std::marker::PhantomData,
        }
    }
}

impl<Resp> Drop for ServerStreamingResponse<Resp> {
    fn drop(&mut self) {
        cleanup_send_task(self.cancel_tx.take(), self.send_task.take());
    }
}

fn cleanup_send_task(cancel_tx: Option<oneshot::Sender<()>>, send_task: Option<JoinHandle<()>>) {
    if let Some(handle) = send_task {
        if let Some(tx) = cancel_tx {
            let _ = tx.send(());
        }
        if let Ok(rt) = tokio::runtime::Handle::try_current() {
            let _cleanup = rt.spawn(async move {
                let mut handle = handle;
                tokio::select! {
                    _ = &mut handle => {}
                    _ = tokio::time::sleep(SEND_TASK_GRACE_PERIOD) => {
                        handle.abort();
                        let _ = handle.await;
                    }
                }
            });
        } else {
            handle.abort();
        }
    }
}

impl<Resp> Stream for ServerStreamingResponse<Resp>
where
    Resp: Message + Default + Unpin + Send + 'static,
{
    type Item = Result<Resp, CurpError>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.ended {
            return Poll::Ready(None);
        }

        // If idle, start a new read
        if matches!(this.state, StreamResponseState::Idle(_)) {
            let state = std::mem::replace(&mut this.state, StreamResponseState::Poisoned);
            if let StreamResponseState::Idle(mut reader) = state {
                this.state = StreamResponseState::Reading(Box::pin(async move {
                    let result = reader.read_frame().await;
                    (result, reader)
                }));
            }
        }

        // Poll the in-flight future
        if let StreamResponseState::Reading(ref mut fut) = this.state {
            match fut.as_mut().poll(cx) {
                Poll::Ready((result, reader)) => {
                    this.state = StreamResponseState::Idle(reader);
                    match result {
                        Ok(Frame::Data(data)) => {
                            let resp = Resp::decode(data.as_slice())
                                .map_err(|e| CurpError::internal(format!("decode error: {e}")));
                            Poll::Ready(Some(resp))
                        }
                        Ok(Frame::Status { code, details }) => {
                            this.ended = true;
                            if code == status_ok() {
                                Poll::Ready(None)
                            } else {
                                Poll::Ready(Some(Err(
                                    QuicChannel::decode_error(&details).unwrap_or_else(|e| e)
                                )))
                            }
                        }
                        Ok(Frame::End) => {
                            this.ended = true;
                            Poll::Ready(Some(Err(CurpError::internal(
                                "unexpected END in server-streaming",
                            ))))
                        }
                        Err(e) => {
                            this.ended = true;
                            Poll::Ready(Some(Err(e)))
                        }
                    }
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            // Poisoned state
            this.ended = true;
            Poll::Ready(Some(Err(CurpError::internal("stream state poisoned"))))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::future::pending;

    use tokio::sync::oneshot;

    use super::{SEND_TASK_GRACE_PERIOD, cleanup_send_task};

    #[tokio::test]
    async fn cleanup_send_task_cancels_send_task() {
        let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
        let (done_tx, done_rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = &mut cancel_rx => {}
                _ = tokio::time::sleep(std::time::Duration::from_secs(10)) => {}
            }
            let _ = done_tx.send(());
        });

        cleanup_send_task(Some(cancel_tx), Some(handle));

        let done = tokio::time::timeout(std::time::Duration::from_secs(1), done_rx).await;
        assert!(
            done.is_ok(),
            "send task should be cancelled and exit quickly"
        );
    }

    #[tokio::test]
    async fn cleanup_send_task_aborts_stuck_task_after_grace() {
        struct DropNotify(Option<oneshot::Sender<()>>);

        impl Drop for DropNotify {
            fn drop(&mut self) {
                if let Some(tx) = self.0.take() {
                    let _ = tx.send(());
                }
            }
        }

        let (dropped_tx, dropped_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _notify = DropNotify(Some(dropped_tx));
            pending::<()>().await;
        });

        cleanup_send_task(None, Some(handle));

        let wait = tokio::time::timeout(
            SEND_TASK_GRACE_PERIOD + std::time::Duration::from_secs(1),
            dropped_rx,
        )
        .await;
        assert!(
            wait.is_ok(),
            "stuck send task should be aborted after grace period"
        );
    }
}
