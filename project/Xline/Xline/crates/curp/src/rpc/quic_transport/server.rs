//! QUIC gRPC server implementation
//!
//! This module provides `QuicGrpcServer` which accepts QUIC connections
//! and dispatches RPC calls to the appropriate service methods.

use std::{marker::PhantomData, sync::Arc, task::Poll};

use futures::{Stream, future::BoxFuture};
use gm_quic::prelude::{Connection, QuicListeners};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::task::JoinHandle;
use tracing::{Instrument, debug, error};

use crate::{
    cmd::{Command, CommandExecutor},
    role_change::RoleChange,
    rpc::{
        CurpError, CurpErrorWrapper, ProposeRequest,
        quic_transport::codec::{
            Frame, FrameReader, FrameWriter, MethodId, read_request_header, status_error, status_ok,
        },
    },
    server::Rpc,
};

use super::super::{CurpService, InnerCurpService, Metadata};

/// QUIC gRPC server
///
/// Generic over the same type parameters as `Rpc<C, CE, RC>`.
/// Internally erases to `Arc<dyn CurpService>` and `Arc<dyn InnerCurpService>`.
pub struct QuicGrpcServer<C, CE, RC>
where
    C: Command,
    CE: CommandExecutor<C>,
    RC: RoleChange,
{
    /// Service for external protocol
    service: Arc<dyn CurpService>,
    /// Service for internal protocol
    inner_service: Arc<dyn InnerCurpService>,
    /// Phantom data for type parameters
    _phantom: PhantomData<(C, CE, RC)>,
}

impl<C, CE, RC> QuicGrpcServer<C, CE, RC>
where
    C: Command,
    CE: CommandExecutor<C>,
    RC: RoleChange,
{
    /// Create a new QUIC gRPC server from an `Rpc` instance
    #[inline]
    pub fn new(rpc: Rpc<C, CE, RC>) -> Self {
        Self {
            service: Arc::new(rpc.clone()),
            inner_service: Arc::new(rpc),
            _phantom: PhantomData,
        }
    }

    /// Start serving on the given listeners
    ///
    /// This method runs the accept loop and dispatches incoming streams
    /// to the appropriate RPC handlers.
    pub async fn serve(self, listeners: Arc<QuicListeners>) -> Result<(), CurpError> {
        let service = self.service;
        let inner_service = self.inner_service;

        loop {
            match listeners.accept().await {
                Ok((conn, server_name, pathway, link)) => {
                    debug!(
                        "Accepted QUIC connection from {:?} to server {}",
                        pathway, server_name
                    );
                    let _ = link; // Link not needed for our use case

                    let svc = Arc::clone(&service);
                    let inner_svc = Arc::clone(&inner_service);

                    let _handle = tokio::spawn(
                        async move {
                            Self::handle_connection(conn, svc, inner_svc).await;
                        }
                        .instrument(tracing::debug_span!("quic_conn")),
                    );
                }
                Err(e) => {
                    error!("Listeners shutdown: {e}");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Spawn a handler for a single QUIC connection.
    ///
    /// This is useful for custom accept loops (e.g., multi-service dispatchers
    /// in integration tests) where the caller manages the accept loop and
    /// routes connections to the appropriate server instance.
    pub fn spawn_connection(&self, conn: Connection) -> JoinHandle<()> {
        let svc = Arc::clone(&self.service);
        let inner_svc = Arc::clone(&self.inner_service);
        tokio::spawn(async move {
            Self::handle_connection(conn, svc, inner_svc).await;
        })
    }

    /// Handle a single connection
    async fn handle_connection(
        conn: Connection,
        service: Arc<dyn CurpService>,
        inner_service: Arc<dyn InnerCurpService>,
    ) {
        loop {
            match conn.accept_bi_stream().await {
                Ok((stream_id, (recv, send))) => {
                    let _ = stream_id;
                    let svc = Arc::clone(&service);
                    let inner_svc = Arc::clone(&inner_service);

                    let _handle = tokio::spawn(
                        async move {
                            if let Err(e) = Self::handle_stream(send, recv, svc, inner_svc).await {
                                debug!("stream handler error: {e:?}");
                            }
                        }
                        .instrument(tracing::debug_span!("quic_stream")),
                    );
                }
                Err(e) => {
                    debug!("accept stream error: {e}");
                    break;
                }
            }
        }
    }

    /// Handle a single bidirectional stream
    async fn handle_stream<S, R>(
        send: S,
        mut recv: R,
        service: Arc<dyn CurpService>,
        inner_service: Arc<dyn InnerCurpService>,
    ) -> Result<(), CurpError>
    where
        S: AsyncWrite + Unpin + Send + 'static,
        R: AsyncRead + Unpin + Send + 'static,
    {
        // Read request header (returns raw u16, not validated yet)
        let (method_raw, meta_pairs) = read_request_header(&mut recv).await?;

        // Validate method ID — unknown IDs get a structured STATUS_ERROR response
        let method = match MethodId::from_u16(method_raw) {
            Some(m) => m,
            None => {
                let mut writer = FrameWriter::new(send);
                let err = CurpError::internal(format!("unknown method id: 0x{method_raw:04X}"));
                let wrapper = CurpErrorWrapper { err: Some(err) };
                writer
                    .write_frame(&Frame::Status {
                        code: status_error(),
                        details: wrapper.encode_to_vec(),
                    })
                    .await?;
                writer.flush().await?;
                return Ok(());
            }
        };

        let meta = Metadata::from_pairs(meta_pairs);
        debug!("QUIC RPC: {}", method.name());

        // Exhaustive match — no wildcard. New MethodId variants trigger compile error.
        match method {
            // --- Streaming RPCs (need direct access to send/recv streams) ---
            MethodId::ProposeStream => {
                return Self::handle_propose_stream(recv, send, &meta, &service).await;
            }
            MethodId::LeaseKeepAlive => {
                return Self::handle_lease_keep_alive(recv, send, &service).await;
            }
            MethodId::InstallSnapshot => {
                return Self::handle_install_snapshot(recv, send, &inner_service).await;
            }

            // --- Unary RPCs: explicitly listed ---
            MethodId::FetchCluster
            | MethodId::FetchReadState
            | MethodId::Record
            | MethodId::ReadIndex
            | MethodId::Shutdown
            | MethodId::ProposeConfChange
            | MethodId::Publish
            | MethodId::MoveLeader
            | MethodId::AppendEntries
            | MethodId::Vote
            | MethodId::TriggerShutdown
            | MethodId::TryBecomeLeaderNow => {
                let result = Self::dispatch(method, recv, &meta, &service, &inner_service).await;

                // Write response
                let mut writer = FrameWriter::new(send);

                match result {
                    Ok(response_bytes) => {
                        writer.write_frame(&Frame::Data(response_bytes)).await?;
                        writer
                            .write_frame(&Frame::Status {
                                code: status_ok(),
                                details: vec![],
                            })
                            .await?;
                    }
                    Err(e) => {
                        let wrapper = CurpErrorWrapper { err: Some(e) };
                        let details = wrapper.encode_to_vec();
                        writer
                            .write_frame(&Frame::Status {
                                code: status_error(),
                                details,
                            })
                            .await?;
                    }
                }
                writer.flush().await?;
            }
        }

        Ok(())
    }

    /// Dispatch unary RPC call based on method ID (exhaustive match)
    async fn dispatch<R>(
        method: MethodId,
        recv: R,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
        inner_service: &Arc<dyn InnerCurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        match method {
            MethodId::FetchCluster => Self::handle_fetch_cluster(recv, service).await,
            MethodId::FetchReadState => Self::handle_fetch_read_state(recv, service).await,
            MethodId::Record => Self::handle_record(recv, meta, service).await,
            MethodId::ReadIndex => Self::handle_read_index(recv, meta, service).await,
            MethodId::Shutdown => Self::handle_shutdown(recv, meta, service).await,
            MethodId::ProposeConfChange => {
                Self::handle_propose_conf_change(recv, meta, service).await
            }
            MethodId::Publish => Self::handle_publish(recv, meta, service).await,
            MethodId::MoveLeader => Self::handle_move_leader(recv, service).await,
            MethodId::AppendEntries => Self::handle_append_entries(recv, inner_service).await,
            MethodId::Vote => Self::handle_vote(recv, inner_service).await,
            MethodId::TriggerShutdown => Self::handle_trigger_shutdown(recv, inner_service).await,
            MethodId::TryBecomeLeaderNow => {
                Self::handle_try_become_leader_now(recv, inner_service).await
            }
            // Streaming methods are handled before dispatch — return error, don't panic
            MethodId::ProposeStream | MethodId::LeaseKeepAlive | MethodId::InstallSnapshot => {
                Err(CurpError::internal(format!(
                    "streaming method {} routed to unary dispatch",
                    method.name()
                )))
            }
        }
    }

    /// Read request data from stream
    async fn read_request<R, Req>(recv: R) -> Result<Req, CurpError>
    where
        R: AsyncRead + Unpin,
        Req: Message + Default,
    {
        let mut reader = FrameReader::new_unary_request(recv);
        let frame = reader.read_frame().await?;

        let data = match frame {
            Frame::Data(d) => d,
            _ => return Err(CurpError::internal("expected DATA frame")),
        };

        // Read END frame
        let end_frame = reader.read_frame().await?;
        if !matches!(end_frame, Frame::End) {
            return Err(CurpError::internal("expected END frame"));
        }

        Req::decode(data.as_slice())
            .map_err(|e| CurpError::internal(format!("decode request error: {e}")))
    }

    // Handler implementations

    async fn handle_fetch_cluster<R>(
        recv: R,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::FetchClusterRequest;

        let req: FetchClusterRequest = Self::read_request(recv).await?;
        let resp = service.fetch_cluster(req)?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_fetch_read_state<R>(
        recv: R,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::FetchReadStateRequest;

        let req: FetchReadStateRequest = Self::read_request(recv).await?;
        let resp = service.fetch_read_state(req)?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_record<R>(
        recv: R,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::RecordRequest;

        let req: RecordRequest = Self::read_request(recv).await?;
        let resp = service.record(req, meta.clone())?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_read_index<R>(
        recv: R,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        // Consume the request frames (DATA+END) even though the message is empty.
        // This ensures the QUIC stream is properly closed and avoids connection
        // reset noise from unconsumed frames.
        let _req: crate::rpc::proto::commandpb::ReadIndexRequest = Self::read_request(recv).await?;
        let resp = service.read_index(meta.clone())?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_shutdown<R>(
        recv: R,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::ShutdownRequest;

        let req: ShutdownRequest = Self::read_request(recv).await?;
        let resp = service.shutdown(req, meta.clone()).await?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_propose_conf_change<R>(
        recv: R,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::ProposeConfChangeRequest;

        let req: ProposeConfChangeRequest = Self::read_request(recv).await?;
        let resp = service.propose_conf_change(req, meta.clone()).await?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_publish<R>(
        recv: R,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::PublishRequest;

        let req: PublishRequest = Self::read_request(recv).await?;
        let resp = service.publish(req, meta.clone())?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_move_leader<R>(
        recv: R,
        service: &Arc<dyn CurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::MoveLeaderRequest;

        let req: MoveLeaderRequest = Self::read_request(recv).await?;
        let resp = service.move_leader(req).await?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_append_entries<R>(
        recv: R,
        service: &Arc<dyn InnerCurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::AppendEntriesRequest;

        let req: AppendEntriesRequest = Self::read_request(recv).await?;
        let resp = service.append_entries(req)?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_vote<R>(
        recv: R,
        service: &Arc<dyn InnerCurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin,
    {
        use crate::rpc::VoteRequest;

        let req: VoteRequest = Self::read_request(recv).await?;
        let resp = service.vote(req)?;
        Ok(resp.encode_to_vec())
    }

    async fn handle_trigger_shutdown<R>(
        recv: R,
        service: &Arc<dyn InnerCurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        use crate::rpc::proto::inner_messagepb::TriggerShutdownResponse;

        // Consume request frames to properly close the stream
        let _req: crate::rpc::TriggerShutdownRequest = Self::read_request(recv).await?;
        service.trigger_shutdown()?;
        Ok(TriggerShutdownResponse::default().encode_to_vec())
    }

    async fn handle_try_become_leader_now<R>(
        recv: R,
        service: &Arc<dyn InnerCurpService>,
    ) -> Result<Vec<u8>, CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        use crate::rpc::proto::inner_messagepb::TryBecomeLeaderNowResponse;

        // Consume request frames to properly close the stream
        let _req: crate::rpc::TryBecomeLeaderNowRequest = Self::read_request(recv).await?;
        service.try_become_leader_now().await?;
        Ok(TryBecomeLeaderNowResponse::default().encode_to_vec())
    }

    // ========================================================================
    // Streaming RPC handlers
    // ========================================================================

    /// Handle ProposeStream (server-streaming)
    ///
    /// Protocol: client sends DATA(request) + END, server sends DATA* + STATUS
    async fn handle_propose_stream<R, S>(
        recv: R,
        send: S,
        meta: &Metadata,
        service: &Arc<dyn CurpService>,
    ) -> Result<(), CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        S: AsyncWrite + Unpin + Send + 'static,
    {
        use futures::StreamExt;

        // Read the single request (same as unary)
        let req: ProposeRequest = Self::read_request(recv).await?;

        let mut writer = FrameWriter::new(send);

        // Call service to get the response stream
        match service.propose_stream(req, meta.clone()).await {
            Ok(mut stream) => {
                // Write each response as a DATA frame
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(resp) => {
                            writer
                                .write_frame(&Frame::Data(resp.encode_to_vec()))
                                .await?;
                        }
                        Err(e) => {
                            // Stream produced an error — send error STATUS and stop
                            let wrapper = CurpErrorWrapper { err: Some(e) };
                            writer
                                .write_frame(&Frame::Status {
                                    code: status_error(),
                                    details: wrapper.encode_to_vec(),
                                })
                                .await?;
                            writer.flush().await?;
                            return Ok(());
                        }
                    }
                }
                // Stream completed normally — send OK STATUS
                writer
                    .write_frame(&Frame::Status {
                        code: status_ok(),
                        details: vec![],
                    })
                    .await?;
            }
            Err(e) => {
                let wrapper = CurpErrorWrapper { err: Some(e) };
                writer
                    .write_frame(&Frame::Status {
                        code: status_error(),
                        details: wrapper.encode_to_vec(),
                    })
                    .await?;
            }
        }
        writer.flush().await?;

        Ok(())
    }

    /// Handle LeaseKeepAlive (client-streaming)
    ///
    /// Protocol: client sends DATA* + END, server sends DATA + STATUS
    async fn handle_lease_keep_alive<R, S>(
        recv: R,
        send: S,
        service: &Arc<dyn CurpService>,
    ) -> Result<(), CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        S: AsyncWrite + Unpin + Send + 'static,
    {
        use crate::rpc::LeaseKeepAliveMsg;

        // Build a stream from the client's DATA frames
        let request_stream = Self::client_streaming_to_stream::<R, LeaseKeepAliveMsg>(recv);

        let mut writer = FrameWriter::new(send);

        // Call service
        match service.lease_keep_alive(request_stream).await {
            Ok(resp) => {
                writer
                    .write_frame(&Frame::Data(resp.encode_to_vec()))
                    .await?;
                writer
                    .write_frame(&Frame::Status {
                        code: status_ok(),
                        details: vec![],
                    })
                    .await?;
            }
            Err(e) => {
                let wrapper = CurpErrorWrapper { err: Some(e) };
                writer
                    .write_frame(&Frame::Status {
                        code: status_error(),
                        details: wrapper.encode_to_vec(),
                    })
                    .await?;
            }
        }
        writer.flush().await?;

        Ok(())
    }

    /// Handle InstallSnapshot (client-streaming)
    ///
    /// Protocol: client sends DATA* + END, server sends DATA + STATUS
    async fn handle_install_snapshot<R, S>(
        recv: R,
        send: S,
        inner_service: &Arc<dyn InnerCurpService>,
    ) -> Result<(), CurpError>
    where
        R: AsyncRead + Unpin + Send + 'static,
        S: AsyncWrite + Unpin + Send + 'static,
    {
        use crate::rpc::InstallSnapshotRequest;

        // Build a stream from the client's DATA frames
        let request_stream = Self::client_streaming_to_stream::<R, InstallSnapshotRequest>(recv);

        let mut writer = FrameWriter::new(send);

        // Call service
        match inner_service.install_snapshot(request_stream).await {
            Ok(resp) => {
                writer
                    .write_frame(&Frame::Data(resp.encode_to_vec()))
                    .await?;
                writer
                    .write_frame(&Frame::Status {
                        code: status_ok(),
                        details: vec![],
                    })
                    .await?;
            }
            Err(e) => {
                let wrapper = CurpErrorWrapper { err: Some(e) };
                writer
                    .write_frame(&Frame::Status {
                        code: status_error(),
                        details: wrapper.encode_to_vec(),
                    })
                    .await?;
            }
        }
        writer.flush().await?;

        Ok(())
    }

    /// Convert a client-streaming recv into a `Box<dyn Stream>` of decoded messages
    ///
    /// Reads DATA frames using `FrameReader::new_client_streaming` until END,
    /// decoding each DATA frame as a protobuf message.
    fn client_streaming_to_stream<R, Msg>(
        recv: R,
    ) -> Box<dyn Stream<Item = Result<Msg, CurpError>> + Send + Unpin>
    where
        R: AsyncRead + Unpin + Send + 'static,
        Msg: Message + Default + Send + 'static,
    {
        // Box-erase the reader type so the adapter doesn't need a generic R
        let boxed_recv: Box<dyn AsyncRead + Unpin + Send> = Box::new(recv);
        let reader = FrameReader::new_client_streaming(boxed_recv);
        Box::new(ClientStreamingAdapter::<Msg> {
            state: ClientStreamState::Idle(reader),
            ended: false,
            _phantom: PhantomData,
        })
    }
}

/// Type alias for the box-erased async reader used in client-streaming
type BoxedAsyncRead = Box<dyn AsyncRead + Unpin + Send>;

/// Internal state for `ClientStreamingAdapter`
enum ClientStreamState {
    /// Idle — reader is available for the next read
    Idle(FrameReader<BoxedAsyncRead>),
    /// Reading — a `read_frame` future is in flight
    Reading(BoxFuture<'static, (Result<Frame, CurpError>, FrameReader<BoxedAsyncRead>)>),
    /// Poisoned — state was taken and not restored (should not happen)
    Poisoned,
}

/// Adapter that converts a `FrameReader` (client-streaming mode) into a `Stream`
///
/// Reads DATA frames and decodes them as protobuf messages.
/// Terminates when an END frame is received.
///
/// Uses an enum state machine to persist the in-flight `read_frame()` future
/// across polls, avoiding the "recreated future" bug.
struct ClientStreamingAdapter<Msg> {
    /// State machine: idle (holding reader) or reading (holding future)
    state: ClientStreamState,
    /// Whether the stream has ended
    ended: bool,
    /// Phantom for message type
    _phantom: PhantomData<Msg>,
}

// All fields are `Send` when `Msg: Send`:
// - `FrameReader<BoxedAsyncRead>` is `Send` (BoxedAsyncRead = Box<dyn AsyncRead + Unpin + Send>)
// - `BoxFuture<'static, _>` is `Send` (= Pin<Box<dyn Future + Send>>)
// - `bool` and `PhantomData<Msg>` are `Send`
// So `ClientStreamingAdapter<Msg>` auto-implements `Send`.

impl<Msg> Unpin for ClientStreamingAdapter<Msg> {}

impl<Msg> Stream for ClientStreamingAdapter<Msg>
where
    Msg: Message + Default + Send + 'static,
{
    type Item = Result<Msg, CurpError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.ended {
            return Poll::Ready(None);
        }

        // If idle, start a new read
        if matches!(this.state, ClientStreamState::Idle(_)) {
            let state = std::mem::replace(&mut this.state, ClientStreamState::Poisoned);
            if let ClientStreamState::Idle(mut reader) = state {
                this.state = ClientStreamState::Reading(Box::pin(async move {
                    let result = reader.read_frame().await;
                    (result, reader)
                }));
            }
        }

        // Poll the in-flight future
        if let ClientStreamState::Reading(ref mut fut) = this.state {
            match fut.as_mut().poll(cx) {
                Poll::Ready((result, reader)) => {
                    this.state = ClientStreamState::Idle(reader);
                    match result {
                        Ok(Frame::Data(data)) => {
                            let msg = Msg::decode(data.as_slice())
                                .map_err(|e| CurpError::internal(format!("decode error: {e}")));
                            Poll::Ready(Some(msg))
                        }
                        Ok(Frame::End) => {
                            this.ended = true;
                            Poll::Ready(None)
                        }
                        Ok(Frame::Status { .. }) => {
                            this.ended = true;
                            Poll::Ready(Some(Err(CurpError::internal(
                                "unexpected STATUS in client-streaming request",
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

impl<C, CE, RC> std::fmt::Debug for QuicGrpcServer<C, CE, RC>
where
    C: Command,
    CE: CommandExecutor<C>,
    RC: RoleChange,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuicGrpcServer").finish_non_exhaustive()
    }
}
