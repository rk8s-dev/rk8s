use std::io;

#[cfg(all(not(feature = "tokio-runtime"), feature = "async-io-runtime"))]
pub use async_io::FuseConnection;
#[cfg(all(not(feature = "async-io-runtime"), feature = "tokio-runtime"))]
pub use tokio::FuseConnection;

#[cfg(feature = "async-io-runtime")]
mod async_io;
#[cfg(feature = "tokio-runtime")]
mod tokio;

pub(crate) type CompleteIoResult<T, U> = (T, io::Result<U>);
