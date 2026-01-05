use crate::meta::store::MetaError;
use std::io;

#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("not found: {path}")]
    NotFound { path: String },

    #[error("already exists: {path}")]
    AlreadyExists { path: String },

    #[error("not a directory: {path}")]
    NotADirectory { path: String },

    #[error("is a directory: {path}")]
    IsADirectory { path: String },

    #[error("directory not empty: {path}")]
    DirectoryNotEmpty { path: String },

    #[error("not a file: {path}")]
    NotAFile { path: String },

    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("permission denied: {message}")]
    PermissionDenied { message: String },

    #[error("unsupported: {message}")]
    Unsupported { message: String },

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("internal error: {message}")]
    Internal { message: String },
}

impl VfsError {
    pub fn kind(&self) -> io::ErrorKind {
        match self {
            VfsError::NotFound { .. } => io::ErrorKind::NotFound,
            VfsError::AlreadyExists { .. } => io::ErrorKind::AlreadyExists,
            VfsError::NotADirectory { .. } => io::ErrorKind::NotADirectory,
            VfsError::IsADirectory { .. } => io::ErrorKind::IsADirectory,
            VfsError::DirectoryNotEmpty { .. } => io::ErrorKind::DirectoryNotEmpty,
            VfsError::NotAFile { .. } => io::ErrorKind::InvalidInput,
            VfsError::InvalidInput { .. } => io::ErrorKind::InvalidInput,
            VfsError::PermissionDenied { .. } => io::ErrorKind::PermissionDenied,
            VfsError::Unsupported { .. } => io::ErrorKind::Unsupported,
            VfsError::Io(e) => e.kind(),
            VfsError::Internal { .. } => io::ErrorKind::Other,
        }
    }

    pub fn from_meta(path: impl Into<String>, err: MetaError) -> Self {
        let path = path.into();
        match err {
            MetaError::NotFound(_) | MetaError::ParentNotFound(_) => VfsError::NotFound { path },
            MetaError::AlreadyExists { .. } => VfsError::AlreadyExists { path },
            MetaError::NotDirectory(_) => VfsError::NotADirectory { path },
            MetaError::DirectoryNotEmpty(_) => VfsError::DirectoryNotEmpty { path },
            MetaError::InvalidPath(p) => VfsError::InvalidInput {
                message: format!("{path}: invalid path: {p}"),
            },
            MetaError::NotSupported(_) | MetaError::NotImplemented => VfsError::Unsupported {
                message: format!("{path}: {err}"),
            },
            MetaError::Io(e) => VfsError::Io(e),
            other => VfsError::Internal {
                message: format!("{path}: {other}"),
            },
        }
    }
}

impl From<VfsError> for io::Error {
    fn from(err: VfsError) -> Self {
        io::Error::new(err.kind(), err)
    }
}
