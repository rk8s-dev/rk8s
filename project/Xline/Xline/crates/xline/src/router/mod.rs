pub(crate) mod endpoint;
pub(crate) mod h3wrapper;
pub(crate) mod makesvc;
pub(crate) mod server;

pub(crate) type Router = axum::Router;
pub(crate) type StateRouter<T> = axum::Router<T>;
pub(crate) type Error = Box<dyn std::error::Error + Send + Sync>;

pub(crate) use http::HeaderValue;
pub(crate) use http_body::Body;
pub(crate) use server::{RouterBuilder, Server};

// use std::{
// task::{Context, Poll},
// pin::Pin,
// };
// use pin_project::pin_project;
//
// // From `futures-util` crate, borrowed since this is the only dependency tonic requires.
// // LICENSE: MIT or Apache-2.0
// // A future which only yields `Poll::Ready` once, and thereafter yields `Poll::Pending`.
// #[pin_project]
// struct Fuse<F> {
//     #[pin]
//     inner: Option<F>,
// }
//
// impl<F> Future for Fuse<F>
// where
//     F: Future,
// {
//     type Output = F::Output;
//
//     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
//         match self.as_mut().project().inner.as_pin_mut() {
//             Some(fut) => fut.poll(cx).map(|output| {
//                 self.project().inner.set(None);
//                 output
//             }),
//             None => Poll::Pending,
//         }
//     }
// }
