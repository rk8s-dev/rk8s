use http::Request;
use http_body::Body;
use prost::Message;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio_stream::Stream;
use xlinerpc::{Request as XlineRequest, Response as XlineResponse, Status, MetaData, BinaryCodec};
use tower::Service;
use bytes::Bytes;

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
    // pub(crate) fn max_encoding_message_size(mut self, limit: usize) -> Self {
    //     self.max_encoding_message_size = Some(limit);
    //     self
    // }
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
        MakeUnarySVC {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output> Service<Request<B>>
    for WithEncodingOption<MakeUnarySVC<SVC, Input, Output>>
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
            // Read body
            let body_bytes = hyper::body::to_bytes(request.into_body()).await
                .map_err(|e| Status::internal(format!("Failed to read body: {}", e)))?;

            // Decode request
            let xline_request = XlineRequest::<Input>::decode_from_slice(&body_bytes)
                .map_err(|e| Status::internal(format!("Failed to decode request: {}", e)))?;

            // Call service
            let xline_response = method.call(xline_request).await
                .map_err(|e| e)?;

            // Encode response
            let response_bytes = xline_response.encode_to_vec()
                .map_err(|e| Status::internal(format!("Failed to encode response: {}", e)))?;

            // Create HTTP response
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .body(http_body::Full::from(Bytes::from(response_bytes)))
                .unwrap();

            Ok(response)
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
        MakeStreamingSvc {
            inner: service,
            _1: std::marker::PhantomData,
            _2: std::marker::PhantomData,
        }
    }
}

impl<B, SVC, Input, Output, RspStream> Service<Request<B>>
    for WithEncodingOption<MakeStreamingSvc<SVC, Input, Output>>
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
    type Response = http::Response<http_body::Full<Bytes>>;
    type Error = std::convert::Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let method = self.svc.inner.clone();
        let fut = async move {
            // Read body
            let body_bytes = hyper::body::to_bytes(request.into_body()).await
                .map_err(|e| Status::internal(format!("Failed to read body: {}", e)))?;

            // Decode request
            let xline_request = XlineRequest::<Input>::decode_from_slice(&body_bytes)
                .map_err(|e| Status::internal(format!("Failed to decode request: {}", e)))?;

            // Call service
            let xline_response = method.call(xline_request).await
                .map_err(|e| e)?;

            // For streaming response, we need to handle it differently
            // Collect all streaming responses and encode them
            let mut response_bytes = Vec::new();
            let mut stream = xline_response.into_inner();
            
            while let Some(item) = stream.next().await {
                match item {
                    Ok(output) => {
                        let mut encoded = output.encode_to_vec();
                        response_bytes.extend_from_slice(&encoded);
                    }
                    Err(e) => {
                        return Err(Status::internal(format!("Streaming response error: {}", e)));
                    }
                }
            }

            // Create HTTP response
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .body(http_body::Full::from(Bytes::from(response_bytes)))
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
    type Response = http::Response<http_body::Full<Bytes>>;
    type Error = std::convert::Infallible;
    type Future = futures::future::BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: Request<B>) -> Self::Future {
        let method = self.svc.inner.clone();
        let fut = async move {
            // Read body
            let body_bytes = hyper::body::to_bytes(request.into_body()).await
                .map_err(|e| Status::internal(format!("Failed to read body: {}", e)))?;

            // Decode request
            let xline_request = XlineRequest::<Input>::decode_from_slice(&body_bytes)
                .map_err(|e| Status::internal(format!("Failed to decode request: {}", e)))?;

            // Call service
            let xline_response = method.call(xline_request).await
                .map_err(|e| e)?;

            // For server streaming response, we need to handle it differently
            // Collect all streaming responses and encode them
            let mut response_bytes = Vec::new();
            let mut stream = xline_response.into_inner();
            
            while let Some(item) = stream.next().await {
                match item {
                    Ok(output) => {
                        let mut encoded = output.encode_to_vec();
                        response_bytes.extend_from_slice(&encoded);
                    }
                    Err(e) => {
                        return Err(Status::internal(format!("Server streaming response error: {}", e)));
                    }
                }
            }

            // Create HTTP response
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .body(http_body::Full::from(Bytes::from(response_bytes)))
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
            // Read body
            let body_bytes = hyper::body::to_bytes(request.into_body()).await
                .map_err(|e| Status::internal(format!("Failed to read body: {}", e)))?;

            // Decode request
            let xline_request = XlineRequest::<Input>::decode_from_slice(&body_bytes)
                .map_err(|e| Status::internal(format!("Failed to decode request: {}", e)))?;

            // Call service
            let xline_response = method.call(xline_request).await
                .map_err(|e| e)?;

            // Encode response
            let response_bytes = xline_response.encode_to_vec()
                .map_err(|e| Status::internal(format!("Failed to encode response: {}", e)))?;

            // Create HTTP response
            let response = http::Response::builder()
                .status(http::StatusCode::OK)
                .header(http::header::CONTENT_TYPE, "application/grpc")
                .body(http_body::Full::from(Bytes::from(response_bytes)))
                .unwrap();

            Ok(response)
        };
        Box::pin(fut)
    }
}