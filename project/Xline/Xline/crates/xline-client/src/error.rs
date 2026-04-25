use curp::cmd::Command as CurpCommand;
use h3;
use http;
use thiserror::Error;
use xlineapi::{command::Command, execute_error::ExecuteError};
use xlinerpc::status::{Code, Status};
/// The result type for `xline-client`
pub type Result<T> = std::result::Result<T, XlineClientError<Command>>;

/// Convert h3 error::StreamError to Status
pub fn h3_stream_error_to_status(e: h3::error::StreamError) -> Status {
    Status::internal(format!("h3 stream error: {e}"))
}

/// Convert http status code to Status result
pub fn http_status_to_result(status: http::StatusCode) -> std::result::Result<(), Status> {
    if status.is_success() {
        return Ok(());
    }

    let code = match status {
        http::StatusCode::BAD_REQUEST => Code::InvalidArgument,
        http::StatusCode::UNAUTHORIZED => Code::Unauthenticated,
        http::StatusCode::FORBIDDEN => Code::PermissionDenied,
        http::StatusCode::NOT_FOUND => Code::NotFound,
        http::StatusCode::TOO_MANY_REQUESTS => Code::ResourceExhausted,
        http::StatusCode::REQUEST_TIMEOUT => Code::DeadlineExceeded,
        http::StatusCode::INTERNAL_SERVER_ERROR => Code::Internal,
        http::StatusCode::SERVICE_UNAVAILABLE => Code::Unavailable,
        http::StatusCode::GATEWAY_TIMEOUT => Code::DeadlineExceeded,
        _ if status.is_client_error() => Code::InvalidArgument,
        _ if status.is_server_error() => Code::Internal,
        _ => Code::Unknown,
    };

    Err(Status::new(code, format!("HTTP {}", status)))
}

/// Error type of client builder
#[allow(clippy::module_name_repetitions)] // this-error generate code false-positive
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum XlineClientBuildError {
    /// Rpc error
    #[error("Rpc error: {0}")]
    RpcError(String),
    /// Invalid arguments
    #[error("Invalid arguments: {0}")]
    InvalidArguments(String),
    /// Authentication error
    #[error("Authenticate error: {0}")]
    AuthError(String),
}

impl XlineClientBuildError {
    /// Create a new `XlineClientBuildError::InvalidArguments`
    #[inline]
    #[must_use]
    pub fn invalid_arguments(msg: &str) -> Self {
        Self::InvalidArguments(msg.to_owned())
    }
}

impl From<Status> for XlineClientBuildError {
    #[inline]
    fn from(e: Status) -> Self {
        Self::RpcError(e.to_string())
    }
}

impl From<curp::rpc::CurpError> for XlineClientBuildError {
    #[inline]
    fn from(e: curp::rpc::CurpError) -> Self {
        Self::RpcError(format!("{e:?}"))
    }
}

/// The error type for `xline-client`
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum XlineClientError<C: CurpCommand> {
    /// Command error
    #[error("command execute error {0}")]
    CommandError(C::Error),
    /// Io error
    #[error("IO error {0}")]
    IoError(String),
    /// RPC error
    #[error("rpc error: {0}")]
    RpcError(String),
    /// Command execution error
    #[error("command execution error: {0}")]
    ExecuteError(ExecuteError),
    /// Arguments invalid error
    #[error("Invalid arguments: {0}")]
    InvalidArgs(String),
    /// Internal Error
    #[error("Client Internal error: {0}")]
    InternalError(String),
    /// Error in watch client
    #[error("Watch client error: {0}")]
    WatchError(String),
    /// Error in lease client
    #[error("Lease client error: {0}")]
    LeaseError(String),
    /// Request Timeout
    #[error("Request timeout")]
    Timeout,
    /// Server is shutting down
    #[error("Curp Server is shutting down")]
    ShuttingDown,
    /// Serialize and Deserialize Error
    #[error("EncodeDecode error: {0}")]
    EncodeDecode(String),
    /// Wrong cluster version
    #[error("Wrong cluster version")]
    WrongClusterVersion,
}

impl From<Status> for XlineClientError<Command> {
    #[inline]
    fn from(e: Status) -> Self {
        Self::RpcError(e.to_string())
    }
}

impl From<ExecuteError> for XlineClientError<Command> {
    #[inline]
    fn from(e: ExecuteError) -> Self {
        Self::ExecuteError(e)
    }
}
