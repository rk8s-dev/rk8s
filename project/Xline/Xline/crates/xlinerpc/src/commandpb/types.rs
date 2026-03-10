//! CURP protocol message types

use prost::Message;
use serde::{Deserialize, Serialize};

/// Propose ID
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ProposeId {
    #[prost(uint64, tag = "1")]
    pub client_id: u64,
    #[prost(uint64, tag = "2")]
    pub seq_num: u64,
}

/// Propose Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ProposeRequest {
    #[prost(message, optional, tag = "1")]
    pub propose_id: ::core::option::Option<ProposeId>,
    #[prost(bytes = "vec", tag = "2")]
    pub command: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag = "3")]
    pub cluster_version: u64,
    #[prost(uint64, tag = "4")]
    pub term: u64,
    #[prost(bool, tag = "5")]
    pub slow_path: bool,
    #[prost(uint64, tag = "6")]
    pub first_incomplete: u64,
}

/// Propose Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ProposeResponse {
    #[prost(message, optional, tag = "1")]
    pub result: ::core::option::Option<CmdResult>,
    #[prost(bool, tag = "2")]
    pub conflict: bool,
}

/// Command Result
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct CmdResult {
    #[prost(oneof = "cmd_result::Result", tags = "1, 2")]
    pub result: ::core::option::Option<cmd_result::Result>,
}

pub mod cmd_result {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Result {
        #[prost(bytes, tag = "1")]
        Ok(::prost::alloc::vec::Vec<u8>),
        #[prost(bytes, tag = "2")]
        Error(::prost::alloc::vec::Vec<u8>),
    }
}

/// Record Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RecordRequest {
    #[prost(message, optional, tag = "1")]
    pub propose_id: ::core::option::Option<ProposeId>,
    #[prost(bytes = "vec", tag = "2")]
    pub command: ::prost::alloc::vec::Vec<u8>,
}

/// Record Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RecordResponse {}

/// Synced Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct SyncedResponse {
    #[prost(message, optional, tag = "1")]
    pub after_sync_result: ::core::option::Option<CmdResult>,
}

/// Fetch Cluster Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct FetchClusterRequest {
    #[prost(uint64, tag = "1")]
    pub cluster_version: u64,
}

/// Member
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Member {
    #[prost(uint64, tag = "1")]
    pub id: u64,
    #[prost(string, repeated, tag = "2")]
    pub peer_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, repeated, tag = "3")]
    pub client_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(bool, tag = "4")]
    pub is_learner: bool,
}

/// Fetch Cluster Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct FetchClusterResponse {
    #[prost(uint64, optional, tag = "1")]
    pub leader_id: ::core::option::Option<u64>,
    #[prost(uint64, tag = "2")]
    pub term: u64,
    #[prost(uint64, tag = "3")]
    pub cluster_id: u64,
    #[prost(message, repeated, tag = "4")]
    pub members: ::prost::alloc::vec::Vec<Member>,
    #[prost(uint64, tag = "5")]
    pub cluster_version: u64,
}

/// Fetch Read State Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct FetchReadStateRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub command: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint64, tag = "2")]
    pub cluster_version: u64,
}

/// Read State
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ReadState {
    #[prost(uint64, tag = "1")]
    pub revision: u64,
    #[prost(message, optional, tag = "2")]
    pub id_set: ::core::option::Option<IdSet>,
}

/// Id Set
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct IdSet {
    #[prost(message, repeated, tag = "1")]
    pub inflight_ids: ::prost::alloc::vec::Vec<ProposeId>,
}

/// Fetch Read State Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct FetchReadStateResponse {
    #[prost(message, optional, tag = "1")]
    pub read_state: ::core::option::Option<ReadState>,
}

/// Read Index Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ReadIndexRequest {
    #[prost(uint64, tag = "1")]
    pub cluster_version: u64,
}

/// Read Index Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ReadIndexResponse {
    #[prost(uint64, tag = "1")]
    pub revision: u64,
}

/// Shutdown Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ShutdownRequest {
    #[prost(message, optional, tag = "1")]
    pub propose_id: ::core::option::Option<ProposeId>,
    #[prost(uint64, tag = "2")]
    pub cluster_version: u64,
}

/// Shutdown Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ShutdownResponse {}

/// Move Leader Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MoveLeaderRequest {
    #[prost(uint64, tag = "1")]
    pub node_id: u64,
    #[prost(uint64, tag = "2")]
    pub cluster_version: u64,
}

/// Move Leader Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MoveLeaderResponse {}

/// Publish Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct PublishRequest {
    #[prost(message, optional, tag = "1")]
    pub propose_id: ::core::option::Option<ProposeId>,
    #[prost(uint64, tag = "2")]
    pub node_id: u64,
    #[prost(string, tag = "3")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "4")]
    pub client_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

/// Publish Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct PublishResponse {}

/// Wait Synced Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct WaitSyncedRequest {
    #[prost(message, optional, tag = "1")]
    pub propose_id: ::core::option::Option<ProposeId>,
    #[prost(uint64, tag = "2")]
    pub cluster_version: u64,
}

/// Wait Synced Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct WaitSyncedResponse {
    #[prost(message, optional, tag = "1")]
    pub result: ::core::option::Option<CmdResult>,
}

/// Conf Change Type
#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum ConfChangeType {
    Add = 0,
    Remove = 1,
    Update = 2,
    AddLearner = 3,
    Promote = 4,
}

/// Conf Change
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ConfChange {
    #[prost(enumeration = "ConfChangeType", tag = "1")]
    pub change_type: i32,
    #[prost(uint64, tag = "2")]
    pub node_id: u64,
    #[prost(string, repeated, tag = "3")]
    pub address: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

/// Propose Conf Change Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ProposeConfChangeRequest {
    #[prost(message, optional, tag = "1")]
    pub propose_id: ::core::option::Option<ProposeId>,
    #[prost(message, repeated, tag = "2")]
    pub changes: ::prost::alloc::vec::Vec<ConfChange>,
    #[prost(uint64, tag = "3")]
    pub cluster_version: u64,
}

/// Propose Conf Change Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ProposeConfChangeResponse {
    #[prost(message, repeated, tag = "1")]
    pub responses: ::prost::alloc::vec::Vec<OpResponse>,
}

/// Op Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct OpResponse {
    #[prost(oneof = "op_response::Op", tags = "1, 2, 3")]
    pub response: ::core::option::Option<op_response::Op>,
}

pub mod op_response {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Op {
        #[prost(message, tag = "1")]
        ResponseRange(super::RangeResponse),
        #[prost(message, tag = "2")]
        ResponsePut(super::PutResponse),
        #[prost(message, tag = "3")]
        ResponseDeleteRange(super::DeleteRangeResponse),
    }
}

/// Optional U64
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct OptionalU64 {
    #[prost(uint64, tag = "1")]
    pub value: u64,
}

/// Range Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RangeRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "3")]
    pub limit: i64,
    #[prost(int32, tag = "4")]
    pub sort_order: i32,
    #[prost(int32, tag = "5")]
    pub sort_target: i32,
    #[prost(int64, tag = "6")]
    pub max_create_revision: i64,
}

/// Range Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RangeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub kvs: ::prost::alloc::vec::Vec<KeyValue>,
}

/// Put Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct PutRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub value: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "3")]
    pub lease: i64,
    #[prost(bool, tag = "4")]
    pub prev_kv: bool,
    #[prost(bool, tag = "5")]
    pub ignore_value: bool,
    #[prost(bool, tag = "6")]
    pub ignore_lease: bool,
}

/// Put Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct PutResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, optional, tag = "2")]
    pub prev_kv: ::core::option::Option<KeyValue>,
}

/// Delete Range Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct DeleteRangeRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(bool, tag = "3")]
    pub prev_kv: bool,
}

/// Delete Range Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct DeleteRangeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(int64, tag = "2")]
    pub deleted: i64,
    #[prost(message, repeated, tag = "3")]
    pub prev_kvs: ::prost::alloc::vec::Vec<KeyValue>,
}

/// Response Header
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ResponseHeader {
    #[prost(uint64, tag = "1")]
    pub cluster_id: u64,
    #[prost(uint64, tag = "2")]
    pub member_id: u64,
    #[prost(int64, tag = "3")]
    pub revision: i64,
    #[prost(uint64, tag = "4")]
    pub raft_term: u64,
}

/// KeyValue
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
    #[prost(bytes = "vec", tag = "6")]
    pub lease: i64,
}

/// CURP Error
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct CurpErrorWrapper {
    #[prost(oneof = "curp_error::Err", tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13")]
    pub err: ::core::option::Option<curp_error::Err>,
}

pub mod curp_error {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Err {
        #[prost(message, tag = "1")]
        Redirect(super::Redirect),
        #[prost((), tag = "2")]
        Duplicated(()),
        #[prost((), tag = "3")]
        ExpiredClientId(()),
        #[prost((), tag = "4")]
        InvalidConfig(()),
        #[prost((), tag = "5")]
        NodeNotExists(()),
        #[prost((), tag = "6")]
        NodeAlreadyExists(()),
        #[prost((), tag = "7")]
        LearnerNotCatchUp(()),
        #[prost((), tag = "8")]
        ShuttingDown(()),
        #[prost((), tag = "9")]
        WrongClusterVersion(()),
        #[prost(string, tag = "10")]
        Internal(::prost::alloc::string::String),
        #[prost((), tag = "11")]
        RpcTransport(()),
        #[prost((), tag = "12")]
        KeyConflict(()),
        #[prost(string, tag = "13")]
        LeaderTransfer(::prost::alloc::string::String),
        #[prost((), tag = "14")]
        Zombie(()),
    }
}

/// Redirect
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Redirect {
    #[prost(uint64, optional, tag = "1")]
    pub leader_id: ::core::option::Option<u64>,
    #[prost(uint64, tag = "2")]
    pub term: u64,
}

/// Sort Order
#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum SortOrder {
    None = 0,
    Ascend = 1,
    Descend = 2,
}

/// Sort Target
#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum SortTarget {
    Key = 0,
    Version = 1,
    Create = 2,
    Mod = 3,
    Value = 4,
}

/// Lease Keep Alive Msg
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseKeepAliveMsg {
    #[prost(int64, tag = "1")]
    pub id: i64,
    #[prost(int64, tag = "2")]
    pub ttl: i64,
}

/// Txn Request
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TxnRequest {
    #[prost(message, repeated, tag = "1")]
    pub compare: ::prost::alloc::vec::Vec<Compare>,
    #[prost(message, repeated, tag = "2")]
    pub success: ::prost::alloc::vec::Vec<RequestOp>,
    #[prost(message, repeated, tag = "3")]
    pub failure: ::prost::alloc::vec::Vec<RequestOp>,
}

/// Txn Response
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TxnResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(bool, tag = "2")]
    pub succeeded: bool,
    #[prost(message, repeated, tag = "3")]
    pub responses: ::prost::alloc::vec::Vec<ResponseOp>,
}

/// Compare
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Compare {
    #[prost(int32, tag = "1")]
    pub result: i32,
    #[prost(int32, tag = "2")]
    pub target: i32,
    #[prost(bytes = "vec", tag = "3")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "4")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(oneof = "compare::TargetUnion", tags = "5, 6, 7, 8")]
    pub target_union: ::core::option::Option<compare::TargetUnion>,
}

pub mod compare {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum TargetUnion {
        #[prost(int64, tag = "5")]
        Version(i64),
        #[prost(int64, tag = "6")]
        CreateRevision(i64),
        #[prost(int64, tag = "7")]
        ModRevision(i64),
        #[prost(bytes, tag = "8")]
        Value(::prost::alloc::vec::Vec<u8>),
    }
}

/// Request Op
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RequestOp {
    #[prost(oneof = "request_op::Request", tags = "1, 2, 3, 4")]
    pub request: ::core::option::Option<request_op::Request>,
}

pub mod request_op {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Request {
        #[prost(message, tag = "1")]
        RequestRange(super::RangeRequest),
        #[prost(message, tag = "2")]
        RequestPut(super::PutRequest),
        #[prost(message, tag = "3")]
        RequestDeleteRange(super::DeleteRangeRequest),
        #[prost(message, tag = "4")]
        RequestTxn(super::TxnRequest),
    }
}

/// Response Op
#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ResponseOp {
    #[prost(oneof = "response_op::Response", tags = "1, 2, 3, 4")]
    pub response: ::core::option::Option<response_op::Response>,
}

pub mod response_op {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Response {
        #[prost(message, tag = "1")]
        ResponseRange(super::RangeResponse),
        #[prost(message, tag = "2")]
        ResponsePut(super::PutResponse),
        #[prost(message, tag = "3")]
        ResponseDeleteRange(super::DeleteRangeResponse),
        #[prost(message, tag = "4")]
        ResponseTxn(super::TxnResponse),
    }
}

/// Compare Result
#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum CompareResult {
    Equal = 0,
    Greater = 1,
    Less = 2,
    NotEqual = 3,
}

/// Compare Target
#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum CompareTarget {
    Version = 0,
    Create = 1,
    Mod = 2,
    Value = 3,
    Lease = 4,
}