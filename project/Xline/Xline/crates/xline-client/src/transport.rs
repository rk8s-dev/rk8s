use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use curp::rpc::{CurpError, CurpErrorWrapper};
use futures::{Stream, StreamExt, future::BoxFuture};
use gm_quic::prelude::{Connection, QuicClient, StreamReader, StreamWriter};
use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::RwLock;
use xlinerpc::status::{Code, Status};

const SEND_TASK_GRACE_PERIOD: Duration = Duration::from_millis(100);
const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;
const FRAME_DATA: u8 = 0x01;
const FRAME_END: u8 = 0x02;
const FRAME_STATUS: u8 = 0x03;
const STATUS_OK: u8 = 0x00;
const STATUS_ERROR: u8 = 0x01;

pub(crate) use curp::rpc::MethodId;

#[derive(Clone)]
pub(crate) struct Channel {
    client: Arc<QuicClient>,
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
            client,
            addrs: Arc::new(RwLock::new(addrs)),
            index: Arc::new(AtomicUsize::new(0)),
            token: token.map(String::into_boxed_str).map(Arc::from),
            timeout,
        }
    }

    #[inline]
    pub(crate) fn with_token(&self, token: Option<String>) -> Self {
        Self {
            client: Arc::clone(&self.client),
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

    pub(crate) async fn unary<Req, Resp>(&self, method: MethodId, req: Req) -> Result<Resp, Status>
    where
        Req: Message,
        Resp: Message + Default,
    {
        let conn = self.connection().await?;
        tokio::time::timeout(self.timeout, async {
            let (recv, send) = Self::open_bi_stream(&conn).await?;
            let mut writer = FrameWriter::new(send);
            let mut reader = FrameReader::new(recv);

            writer
                .write_request_header(method, &self.metadata())
                .await?;
            writer.write_data(req.encode_to_vec()).await?;
            writer.write_end().await?;
            writer.flush().await?;
            writer.shutdown().await?;

            let payload = match reader.read_frame().await? {
                Frame::Data(data) => data,
                Frame::Status(status) => return Err(status),
                Frame::End => return Err(Status::internal("unexpected END in unary response")),
            };
            match reader.read_frame().await? {
                Frame::Status(status) if status.code() == Code::Ok => {}
                Frame::Status(status) => return Err(status),
                Frame::Data(_) | Frame::End => {
                    return Err(Status::internal("expected STATUS frame in unary response"));
                }
            }

            Resp::decode(payload.as_slice())
                .map_err(|e| Status::internal(format!("decode response error: {e}")))
        })
        .await
        .map_err(|_| Status::deadline_exceeded("request timeout"))?
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
        let conn = self.connection().await?;
        let (recv, send) = tokio::time::timeout(self.timeout, Self::open_bi_stream(&conn))
            .await
            .map_err(|_| Status::deadline_exceeded("request timeout"))??;

        let mut writer = FrameWriter::new(send);
        writer
            .write_request_header(method, &self.metadata())
            .await?;
        writer.write_data(req.encode_to_vec()).await?;
        writer.write_end().await?;
        writer.flush().await?;
        writer.shutdown().await?;

        Ok(Streaming::new(Box::pin(ResponseStream::new(
            FrameReader::new(recv),
            conn,
        ))))
    }

    pub(crate) async fn client_streaming<Req, Resp, St>(
        &self,
        method: MethodId,
        stream: St,
    ) -> Result<Streaming<Resp>, Status>
    where
        Req: Message + Send + 'static,
        Resp: Message + Default + Send + Unpin + 'static,
        St: Stream<Item = Req> + Send + 'static,
    {
        let conn = self.connection().await?;
        let (recv, send) = tokio::time::timeout(self.timeout, Self::open_bi_stream(&conn))
            .await
            .map_err(|_| Status::deadline_exceeded("request timeout"))??;

        let mut writer = FrameWriter::new(send);
        writer
            .write_request_header(method, &self.metadata())
            .await?;
        writer.flush().await?;

        let send_task = tokio::spawn(async move {
            futures::pin_mut!(stream);
            while let Some(req) = stream.next().await {
                writer.write_data(req.encode_to_vec()).await?;
            }
            writer.write_end().await?;
            writer.flush().await?;
            writer.shutdown().await
        });

        Ok(Streaming::new(Box::pin(ResponseStream::with_send_task(
            FrameReader::new(recv),
            conn,
            send_task,
        ))))
    }

    async fn connection(&self) -> Result<Connection, Status> {
        let addrs = self.addrs.read().await;
        if addrs.is_empty() {
            return Err(Status::unavailable("no available endpoints"));
        }

        let len = addrs.len();
        let start = self.index.fetch_add(1, Ordering::Relaxed) % len;
        let snapshot: Vec<_> = addrs.iter().cloned().collect();
        drop(addrs);

        let mut last_err = None;
        for offset in 0..len {
            let idx = (start + offset) % len;
            let addr = snapshot[idx]
                .strip_prefix("quic://")
                .or_else(|| snapshot[idx].strip_prefix("https://"))
                .or_else(|| snapshot[idx].strip_prefix("http://"))
                .unwrap_or(&snapshot[idx]);
            match self.client.connect(addr).await {
                Ok(conn) => return Ok(conn),
                Err(err) => {
                    last_err = Some(Status::unavailable(format!(
                        "QUIC connect error for {addr}: {err}"
                    )));
                }
            }
        }

        Err(last_err.unwrap_or_else(|| Status::unavailable("QUIC connect error")))
    }

    async fn open_bi_stream(conn: &Connection) -> Result<(StreamReader, StreamWriter), Status> {
        let opened = conn
            .open_bi_stream()
            .await
            .map_err(|e| Status::internal(format!("open stream error: {e}")))?;
        match opened {
            Some((_id, (reader, writer))) => Ok((reader, writer)),
            None => Err(Status::resource_exhausted(
                "stream concurrency limit reached",
            )),
        }
    }
}

/// QUIC-backed response stream used by xline client streaming APIs.
pub struct Streaming<T> {
    inner: Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>,
}

impl<T> std::fmt::Debug for Streaming<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Streaming").finish_non_exhaustive()
    }
}

impl<T> Streaming<T> {
    #[inline]
    fn new(inner: Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>) -> Self {
        Self { inner }
    }

    /// Receive the next message from the stream.
    pub async fn message(&mut self) -> Result<Option<T>, Status> {
        self.inner.next().await.transpose()
    }
}

impl<T> Stream for Streaming<T> {
    type Item = Result<T, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().inner.as_mut().poll_next(cx)
    }
}

enum Frame {
    Data(Vec<u8>),
    End,
    Status(Status),
}

struct FrameWriter {
    inner: StreamWriter,
}

impl FrameWriter {
    #[inline]
    fn new(inner: StreamWriter) -> Self {
        Self { inner }
    }

    async fn write_request_header(
        &mut self,
        method: MethodId,
        metadata: &[(String, String)],
    ) -> Result<(), Status> {
        self.inner
            .write_u16(method.as_u16())
            .await
            .map_err(Status::from)?;
        let count = u16::try_from(metadata.len())
            .map_err(|_| Status::internal("too many metadata entries"))?;
        self.inner.write_u16(count).await.map_err(Status::from)?;
        for (key, value) in metadata {
            self.write_bytes(key.as_bytes()).await?;
            self.write_bytes(value.as_bytes()).await?;
        }
        Ok(())
    }

    async fn write_data(&mut self, data: Vec<u8>) -> Result<(), Status> {
        self.inner
            .write_u8(FRAME_DATA)
            .await
            .map_err(Status::from)?;
        self.write_vec(data).await
    }

    async fn write_end(&mut self) -> Result<(), Status> {
        self.inner.write_u8(FRAME_END).await.map_err(Status::from)
    }

    async fn flush(&mut self) -> Result<(), Status> {
        self.inner.flush().await.map_err(Status::from)
    }

    async fn shutdown(&mut self) -> Result<(), Status> {
        self.inner.shutdown().await.map_err(Status::from)
    }

    async fn write_vec(&mut self, bytes: Vec<u8>) -> Result<(), Status> {
        let len = u32::try_from(bytes.len()).map_err(|_| Status::internal("frame too large"))?;
        self.inner.write_u32(len).await.map_err(Status::from)?;
        self.inner.write_all(&bytes).await.map_err(Status::from)
    }

    async fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Status> {
        let len = u16::try_from(bytes.len()).map_err(|_| Status::internal("metadata too large"))?;
        self.inner.write_u16(len).await.map_err(Status::from)?;
        self.inner.write_all(bytes).await.map_err(Status::from)
    }
}

struct FrameReader {
    inner: StreamReader,
}

impl FrameReader {
    #[inline]
    fn new(inner: StreamReader) -> Self {
        Self { inner }
    }

    async fn read_frame(&mut self) -> Result<Frame, Status> {
        match self.inner.read_u8().await.map_err(Status::from)? {
            FRAME_DATA => Ok(Frame::Data(self.read_vec().await?)),
            FRAME_END => Ok(Frame::End),
            FRAME_STATUS => {
                let code = self.inner.read_u8().await.map_err(Status::from)?;
                let details = self.read_vec().await?;
                match code {
                    STATUS_OK => Ok(Frame::Status(Status::new(Code::Ok, String::new()))),
                    STATUS_ERROR => {
                        let err = if details.is_empty() {
                            CurpError::from(Status::internal("missing error details"))
                        } else {
                            CurpErrorWrapper::decode(details.as_slice())
                                .map_err(|e| {
                                    Status::internal(format!("decode error details: {e}"))
                                })?
                                .err
                                .unwrap_or_else(|| {
                                    CurpError::from(Status::internal("missing error"))
                                })
                        };
                        Ok(Frame::Status(Status::from(err)))
                    }
                    other => Err(Status::internal(format!("unknown status code: {other}"))),
                }
            }
            kind => Err(Status::internal(format!("unknown frame kind: {kind}"))),
        }
    }

    async fn read_vec(&mut self) -> Result<Vec<u8>, Status> {
        let len = usize::try_from(self.inner.read_u32().await.map_err(Status::from)?)
            .map_err(|_| Status::internal("invalid frame length"))?;
        if len > MAX_FRAME_LEN {
            return Err(Status::internal("frame too large"));
        }
        let mut buf = vec![0; len];
        let _read = self
            .inner
            .read_exact(&mut buf)
            .await
            .map_err(Status::from)?;
        Ok(buf)
    }
}

struct ResponseStream<T> {
    state: ResponseState,
    done: bool,
    _conn: Connection,
    _send_task: Option<tokio::task::JoinHandle<Result<(), Status>>>,
    _marker: std::marker::PhantomData<T>,
}

enum ResponseState {
    Idle(FrameReader),
    Reading(BoxFuture<'static, (Result<Frame, Status>, FrameReader)>),
    Poisoned,
}

impl<T> ResponseStream<T> {
    fn new(reader: FrameReader, conn: Connection) -> Self {
        Self {
            state: ResponseState::Idle(reader),
            done: false,
            _conn: conn,
            _send_task: None,
            _marker: std::marker::PhantomData,
        }
    }

    fn with_send_task(
        reader: FrameReader,
        conn: Connection,
        send_task: tokio::task::JoinHandle<Result<(), Status>>,
    ) -> Self {
        Self {
            state: ResponseState::Idle(reader),
            done: false,
            _conn: conn,
            _send_task: Some(send_task),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<T> Drop for ResponseStream<T> {
    fn drop(&mut self) {
        if let Some(handle) = self._send_task.take() {
            let _jh = tokio::spawn(async move {
                tokio::pin!(handle);
                tokio::select! {
                    _ = &mut handle => {}
                    _ = tokio::time::sleep(SEND_TASK_GRACE_PERIOD) => {
                        handle.abort();
                        let _ = (&mut handle).await;
                    }
                }
            });
        }
    }
}

impl<T> Stream for ResponseStream<T>
where
    T: Message + Default + Send + Unpin + 'static,
{
    type Item = Result<T, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }

        if matches!(this.state, ResponseState::Idle(_)) {
            let state = std::mem::replace(&mut this.state, ResponseState::Poisoned);
            if let ResponseState::Idle(mut reader) = state {
                this.state = ResponseState::Reading(Box::pin(async move {
                    let frame = reader.read_frame().await;
                    (frame, reader)
                }));
            }
        }

        if let ResponseState::Reading(ref mut fut) = this.state {
            match fut.as_mut().poll(cx) {
                Poll::Ready((result, reader)) => {
                    this.state = ResponseState::Idle(reader);
                    match result {
                        Ok(Frame::Data(data)) => {
                            Poll::Ready(Some(T::decode(data.as_slice()).map_err(|e| {
                                Status::internal(format!("decode response error: {e}"))
                            })))
                        }
                        Ok(Frame::Status(status)) if status.code() == Code::Ok => {
                            this.done = true;
                            Poll::Ready(None)
                        }
                        Ok(Frame::Status(status)) => {
                            this.done = true;
                            Poll::Ready(Some(Err(status)))
                        }
                        Ok(Frame::End) => {
                            this.done = true;
                            Poll::Ready(Some(Err(Status::internal("unexpected END frame"))))
                        }
                        Err(err) => {
                            this.done = true;
                            Poll::Ready(Some(Err(err)))
                        }
                    }
                }
                Poll::Pending => Poll::Pending,
            }
        } else {
            this.done = true;
            Poll::Ready(Some(Err(Status::internal("stream state poisoned"))))
        }
    }
}
