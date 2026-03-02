//! Status types for RPC errors
//!
//! Simplified version of gRPC status, containing only commonly-used functionality.

use std::{error::Error, fmt};

use bytes::Bytes;

/// RPC status representing success or various error conditions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// The gRPC status code
    code: Code,
    /// Error message
    message: String,
    /// Binary details payload (e.g. protobuf-encoded error wrapper)
    details: Bytes,
}

/// gRPC status codes
///
/// These match the [gRPC status codes specification](https://github.com/grpc/grpc/blob/master/doc/statuscodes.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    /// The operation completed successfully.
    Ok = 0,
    /// The operation was cancelled.
    Cancelled = 1,
    /// Unknown error.
    Unknown = 2,
    /// Client specified an invalid argument.
    InvalidArgument = 3,
    /// Deadline expired before operation could complete.
    DeadlineExceeded = 4,
    /// Some requested entity was not found.
    NotFound = 5,
    /// Some entity that we attempted to create already exists.
    AlreadyExists = 6,
    /// The caller does not have permission to execute the specified operation.
    PermissionDenied = 7,
    /// Some resource has been exhausted.
    ResourceExhausted = 8,
    /// The system is not in a state required for the operation's execution.
    FailedPrecondition = 9,
    /// The operation was aborted.
    Aborted = 10,
    /// Operation was attempted past the valid range.
    OutOfRange = 11,
    /// Operation is not implemented or not supported.
    Unimplemented = 12,
    /// Internal error.
    Internal = 13,
    /// The service is currently unavailable.
    Unavailable = 14,
    /// Unrecoverable data loss or corruption.
    DataLoss = 15,
    /// The request does not have valid authentication credentials
    Unauthenticated = 16,
}

impl Code {
    /// Get human-readable description of this code
    #[must_use]
    pub const fn description(&self) -> &'static str {
        match self {
            Code::Ok => "The operation completed successfully",
            Code::Cancelled => "The operation was cancelled",
            Code::Unknown => "Unknown error",
            Code::InvalidArgument => "Client specified an invalid argument",
            Code::DeadlineExceeded => "Deadline expired before operation could complete",
            Code::NotFound => "Some requested entity was not found",
            Code::AlreadyExists => "Some entity that we attempted to create already exists",
            Code::PermissionDenied => {
                "The caller does not have permission to execute the specified operation"
            }
            Code::ResourceExhausted => "Some resource has been exhausted",
            Code::FailedPrecondition => {
                "The system is not in a state required for the operation's execution"
            }
            Code::Aborted => "The operation was aborted",
            Code::OutOfRange => "Operation was attempted past the valid range",
            Code::Unimplemented => "Operation is not implemented or not supported",
            Code::Internal => "Internal error",
            Code::Unavailable => "The service is currently unavailable",
            Code::DataLoss => "Unrecoverable data loss or corruption",
            Code::Unauthenticated => "The request does not have valid authentication credentials",
        }
    }

    /// Convert integer to Code
    #[must_use]
    pub const fn from_i32(i: i32) -> Code {
        match i {
            0 => Code::Ok,
            1 => Code::Cancelled,
            2 => Code::Unknown,
            3 => Code::InvalidArgument,
            4 => Code::DeadlineExceeded,
            5 => Code::NotFound,
            6 => Code::AlreadyExists,
            7 => Code::PermissionDenied,
            8 => Code::ResourceExhausted,
            9 => Code::FailedPrecondition,
            10 => Code::Aborted,
            11 => Code::OutOfRange,
            12 => Code::Unimplemented,
            13 => Code::Internal,
            14 => Code::Unavailable,
            15 => Code::DataLoss,
            16 => Code::Unauthenticated,
            _ => Code::Unknown,
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl From<i32> for Code {
    fn from(i: i32) -> Self {
        Code::from_i32(i)
    }
}

impl From<Code> for i32 {
    fn from(code: Code) -> i32 {
        code as i32
    }
}

impl Status {
    /// Create a new `Status` with the associated code and message
    #[must_use]
    pub fn new(code: Code, message: impl Into<String>) -> Status {
        Status {
            code,
            message: message.into(),
            details: Bytes::new(),
        }
    }

    /// Get the gRPC `Code` of this `Status`
    #[must_use]
    pub const fn code(&self) -> Code {
        self.code
    }

    /// Get the text error message of this `Status`
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Create a new `Status` with the associated code, message, and binary details payload
    #[must_use]
    pub fn with_details(code: Code, message: impl Into<String>, details: Bytes) -> Status {
        Status {
            code,
            message: message.into(),
            details,
        }
    }

    /// Get the binary details payload of this `Status`
    #[must_use]
    pub fn details(&self) -> &[u8] {
        &self.details
    }

    // === Commonly-used constructor methods ===

    /// Create a new `Status` representing a successful operation
    #[must_use]
    pub fn ok() -> Status {
        Status::new(Code::Ok, "")
    }

    /// The operation was cancelled (typically by the caller).
    #[must_use]
    pub fn cancelled(message: impl Into<String>) -> Status {
        Status::new(Code::Cancelled, message)
    }

    /// Unknown error.
    #[must_use]
    pub fn unknown(message: impl Into<String>) -> Status {
        Status::new(Code::Unknown, message)
    }

    /// Client specified an invalid argument.
    #[must_use]
    pub fn invalid_argument(message: impl Into<String>) -> Status {
        Status::new(Code::InvalidArgument, message)
    }

    /// Deadline expired before operation could complete.
    #[must_use]
    pub fn deadline_exceeded(message: impl Into<String>) -> Status {
        Status::new(Code::DeadlineExceeded, message)
    }

    /// Some requested entity (e.g., file or directory) was not found.
    #[must_use]
    pub fn not_found(message: impl Into<String>) -> Status {
        Status::new(Code::NotFound, message)
    }

    /// Some entity that we attempted to create already exists.
    #[must_use]
    pub fn already_exists(message: impl Into<String>) -> Status {
        Status::new(Code::AlreadyExists, message)
    }

    /// The caller does not have permission to execute the specified operation.
    #[must_use]
    pub fn permission_denied(message: impl Into<String>) -> Status {
        Status::new(Code::PermissionDenied, message)
    }

    /// Some resource has been exhausted (e.g., per-user quota, disk space).
    #[must_use]
    pub fn resource_exhausted(message: impl Into<String>) -> Status {
        Status::new(Code::ResourceExhausted, message)
    }

    /// Operation was rejected because the system is not in a state required for execution.
    #[must_use]
    pub fn failed_precondition(message: impl Into<String>) -> Status {
        Status::new(Code::FailedPrecondition, message)
    }

    /// The operation was aborted, typically due to a concurrency issue.
    #[must_use]
    pub fn aborted(message: impl Into<String>) -> Status {
        Status::new(Code::Aborted, message)
    }

    /// Operation was attempted past the valid range.
    #[must_use]
    pub fn out_of_range(message: impl Into<String>) -> Status {
        Status::new(Code::OutOfRange, message)
    }

    /// Operation is not implemented or not supported/enabled in this service.
    #[must_use]
    pub fn unimplemented(message: impl Into<String>) -> Status {
        Status::new(Code::Unimplemented, message)
    }

    /// Internal errors. Something is very broken.
    #[must_use]
    pub fn internal(message: impl Into<String>) -> Status {
        Status::new(Code::Internal, message)
    }

    /// The service is currently unavailable. This is likely a transient condition.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Status {
        Status::new(Code::Unavailable, message)
    }

    /// Unrecoverable data loss or corruption.
    #[must_use]
    pub fn data_loss(message: impl Into<String>) -> Status {
        Status::new(Code::DataLoss, message)
    }

    /// The request does not have valid authentication credentials.
    #[must_use]
    pub fn unauthenticated(message: impl Into<String>) -> Status {
        Status::new(Code::Unauthenticated, message)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "status: {}, message: {}", self.code, self.message)
    }
}

impl Error for Status {}

// TODO: remove this after curp and xline-api build without tonic.
// Conversion from tonic::Status
impl From<tonic::Status> for Status {
    fn from(s: tonic::Status) -> Self {
        let code = Code::from_i32(s.code() as i32);
        let details = bytes::Bytes::copy_from_slice(s.details());
        Status::with_details(code, s.message(), details)
    }
}

impl From<Status> for tonic::Status {
    fn from(s: Status) -> Self {
        let code = tonic::Code::from(s.code as i32);
        if s.details.is_empty() {
            tonic::Status::new(code, s.message)
        } else {
            tonic::Status::with_details(code, s.message, s.details)
        }
    }
}

// Conversion from std::io::Error
impl From<std::io::Error> for Status {
    fn from(err: std::io::Error) -> Self {
        use std::io::ErrorKind;
        let code = match err.kind() {
            ErrorKind::BrokenPipe
            | ErrorKind::WouldBlock
            | ErrorKind::WriteZero
            | ErrorKind::Interrupted => Code::Internal,
            ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::NotConnected
            | ErrorKind::AddrInUse
            | ErrorKind::AddrNotAvailable => Code::Unavailable,
            ErrorKind::AlreadyExists => Code::AlreadyExists,
            ErrorKind::ConnectionAborted => Code::Aborted,
            ErrorKind::InvalidData => Code::DataLoss,
            ErrorKind::InvalidInput => Code::InvalidArgument,
            ErrorKind::NotFound => Code::NotFound,
            ErrorKind::PermissionDenied => Code::PermissionDenied,
            ErrorKind::TimedOut => Code::DeadlineExceeded,
            ErrorKind::UnexpectedEof => Code::OutOfRange,
            _ => Code::Unknown,
        };
        Status::new(code, err.to_string())
    }
}
