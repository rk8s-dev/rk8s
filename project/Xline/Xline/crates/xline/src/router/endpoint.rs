use crate::router::makesvc::WithEncodingOption;

use super::{
    Router, StateRouter,
    makesvc::{MakeClientStreamingSvc, MakeServerStreamingSvc, MakeStreamingSvc, MakeUnarySvc},
};
use prost::Message;
use tokio_stream::Stream;
use xlinerpc::{Request as XlineRequest, Response as XlineResponse, Status};
use tower::{Service, service_fn};

#[derive(Debug)]
pub struct EndPoint<T> {
    router: StateRouter<T>,
    state: T,
}

impl<T> EndPoint<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new(state: T) -> Self {
        Self {
            router: StateRouter::<T>::new(),
            state,
        }
    }

    pub fn add_unary_service<InputScheme, OutputScheme, SVC>(
        mut self,
        name: &str,
        service: SVC,
    ) -> Self
    where
        SVC: Service<
                XlineRequest<InputScheme>,
                Response = XlineResponse<OutputScheme>,
                Error = Status,
            > + Sync
            + Send
            + Clone
            + 'static,
        SVC::Future: Send,
        InputScheme: 'static + Clone + Default + Message,
        OutputScheme: 'static + Clone + Default + Message,
    {
        self.router = self.router.route_service(
            name,
            WithEncodingOption::<_>::new(MakeUnarySvc::new(service)),
        );
        self
    }

    pub fn add_unary_fn<InputScheme, OutputScheme, F, Fut>(mut self, name: &str, handler: F) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        F: FnMut(T, XlineRequest<InputScheme>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<XlineResponse<OutputScheme>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: XlineRequest<InputScheme>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(WithEncodingOption::<_>::new(MakeUnarySvc::new(
                handler_service,
            ))),
        );
        self
    }

    pub fn add_streaming_fn<InputScheme, OutputScheme, RspStream, F, Fut>(
        mut self,
        name: &str,
        handler: F,
    ) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        RspStream: Stream<Item = Result<OutputScheme, Status>> + Send + 'static,
        F: FnMut(T, XlineRequest<InputScheme>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<XlineResponse<RspStream>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: XlineRequest<InputScheme>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(WithEncodingOption::new(MakeStreamingSvc::new(
                handler_service,
            ))),
        );
        self
    }

    pub fn add_server_streaming_fn<InputScheme, OutputScheme, RspStream, F, Fut>(
        mut self,
        name: &str,
        handler: F,
    ) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        RspStream: Stream<Item = Result<OutputScheme, Status>> + Send + 'static,
        F: FnMut(T, XlineRequest<InputScheme>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<XlineResponse<RspStream>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: XlineRequest<InputScheme>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(WithEncodingOption::new(MakeServerStreamingSvc::new(
                handler_service,
            ))),
        );
        self
    }

    pub fn add_client_streaming_fn<InputScheme, OutputScheme, F, Fut>(
        mut self,
        name: &str,
        handler: F,
    ) -> Self
    where
        InputScheme: Clone + Default + Message + Send + 'static,
        OutputScheme: Clone + Default + Message + Send + 'static,
        F: FnMut(T, XlineRequest<InputScheme>) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<XlineResponse<OutputScheme>, Status>> + Send + 'static,
    {
        let state = self.state.clone();
        let handler_service = service_fn(move |request: XlineRequest<InputScheme>| {
            let mut handler = handler.clone();
            let state = state.clone();
            async move { handler(state.clone(), request).await }
        });

        self.router = self.router.route_service(
            name,
            axum::routing::post_service(WithEncodingOption::new(MakeClientStreamingSvc::new(
                handler_service,
            ))),
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