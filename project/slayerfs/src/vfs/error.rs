//! std::io::Error covers files, sockets, pipes, etc., so we mirror its ErrorKind here
//! and map from io::Error for a unified VFS error surface.

use crate::meta::store::MetaError;
use std::fmt;
use std::io::ErrorKind;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct PathHint(Option<String>);

impl PathHint {
    pub fn none() -> Self {
        Self(None)
    }

    pub fn some(path: impl Into<String>) -> Self {
        Self(Some(path.into()))
    }
}

impl fmt::Display for PathHint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Some(path) if !path.is_empty() => write!(f, ": {path}"),
            _ => Ok(()),
        }
    }
}

impl From<String> for PathHint {
    fn from(value: String) -> Self {
        Self::some(value)
    }
}

impl From<&str> for PathHint {
    fn from(value: &str) -> Self {
        Self::some(value)
    }
}

#[allow(dead_code)]
#[derive(Error, Debug)]
pub enum VfsError {
    // Filesystem/path-related errors (often used with a path hint).
    #[error("not found{path}")]
    NotFound { path: PathHint },

    #[error("already exists{path}")]
    AlreadyExists { path: PathHint },

    #[error("not a directory{path}")]
    NotADirectory { path: PathHint },

    #[error("is a directory{path}")]
    IsADirectory { path: PathHint },

    #[error("directory not empty{path}")]
    DirectoryNotEmpty { path: PathHint },

    #[error("permission denied{path}")]
    PermissionDenied { path: PathHint },

    #[error("read-only filesystem{path}")]
    ReadOnlyFilesystem { path: PathHint },

    // I/O and networking style errors (mirrors std::io::ErrorKind).
    #[error("connection refused")]
    ConnectionRefused,

    #[error("connection reset")]
    ConnectionReset,

    #[error("host unreachable")]
    HostUnreachable,

    #[error("network unreachable")]
    NetworkUnreachable,

    #[error("connection aborted")]
    ConnectionAborted,

    #[error("not connected")]
    NotConnected,

    #[error("address in use")]
    AddrInUse,

    #[error("address not available")]
    AddrNotAvailable,

    #[error("network down")]
    NetworkDown,

    #[error("broken pipe")]
    BrokenPipe,

    #[error("would block")]
    WouldBlock,

    #[error("invalid input")]
    InvalidInput,

    #[error("invalid data")]
    InvalidData,

    #[error("timed out")]
    TimedOut,

    #[error("write zero")]
    WriteZero,

    #[error("storage full")]
    StorageFull,

    #[error("not seekable")]
    NotSeekable,

    #[error("quota exceeded")]
    QuotaExceeded,

    #[error("file too large")]
    FileTooLarge,

    #[error("resource busy")]
    ResourceBusy,

    #[error("executable file busy")]
    ExecutableFileBusy,

    #[error("deadlock")]
    Deadlock,

    #[error("crosses devices")]
    CrossesDevices,

    #[error("circular rename{path}")]
    CircularRename { path: PathHint },

    #[error("invalid rename target{path}")]
    InvalidRenameTarget { path: PathHint },

    #[error("too many links")]
    TooManyLinks,

    #[error("invalid filename")]
    InvalidFilename,

    #[error("argument list too long")]
    ArgumentListTooLong,

    #[error("interrupted")]
    Interrupted,

    #[error("unsupported")]
    Unsupported,

    #[error("unexpected eof")]
    UnexpectedEof,

    #[error("out of memory")]
    OutOfMemory,

    #[error("stale network file handle")]
    StaleNetworkFileHandle,

    #[error("{0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("{0}")]
    Meta(#[from] MetaError),

    #[error("other error")]
    Other,
}

impl From<std::io::Error> for VfsError {
    fn from(value: std::io::Error) -> Self {
        // Map std::io::ErrorKind to VfsError. When no path context is available,
        // use PathHint::none() so formatting stays consistent.
        match value.kind() {
            ErrorKind::NotFound => VfsError::NotFound {
                path: PathHint::none(),
            },
            ErrorKind::AlreadyExists => VfsError::AlreadyExists {
                path: PathHint::none(),
            },
            ErrorKind::NotADirectory => VfsError::NotADirectory {
                path: PathHint::none(),
            },
            ErrorKind::IsADirectory => VfsError::IsADirectory {
                path: PathHint::none(),
            },
            ErrorKind::DirectoryNotEmpty => VfsError::DirectoryNotEmpty {
                path: PathHint::none(),
            },
            ErrorKind::PermissionDenied => VfsError::PermissionDenied {
                path: PathHint::none(),
            },
            ErrorKind::ReadOnlyFilesystem => VfsError::ReadOnlyFilesystem {
                path: PathHint::none(),
            },
            ErrorKind::ConnectionRefused => VfsError::ConnectionRefused,
            ErrorKind::ConnectionReset => VfsError::ConnectionReset,
            ErrorKind::HostUnreachable => VfsError::HostUnreachable,
            ErrorKind::NetworkUnreachable => VfsError::NetworkUnreachable,
            ErrorKind::ConnectionAborted => VfsError::ConnectionAborted,
            ErrorKind::NotConnected => VfsError::NotConnected,
            ErrorKind::AddrInUse => VfsError::AddrInUse,
            ErrorKind::AddrNotAvailable => VfsError::AddrNotAvailable,
            ErrorKind::NetworkDown => VfsError::NetworkDown,
            ErrorKind::BrokenPipe => VfsError::BrokenPipe,
            ErrorKind::WouldBlock => VfsError::WouldBlock,
            ErrorKind::InvalidInput => VfsError::InvalidInput,
            ErrorKind::InvalidData => VfsError::InvalidData,
            ErrorKind::TimedOut => VfsError::TimedOut,
            ErrorKind::WriteZero => VfsError::WriteZero,
            ErrorKind::StorageFull => VfsError::StorageFull,
            ErrorKind::NotSeekable => VfsError::NotSeekable,
            ErrorKind::QuotaExceeded => VfsError::QuotaExceeded,
            ErrorKind::FileTooLarge => VfsError::FileTooLarge,
            ErrorKind::ResourceBusy => VfsError::ResourceBusy,
            ErrorKind::ExecutableFileBusy => VfsError::ExecutableFileBusy,
            ErrorKind::Deadlock => VfsError::Deadlock,
            ErrorKind::CrossesDevices => VfsError::CrossesDevices,
            ErrorKind::TooManyLinks => VfsError::TooManyLinks,
            ErrorKind::InvalidFilename => VfsError::InvalidFilename,
            ErrorKind::ArgumentListTooLong => VfsError::ArgumentListTooLong,
            ErrorKind::Interrupted => VfsError::Interrupted,
            ErrorKind::Unsupported => VfsError::Unsupported,
            ErrorKind::UnexpectedEof => VfsError::UnexpectedEof,
            ErrorKind::OutOfMemory => VfsError::OutOfMemory,
            ErrorKind::StaleNetworkFileHandle => VfsError::StaleNetworkFileHandle,
            ErrorKind::Other => VfsError::Other,
            _ => VfsError::Other,
        }
    }
}
