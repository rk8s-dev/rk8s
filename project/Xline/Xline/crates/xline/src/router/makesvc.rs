use http::Request;
use http_body::{Body, SizeHint};
use prost::Message;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_stream::StreamExt;
use xlinerpc::{Request as XlineRequest, Response as XlineResponse, Status, MetaData, BinaryCodec};
use tower::Service;
use bytes::Bytes;

/// Decode a gRPC frame from bytes
/// gRPC frame format: [flags (1 byte)] [length (4 bytes)] [data]
fn decode_grpc_frame(data: &[u8]) -> Result<&[u8], String> {
    if data.len() < 5 {
        return Err("Insufficient data for gRPC frame".to_string());
    }
    
    // Skip flags (1 byte)
    let length_start = 1;
    
    // Read length (4 bytes, big-endian)
    let length = u32::from_be_bytes([
        data[length_start],
        data[length_start + 1],
        data[length_start + 2],
        data[length_start + 3]
    ]) as usize;
    
    let data_start = length_start + 4;
    let expected_end = data_start + length;
    
    if expected_end > data.len() {
        return Err("Invalid gRPC frame length".to_string());
    }
    
    Ok(&data[data_start..expected_end])
}

/// Create an HTTP response for a gRPC error
fn create_error_response(status: Status) -> http::Response<http_body::Full<Bytes>> {
    let status_code = status.code();
    let status_message = status.message().to_string();
    
    http::Response::builder()
        .status(http::StatusCode::OK) // gRPC errors still use 200 OK at HTTP level
        .header(http::header::CONTENT_TYPE, "application/grpc")
        .header("grpc-status", status_code.to_string())
        .header("grpc-message", status_message)
        .body(http_body::Full::from(Bytes::new()))
        .unwrap()
}

/// Custom body for gRPC streaming responses that encodes items incrementally
#[pin_project::pin_project]
pub(crate) struct GrpcStreamingBody<S, M>
where
    S: Stream<Item = Result<M, Status>> + Send + 'static,
    M: Message + Default + Clone,
{
    #[pin]
    inner: S,
    encoding_buffer: Vec<u8>,
    stream_ended: bool,
}

impl<S, M> GrpcStreamingBody<S, M>
where
    S: Stream<Item = Result<M, Status>> + Send + 'static,
    M: Message + Default + Clone,
{
    pub(crate) fn new(stream: S) -> Self {
        Self {
            inner: stream,
            encoding_buffer: Vec::new(),
            stream_ended: false,
        }
    }
}

impl<S, M> Body for GrpcStreamingBody<S, M>
where
    S: Stream<Item = Result<M, Status>> + Send + 'static,
    M: Message + Default + Clone,
{
    type Data = Bytes;
    type Error = Status;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        let mut this = self.project();

        loop {
            // If we have buffered data, send it as a data frame
            if !this.encoding_buffer.is_empty() {
                let data = Bytes::from(this.encoding_buffer.drain(..).collect());
                return Poll::Ready(Some(Ok(http_body::Frame::data(data))));
            }

            // Get the next item from the stream
            match this.inner.poll_next_unpin(cx) {
                Poll::Ready(Some(Ok(item))) => {
                    // Encode the item into a temporary buffer first
                    let mut message_buffer = Vec::new();
                    if let Err(e) = item.encode(&mut message_buffer) {
                        return Poll::Ready(Some(Err(Status::internal(format!("Failed to encode item: {}", e)))));
                    }
                    
                    // Add gRPC frame header: [flags (1 byte)] [length (4 bytes)] [data]
                    // Flags: 0 for uncompressed
                    this.encoding_buffer.push(0);
                    // Length: big-endian
                    this.encoding_buffer.extend_from_slice(&u32::to_be_bytes(message_buffer.len() as u32));
                    // Data
                    this.encoding_buffer.extend_from_slice(&message_buffer);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(Status::internal(format!("Stream error: {}", e)))));
                }
                Poll::Ready(None) => {
                    // Stream completed, send trailers
                    this.stream_ended = true;
                    let mut trailers = http::HeaderMap::new();
                    trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
                    return Poll::Ready(Some(Ok(http_body::Frame::trailers(trailers))));
                }
                Poll::Pending => {
                    return Poll::Pending; // Wait for more data
                }
            }
        }
    }

    fn is_end_stream(&self) -> bool {
        self.stream_ended && self.encoding_buffer.is_empty()
    }

    fn size_hint(&self) -> SizeHint {
        // We don't know the size in advance
        SizeHint::default()
    }
}

#[derive(Clone)]
pub(crate) struct WithEncodingOption<T> {
    svc: Arc<T>,
    max_decoding_message_size: Option<usize>,
    max_encoding_message_size: Option<usize>,
}

impl<T> WithEncodingOption<T> {
    pub(crate) fn new(inner: T) -> Self {
        Self::from_arc(Arc::new(inner))
    }

    pub(crate) fn from_arc(inner: Arc<T>) -> Self {
        Self {
            svc: inner,
            max_decoding_message_size: None,
            max_encoding_message_size: None,
        }
    }

    // /// Enable decompressing requests with the given encoding.
    // pub(crate) fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
    //     self.accept_compression_encodings.enable(encoding);
    //     self
    // }
    // /// Compress responses with the given encoding, if the client supports it.
    // pub(crate) fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
    //     self.send_compression_encodings.enable(encoding);
    //     self
    // }
    // /// Limits the maximum size of a decoded message.
    // ///
    // /// Default: `4MB`
    // pub(crate) fn max_decoding_message_size(mut self, limit: usize) -> Self {
    //     self.max_decoding_message_size = Some(limit);
    //     self
    // }
    // /// Limits the maximum size of an encoded message.
    // ///
    // /// Default: `usize::MAX`
}

#[derive(Clone)]
pub(crate) struct MakeUnarySvc<SVC, Input, Output> {
    inner: SVC,
    _1: std::marker::PhantomData<Input>,
    _2: std::marker::PhantomData<Output>,
}

impl<SVC, Input, Output> MakeUnarySvc<SVC, Input, Output>
where
    SVC: Clone,
{
    pub(crate) fn new(service: SVC) -> Self {
        MakeUnarySvc {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output> Service<Request<B>>
    for WithEncodingOption<MakeUnarySvc<SVC, Input, Output>>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    SVC: Service<XlineRequest<Input>, Response = XlineResponse<Output>, Error = Status>
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: Body + Send + 'static,
    B::Error: Into<super::Error> + Send + 'static,
{
    type Response = http::Response<http_body::Full<Bytes>>;
    type Error = std::convert::Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut method = self.svc.inner.clone();
        let fut = async move {
            // Read body
            let body_bytes = match hyper::body::to_bytes(request.into_body()).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    let status = Status::internal(format!("Failed to read body: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Decode gRPC frame first
            let xline_request = match decode_grpc_frame(&body_bytes) {
                Ok(frame_data) => match XlineRequest::<Input>::decode_from_slice(&frame_data) {
                    Ok(req) => req,
                    Err(e) => {
                        let status = Status::internal(format!("Failed to decode request: {}", e));
                        return Ok(create_error_response(status));
                    }
                },
                Err(e) => {
                    let status = Status::internal(format!("Failed to decode gRPC frame: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Call service
            let xline_response = match method.call(xline_request).await {
                Ok(resp) => resp,
                Err(status) => {
                    return Ok(create_error_response(status));
                }
            };

            // Encode response
            let response_bytes = match xline_response.encode_to_vec() {
                Ok(bytes) => bytes,
                Err(e) => {
                    let status = Status::internal(format!("Failed to encode response: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Wrap response in gRPC frame
            // gRPC frame format: [flags (1 byte)] [length (4 bytes)] [data]
            let mut framed_response = Vec::with_capacity(1 + 4 + response_bytes.len());
            // Write flags (0 for uncompressed)
            framed_response.push(0);
            // Write length (big-endian)
            framed_response.extend_from_slice(&u32::to_be_bytes(response_bytes.len() as u32));
            // Write data
            framed_response.extend_from_slice(&response_bytes);

            // Create HTTP response
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .header("grpc-status", "0") // gRPC OK status code
                .body(http_body::Full::from(Bytes::from(framed_response)))
                .unwrap();

            Ok(response)
        };
        Box::pin(fut)
    }
}

#[derive(Clone)]
pub(crate) struct MakeStreamingSvc<SVC, Input, Output, RspStream>
where
    RspStream: Stream<Item = Result<Output, Status>> + Send + 'static,
{
    inner: SVC,
    _1: std::marker::PhantomData<Input>,
    _2: std::marker::PhantomData<Output>,
}

impl<SVC, Input, Output, RspStream> MakeStreamingSvc<SVC, Input, Output, RspStream>
where
    RspStream: Stream<Item = Result<Output, Status>> + Send + 'static,
    SVC: Clone,
{
    pub(crate) fn new(service: SVC) -> Self {
        MakeStreamingSvc {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output, RspStream> Service<Request<B>>
    for WithEncodingOption<MakeStreamingSvc<SVC, Input, Output, RspStream>>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    RspStream: Stream<Item = Result<Output, Status>> + Send + 'static,
    SVC: Service<XlineRequest<Input>, Response = XlineResponse<RspStream>, Error = Status>
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: Body + Send + 'static,
    B::Error: Into<super::Error> + Send + 'static,
{
    type Response = http::Response<GrpcStreamingBody<RspStream, Output>>;
    type Error = std::convert::Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut method = self.svc.inner.clone();
        let fut = async move {
            // Read body
            let body_bytes = match hyper::body::to_bytes(request.into_body()).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    let status = Status::internal(format!("Failed to read body: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Decode gRPC frame first
            let xline_request = match decode_grpc_frame(&body_bytes) {
                Ok(frame_data) => match XlineRequest::<Input>::decode_from_slice(&frame_data) {
                    Ok(req) => req,
                    Err(e) => {
                        let status = Status::internal(format!("Failed to decode request: {}", e));
                        return Ok(create_error_response(status));
                    }
                },
                Err(e) => {
                    let status = Status::internal(format!("Failed to decode gRPC frame: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Call service
            let xline_response = match method.call(xline_request).await {
                Ok(resp) => resp,
                Err(status) => {
                    return Ok(create_error_response(status));
                }
            };

            // Create streaming response body that encodes items incrementally
            let stream = xline_response.into_inner();
            let body = GrpcStreamingBody::<RspStream, Output>::new(stream);

            // Create HTTP response with streaming body
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .body(body)
                .unwrap();

            Ok(response)
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
        MakeServerStreamingSvc {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output, RspStream> Service<Request<B>>
    for WithEncodingOption<MakeServerStreamingSvc<SVC, Input, Output>>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    RspStream: Stream<Item = Result<Output, Status>> + Send + 'static,
    SVC: Service<XlineRequest<Input>, Response = XlineResponse<RspStream>, Error = Status>
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: Body + Send + 'static,
    B::Error: Into<super::Error> + Send + 'static,
{
    type Response = http::Response<GrpcStreamingBody<RspStream, Output>>;
    type Error = std::convert::Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let mut method = self.svc.inner.clone();
        let fut = async move {
            // Read body
            let body_bytes = match hyper::body::to_bytes(request.into_body()).await {
                Ok(bytes) => bytes,
                Err(e) => {
                    let status = Status::internal(format!("Failed to read body: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Decode gRPC frame first
            let xline_request = match decode_grpc_frame(&body_bytes) {
                Ok(frame_data) => match XlineRequest::<Input>::decode_from_slice(&frame_data) {
                    Ok(req) => req,
                    Err(e) => {
                        let status = Status::internal(format!("Failed to decode request: {}", e));
                        return Ok(create_error_response(status));
                    }
                },
                Err(e) => {
                    let status = Status::internal(format!("Failed to decode gRPC frame: {}", e));
                    return Ok(create_error_response(status));
                }
            };

            // Call service
            let xline_response = match method.call(xline_request).await {
                Ok(resp) => resp,
                Err(status) => {
                    return Ok(create_error_response(status));
                }
            };

            // Create streaming response body that encodes items incrementally
            let stream = xline_response.into_inner();
            let body = GrpcStreamingBody::<RspStream, Output>::new(stream);

            // Create HTTP response with streaming body
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .body(body)
                .unwrap();

            Ok(response)
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
        MakeClientStreamingSvc {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output> Service<Request<B>>
    for WithEncodingOption<MakeClientStreamingSvc<SVC, Input, Output>>
where
    Input: Message + Default + Send + 'static,
    Output: Message + Default + Send + 'static + Clone,
    SVC: Service<XlineRequest<Input>, Response = XlineResponse<Output>, Error = Status>
        + Clone
        + 'static
        + Send
        + Sync,
    SVC::Future: Send,
    B: Body + Send + 'static,
    B::Error: Into<super::Error> + Send + 'static,
{
    type Response = http::Response<http_body::Full<Bytes>>;
    type Error = std::convert::Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let method = self.svc.inner.clone();
        let fut = async move {
            // Read and decode streaming request
            let body = request.into_body();
            let mut body_bytes = hyper::body::aggregate(body).await
                .map_err(|e| Status::internal(format!("Failed to read body: {}", e)))?;

            let mut all_requests = Vec::new();
            let mut buffer = Vec::new();
            let codec = BinaryCodec::new();

            // Read all bytes from the body
            while let Some(chunk) = body_bytes.data_ref() {
                buffer.extend_from_slice(chunk);
                if let Err(e) = body_bytes.advance(chunk.len()) {
                    return Err(Status::internal(format!("Failed to advance body: {}", e)));
                }
            }

            // Parse multiple frames from the buffer
            let mut pos = 0;
            while pos < buffer.len() {
                // Each frame starts with a 4-byte metadata length
                if pos + 4 > buffer.len() {
                    return Err(Status::internal("Incomplete frame: missing metadata length"));
                }
                
                // Read metadata length to determine frame boundaries
                let meta_len = u32::from_be_bytes([
                    buffer[pos],
                    buffer[pos + 1],
                    buffer[pos + 2],
                    buffer[pos + 3]
                ]) as usize;
                pos += 4;
                
                // Calculate total frame size
                // Minimum frame size is metadata (meta_len) + at least 1 byte of protobuf data
                if pos + meta_len + 1 > buffer.len() {
                    return Err(Status::internal("Incomplete frame: insufficient data"));
                }
                
                // Extract full frame (including the 4-byte meta_len we already read)
                let frame_start = pos - 4;
                let frame_end = pos + meta_len + 1;
                
                // Find the actual end of the frame by looking for the next frame's metadata length
                // This is necessary because protobuf data can be variable length
                let mut next_frame_pos = frame_end;
                while next_frame_pos + 4 <= buffer.len() {
                    let next_meta_len = u32::from_be_bytes([
                        buffer[next_frame_pos],
                        buffer[next_frame_pos + 1],
                        buffer[next_frame_pos + 2],
                        buffer[next_frame_pos + 3]
                    ]) as usize;
                    
                    // Check if this looks like a valid frame start
                    if next_frame_pos + 4 + next_meta_len + 1 <= buffer.len() {
                        break;
                    }
                    next_frame_pos += 1;
                }
                
                // If we didn't find a next frame, this is the last frame
                let actual_frame_end = if next_frame_pos + 4 <= buffer.len() {
                    next_frame_pos
                } else {
                    buffer.len()
                };
                
                // Decode the frame
                let frame = &buffer[frame_start..actual_frame_end];
                let xline_request = XlineRequest::<Input>::decode_from_slice(frame)
                    .map_err(|e| Status::internal(format!("Failed to decode request frame: {}", e)))?;
                
                all_requests.push(xline_request);
                pos = actual_frame_end;
            }

            // For client streaming, we need to decide how to handle multiple requests
            // This implementation processes all requests and returns the response from the last one
            // Depending on the service implementation, this might need to be adjusted
            let mut last_response = None;
            for xline_request in all_requests {
                let xline_response = method.clone().call(xline_request).await
                    .map_err(|e| e)?;
                last_response = Some(xline_response);
            }

            let xline_response = last_response.ok_or_else(|| {
                Status::internal("No requests received in client stream")
            })?;

            // Encode response
            let response_bytes = xline_response.encode_to_vec()
                .map_err(|e| Status::internal(format!("Failed to encode response: {}", e)))?;

            // Wrap response in gRPC frame
            // gRPC frame format: [flags (1 byte)] [length (4 bytes)] [data]
            let mut framed_response = Vec::with_capacity(1 + 4 + response_bytes.len());
            // Write flags (0 for uncompressed)
            framed_response.push(0);
            // Write length (big-endian)
            framed_response.extend_from_slice(&u32::to_be_bytes(response_bytes.len() as u32));
            // Write data
            framed_response.extend_from_slice(&response_bytes);

            // Create HTTP response
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .header("grpc-status", "0") // gRPC OK status code
                .body(http_body::Full::from(Bytes::from(framed_response)))
                .unwrap();

            Ok(response)
        };
        Box::pin(fut)
    }
}