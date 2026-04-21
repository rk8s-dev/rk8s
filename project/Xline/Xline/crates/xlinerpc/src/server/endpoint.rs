//! Router endpoint builder for HTTP/3 services
//!
//! Provides a builder pattern for constructing HTTP/3 routers with gRPC services.

use crate::{Request, Response, Status, Streaming};
use prost::Message;
use std::future::Future;
use tokio_stream::Stream;
use tower::{Service, service_fn};

/// Type aliases for axum router
pub type Router = axum::Router;
pub type StateRouter<T> = axum::Router<T>;

/// Endpoint builder for adding gRPC services to a router
#[derive(Debug)]
pub struct EndPoint<T> {
    router: StateRouter<T>,
    state: T,
}

impl<T> EndPoint<T>
where
    T: Clone + Send + Sync + 'static,
{
    /// Create a new endpoint with the given state
    pub fn new(state: T) -> Self {
        Self {
            router: StateRouter::<T>::new(),
            state,
        }
    }

    /// Add a unary service to the router
    pub fn add_unary_service<InputScheme, OutputScheme, SVC>(
        mut self,
        name: &str,
        service: SVC,
    ) -> Self
    where
        SVC: Service<Request<InputScheme>, Response = Response<OutputScheme>, Error = Status>
            + Sync
            + Send
            + Clone
            + 'static,
        SVC::Future: Send,
        InputScheme: 'static + Clone + Default + Message,
        OutputScheme: 'static + Clone + Default + Message,
    {
        self.router = self
            .router
            .route_service(name, super::h3wrapper::MakeUnarySVC::new(service));
        self
    }

    /// Add a unary handler function to the router
    pub fn add_unary_fn<InputScheme, OutputScheme, F, Fut>(mut self, name: &str, handler: F) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        F: FnMut(T, Request<InputScheme>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Response<OutputScheme>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: Request<InputScheme>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(super::h3wrapper::MakeUnarySVC::new(handler_service)),
        );
        self
    }

    /// Add a bidirectional streaming handler to the router
    pub fn add_streaming_fn<InputScheme, OutputScheme, RspStream, F, Fut>(
        mut self,
        name: &str,
        handler: F,
    ) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        RspStream: Stream<Item = Result<OutputScheme, Status>> + Send + 'static,
        F: FnMut(T, Request<Streaming<InputScheme>>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Response<RspStream>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: Request<Streaming<InputScheme>>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(super::h3wrapper::MakeStreamingSvc::new(handler_service)),
        );
        self
    }

    /// Add a server-side streaming handler to the router
    pub fn add_server_streaming_fn<InputScheme, OutputScheme, RspStream, F, Fut>(
        mut self,
        name: &str,
        handler: F,
    ) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        RspStream: Stream<Item = Result<OutputScheme, Status>> + Send + 'static,
        F: FnMut(T, Request<InputScheme>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Response<RspStream>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: Request<InputScheme>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(super::h3wrapper::MakeServerStreamingSvc::new(
                handler_service,
            )),
        );
        self
    }

    /// Add a client-side streaming handler to the router
    pub fn add_client_streaming_fn<InputScheme, OutputScheme, F, Fut>(
        mut self,
        name: &str,
        handler: F,
    ) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        F: FnMut(T, Request<Streaming<InputScheme>>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<Response<OutputScheme>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: Request<Streaming<InputScheme>>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(super::h3wrapper::MakeClientStreamingSvc::new(
                handler_service,
            )),
        );
        self
    }
}

impl<T> From<EndPoint<T>> for Router
where
    T: Clone + Send + Sync + 'static,
{
    fn from(end: EndPoint<T>) -> Router {
        end.router.with_state(end.state)
    }
}

impl<T> Default for EndPoint<T>
where
    T: Clone + Send + Sync + Default + 'static,
{
    fn default() -> Self {
        Self {
            router: StateRouter::<T>::new(),
            state: T::default(),
        }
    }
}
