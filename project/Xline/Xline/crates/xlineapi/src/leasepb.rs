//! Lease protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// Lease
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Lease {
    #[prost(int64, tag = "1")]
    pub id: i64,
    #[prost(int64, tag = "2")]
    pub ttl: i64,
    #[prost(int64, tag = "3")]
    pub granted_ttl: i64,
    #[prost(int64, tag = "4")]
    pub age: i64,
}