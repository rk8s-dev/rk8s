use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use bytes::{Buf, Bytes, BytesMut};
use futures::{Stream, StreamExt};
use gm_quic::prelude::{Connection as GmConnection, QuicClient};
use h3::client::RequestStream;
use h3_shim::BidiStream;
use prost::Message;
use tokio::sync::RwLock;
use xlinerpc::status::Status;

pub(crate) use curp::rpc::MethodId;
pub use xlinerpc::Streaming;

use crate::error::{h3_stream_error_to_status, http_status_to_result};
use crate::h3_pool::H3ConnectionPool;

/// Type alias for h3 request stream with BidiStream
type H3RequestStream = RequestStream<BidiStream<Bytes>, Bytes>;

/// gRPC frame header size: 1 byte compression flag + 4 bytes length
const GRPC_HEADER_SIZE: usize = 5;

/// Calculate total frame size from gRPC header buffer.
/// Returns (length, total bytes) where `length` is the message length and `total` is
/// the entire frame size including the header.
fn grpc_calculate_frame_size(buf: &[u8]) -> (usize, usize) {
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    let total = GRPC_HEADER_SIZE + len;
    (len, total)
}

/// Encode a protobuf message into a gRPC framed bytes buffer.
///
/// gRPC frame format:
/// ```text
/// [0]        = compression flag (0 = uncompressed, 1 = compressed)
/// [1..5]     = message length (big-endian u32)
/// [5..5+len] = message body (protobuf)
/// ```
fn grpc_frame_encode<M: Message>(msg: &M) -> Bytes {
    let body = msg.encode_to_vec();
    let len = body.len() as u32;
    let mut buf = BytesMut::with_capacity(GRPC_HEADER_SIZE + body.len());
    buf.extend_from_slice(&[0u8]); // uncompressed
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&body);
    buf.freeze()
}

/// Decode a gRPC framed buffer into a protobuf message.
///
/// Returns the protobuf message and the number of bytes consumed.
fn grpc_frame_decode<M: Message + Default>(buf: &[u8]) -> Result<(M, usize), Status> {
    if buf.len() < GRPC_HEADER_SIZE {
        return Err(Status::internal("gRPC frame header too short"));
    }
    let compressed = buf[0] != 0;
    if compressed {
        return Err(Status::internal(
            "compressed gRPC frames are not supported (compression flag set)",
        ));
    }
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    let total = GRPC_HEADER_SIZE + len;
    if buf.len() < total {
        return Err(Status::internal("gRPC frame body too short"));
    }
    let body = &buf[GRPC_HEADER_SIZE..total];
    let msg =
        M::decode(body).map_err(|e| Status::internal(format!("protobuf decode error: {e}")))?;
    Ok((msg, total))
}

/// Extract a non-OK gRPC status from headers/trailers.
fn grpc_error_from_headers(headers: &http::HeaderMap) -> Option<Status> {
    let raw = headers.get("grpc-status")?;
    let code = raw
        .to_str()
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(2);
    if code == 0 {
        return None;
    }
    let msg = headers
        .get("grpc-message")
        .and_then(|v| v.to_str().ok())
        .map(decode_grpc_message_header)
        .unwrap_or_else(|| "unknown".to_string());
    Some(Status::new(code.into(), msg))
}

fn decode_grpc_message_header(message: &str) -> String {
    let bytes = message.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h1 = bytes[i + 1] as char;
            let h2 = bytes[i + 2] as char;
            if let (Some(hi), Some(lo)) = (h1.to_digit(16), h2.to_digit(16)) {
                out.push(((hi << 4) as u8) | (lo as u8));
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Clone)]
pub(crate) struct Channel {
    pool: Arc<H3ConnectionPool>,
    addrs: Arc<RwLock<Vec<String>>>,
    index: Arc<AtomicUsize>,
    token: Option<Arc<str>>,
    timeout: Duration,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("index", &self.index)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl Channel {
    #[inline]
    pub(crate) fn new(
        client: Arc<QuicClient>,
        addrs: Vec<String>,
        token: Option<String>,
        timeout: Duration,
    ) -> Self {
        Self {
            pool: Arc::new(H3ConnectionPool::new(client)),
            addrs: Arc::new(RwLock::new(addrs)),
            index: Arc::new(AtomicUsize::new(0)),
            token: token.map(String::into_boxed_str).map(Arc::from),
            timeout,
        }
    }

    #[inline]
    pub(crate) fn with_token(&self, token: Option<String>) -> Self {
        Self {
            pool: Arc::clone(&self.pool),
            addrs: Arc::clone(&self.addrs),
            index: Arc::clone(&self.index),
            token: token.map(String::into_boxed_str).map(Arc::from),
            timeout: self.timeout,
        }
    }

    #[inline]
    fn metadata(&self) -> Vec<(String, String)> {
        self.token
            .as_ref()
            .map(|token| vec![(String::from("authorization"), token.to_string())])
            .unwrap_or_default()
    }

    /// Convert MethodId to hex string for HTTP header
    fn method_id_to_hex(method: MethodId) -> String {
        format!("{:04X}", method.as_u16())
    }

    /// Map MethodId to the corresponding service URL path
    fn method_to_path(method: MethodId) -> &'static str {
        match method {
            // Xline Auth -> /etcdserverpb.Auth
            MethodId::XlineAuthenticate => "/etcdserverpb.Auth/Authenticate",

            // Xline Lease -> /etcdserverpb.Lease
            MethodId::XlineLeaseRevoke => "/etcdserverpb.Lease/LeaseRevoke",
            MethodId::XlineLeaseKeepAlive => "/etcdserverpb.Lease/LeaseKeepAlive",
            MethodId::XlineLeaseTtl => "/etcdserverpb.Lease/LeaseTimeToLive",

            // Xline Watch -> /etcdserverpb.Watch
            MethodId::XlineWatch => "/etcdserverpb.Watch/Watch",

            // Xline Maintenance -> /etcdserverpb.Maintenance
            MethodId::XlineSnapshot => "/etcdserverpb.Maintenance/Snapshot",
            MethodId::XlineAlarm => "/etcdserverpb.Maintenance/Alarm",
            MethodId::XlineMaintStatus => "/etcdserverpb.Maintenance/Status",

            // Xline Cluster -> /etcdserverpb.Cluster
            MethodId::XlineMemberAdd => "/etcdserverpb.Cluster/MemberAdd",
            MethodId::XlineMemberRemove => "/etcdserverpb.Cluster/MemberRemove",
            MethodId::XlineMemberPromote => "/etcdserverpb.Cluster/MemberPromote",
            MethodId::XlineMemberUpdate => "/etcdserverpb.Cluster/MemberUpdate",
            MethodId::XlineMemberList => "/etcdserverpb.Cluster/MemberList",

            // Xline KV -> /etcdserverpb.KV
            MethodId::XlineCompact => "/etcdserverpb.KV/Compact",

            // All curp protocol methods
            MethodId::FetchCluster => "/commandpb.Protocol/FetchCluster",
            MethodId::FetchReadState => "/commandpb.Protocol/FetchReadState",
            MethodId::Record => "/commandpb.Protocol/Record",
            MethodId::ReadIndex => "/commandpb.Protocol/ReadIndex",
            MethodId::Shutdown => "/commandpb.Protocol/Shutdown",
            MethodId::ProposeConfChange => "/commandpb.Protocol/ProposeConfChange",
            MethodId::Publish => "/commandpb.Protocol/Publish",
            MethodId::MoveLeader => "/commandpb.Protocol/MoveLeader",
            MethodId::ProposeStream => "/commandpb.Protocol/ProposeStream",
            MethodId::LeaseKeepAlive => "/commandpb.Protocol/LeaseKeepAlive",

            // Inner protocol methods (currently not used by xline client) - fall back to service path
            _ => "/commandpb.Protocol/Propose",
        }
    }

    async fn get_endpoint(&self) -> Result<String, Status> {
        let addrs = self.addrs.read().await;
        if addrs.is_empty() {
            return Err(Status::unavailable("no available endpoints"));
        }

        let len = addrs.len();
        let idx = self.index.fetch_add(1, Ordering::Relaxed) % len;
        let addr = addrs[idx]
            .strip_prefix("quic://")
            .or_else(|| addrs[idx].strip_prefix("https://"))
            .or_else(|| addrs[idx].strip_prefix("http://"))
            .unwrap_or(&addrs[idx]);
        Ok(addr.to_string())
    }

    /// Build an HTTP request with method id and metadata headers
    fn build_request(
        method: MethodId,
        metadata: &[(String, String)],
        endpoint: &str,
    ) -> Result<http::Request<()>, Status> {
        let path = Self::method_to_path(method);
        let uri = format!("https://{endpoint}{path}");

        let mut builder = http::Request::builder().method(http::Method::POST).uri(uri);

        // Use parsed header values to avoid invalid header errors
        let method_id_header = http::header::HeaderName::from_bytes(b"x-method-id")
            .map_err(|e| Status::internal(format!("invalid x-method-id header: {e}")))?;
        builder = builder.header(method_id_header, Self::method_id_to_hex(method));

        let content_type = http::header::HeaderName::from_bytes(b"content-type")
            .map_err(|e| Status::internal(format!("invalid content-type header: {e}")))?;
        builder = builder.header(content_type, "application/grpc+proto");

        for (key, value) in metadata {
            let header_name = http::header::HeaderName::from_bytes(key.as_bytes())
                .map_err(|e| Status::internal(format!("invalid header name {key}: {e}")))?;
            let header_value = http::header::HeaderValue::from_str(value)
                .map_err(|e| Status::internal(format!("invalid header value for {key}: {e}")))?;
            builder = builder.header(header_name, header_value);
        }

        builder
            .body(())
            .map_err(|e| Status::internal(format!("build request error: {e}")))
    }

    /// Helper: create new connection and send request
    async fn create_connection(&self, endpoint_str: &str) -> Result<GmConnection, Status> {
        let client = self.pool.client();
        let conn = client
            .connect(endpoint_str)
            .await
            .map_err(|e| Status::unavailable(format!("QUIC connect error: {e}")))?;
        Ok(conn)
    }

    pub(crate) async fn unary<Req, Resp>(&self, method: MethodId, req: Req) -> Result<Resp, Status>
    where
        Req: Message,
        Resp: Message + Default,
    {
        let endpoint_str = self.get_endpoint().await?;
        let conn = self.create_connection(&endpoint_str).await?;

        let (mut h3_driver, mut send_req) =
            h3::client::new(h3_shim::QuicConnection::new(Arc::new(conn)))
                .await
                .map_err(|e| Status::unavailable(format!("h3 init error: {e}")))?;

        let request = Self::build_request(method, &self.metadata(), &endpoint_str)?;
        let mut stream = send_req
            .send_request(request)
            .await
            .map_err(h3_stream_error_to_status)?;

        // Send gRPC framed request body
        let grpc_body = grpc_frame_encode(&req);
        stream
            .send_data(grpc_body)
            .await
            .map_err(h3_stream_error_to_status)?;

        // Finish sending
        stream.finish().await.map_err(h3_stream_error_to_status)?;

        // Receive response
        let resp = stream
            .recv_response()
            .await
            .map_err(h3_stream_error_to_status)?;

        // Check HTTP status
        http_status_to_result(resp.status())?;

        // Check gRPC status from response headers (server may return errors here).
        if let Some(err) = grpc_error_from_headers(resp.headers()) {
            return Err(err);
        }

        // Receive gRPC framed response body
        let mut buf = Vec::new();
        loop {
            match stream.recv_data().await {
                Ok(Some(data)) => {
                    tracing::debug!("Received data chunk: {} bytes", data.chunk().len());
                    buf.extend_from_slice(data.chunk());
                }
                Ok(None) => break, // No more data
                Err(e) => {
                    tracing::error!("Error receiving data: {:?}", e);
                    // Try to get trailers for grpc status
                    if let Ok(Some(trailers)) = stream.recv_trailers().await {
                        tracing::debug!("Received trailers: {:?}", trailers);
                        if let Some(err) = grpc_error_from_headers(&trailers) {
                            return Err(err);
                        }
                    }
                    return Err(h3_stream_error_to_status(e));
                }
            }
        }

        // Always check gRPC status from trailers after all data is received
        if let Ok(Some(trailers)) = stream.recv_trailers().await {
            tracing::debug!("Received trailers: {:?}", trailers);
            if let Some(err) = grpc_error_from_headers(&trailers) {
                return Err(err);
            }
        }

        tracing::debug!("Total response buffer size: {} bytes", buf.len());

        // If we got no data at all, check for gRPC status in response trailers
        if buf.is_empty() {
            if let Ok(Some(trailers)) = stream.recv_trailers().await {
                tracing::debug!("Empty body, trailers: {:?}", trailers);
                if let Some(err) = grpc_error_from_headers(&trailers) {
                    return Err(err);
                }
            }
            return Err(Status::internal("empty response body"));
        }

        // Decode gRPC frame -> protobuf
        tracing::debug!(
            "Buffer content (first 20 bytes): {:02x?}",
            &buf[..buf.len().min(20)]
        );
        let (msg, _) = grpc_frame_decode::<Resp>(&buf)?;

        // Spawn a background task to drive the h3 connection state machine
        let _ = tokio::spawn(async move {
            let _ = h3_driver.wait_idle().await;
        });

        Ok(msg)
    }

    pub(crate) async fn server_streaming<Req, Resp>(
        &self,
        method: MethodId,
        req: Req,
    ) -> Result<Streaming<Resp>, Status>
    where
        Req: Message,
        Resp: Message + Default + Send + Unpin + 'static,
    {
        let endpoint_str = self.get_endpoint().await?;
        let conn = self.create_connection(&endpoint_str).await?;

        let (mut h3_driver, mut send_req) =
            h3::client::new(h3_shim::QuicConnection::new(Arc::new(conn)))
                .await
                .map_err(|e| Status::unavailable(format!("h3 init error: {e}")))?;

        let request = Self::build_request(method, &self.metadata(), &endpoint_str)?;
        let mut stream = send_req
            .send_request(request)
            .await
            .map_err(h3_stream_error_to_status)?;

        // Send gRPC framed request body
        let grpc_body = grpc_frame_encode(&req);
        stream
            .send_data(grpc_body)
            .await
            .map_err(h3_stream_error_to_status)?;

        // Finish sending
        stream.finish().await.map_err(h3_stream_error_to_status)?;

        // Receive response headers
        let resp = stream
            .recv_response()
            .await
            .map_err(h3_stream_error_to_status)?;

        // Check HTTP status
        http_status_to_result(resp.status())?;

        // Check gRPC status from response headers (server may return errors here).
        if let Some(err) = grpc_error_from_headers(resp.headers()) {
            return Err(err);
        }

        // Spawn a background task to drive the h3 connection
        let _ = tokio::spawn(async move {
            let _ = h3_driver.wait_idle().await;
        });

        Ok(Streaming::new(Box::pin(GrpcResponseStream {
            stream,
            buf: Vec::new(),
            finished: false,
            _marker: std::marker::PhantomData,
        })))
    }

    pub(crate) async fn client_streaming<Req, Resp, St>(
        &self,
        method: MethodId,
        input: St,
    ) -> Result<Streaming<Resp>, Status>
    where
        Req: Message + Send + 'static,
        Resp: Message + Default + Send + Unpin + 'static,
        St: Stream<Item = Req> + Send + 'static,
    {
        let endpoint_str = self.get_endpoint().await?;
        let conn = self.create_connection(&endpoint_str).await?;

        let (mut h3_driver, mut send_req) =
            h3::client::new(h3_shim::QuicConnection::new(Arc::new(conn)))
                .await
                .map_err(|e| Status::unavailable(format!("h3 init error: {e}")))?;

        let request = Self::build_request(method, &self.metadata(), &endpoint_str)?;
        let mut stream = send_req
            .send_request(request)
            .await
            .map_err(h3_stream_error_to_status)?;

        // Send all messages from the input stream
        futures::pin_mut!(input);
        while let Some(msg) = input.next().await {
            let grpc_body = grpc_frame_encode(&msg);
            stream
                .send_data(grpc_body)
                .await
                .map_err(h3_stream_error_to_status)?;
        }

        // Finish sending
        stream.finish().await.map_err(h3_stream_error_to_status)?;

        // Receive response headers
        let resp = stream
            .recv_response()
            .await
            .map_err(h3_stream_error_to_status)?;

        // Check HTTP status
        http_status_to_result(resp.status())?;

        // Check gRPC status from response headers (server may return errors here).
        if let Some(err) = grpc_error_from_headers(resp.headers()) {
            return Err(err);
        }

        // Spawn a background task to drive the h3 connection
        let _ = tokio::spawn(async move {
            let _ = h3_driver.wait_idle().await;
        });

        Ok(Streaming::new(Box::pin(GrpcResponseStream {
            stream,
            buf: Vec::new(),
            finished: false,
            _marker: std::marker::PhantomData,
        })))
    }
}

/// gRPC framed response stream - accumulates data and decodes gRPC frames
struct GrpcResponseStream<T> {
    stream: H3RequestStream,
    buf: Vec<u8>,
    finished: bool,
    _marker: std::marker::PhantomData<T>,
}

impl<T> Stream for GrpcResponseStream<T>
where
    T: Message + Default + Send + Unpin + 'static,
{
    type Item = Result<T, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.finished {
            return Poll::Ready(None);
        }

        // Try to decode a complete frame from existing buffer
        if this.buf.len() >= GRPC_HEADER_SIZE {
            let (_, total) = grpc_calculate_frame_size(&this.buf);
            if this.buf.len() >= total {
                // We have a complete frame
                let mut msg_buf = Vec::with_capacity(total);
                msg_buf.extend_from_slice(&this.buf[0..total]);
                let _ = this.buf.drain(0..total);

                match grpc_frame_decode::<T>(&msg_buf) {
                    Ok((msg, _)) => return Poll::Ready(Some(Ok(msg))),
                    Err(e) => {
                        this.finished = true;
                        return Poll::Ready(Some(Err(e)));
                    }
                }
            }
        }

        // Poll for more data
        let mut recv_fut = Box::pin(this.stream.recv_data());
        match recv_fut.as_mut().poll(cx) {
            Poll::Ready(Ok(Some(data))) => {
                this.buf.extend_from_slice(data.chunk());

                // Try to decode
                if this.buf.len() >= GRPC_HEADER_SIZE {
                    let (_, total) = grpc_calculate_frame_size(&this.buf);
                    if this.buf.len() >= total {
                        let mut msg_buf = Vec::with_capacity(total);
                        msg_buf.extend_from_slice(&this.buf[0..total]);
                        let _ = this.buf.drain(0..total);

                        match grpc_frame_decode::<T>(&msg_buf) {
                            Ok((msg, _)) => return Poll::Ready(Some(Ok(msg))),
                            Err(e) => {
                                this.finished = true;
                                return Poll::Ready(Some(Err(e)));
                            }
                        }
                    }
                }
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Poll::Ready(Ok(None)) => {
                // Stream ended
                this.finished = true;
                if !this.buf.is_empty() {
                    // Try to decode remaining data
                    if this.buf.len() >= GRPC_HEADER_SIZE {
                        match grpc_frame_decode::<T>(&this.buf) {
                            Ok((msg, _)) => {
                                this.buf.clear();
                                return Poll::Ready(Some(Ok(msg)));
                            }
                            Err(e) => return Poll::Ready(Some(Err(e))),
                        }
                    }
                }
                Poll::Ready(None)
            }
            Poll::Ready(Err(e)) => {
                this.finished = true;
                Poll::Ready(Some(Err(h3_stream_error_to_status(e))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}
