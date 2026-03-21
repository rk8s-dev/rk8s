use std::{fmt::Debug, future::Future, sync::Arc};

use curp::rpc::{
    CurpError, CurpErrorWrapper, Frame, FrameReader, FrameWriter, Metadata, MethodId,
    QuicServiceExt, status_error, status_ok,
};
use futures::{Stream, StreamExt, future::BoxFuture};
use prost::Message;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Response, Status};

use super::{
    auth_server::AuthServer, cluster_server::ClusterServer, kv_server::KvServer,
    lease_server::LeaseServer, maintenance::MaintenanceServer, watch_server::WatchServer,
};
use crate::rpc::{
    AlarmRequest, AuthenticateRequest, CompactionRequest, LeaseKeepAliveRequest,
    LeaseRevokeRequest, LeaseTimeToLiveRequest, MemberAddRequest, MemberListRequest,
    MemberPromoteRequest, MemberRemoveRequest, MemberUpdateRequest, SnapshotRequest, WatchRequest,
};

#[derive(Clone)]
pub(crate) struct XlineQuicService {
    auth: AuthServer,
    cluster: ClusterServer,
    kv: KvServer,
    lease: Arc<LeaseServer>,
    maintenance: MaintenanceServer,
    watch: WatchServer,
}

#[allow(unused)]
impl XlineQuicService {
    pub(crate) fn new(
        auth: AuthServer,
        cluster: ClusterServer,
        kv: KvServer,
        lease: Arc<LeaseServer>,
        maintenance: MaintenanceServer,
        watch: WatchServer,
    ) -> Self {
        Self {
            auth,
            cluster,
            kv,
            lease,
            maintenance,
            watch,
        }
    }

    fn tonic_request<T>(message: T, meta: &Metadata) -> Result<tonic::Request<T>, CurpError> {
        let mut request = tonic::Request::new(message);
        for (key, value) in meta.iter() {
            let key = key
                .parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>()
                .map_err(|e| {
                    CurpError::from(Status::internal(format!(
                        "invalid metadata key '{key}': {e}"
                    )))
                })?;
            let parsed = value.parse().map_err(|e| {
                CurpError::from(Status::internal(format!(
                    "invalid metadata value for key '{key}': {e}"
                )))
            })?;
            let _prev = request.metadata_mut().insert(key, parsed);
        }
        Ok(request)
    }

    fn tonic_status_to_curp(status: Status) -> CurpError {
        let status: xlinerpc::status::Status = status.into();
        CurpError::from(status)
    }

    async fn write_error<W>(writer: &mut FrameWriter<W>, err: Status) -> Result<(), CurpError>
    where
        W: AsyncWrite + Unpin,
    {
        let wrapper = CurpErrorWrapper {
            err: Some(Self::tonic_status_to_curp(err)),
        };
        writer
            .write_frame(&Frame::Status {
                code: status_error(),
                details: wrapper.encode_to_vec(),
            })
            .await
    }

    async fn read_unary_request<R, Req>(recv: R) -> Result<Req, CurpError>
    where
        R: AsyncRead + Unpin,
        Req: Message + Default,
    {
        let mut reader = FrameReader::new_unary_request(recv);
        let payload = match reader.read_frame().await? {
            Frame::Data(data) => data,
            Frame::End => {
                return Err(CurpError::from(Status::internal(
                    "unexpected END in unary request",
                )));
            }
            Frame::Status { .. } => {
                return Err(CurpError::from(Status::internal(
                    "unexpected STATUS in unary request",
                )));
            }
        };

        match reader.read_frame().await? {
            Frame::End => {}
            Frame::Data(_) | Frame::Status { .. } => {
                return Err(CurpError::from(Status::internal(
                    "expected END after unary request payload",
                )));
            }
        }

        Req::decode(payload.as_slice())
            .map_err(|e| CurpError::from(Status::internal(format!("decode request error: {e}"))))
    }

    async fn handle_unary<S, R, Req, Resp, F, Fut>(
        send: S,
        recv: R,
        meta: Metadata,
        f: F,
    ) -> Result<(), CurpError>
    where
        S: AsyncWrite + Unpin,
        R: AsyncRead + Unpin,
        Req: Message + Default,
        Resp: Message,
        F: FnOnce(tonic::Request<Req>) -> Fut,
        Fut: Future<Output = Result<Response<Resp>, Status>>,
    {
        let request = Self::read_unary_request::<R, Req>(recv).await?;
        let mut writer = FrameWriter::new(send);

        match f(Self::tonic_request(request, &meta)?).await {
            Ok(response) => {
                writer
                    .write_frame(&Frame::Data(response.into_inner().encode_to_vec()))
                    .await?;
                writer
                    .write_frame(&Frame::Status {
                        code: status_ok(),
                        details: vec![],
                    })
                    .await?;
            }
            Err(err) => Self::write_error(&mut writer, err).await?,
        }

        writer.flush().await
    }

    async fn handle_server_streaming<S, R, Req, Resp, St, F, Fut>(
        send: S,
        recv: R,
        meta: Metadata,
        f: F,
    ) -> Result<(), CurpError>
    where
        S: AsyncWrite + Unpin,
        R: AsyncRead + Unpin,
        Req: Message + Default,
        Resp: Message + Debug,
        St: Stream<Item = Result<Resp, Status>> + Send + Unpin + 'static,
        F: FnOnce(tonic::Request<Req>) -> Fut,
        Fut: Future<Output = Result<Response<St>, Status>>,
    {
        let request = Self::read_unary_request::<R, Req>(recv).await?;
        let mut writer = FrameWriter::new(send);

        match f(Self::tonic_request(request, &meta)?).await {
            Ok(response) => {
                let mut stream = response.into_inner();
                while let Some(item) = stream.next().await {
                    match item {
                        Ok(message) => {
                            writer
                                .write_frame(&Frame::Data(message.encode_to_vec()))
                                .await?;
                        }
                        Err(err) => {
                            Self::write_error(&mut writer, err).await?;
                            writer.flush().await?;
                            return Ok(());
                        }
                    }
                }
                writer
                    .write_frame(&Frame::Status {
                        code: status_ok(),
                        details: vec![],
                    })
                    .await?;
            }
            Err(err) => Self::write_error(&mut writer, err).await?,
        }

        writer.flush().await
    }

    fn spawn_request_pump<R, Req>(recv: R) -> ReceiverStream<Result<Req, Status>>
    where
        R: AsyncRead + Unpin + Send + 'static,
        Req: Message + Default + Send + 'static,
    {
        let (tx, rx) = mpsc::channel(128);
        let _task = tokio::spawn(async move {
            let mut reader = FrameReader::new_client_streaming(recv);
            loop {
                let item = match reader.read_frame().await {
                    Ok(Frame::Data(data)) => Req::decode(data.as_slice())
                        .map_err(|e| Status::internal(format!("decode request error: {e}"))),
                    Ok(Frame::End) => break,
                    Ok(Frame::Status { .. }) => Err(Status::internal(
                        "unexpected STATUS frame in client-streaming request",
                    )),
                    Err(err) => {
                        let status: xlinerpc::status::Status = err.into();
                        Err(status.into())
                    }
                };

                if tx.send(item).await.is_err() {
                    break;
                }
            }
        });
        ReceiverStream::new(rx)
    }

    async fn write_stream<S, Resp, St>(send: S, mut stream: St) -> Result<(), CurpError>
    where
        S: AsyncWrite + Unpin,
        Resp: Message + Debug,
        St: Stream<Item = Result<Resp, Status>> + Send + Unpin,
    {
        let mut writer = FrameWriter::new(send);

        while let Some(item) = stream.next().await {
            match item {
                Ok(message) => {
                    writer
                        .write_frame(&Frame::Data(message.encode_to_vec()))
                        .await?;
                }
                Err(err) => {
                    Self::write_error(&mut writer, err).await?;
                    writer.flush().await?;
                    return Ok(());
                }
            }
        }

        writer
            .write_frame(&Frame::Status {
                code: status_ok(),
                details: vec![],
            })
            .await?;
        writer.flush().await?;
        let mut send = writer.into_inner();
        send.shutdown()
            .await
            .map_err(|e| CurpError::from(Status::internal(format!("shutdown stream error: {e}"))))
    }

    async fn handle_watch<S, R>(&self, send: S, recv: R) -> Result<(), CurpError>
    where
        S: AsyncWrite + Unpin + Send + 'static,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let stream = self
            .watch
            .watch_stream(Self::spawn_request_pump::<R, WatchRequest>(recv));
        Self::write_stream(send, stream).await
    }

    async fn handle_lease_keep_alive<S, R>(&self, send: S, recv: R) -> Result<(), CurpError>
    where
        S: AsyncWrite + Unpin + Send + 'static,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let stream = self
            .lease
            .lease_keep_alive_stream(Self::spawn_request_pump::<R, LeaseKeepAliveRequest>(recv))
            .await
            .map_err(Self::tonic_status_to_curp)?;
        Self::write_stream(send, stream).await
    }
}

impl QuicServiceExt for XlineQuicService {
    fn handle<S, R>(
        &self,
        method: MethodId,
        send: S,
        recv: R,
        meta: Metadata,
    ) -> BoxFuture<'static, Result<(), CurpError>>
    where
        S: AsyncWrite + Unpin + Send + 'static,
        R: AsyncRead + Unpin + Send + 'static,
    {
        let this = self.clone();
        Box::pin(async move {
            match method {
                MethodId::XlineAuthenticate => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<AuthenticateRequest>| async move {
                            this.auth.authenticate(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineLeaseRevoke => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<LeaseRevokeRequest>| async move {
                            this.lease.lease_revoke(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineLeaseKeepAlive => {
                    this.handle_lease_keep_alive(send, recv).await?;
                }
                MethodId::XlineLeaseTtl => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<LeaseTimeToLiveRequest>| async move {
                            this.lease.lease_time_to_live(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineWatch => {
                    this.handle_watch(send, recv).await?;
                }
                MethodId::XlineSnapshot => {
                    Self::handle_server_streaming(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<SnapshotRequest>| async move {
                            this.maintenance.snapshot(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineAlarm => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<AlarmRequest>| async move {
                            this.maintenance.alarm(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineMaintStatus => {
                    Self::handle_unary(send, recv, meta, |req| async move {
                        this.maintenance.status(req).await
                    })
                    .await?;
                }
                MethodId::XlineMemberAdd => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<MemberAddRequest>| async move {
                            this.cluster.member_add(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineMemberRemove => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<MemberRemoveRequest>| async move {
                            this.cluster.member_remove(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineMemberPromote => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<MemberPromoteRequest>| async move {
                            this.cluster.member_promote(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineMemberUpdate => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<MemberUpdateRequest>| async move {
                            this.cluster.member_update(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineMemberList => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<MemberListRequest>| async move {
                            this.cluster.member_list(req).await
                        },
                    )
                    .await?;
                }
                MethodId::XlineCompact => {
                    Self::handle_unary(
                        send,
                        recv,
                        meta,
                        |req: tonic::Request<CompactionRequest>| async move {
                            this.kv.compact(req).await
                        },
                    )
                    .await?;
                }
                _ => {
                    return Err(CurpError::from(Status::internal(format!(
                        "unsupported xline QUIC method: {}",
                        method.name()
                    ))));
                }
            }

            Ok(())
        })
    }
}
