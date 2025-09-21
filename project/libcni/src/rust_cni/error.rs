// Copyright (c) 2024 https://github.com/divinerapier/cni-rs
use std::io;
use std::result;
use thiserror::Error;

// const CODE_UNKNOWN: usize = 0;
// const CODE_INCOMPATIBLE_CNI_VERSION: usize = 1;
// const CODE_UNSUPPORTED_FIELD: usize = 2;
// const CODE_UNKNOWN_CONTAINER: usize = 3;
// const CODE_INVALID_ENVIRONMENT_VARIABLES: usize = 4;
// const CODE_IO_FILURE: usize = 5;
// const CODE_DECODING_FAILURE: usize = 6;
// const CODE_INVALID_NETWORK_CONFIG: usize = 7;
// const CODE_TRY_AGAIN_LATER: usize = 11;
// const CODE_INTERNAL: usize = 999;

#[derive(Debug, Error)]
pub enum CNIError {
    #[error("no net configuration with name {0:?} in {1}")]
    NotFound(String, String),
    #[error("no net configurations found in {0}")]
    NoConfigsFound(String),
    #[error("execute CNI error {0}")]
    ExecuteError(String),
    #[error("Invalid Configuration: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    Io(#[source] Box<io::Error>),
    #[error("Empty key")]
    EmptyKey,
    #[error("{0}")]
    TooLong(String),
    #[error("Invalid checksum")]
    InvalidChecksum(String),
    #[error("Invalid filename")]
    InvalidFilename(String),
    // #[error("Invalid prost data: {0}")]
    // Decode(#[source] Box<prost::DecodeError>),
    #[error("Invalid data: {0}")]
    VarDecode(String),
    #[error("{0}")]
    TableRead(String),
    #[error("Database Closed")]
    DBClosed,
    #[error("{0}")]
    LogRead(String),
}

impl From<io::Error> for CNIError {
    #[inline]
    fn from(e: io::Error) -> CNIError {
        CNIError::Io(Box::new(e))
    }
}

// impl From<prost::DecodeError> for Error {
//     #[inline]
//     fn from(e: prost::DecodeError) -> Error {
//         Error::Decode(Box::new(e))
//     }
// }

pub type Result<T> = result::Result<T, CNIError>;
