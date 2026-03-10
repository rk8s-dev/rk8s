//! MVCC protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// KeyValue
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct KeyValue {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "2")]
    pub create_revision: i64,
    #[prost(int64, tag = "3")]
    pub mod_revision: i64,
    #[prost(int64, tag = "4")]
    pub version: i64,
    #[prost(bytes = "vec", tag = "5")]
    pub value: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "6")]
    pub lease: i64,
}

// ============================================================================
// Event
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Event {
    #[prost(enumeration = "event::EventType", tag = "1")]
    pub r#type: i32,
    #[prost(message, optional, tag = "2")]
    pub kv: ::core::option::Option<KeyValue>,
    #[prost(message, optional, tag = "3")]
    pub prev_kv: ::core::option::Option<KeyValue>,
}

pub mod event {
    use super::*;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
    #[repr(i32)]
    pub enum EventType {
        Put = 0,
        Delete = 1,
    }
}