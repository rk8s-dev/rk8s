//! Auth protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// Permission
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Permission {
    #[prost(enumeration = "permission::Type", tag = "1")]
    pub perm_type: i32,
    #[prost(bytes = "vec", tag = "2")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
}

pub mod permission {
    use super::*;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
    #[repr(i32)]
    pub enum Type {
        Read = 0,
        Write = 1,
        Readwrite = 2,
    }
}

// ============================================================================
// Role
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Role {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub key_permission: ::prost::alloc::vec::Vec<Permission>,
}

// ============================================================================
// User
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct User {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "2")]
    pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "3")]
    pub options: ::core::option::Option<UserAddOptions>,
}

// ============================================================================
// UserAddOptions
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct UserAddOptions {
    #[prost(bool, tag = "1")]
    pub no_password: bool,
}