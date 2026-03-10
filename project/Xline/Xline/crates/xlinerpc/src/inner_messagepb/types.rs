//! CURP inner message types

use prost::Message;
use serde::{Deserialize, Serialize};

/// Append Entries Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AppendEntriesRequest {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(uint64, tag = "2")]
    pub leader_id: u64,
    #[prost(uint64, tag = "3")]
    pub prev_log_index: u64,
    #[prost(uint64, tag = "4")]
    pub prev_log_term: u64,
    #[prost(bytes = "vec", repeated, tag = "5")]
    pub entries: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    #[prost(uint64, tag = "6")]
    pub leader_commit: u64,
}

/// Append Entries Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AppendEntriesResponse {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(bool, tag = "2")]
    pub success: bool,
    #[prost(uint64, tag = "3")]
    pub hint_index: u64,
}

/// Vote Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct VoteRequest {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(uint64, tag = "2")]
    pub candidate_id: u64,
    #[prost(uint64, tag = "3")]
    pub last_log_index: u64,
    #[prost(uint64, tag = "4")]
    pub last_log_term: u64,
    #[prost(bool, tag = "5")]
    pub is_pre_vote: bool,
}

/// Vote Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct VoteResponse {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(bool, tag = "2")]
    pub vote_granted: bool,
    #[prost(bytes = "vec", repeated, tag = "3")]
    pub spec_pool: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
    #[prost(bool, tag = "4")]
    pub shutdown_candidate: bool,
}

/// Install Snapshot Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct InstallSnapshotRequest {
    #[prost(uint64, tag = "1")]
    pub term: u64,
    #[prost(uint64, tag = "2")]
    pub leader_id: u64,
    #[prost(uint64, tag = "3")]
    pub last_included_index: u64,
    #[prost(uint64, tag = "4")]
    pub last_included_term: u64,
    #[prost(bytes = "vec", tag = "5")]
    pub data: ::prost::alloc::vec::Vec<u8>,
    #[prost(bool, tag = "6")]
    pub done: bool,
}

/// Install Snapshot Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct InstallSnapshotResponse {
    #[prost(uint64, tag = "1")]
    pub term: u64,
}

/// Trigger Shutdown Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TriggerShutdownRequest {}

/// Trigger Shutdown Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TriggerShutdownResponse {}

/// Try Become Leader Now Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TryBecomeLeaderNowRequest {}

/// Try Become Leader Now Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TryBecomeLeaderNowResponse {}