//! Types for parsing HTTP/3 bodies.
//!
//! This module is inspired by the HTTP/3 body implementation from the [Scuffle](https://github.com/ScuffleCloud/scuffle) project.
//! Original Scuffle code is Copyright 2025 Scuffle LLC and licensed under the MIT license.
//! The design patterns (state machine, trailer polling, size hint enforcement) draw from Scuffle's approach.
//!
//! See the original repository for more context: <https://github.com/ScuffleCloud/scuffle>
use std::convert::Infallible;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Error, anyhow};
use bytes::{Buf, Bytes};
use futures::StreamExt;
use h3::server::RequestStream;
use http::Request;
use prost::Message;
use tokio_stream::Stream;
use tower::Service;
use xlinerpc::{
    MetaData, Request as XlineRequest, Response as XlineResponse, Status as XlineStatus,
    Streaming as XlineStreaming,
};

type RpcFuture<T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send>>;

/// An incoming HTTP/3 body.
///
/// Implements [`http_body::Body`].
pub(crate) struct QuicIncomingBody<S> {
    stream: RequestStream<S, Bytes>,
    state: State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Data(Option<u64>),
    Trailers,
    Done,
}

impl<S> QuicIncomingBody<S> {
    /// Create a new incoming HTTP/3 body.
    pub(crate) fn new(stream: RequestStream<S, Bytes>, size_hint: Option<u64>) -> Self {
        Self {
            stream,
            state: State::Data(size_hint),
        }
    }
}

impl<S: h3::quic::RecvStream> http_body::Body for QuicIncomingBody<S> {
    type Data = Bytes;
    type Error = Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let QuicIncomingBody { stream, state } = self.as_mut().get_mut();

        if *state == State::Done {
            return Poll::Ready(None);
        }

        if let State::Data(remaining) = state {
            match stream.poll_recv_data(cx) {
                Poll::Ready(Ok(Some(mut buf))) => {
                    let buf_size = buf.remaining() as u64;

                    if let Some(remaining) = remaining {
                        if buf_size > *remaining {
                            *state = State::Done;
                            return Poll::Ready(Some(Err(anyhow!(
                                "the given buffer size hint was exceeded"
                            ))));
                        }

                        *remaining -= buf_size;
                    }

                    return Poll::Ready(Some(Ok(http_body::Frame::data(
                        buf.copy_to_bytes(buf_size as usize),
                    ))));
                }
                Poll::Ready(Ok(None)) => {
                    *state = State::Trailers;
                }
                Poll::Ready(Err(err)) => {
                    *state = State::Done;
                    return Poll::Ready(Some(Err(Error::from(err))));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }

        // We poll the recv data again even though we already got the None
        // because we want to make sure there is not a frame after the trailers
        // This is a workaround because h3 does not allow us to poll the trailer
        // directly, so we need to make sure the future recv_trailers is going to be
        // ready after a single poll We avoid pinning to the heap.
        let resp = match stream.poll_recv_data(cx) {
            Poll::Ready(Ok(None)) => match std::pin::pin!(stream.recv_trailers()).poll(cx) {
                Poll::Ready(Ok(Some(trailers))) => {
                    Poll::Ready(Some(Ok(http_body::Frame::trailers(trailers))))
                }
                // We will only poll the recv_trailers once so if pending is returned we are done.
                Poll::Pending => {
                    // #[cfg(feature = "debug")]
                    // tracing::warn!("recv_trailers is pending");
                    Poll::Ready(None)
                }
                Poll::Ready(Ok(None)) => Poll::Ready(None),
                Poll::Ready(Err(err)) => Poll::Ready(Some(Err(Error::from(err)))),
            },
            // We are not expecting any data after the previous poll returned None
            Poll::Ready(Ok(Some(_))) => {
                Poll::Ready(Some(Err(anyhow!("unexpected data after trailers"))))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Some(Err(Error::from(err)))),
            Poll::Pending => return Poll::Pending,
        };

        *state = State::Done;

        resp
    }

    fn size_hint(&self) -> http_body::SizeHint {
        match self.state {
            State::Data(Some(remaining)) => http_body::SizeHint::with_exact(remaining),
            State::Data(None) => http_body::SizeHint::default(),
            State::Trailers | State::Done => http_body::SizeHint::with_exact(0),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self.state {
            State::Data(Some(0)) | State::Trailers | State::Done => true,
            State::Data(_) => false,
        }
    }
}

const GRPC_HEADER_SIZE: usize = 5;

fn grpc_ok_response(body: axum::body::Body) -> http::Response<axum::body::Body> {
    let mut response = http::Response::new(body);
    let headers = response.headers_mut();
    let _ = headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );
    let _ = headers.insert(
        http::header::HeaderName::from_static("grpc-status"),
        http::HeaderValue::from_static("0"),
    );
    response
}

fn grpc_trailers_from_status(status: Option<XlineStatus>) -> http::HeaderMap {
    let mut trailers = http::HeaderMap::new();
    let code = status
        .as_ref()
        .map(|s| i32::from(s.code()).to_string())
        .unwrap_or_else(|| "0".to_string());
    let grpc_status =
        http::HeaderValue::from_str(&code).unwrap_or_else(|_| http::HeaderValue::from_static("2"));
    let _ = trailers.insert(
        http::header::HeaderName::from_static("grpc-status"),
        grpc_status,
    );

    if let Some(status) = status {
        let msg = status.message();
        if !msg.is_empty() {
            let encoded = encode_grpc_message_header(msg);
            let grpc_message = http::HeaderValue::from_str(&encoded)
                .unwrap_or_else(|_| http::HeaderValue::from_static("rpc error"));
            let _ = trailers.insert(
                http::header::HeaderName::from_static("grpc-message"),
                grpc_message,
            );
        }
    }

    trailers
}

struct GrpcStreamingBody {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, XlineStatus>> + Send>>,
    finished: bool,
}

impl GrpcStreamingBody {
    fn new(inner: Pin<Box<dyn Stream<Item = Result<Bytes, XlineStatus>> + Send>>) -> Self {
        Self {
            inner,
            finished: false,
        }
    }
}

impl http_body::Body for GrpcStreamingBody {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if self.finished {
            return Poll::Ready(None);
        }

        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(data))) => Poll::Ready(Some(Ok(http_body::Frame::data(data)))),
            Poll::Ready(Some(Err(status))) => {
                self.finished = true;
                let trailers = grpc_trailers_from_status(Some(status));
                Poll::Ready(Some(Ok(http_body::Frame::trailers(trailers))))
            }
            Poll::Ready(None) => {
                self.finished = true;
                let trailers = grpc_trailers_from_status(None);
                Poll::Ready(Some(Ok(http_body::Frame::trailers(trailers))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

fn grpc_error_response(status: XlineStatus) -> http::Response<axum::body::Body> {
    let mut response = http::Response::new(axum::body::Body::empty());
    let headers = response.headers_mut();
    let _ = headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/grpc+proto"),
    );

    let grpc_code = i32::from(status.code()).to_string();
    let grpc_status = http::HeaderValue::from_str(&grpc_code)
        .unwrap_or_else(|_| http::HeaderValue::from_static("2"));
    let _ = headers.insert(
        http::header::HeaderName::from_static("grpc-status"),
        grpc_status,
    );

    let encoded_message = encode_grpc_message_header(status.message());
    let grpc_message = http::HeaderValue::from_str(&encoded_message)
        .unwrap_or_else(|_| http::HeaderValue::from_static("rpc error"));
    let _ = headers.insert(
        http::header::HeaderName::from_static("grpc-message"),
        grpc_message,
    );

    *response.status_mut() = http::StatusCode::OK;
    response
}

fn encode_grpc_message_header(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    for b in message.bytes() {
        if (0x20..=0x7e).contains(&b) && b != b'%' {
            out.push(char::from(b));
        } else {
            out.push('%');
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{b:02X}"));
        }
    }
    out
}

fn grpc_frame_encode<M: Message>(msg: &M) -> Result<Bytes, XlineStatus> {
    let body = msg.encode_to_vec();
    let len = u32::try_from(body.len())
        .map_err(|_| XlineStatus::internal("gRPC message too large"))?;
    let mut out = Vec::with_capacity(GRPC_HEADER_SIZE + body.len());
    out.push(0); // uncompressed
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(&body);
    Ok(Bytes::from(out))
}

fn grpc_frame_decode<M: Message + Default>(buf: &[u8]) -> Result<(M, usize), XlineStatus> {
    if buf.len() < GRPC_HEADER_SIZE {
        return Err(XlineStatus::internal("gRPC frame header too short"));
    }
    if buf[0] != 0 {
        return Err(XlineStatus::internal(
            "compressed gRPC frames are not supported",
        ));
    }
    let len = u32::from_be_bytes([buf[1], buf[2], buf[3], buf[4]]) as usize;
    let total = GRPC_HEADER_SIZE + len;
    if buf.len() < total {
        return Err(XlineStatus::internal("gRPC frame body too short"));
    }
    let msg = M::decode(&buf[GRPC_HEADER_SIZE..total])
        .map_err(|e| XlineStatus::internal(format!("protobuf decode error: {e}")))?;
    Ok((msg, total))
}

fn decode_all_grpc_frames<M: Message + Default>(buf: &[u8]) -> Result<Vec<M>, XlineStatus> {
    let mut msgs = Vec::new();
    let mut offset = 0;
    while offset < buf.len() {
        let (msg, consumed) = grpc_frame_decode::<M>(&buf[offset..])?;
        msgs.push(msg);
        offset += consumed;
    }
    Ok(msgs)
}

fn metadata_from_headers(headers: &http::HeaderMap) -> MetaData {
    let mut meta = MetaData::new();
    for (k, v) in headers {
        if let Ok(vs) = v.to_str() {
            meta.insert(k.as_str(), vs);
        }
    }
    meta
}

async fn read_body_bytes<B>(body: B) -> Result<Vec<u8>, XlineStatus>
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<crate::router::Error> + Send + 'static,
{
    let mut bytes = Vec::new();
    let mut body = std::pin::pin!(body);

    while let Some(frame) = futures::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
        let frame = frame.map_err(|e| XlineStatus::internal(e.into().to_string()))?;
        if let Ok(data) = frame.into_data() {
            bytes.extend_from_slice(data.as_ref());
        }
    }

    Ok(bytes)
}

fn spawn_grpc_request_stream<B, M>(body: B) -> XlineStreaming<M>
where
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<crate::router::Error> + Send + 'static,
    M: Message + Default + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<M, XlineStatus>>(128);

    let _ = tokio::spawn(async move {
        let mut body = std::pin::pin!(body);
        let mut buf = Vec::new();
        let mut read_pos = 0usize;

        while let Some(frame) = futures::future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
            let frame = match frame {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx
                        .send(Err(XlineStatus::internal(e.into().to_string())))
                        .await;
                    return;
                }
            };

            if let Ok(data) = frame.into_data() {
                buf.extend_from_slice(data.as_ref());

                loop {
                    let available = buf.len().saturating_sub(read_pos);
                    if available < GRPC_HEADER_SIZE {
                        break;
                    }

                    let base = read_pos;
                    let len = u32::from_be_bytes([
                        buf[base + 1],
                        buf[base + 2],
                        buf[base + 3],
                        buf[base + 4],
                    ]) as usize;
                    let total = GRPC_HEADER_SIZE + len;
                    if available < total {
                        break;
                    }

                    let decoded = grpc_frame_decode::<M>(&buf[base..base + total]);
                    read_pos += total;

                    match decoded {
                        Ok((msg, _)) => {
                            if tx.send(Ok(msg)).await.is_err() {
                                return;
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e)).await;
                            return;
                        }
                    }
                }

                // Periodically compact consumed prefix to bound memory and keep indexing cheap.
                if read_pos > 0 && (read_pos >= 4096 || read_pos * 2 >= buf.len()) {
                    let _ = buf.drain(..read_pos);
                    read_pos = 0;
                }
            }
        }

        if buf.len().saturating_sub(read_pos) != 0 {
            let _ = tx
                .send(Err(XlineStatus::internal(
                    "incomplete gRPC frame at end of request stream",
                )))
                .await;
        }
    });

    XlineStreaming::new(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
}

#[derive(Clone)]
pub(crate) struct MakeUnarySVC<SVC, Input, Output> {
    inner: SVC,
    _1: std::marker::PhantomData<Input>,
    _2: std::marker::PhantomData<Output>,
}

impl<SVC, Input, Output> MakeUnarySVC<SVC, Input, Output>
where
    SVC: Clone,
{
    pub(crate) fn new(service: SVC) -> Self {
        Self {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output> Service<Request<B>> for MakeUnarySVC<SVC, Input, Output>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    SVC: Service<XlineRequest<Input>, Response = XlineResponse<Output>, Error = XlineStatus>
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<crate::router::Error> + Send + 'static,
{
    type Response = http::Response<axum::body::Body>;
    type Error = Infallible;
    type Future = RpcFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut svc = self.inner.clone();
        let fut = async move {
            let (parts, body) = request.into_parts();
            let meta = metadata_from_headers(&parts.headers);

            let req_bytes = match read_body_bytes(body).await {
                Ok(b) => b,
                Err(e) => return Ok(grpc_error_response(e)),
            };

            let mut reqs = match decode_all_grpc_frames::<Input>(&req_bytes) {
                Ok(v) => v,
                Err(e) => return Ok(grpc_error_response(e)),
            };

            if reqs.len() != 1 {
                return Ok(grpc_error_response(XlineStatus::invalid_argument(format!(
                    "unary request expects exactly 1 frame, got {}",
                    reqs.len()
                ))));
            }

            let rpc_req = XlineRequest::new(reqs.remove(0), meta);
            match svc.call(rpc_req).await {
                Ok(resp) => {
                    let framed = grpc_frame_encode(resp.data());
                    Ok(grpc_ok_response(axum::body::Body::from(framed)))
                }
                Err(e) => Ok(grpc_error_response(e)),
            }
        };
        Box::pin(fut)
    }
}

#[derive(Clone)]
pub(crate) struct MakeStreamingSvc<SVC, Input, Output> {
    inner: SVC,
    _1: std::marker::PhantomData<Input>,
    _2: std::marker::PhantomData<Output>,
}

impl<SVC, Input, Output> MakeStreamingSvc<SVC, Input, Output>
where
    SVC: Clone,
{
    pub(crate) fn new(service: SVC) -> Self {
        Self {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output, RspStream> Service<Request<B>> for MakeStreamingSvc<SVC, Input, Output>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    RspStream: Stream<Item = Result<Output, XlineStatus>> + Send + 'static,
    SVC: Service<
            XlineRequest<XlineStreaming<Input>>,
            Response = XlineResponse<RspStream>,
            Error = XlineStatus,
        >
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<crate::router::Error> + Send + 'static,
{
    type Response = http::Response<axum::body::Body>;
    type Error = Infallible;
    type Future = RpcFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut svc = self.inner.clone();
        let fut = async move {
            let (parts, body) = request.into_parts();
            let meta = metadata_from_headers(&parts.headers);
            let input_stream = spawn_grpc_request_stream::<B, Input>(body);
            let rpc_req = XlineRequest::new(input_stream, meta);

            match svc.call(rpc_req).await {
                Ok(resp) => {
                    let out = resp
                        .into_inner()
                        .map(|item| item.map(|msg| grpc_frame_encode(&msg)));
                    let body = axum::body::Body::new(GrpcStreamingBody::new(Box::pin(out)));
                    Ok(grpc_ok_response(body))
                }
                Err(e) => Ok(grpc_error_response(e)),
            }
        };
        Box::pin(fut)
    }
}

#[derive(Clone)]
pub(crate) struct MakeServerStreamingSvc<SVC, Input, Output> {
    inner: SVC,
    _1: std::marker::PhantomData<Input>,
    _2: std::marker::PhantomData<Output>,
}

impl<SVC, Input, Output> MakeServerStreamingSvc<SVC, Input, Output>
where
    SVC: Clone,
{
    pub(crate) fn new(service: SVC) -> Self {
        Self {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output, RspStream> Service<Request<B>>
    for MakeServerStreamingSvc<SVC, Input, Output>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    RspStream: Stream<Item = Result<Output, XlineStatus>> + Send + 'static,
    SVC: Service<XlineRequest<Input>, Response = XlineResponse<RspStream>, Error = XlineStatus>
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<crate::router::Error> + Send + 'static,
{
    type Response = http::Response<axum::body::Body>;
    type Error = Infallible;
    type Future = RpcFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut svc = self.inner.clone();
        let fut = async move {
            let (parts, body) = request.into_parts();
            let meta = metadata_from_headers(&parts.headers);

            let req_bytes = match read_body_bytes(body).await {
                Ok(b) => b,
                Err(e) => return Ok(grpc_error_response(e)),
            };

            let mut reqs = match decode_all_grpc_frames::<Input>(&req_bytes) {
                Ok(v) => v,
                Err(e) => return Ok(grpc_error_response(e)),
            };

            if reqs.len() != 1 {
                return Ok(grpc_error_response(XlineStatus::invalid_argument(format!(
                    "server-streaming request expects exactly 1 frame, got {}",
                    reqs.len()
                ))));
            }

            let rpc_req = XlineRequest::new(reqs.remove(0), meta);
            match svc.call(rpc_req).await {
                Ok(resp) => {
                    let out = resp
                        .into_inner()
                        .map(|item| item.map(|msg| grpc_frame_encode(&msg)));
                    let body = axum::body::Body::new(GrpcStreamingBody::new(Box::pin(out)));
                    Ok(grpc_ok_response(body))
                }
                Err(e) => Ok(grpc_error_response(e)),
            }
        };
        Box::pin(fut)
    }
}

#[derive(Clone)]
pub(crate) struct MakeClientStreamingSvc<SVC, Input, Output> {
    inner: SVC,
    _1: std::marker::PhantomData<Input>,
    _2: std::marker::PhantomData<Output>,
}

impl<SVC, Input, Output> MakeClientStreamingSvc<SVC, Input, Output>
where
    SVC: Clone,
{
    pub(crate) fn new(service: SVC) -> Self {
        Self {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output> Service<Request<B>> for MakeClientStreamingSvc<SVC, Input, Output>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    SVC: Service<
            XlineRequest<XlineStreaming<Input>>,
            Response = XlineResponse<Output>,
            Error = XlineStatus,
        >
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: http_body::Body<Data = Bytes> + Send + 'static,
    B::Error: Into<crate::router::Error> + Send + 'static,
{
    type Response = http::Response<axum::body::Body>;
    type Error = Infallible;
    type Future = RpcFuture<Self::Response, Self::Error>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut svc = self.inner.clone();
        let fut = async move {
            let (parts, body) = request.into_parts();
            let meta = metadata_from_headers(&parts.headers);
            let input_stream = spawn_grpc_request_stream::<B, Input>(body);
            let rpc_req = XlineRequest::new(input_stream, meta);

            match svc.call(rpc_req).await {
                Ok(resp) => {
                    let framed = grpc_frame_encode(resp.data());
                    Ok(grpc_ok_response(axum::body::Body::from(framed)))
                }
                Err(e) => Ok(grpc_error_response(e)),
            }
        };
        Box::pin(fut)
    }
}
