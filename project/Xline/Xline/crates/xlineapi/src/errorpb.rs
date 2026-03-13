//! Error protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// ExecuteError
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ExecuteError {
    #[prost(oneof = "execute_error::Error", tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10")]
    pub error: ::core::option::Option<execute_error::Error>,
}

pub mod execute_error {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Error {
        #[prost((), tag = "1")]
        KvError(()),
        #[prost((), tag = "2")]
        AuthError(()),
        #[prost((), tag = "3")]
        LeaseError(()),
        #[prost((), tag = "4")]
        WatchError(()),
        #[prost((), tag = "5")]
        LockError(()),
        #[prost((), tag = "6")]
        CompactionError(()),
        #[prost(string, tag = "7")]
        Internal(::prost::alloc::string::String),
        #[prost((), tag = "8")]
        LeaseExpired(i64),
        #[prost((), tag = "9")]
        KeyConflict(()),
        #[prost(message, tag = "10")]
        Revisions(super::Revisions),
    }
}

// ============================================================================
// Revisions
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Revisions {
    #[prost(int64, tag = "1")]
    pub current: i64,
    #[prost(int64, tag = "2")]
    pub compacted: i64,
}

// ============================================================================
// UserRole
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct UserRole {
    #[prost(string, tag = "1")]
    pub user: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub role: ::prost::alloc::string::String,
}