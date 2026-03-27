// Hand-maintained curp Protocol service definitions for the xline gRPC boundary layer.
//
// This module was extracted from tonic_build codegen output for curp-command.proto.
// It is now maintained manually to avoid depending on tonic_build codegen.
// The Protocol trait and ProtocolServer are used ONLY at the xline gRPC boundary
// (AuthWrapper) to expose curp RPCs on the client-facing tonic endpoint.
//
// Message types are imported from curp::rpc (prost-generated, no tonic dependency).

#[allow(
    clippy::all,
    clippy::restriction,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    unused_qualifications,
    unreachable_pub,
    variant_size_differences,
    missing_copy_implementations,
    missing_docs,
    trivial_casts,
    unused_results
)]
pub(crate) mod commandpb {
    pub mod protocol_server {
        #![allow(
            unused_variables,
            dead_code,
            missing_docs,
            clippy::wildcard_imports,
            clippy::let_unit_value
        )]
        use tonic::codegen::*;
        /// Generated trait containing gRPC methods that should be implemented for
        /// use with ProtocolServer.
        #[async_trait]
        pub trait Protocol: std::marker::Send + std::marker::Sync + 'static {
            /// Server streaming response type for the ProposeStream method.
            type ProposeStreamStream: tonic::codegen::tokio_stream::Stream<
                    Item = std::result::Result<::curp::rpc::OpResponse, tonic::Status>,
                > + std::marker::Send
                + 'static;
            /// Unary
            async fn propose_stream(
                &self,
                request: tonic::Request<::curp::rpc::ProposeRequest>,
            ) -> std::result::Result<tonic::Response<Self::ProposeStreamStream>, tonic::Status>;
            async fn record(
                &self,
                request: tonic::Request<::curp::rpc::RecordRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::RecordResponse>, tonic::Status>;
            async fn read_index(
                &self,
                request: tonic::Request<::curp::rpc::ReadIndexRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::ReadIndexResponse>, tonic::Status>;
            async fn propose_conf_change(
                &self,
                request: tonic::Request<::curp::rpc::ProposeConfChangeRequest>,
            ) -> std::result::Result<
                tonic::Response<::curp::rpc::ProposeConfChangeResponse>,
                tonic::Status,
            >;
            async fn publish(
                &self,
                request: tonic::Request<::curp::rpc::PublishRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::PublishResponse>, tonic::Status>;
            async fn shutdown(
                &self,
                request: tonic::Request<::curp::rpc::ShutdownRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::ShutdownResponse>, tonic::Status>;
            async fn fetch_cluster(
                &self,
                request: tonic::Request<::curp::rpc::FetchClusterRequest>,
            ) -> std::result::Result<
                tonic::Response<::curp::rpc::FetchClusterResponse>,
                tonic::Status,
            >;
            async fn fetch_read_state(
                &self,
                request: tonic::Request<::curp::rpc::FetchReadStateRequest>,
            ) -> std::result::Result<
                tonic::Response<::curp::rpc::FetchReadStateResponse>,
                tonic::Status,
            >;
            async fn move_leader(
                &self,
                request: tonic::Request<::curp::rpc::MoveLeaderRequest>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::MoveLeaderResponse>, tonic::Status>;
            /// Stream
            async fn lease_keep_alive(
                &self,
                request: tonic::Request<tonic::Streaming<::curp::rpc::LeaseKeepAliveMsg>>,
            ) -> std::result::Result<tonic::Response<::curp::rpc::LeaseKeepAliveMsg>, tonic::Status>;
        }
        #[derive(Debug)]
        pub struct ProtocolServer<T> {
            inner: Arc<T>,
            accept_compression_encodings: EnabledCompressionEncodings,
            send_compression_encodings: EnabledCompressionEncodings,
            max_decoding_message_size: Option<usize>,
            max_encoding_message_size: Option<usize>,
        }
        impl<T> ProtocolServer<T> {
            pub fn new(inner: T) -> Self {
                Self::from_arc(Arc::new(inner))
            }
            pub fn from_arc(inner: Arc<T>) -> Self {
                Self {
                    inner,
                    accept_compression_encodings: Default::default(),
                    send_compression_encodings: Default::default(),
                    max_decoding_message_size: None,
                    max_encoding_message_size: None,
                }
            }
            pub fn with_interceptor<F>(inner: T, interceptor: F) -> InterceptedService<Self, F>
            where
                F: tonic::service::Interceptor,
            {
                InterceptedService::new(Self::new(inner), interceptor)
            }
            /// Enable decompressing requests with the given encoding.
            #[must_use]
            pub fn accept_compressed(mut self, encoding: CompressionEncoding) -> Self {
                self.accept_compression_encodings.enable(encoding);
                self
            }
            /// Compress responses with the given encoding, if the client supports it.
            #[must_use]
            pub fn send_compressed(mut self, encoding: CompressionEncoding) -> Self {
                self.send_compression_encodings.enable(encoding);
                self
            }
            /// Limits the maximum size of a decoded message.
            ///
            /// Default: `4MB`
            #[must_use]
            pub fn max_decoding_message_size(mut self, limit: usize) -> Self {
                self.max_decoding_message_size = Some(limit);
                self
            }
            /// Limits the maximum size of an encoded message.
            ///
            /// Default: `usize::MAX`
            #[must_use]
            pub fn max_encoding_message_size(mut self, limit: usize) -> Self {
                self.max_encoding_message_size = Some(limit);
                self
            }
        }
        impl<T, B> tonic::codegen::Service<http::Request<B>> for ProtocolServer<T>
        where
            T: Protocol,
            B: Body + std::marker::Send + 'static,
            B::Error: Into<StdError> + std::marker::Send + 'static,
        {
            type Response = http::Response<tonic::body::BoxBody>;
            type Error = std::convert::Infallible;
            type Future = BoxFuture<Self::Response, Self::Error>;
            fn poll_ready(
                &mut self,
                _cx: &mut Context<'_>,
            ) -> Poll<std::result::Result<(), Self::Error>> {
                Poll::Ready(Ok(()))
            }
            fn call(&mut self, req: http::Request<B>) -> Self::Future {
                match req.uri().path() {
                    "/commandpb.Protocol/ProposeStream" => {
                        #[allow(non_camel_case_types)]
                        struct ProposeStreamSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol>
                            tonic::server::ServerStreamingService<::curp::rpc::ProposeRequest>
                            for ProposeStreamSvc<T>
                        {
                            type Response = ::curp::rpc::OpResponse;
                            type ResponseStream = T::ProposeStreamStream;
                            type Future =
                                BoxFuture<tonic::Response<Self::ResponseStream>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::ProposeRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::propose_stream(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = ProposeStreamSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.server_streaming(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/Record" => {
                        #[allow(non_camel_case_types)]
                        struct RecordSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol> tonic::server::UnaryService<::curp::rpc::RecordRequest> for RecordSvc<T> {
                            type Response = ::curp::rpc::RecordResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::RecordRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut =
                                    async move { <T as Protocol>::record(&inner, request).await };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = RecordSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/ReadIndex" => {
                        #[allow(non_camel_case_types)]
                        struct ReadIndexSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol> tonic::server::UnaryService<::curp::rpc::ReadIndexRequest> for ReadIndexSvc<T> {
                            type Response = ::curp::rpc::ReadIndexResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::ReadIndexRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::read_index(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = ReadIndexSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/ProposeConfChange" => {
                        #[allow(non_camel_case_types)]
                        struct ProposeConfChangeSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol>
                            tonic::server::UnaryService<::curp::rpc::ProposeConfChangeRequest>
                            for ProposeConfChangeSvc<T>
                        {
                            type Response = ::curp::rpc::ProposeConfChangeResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::ProposeConfChangeRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::propose_conf_change(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = ProposeConfChangeSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/Publish" => {
                        #[allow(non_camel_case_types)]
                        struct PublishSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol> tonic::server::UnaryService<::curp::rpc::PublishRequest> for PublishSvc<T> {
                            type Response = ::curp::rpc::PublishResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::PublishRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut =
                                    async move { <T as Protocol>::publish(&inner, request).await };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = PublishSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/Shutdown" => {
                        #[allow(non_camel_case_types)]
                        struct ShutdownSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol> tonic::server::UnaryService<::curp::rpc::ShutdownRequest> for ShutdownSvc<T> {
                            type Response = ::curp::rpc::ShutdownResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::ShutdownRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut =
                                    async move { <T as Protocol>::shutdown(&inner, request).await };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = ShutdownSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/FetchCluster" => {
                        #[allow(non_camel_case_types)]
                        struct FetchClusterSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol>
                            tonic::server::UnaryService<::curp::rpc::FetchClusterRequest>
                            for FetchClusterSvc<T>
                        {
                            type Response = ::curp::rpc::FetchClusterResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::FetchClusterRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::fetch_cluster(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = FetchClusterSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/FetchReadState" => {
                        #[allow(non_camel_case_types)]
                        struct FetchReadStateSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol>
                            tonic::server::UnaryService<::curp::rpc::FetchReadStateRequest>
                            for FetchReadStateSvc<T>
                        {
                            type Response = ::curp::rpc::FetchReadStateResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::FetchReadStateRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::fetch_read_state(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = FetchReadStateSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/MoveLeader" => {
                        #[allow(non_camel_case_types)]
                        struct MoveLeaderSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol>
                            tonic::server::UnaryService<::curp::rpc::MoveLeaderRequest>
                            for MoveLeaderSvc<T>
                        {
                            type Response = ::curp::rpc::MoveLeaderResponse;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<::curp::rpc::MoveLeaderRequest>,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::move_leader(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = MoveLeaderSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.unary(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    "/commandpb.Protocol/LeaseKeepAlive" => {
                        #[allow(non_camel_case_types)]
                        struct LeaseKeepAliveSvc<T: Protocol>(pub Arc<T>);
                        impl<T: Protocol>
                            tonic::server::ClientStreamingService<::curp::rpc::LeaseKeepAliveMsg>
                            for LeaseKeepAliveSvc<T>
                        {
                            type Response = ::curp::rpc::LeaseKeepAliveMsg;
                            type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;
                            fn call(
                                &mut self,
                                request: tonic::Request<
                                    tonic::Streaming<::curp::rpc::LeaseKeepAliveMsg>,
                                >,
                            ) -> Self::Future {
                                let inner = Arc::clone(&self.0);
                                let fut = async move {
                                    <T as Protocol>::lease_keep_alive(&inner, request).await
                                };
                                Box::pin(fut)
                            }
                        }
                        let accept_compression_encodings = self.accept_compression_encodings;
                        let send_compression_encodings = self.send_compression_encodings;
                        let max_decoding_message_size = self.max_decoding_message_size;
                        let max_encoding_message_size = self.max_encoding_message_size;
                        let inner = self.inner.clone();
                        let fut = async move {
                            let method = LeaseKeepAliveSvc(inner);
                            let codec = tonic::codec::ProstCodec::default();
                            let mut grpc = tonic::server::Grpc::new(codec)
                                .apply_compression_config(
                                    accept_compression_encodings,
                                    send_compression_encodings,
                                )
                                .apply_max_message_size_config(
                                    max_decoding_message_size,
                                    max_encoding_message_size,
                                );
                            let res = grpc.client_streaming(method, req).await;
                            Ok(res)
                        };
                        Box::pin(fut)
                    }
                    _ => Box::pin(async move {
                        let mut response = http::Response::new(empty_body());
                        let headers = response.headers_mut();
                        headers.insert(
                            tonic::Status::GRPC_STATUS,
                            (tonic::Code::Unimplemented as i32).into(),
                        );
                        headers.insert(
                            http::header::CONTENT_TYPE,
                            tonic::metadata::GRPC_CONTENT_TYPE,
                        );
                        Ok(response)
                    }),
                }
            }
        }
        impl<T> Clone for ProtocolServer<T> {
            fn clone(&self) -> Self {
                let inner = self.inner.clone();
                Self {
                    inner,
                    accept_compression_encodings: self.accept_compression_encodings,
                    send_compression_encodings: self.send_compression_encodings,
                    max_decoding_message_size: self.max_decoding_message_size,
                    max_encoding_message_size: self.max_encoding_message_size,
                }
            }
        }
        /// Generated gRPC service name
        pub const SERVICE_NAME: &str = "commandpb.Protocol";
        impl<T> tonic::server::NamedService for ProtocolServer<T> {
            const NAME: &'static str = SERVICE_NAME;
        }
    }
}
