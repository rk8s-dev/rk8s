//! etcd server protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// Response Header
// ============================================================================

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

// ============================================================================
// Range Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RangeRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "3")]
    pub limit: i64,
    #[prost(int64, tag = "4")]
    pub revision: i64,
    #[prost(int32, tag = "5")]
    pub sort_order: i32,
    #[prost(int32, tag = "6")]
    pub sort_target: i32,
    #[prost(bool, tag = "7")]
    pub serializable: bool,
    #[prost(bool, tag = "8")]
    pub keys_only: bool,
    #[prost(bool, tag = "9")]
    pub count_only: bool,
    #[prost(int64, tag = "10")]
    pub min_mod_revision: i64,
    #[prost(int64, tag = "11")]
    pub max_mod_revision: i64,
    #[prost(int64, tag = "12")]
    pub min_create_revision: i64,
    #[prost(int64, tag = "13")]
    pub max_create_revision: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct RangeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub kvs: ::prost::alloc::vec::Vec<super::mvccpb::KeyValue>,
    #[prost(bool, tag = "3")]
    pub more: bool,
    #[prost(int64, tag = "4")]
    pub count: i64,
}

// ============================================================================
// Put Request/Response
// ============================================================================

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

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct PutResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, optional, tag = "2")]
    pub prev_kv: ::core::option::Option<super::mvccpb::KeyValue>,
}

// ============================================================================
// Delete Range Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct DeleteRangeRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(bool, tag = "3")]
    pub prev_kv: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct DeleteRangeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(int64, tag = "2")]
    pub deleted: i64,
    #[prost(message, repeated, tag = "3")]
    pub prev_kvs: ::prost::alloc::vec::Vec<super::mvccpb::KeyValue>,
}

// ============================================================================
// Txn Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct TxnRequest {
    #[prost(message, repeated, tag = "1")]
    pub compare: ::prost::alloc::vec::Vec<Compare>,
    #[prost(message, repeated, tag = "2")]
    pub success: ::prost::alloc::vec::Vec<RequestOp>,
    #[prost(message, repeated, tag = "3")]
    pub failure: ::prost::alloc::vec::Vec<RequestOp>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct TxnResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(bool, tag = "2")]
    pub succeeded: bool,
    #[prost(message, repeated, tag = "3")]
    pub responses: ::prost::alloc::vec::Vec<ResponseOp>,
}

// ============================================================================
// Compare
// ============================================================================

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum CompareResult {
    Equal = 0,
    Greater = 1,
    Less = 2,
    NotEqual = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum CompareTarget {
    Version = 0,
    Create = 1,
    Mod = 2,
    Value = 3,
    Lease = 4,
}

// ============================================================================
// RequestOp / ResponseOp
// ============================================================================

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

// ============================================================================
// Sort Order/Target
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum SortOrder {
    None = 0,
    Ascend = 1,
    Descend = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum SortTarget {
    Key = 0,
    Version = 1,
    Create = 2,
    Mod = 3,
    Value = 4,
}

// ============================================================================
// Watch Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct WatchRequest {
    #[prost(oneof = "watch_request::RequestUnion", tags = "1, 2, 3")]
    pub request_union: ::core::option::Option<watch_request::RequestUnion>,
}

pub mod watch_request {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum RequestUnion {
        #[prost(message, tag = "1")]
        CreateRequest(super::WatchCreateRequest),
        #[prost(message, tag = "2")]
        CancelRequest(super::WatchCancelRequest),
        #[prost(message, tag = "3")]
        ProgressRequest(super::WatchProgressRequest),
    }
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct WatchCreateRequest {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
    #[prost(int64, tag = "3")]
    pub start_revision: i64,
    #[prost(bool, tag = "4")]
    pub progress_notify: bool,
    #[prost(bool, tag = "5")]
    pub prev_kv: bool,
    #[prost(int64, tag = "6")]
    pub watch_id: i64,
    #[prost(bool, tag = "7")]
    pub filters: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct WatchCancelRequest {
    #[prost(int64, tag = "1")]
    pub watch_id: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct WatchProgressRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct WatchResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(int64, tag = "2")]
    pub watch_id: i64,
    #[prost(message, repeated, tag = "3")]
    pub events: ::prost::alloc::vec::Vec<super::mvccpb::Event>,
    #[prost(bool, tag = "4")]
    pub created: bool,
    #[prost(bool, tag = "5")]
    pub canceled: bool,
    #[prost(int64, tag = "6")]
    pub compact_revision: i64,
}

// ============================================================================
// Lease Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseGrantRequest {
    #[prost(int64, tag = "1")]
    pub ttl: i64,
    #[prost(int64, tag = "2")]
    pub id: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct LeaseGrantResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(int64, tag = "2")]
    pub id: i64,
    #[prost(int64, tag = "3")]
    pub ttl: i64,
    #[prost(string, tag = "4")]
    pub error: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseRevokeRequest {
    #[prost(int64, tag = "1")]
    pub id: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct LeaseRevokeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseKeepAliveRequest {
    #[prost(int64, tag = "1")]
    pub id: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct LeaseKeepAliveResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(int64, tag = "2")]
    pub id: i64,
    #[prost(int64, tag = "3")]
    pub ttl: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseLeasesRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct LeaseLeasesResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub leases: ::prost::alloc::vec::Vec<LeaseStatus>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseStatus {
    #[prost(int64, tag = "1")]
    pub id: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct LeaseTimeToLiveRequest {
    #[prost(int64, tag = "1")]
    pub id: i64,
    #[prost(bool, tag = "2")]
    pub keys: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct LeaseTimeToLiveResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(int64, tag = "2")]
    pub id: i64,
    #[prost(int64, tag = "3")]
    pub ttl: i64,
    #[prost(int64, tag = "4")]
    pub granted_ttl: i64,
    #[prost(int64, tag = "5")]
    pub age: i64,
    #[prost(bytes = "vec", repeated, tag = "6")]
    pub keys: ::prost::alloc::vec::Vec<::prost::alloc::vec::Vec<u8>>,
}

// ============================================================================
// Auth Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthEnableRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthEnableResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthDisableRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthDisableResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthStatusRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthStatusResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(bool, tag = "2")]
    pub enabled: bool,
    #[prost(int64, tag = "3")]
    pub auth_revision: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthRoleAddRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthRoleAddResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthRoleDeleteRequest {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthRoleDeleteResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthRoleGetRequest {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthRoleGetResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub perm: ::prost::alloc::vec::Vec<super::authpb::Permission>,
    #[prost(string, repeated, tag = "3")]
    pub role: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthRoleGrantPermissionRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub perm: ::core::option::Option<super::authpb::Permission>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthRoleGrantPermissionResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthRoleListRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthRoleListResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, repeated, tag = "2")]
    pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthRoleRevokePermissionRequest {
    #[prost(string, tag = "1")]
    pub role: ::prost::alloc::string::String,
    #[prost(bytes = "vec", tag = "2")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthRoleRevokePermissionResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserAddRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub password: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "3")]
    pub options: ::core::option::Option<super::authpb::UserAddOptions>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserAddResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserChangePasswordRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub password: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserChangePasswordResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserDeleteRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserDeleteResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserGetRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserGetResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, repeated, tag = "2")]
    pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, optional, tag = "3")]
    pub grant: ::core::option::Option<super::authpb::User>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserGrantRoleRequest {
    #[prost(string, tag = "1")]
    pub user: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub role: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserGrantRoleResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserListRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserListResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, repeated, tag = "2")]
    pub users: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthUserRevokeRoleRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub role: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthUserRevokeRoleResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthenticateRequest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub password: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AuthenticateResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, tag = "2")]
    pub token: ::prost::alloc::string::String,
}

// ============================================================================
// Compaction Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct CompactionRequest {
    #[prost(int64, tag = "1")]
    pub revision: i64,
    #[prost(bool, tag = "2")]
    pub physical: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct CompactionResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

// ============================================================================
// Defragment Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct DefragmentRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct DefragmentResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

// ============================================================================
// Hash Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct HashRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct HashResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(uint32, tag = "2")]
    pub hash: u32,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct HashKvRequest {
    #[prost(int64, tag = "1")]
    pub revision: i64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct HashKvResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(uint32, tag = "2")]
    pub hash: u32,
    #[prost(int64, tag = "3")]
    pub compact_version: i64,
}

// ============================================================================
// Member Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Member {
    #[prost(uint64, tag = "1")]
    pub id: u64,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "3")]
    pub peer_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, repeated, tag = "4")]
    pub client_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(bool, tag = "5")]
    pub is_learner: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MemberAddRequest {
    #[prost(string, repeated, tag = "1")]
    pub peer_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(bool, tag = "2")]
    pub is_learner: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct MemberAddResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, optional, tag = "2")]
    pub member: ::core::option::Option<Member>,
    #[prost(message, repeated, tag = "3")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MemberRemoveRequest {
    #[prost(uint64, tag = "1")]
    pub id: u64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct MemberRemoveResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MemberUpdateRequest {
    #[prost(uint64, tag = "1")]
    pub id: u64,
    #[prost(string, repeated, tag = "2")]
    pub peer_urls: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct MemberUpdateResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MemberListRequest {
    #[prost(bool, tag = "1")]
    pub linearizable: bool,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct MemberListResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MemberPromoteRequest {
    #[prost(uint64, tag = "1")]
    pub id: u64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct MemberPromoteResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub members: ::prost::alloc::vec::Vec<Member>,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct MoveLeaderRequest {
    #[prost(uint64, tag = "1")]
    pub target_id: u64,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct MoveLeaderResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

// ============================================================================
// Status Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct StatusRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct StatusResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
    #[prost(uint64, tag = "3")]
    pub db_size: u64,
    #[prost(uint64, tag = "4")]
    pub leader: u64,
    #[prost(uint64, tag = "5")]
    pub raft_index: u64,
    #[prost(uint64, tag = "6")]
    pub raft_term: u64,
    #[prost(uint64, tag = "7")]
    pub raft_applied_index: u64,
    #[prost(string, tag = "8")]
    pub db_size_in_use: ::prost::alloc::string::String,
}

// ============================================================================
// Snapshot Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct SnapshotRequest {}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct SnapshotResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(uint64, tag = "2")]
    pub remaining_bytes: u64,
    #[prost(uint64, tag = "3")]
    pub total_bytes: u64,
    #[prost(bytes = "vec", tag = "4")]
    pub blob: ::prost::alloc::vec::Vec<u8>,
}

// ============================================================================
// Downgrade Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct DowngradeRequest {
    #[prost(string, tag = "1")]
    pub action: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub version: ::prost::alloc::string::String,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct DowngradeResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
}

// ============================================================================
// Alarm Request/Response
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AlarmRequest {
    #[prost(int32, tag = "1")]
    pub action: i32,
    #[prost(uint64, tag = "2")]
    pub member_id: u64,
    #[prost(int32, tag = "3")]
    pub alarm: i32,
}

pub mod alarm_request {
    use super::*;
    #[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
    #[repr(i32)]
    pub enum AlarmAction {
        Get = 0,
        Activate = 1,
        Deactivate = 2,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ::prost::Enumeration, Serialize, Deserialize)]
#[repr(i32)]
pub enum AlarmType {
    None = 0,
    Nospace = 1,
    Corrupt = 2,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AlarmMember {
    #[prost(uint64, tag = "1")]
    pub member_id: u64,
    #[prost(int32, tag = "2")]
    pub alarm: i32,
}

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct AlarmResponse {
    #[prost(message, optional, tag = "1")]
    pub header: ::core::option::Option<ResponseHeader>,
    #[prost(message, repeated, tag = "2")]
    pub alarms: ::prost::alloc::vec::Vec<AlarmMember>,
}

// ============================================================================
// gRPC Service Traits
// ============================================================================

pub mod kv_server {
    use super::*;
    use tonic::{Request, Response, Status};
    use tonic::server::NamedService;
    use tonic::codegen::{http, Body, BoxFuture};
    use tower_service::Service;
    use std::task::{Context, Poll};
    use std::sync::Arc;

    #[async_trait::async_trait]
    pub trait Kv: Send + Sync + 'static {
        async fn range(&self, request: Request<RangeRequest>) -> Result<Response<RangeResponse>, Status>;
        async fn put(&self, request: Request<PutRequest>) -> Result<Response<PutResponse>, Status>;
        async fn delete_range(&self, request: Request<DeleteRangeRequest>) -> Result<Response<DeleteRangeResponse>, Status>;
        async fn txn(&self, request: Request<TxnRequest>) -> Result<Response<TxnResponse>, Status>;
        async fn compact(&self, request: Request<CompactionRequest>) -> Result<Response<CompactionResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct KvServer<S> {
        inner: std::sync::Arc<S>,
    }

    impl<S> KvServer<S>
    where
        S: Kv + Send + Sync + 'static,
    {
        pub fn new(service: S) -> Self {
            Self {
                inner: std::sync::Arc::new(service),
            }
        }
    }

    impl<S> NamedService for KvServer<S>
    where
        S: Kv + Send + Sync + 'static,
    {
        const NAME: &'static str = "etcdserverpb.KV";
    }

    impl<S> Service<http::Request<tonic::body::BoxBody>> for KvServer<S>
    where
        S: Kv + Send + Sync + 'static,
        S: 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = http::Error;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
            let inner = Arc::clone(&self.inner);
            Box::pin(async move {
                let path = req.uri().path().to_string();
                match path.as_str() {
                    "/etcdserverpb.KV/Range" => {
                        let msg = decode_request::<RangeRequest>(req).await?;
                        let resp = inner.range(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.KV/Put" => {
                        let msg = decode_request::<PutRequest>(req).await?;
                        let resp = inner.put(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.KV/DeleteRange" => {
                        let msg = decode_request::<DeleteRangeRequest>(req).await?;
                        let resp = inner.delete_range(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.KV/Txn" => {
                        let msg = decode_request::<TxnRequest>(req).await?;
                        let resp = inner.txn(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.KV/Compaction" => {
                        let msg = decode_request::<CompactionRequest>(req).await?;
                        let resp = inner.compact(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    _ => {
                        Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_FOUND)
                            .body(tonic::body::BoxBody::default())
                            .unwrap())
                    }
                }
            })
        }
    }
}

pub mod watch_server {
    #[async_trait::async_trait]
    pub trait Watch: Send + Sync + 'static {
        type WatchStream: Stream<Item = Result<WatchResponse, Status>> + Send + 'static;
        async fn watch(&self, request: Request<tonic::Streaming<WatchRequest>>) 
            -> Result<Response<Self::WatchStream>, Status>;
    }
}

pub mod lease_server {
    use super::*;
    use tonic::{Request, Response, Status};
    use futures::Stream;

    #[async_trait::async_trait]
    pub trait Lease: Send + Sync + 'static {
        type LeaseKeepAliveStream: Stream<Item = Result<LeaseKeepAliveResponse, Status>> + Send + 'static;
        async fn lease_grant(&self, request: Request<LeaseGrantRequest>) -> Result<Response<LeaseGrantResponse>, Status>;
        async fn lease_revoke(&self, request: Request<LeaseRevokeRequest>) -> Result<Response<LeaseRevokeResponse>, Status>;
        async fn lease_keep_alive(&self, request: Request<tonic::Streaming<LeaseKeepAliveRequest>>) -> Result<Response<Self::LeaseKeepAliveStream>, Status>;
        async fn lease_time_to_live(&self, request: Request<LeaseTimeToLiveRequest>) -> Result<Response<LeaseTimeToLiveResponse>, Status>;
        async fn lease_leases(&self, request: Request<LeaseLeasesRequest>) -> Result<Response<LeaseLeasesResponse>, Status>;
    }
}

pub mod auth_server {
    use super::*;
    use tonic::{Request, Response, Status};
    use tonic::server::NamedService;
    use tonic::codegen::{http, Body, BoxFuture};
    use tower_service::Service;
    use std::task::{Context, Poll};
    use std::sync::Arc;

    #[async_trait::async_trait]
    pub trait Auth: Send + Sync + 'static {
        async fn auth_enable(&self, request: Request<AuthEnableRequest>) -> Result<Response<AuthEnableResponse>, Status>;
        async fn auth_disable(&self, request: Request<AuthDisableRequest>) -> Result<Response<AuthDisableResponse>, Status>;
        async fn authenticate(&self, request: Request<AuthenticateRequest>) -> Result<Response<AuthenticateResponse>, Status>;
        async fn user_add(&self, request: Request<AuthUserAddRequest>) -> Result<Response<AuthUserAddResponse>, Status>;
        async fn user_get(&self, request: Request<AuthUserGetRequest>) -> Result<Response<AuthUserGetResponse>, Status>;
        async fn user_list(&self, request: Request<AuthUserListRequest>) -> Result<Response<AuthUserListResponse>, Status>;
        async fn user_delete(&self, request: Request<AuthUserDeleteRequest>) -> Result<Response<AuthUserDeleteResponse>, Status>;
        async fn user_change_password(&self, request: Request<AuthUserChangePasswordRequest>) -> Result<Response<AuthUserChangePasswordResponse>, Status>;
        async fn user_grant_role(&self, request: Request<AuthUserGrantRoleRequest>) -> Result<Response<AuthUserGrantRoleResponse>, Status>;
        async fn user_revoke_role(&self, request: Request<AuthUserRevokeRoleRequest>) -> Result<Response<AuthUserRevokeRoleResponse>, Status>;
        async fn role_add(&self, request: Request<AuthRoleAddRequest>) -> Result<Response<AuthRoleAddResponse>, Status>;
        async fn role_get(&self, request: Request<AuthRoleGetRequest>) -> Result<Response<AuthRoleGetResponse>, Status>;
        async fn role_list(&self, request: Request<AuthRoleListRequest>) -> Result<Response<AuthRoleListResponse>, Status>;
        async fn role_delete(&self, request: Request<AuthRoleDeleteRequest>) -> Result<Response<AuthRoleDeleteResponse>, Status>;
        async fn role_grant_permission(&self, request: Request<AuthRoleGrantPermissionRequest>) -> Result<Response<AuthRoleGrantPermissionResponse>, Status>;
        async fn role_revoke_permission(&self, request: Request<AuthRoleRevokePermissionRequest>) -> Result<Response<AuthRoleRevokePermissionResponse>, Status>;
        async fn auth_status(&self, request: Request<AuthStatusRequest>) -> Result<Response<AuthStatusResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct AuthServer<S> {
        inner: std::sync::Arc<S>,
    }

    impl<S> AuthServer<S>
    where
        S: Auth + Send + Sync + 'static,
    {
        pub fn new(service: S) -> Self {
            Self {
                inner: std::sync::Arc::new(service),
            }
        }
    }

    impl<S> NamedService for AuthServer<S>
    where
        S: Auth + Send + Sync + 'static,
    {
        const NAME: &'static str = "etcdserverpb.Auth";
    }

    impl<S> Service<http::Request<tonic::body::BoxBody>> for AuthServer<S>
    where
        S: Auth + Send + Sync + 'static,
        S: 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = http::Error;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
            let inner = Arc::clone(&self.inner);
            Box::pin(async move {
                let path = req.uri().path().to_string();
                match path.as_str() {
                    "/etcdserverpb.Auth/AuthEnable" => {
                        let msg = decode_request::<AuthEnableRequest>(req).await?;
                        let resp = inner.auth_enable(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/AuthDisable" => {
                        let msg = decode_request::<AuthDisableRequest>(req).await?;
                        let resp = inner.auth_disable(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/Authenticate" => {
                        let msg = decode_request::<AuthenticateRequest>(req).await?;
                        let resp = inner.authenticate(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserAdd" => {
                        let msg = decode_request::<AuthUserAddRequest>(req).await?;
                        let resp = inner.user_add(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserGet" => {
                        let msg = decode_request::<AuthUserGetRequest>(req).await?;
                        let resp = inner.user_get(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserList" => {
                        let msg = decode_request::<AuthUserListRequest>(req).await?;
                        let resp = inner.user_list(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserDelete" => {
                        let msg = decode_request::<AuthUserDeleteRequest>(req).await?;
                        let resp = inner.user_delete(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserChangePassword" => {
                        let msg = decode_request::<AuthUserChangePasswordRequest>(req).await?;
                        let resp = inner.user_change_password(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserGrantRole" => {
                        let msg = decode_request::<AuthUserGrantRoleRequest>(req).await?;
                        let resp = inner.user_grant_role(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/UserRevokeRole" => {
                        let msg = decode_request::<AuthUserRevokeRoleRequest>(req).await?;
                        let resp = inner.user_revoke_role(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/RoleAdd" => {
                        let msg = decode_request::<AuthRoleAddRequest>(req).await?;
                        let resp = inner.role_add(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/RoleGet" => {
                        let msg = decode_request::<AuthRoleGetRequest>(req).await?;
                        let resp = inner.role_get(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/RoleList" => {
                        let msg = decode_request::<AuthRoleListRequest>(req).await?;
                        let resp = inner.role_list(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/RoleDelete" => {
                        let msg = decode_request::<AuthRoleDeleteRequest>(req).await?;
                        let resp = inner.role_delete(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/RoleGrantPermission" => {
                        let msg = decode_request::<AuthRoleGrantPermissionRequest>(req).await?;
                        let resp = inner.role_grant_permission(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/RoleRevokePermission" => {
                        let msg = decode_request::<AuthRoleRevokePermissionRequest>(req).await?;
                        let resp = inner.role_revoke_permission(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Auth/AuthStatus" => {
                        let msg = decode_request::<AuthStatusRequest>(req).await?;
                        let resp = inner.auth_status(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    _ => {
                        Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_FOUND)
                            .body(tonic::body::BoxBody::default())
                            .unwrap())
                    }
                }
            })
        }
    }
}

pub mod maintenance_server {
    use super::*;
    use tonic::{Request, Response, Status};
    use tonic::server::NamedService;
    use tonic::codegen::{http, Body, BoxFuture};
    use tower_service::Service;
    use std::task::{Context, Poll};
    use std::sync::Arc;
    use futures::Stream;

    #[async_trait::async_trait]
    pub trait Maintenance: Send + Sync + 'static {
        type AlarmStream: Stream<Item = Result<AlarmResponse, Status>> + Send + 'static;
        type StatusStream: Stream<Item = Result<StatusResponse, Status>> + Send + 'static;
        type SnapshotStream: Stream<Item = Result<SnapshotResponse, Status>> + Send + 'static;
        async fn alarm(&self, request: Request<AlarmRequest>) -> Result<Response<Self::AlarmStream>, Status>;
        async fn status(&self, request: Request<StatusRequest>) -> Result<Response<Self::StatusStream>, Status>;
        async fn hash(&self, request: Request<HashRequest>) -> Result<Response<HashResponse>, Status>;
        async fn hash_kv(&self, request: Request<HashKvRequest>) -> Result<Response<HashKvResponse>, Status>;
        async fn snapshot(&self, request: Request<SnapshotRequest>) -> Result<Response<Self::SnapshotStream>, Status>;
        async fn move_leader(&self, request: Request<MoveLeaderRequest>) -> Result<Response<MoveLeaderResponse>, Status>;
        async fn defragment(&self, request: Request<DefragmentRequest>) -> Result<Response<DefragmentResponse>, Status>;
        async fn member_list(&self, request: Request<MemberListRequest>) -> Result<Response<MemberListResponse>, Status>;
        async fn member_add(&self, request: Request<MemberAddRequest>) -> Result<Response<MemberAddResponse>, Status>;
        async fn member_remove(&self, request: Request<MemberRemoveRequest>) -> Result<Response<MemberRemoveResponse>, Status>;
        async fn member_update(&self, request: Request<MemberUpdateRequest>) -> Result<Response<MemberUpdateResponse>, Status>;
        async fn member_promote(&self, request: Request<MemberPromoteRequest>) -> Result<Response<MemberPromoteResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct MaintenanceServer<S> {
        inner: std::sync::Arc<S>,
    }

    impl<S> MaintenanceServer<S>
    where
        S: Maintenance + Send + Sync + 'static,
    {
        pub fn new(service: S) -> Self {
            Self {
                inner: std::sync::Arc::new(service),
            }
        }
    }

    impl<S> NamedService for MaintenanceServer<S>
    where
        S: Maintenance + Send + Sync + 'static,
    {
        const NAME: &'static str = "etcdserverpb.Maintenance";
    }

    impl<S> Service<http::Request<tonic::body::BoxBody>> for MaintenanceServer<S>
    where
        S: Maintenance + Send + Sync + 'static,
        S: 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = http::Error;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
            let inner = Arc::clone(&self.inner);
            Box::pin(async move {
                let path = req.uri().path().to_string();
                match path.as_str() {
                    "/etcdserverpb.Maintenance/Hash" => {
                        let msg = decode_request::<HashRequest>(req).await?;
                        let resp = inner.hash(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/HashKv" => {
                        let msg = decode_request::<HashKvRequest>(req).await?;
                        let resp = inner.hash_kv(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/MoveLeader" => {
                        let msg = decode_request::<MoveLeaderRequest>(req).await?;
                        let resp = inner.move_leader(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/Defragment" => {
                        let msg = decode_request::<DefragmentRequest>(req).await?;
                        let resp = inner.defragment(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/MemberList" => {
                        let msg = decode_request::<MemberListRequest>(req).await?;
                        let resp = inner.member_list(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/MemberAdd" => {
                        let msg = decode_request::<MemberAddRequest>(req).await?;
                        let resp = inner.member_add(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/MemberRemove" => {
                        let msg = decode_request::<MemberRemoveRequest>(req).await?;
                        let resp = inner.member_remove(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/MemberUpdate" => {
                        let msg = decode_request::<MemberUpdateRequest>(req).await?;
                        let resp = inner.member_update(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/MemberPromote" => {
                        let msg = decode_request::<MemberPromoteRequest>(req).await?;
                        let resp = inner.member_promote(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Maintenance/Alarm" |
                    "/etcdserverpb.Maintenance/Status" |
                    "/etcdserverpb.Maintenance/Snapshot" => {
                        Ok(http::Response::builder()
                            .status(http::StatusCode::UNIMPLEMENTED)
                            .header("grpc-status", "12")
                            .header("grpc-message", "Streaming RPC: Use direct registration in xline_server.rs")
                            .body(tonic::body::BoxBody::default())
                            .unwrap())
                    }
                    _ => {
                        Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_FOUND)
                            .body(tonic::body::BoxBody::default())
                            .unwrap())
                    }
                }
            })
        }
    }
}

pub mod cluster_server {
    use super::*;
    use tonic::{Request, Response, Status};
    use tonic::server::NamedService;
    use tonic::codegen::{http, Body, BoxFuture};
    use tower_service::Service;
    use std::task::{Context, Poll};
    use std::sync::Arc;

    #[async_trait::async_trait]
    pub trait Cluster: Send + Sync + 'static {
        async fn member_list(&self, request: Request<MemberListRequest>) -> Result<Response<MemberListResponse>, Status>;
        async fn member_add(&self, request: Request<MemberAddRequest>) -> Result<Response<MemberAddResponse>, Status>;
        async fn member_remove(&self, request: Request<MemberRemoveRequest>) -> Result<Response<MemberRemoveResponse>, Status>;
        async fn member_update(&self, request: Request<MemberUpdateRequest>) -> Result<Response<MemberUpdateResponse>, Status>;
        async fn member_promote(&self, request: Request<MemberPromoteRequest>) -> Result<Response<MemberPromoteResponse>, Status>;
    }

    #[derive(Debug, Clone)]
    pub struct ClusterServer<S> {
        inner: std::sync::Arc<S>,
    }

    impl<S> ClusterServer<S>
    where
        S: Cluster + Send + Sync + 'static,
    {
        pub fn new(service: S) -> Self {
            Self {
                inner: std::sync::Arc::new(service),
            }
        }
    }

    impl<S> NamedService for ClusterServer<S>
    where
        S: Cluster + Send + Sync + 'static,
    {
        const NAME: &'static str = "etcdserverpb.Cluster";
    }

    impl<S> Service<http::Request<tonic::body::BoxBody>> for ClusterServer<S>
    where
        S: Cluster + Send + Sync + 'static,
        S: 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = http::Error;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<tonic::body::BoxBody>) -> Self::Future {
            let inner = Arc::clone(&self.inner);
            Box::pin(async move {
                let path = req.uri().path().to_string();
                match path.as_str() {
                    "/etcdserverpb.Cluster/MemberList" => {
                        let msg = decode_request::<MemberListRequest>(req).await?;
                        let resp = inner.member_list(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Cluster/MemberAdd" => {
                        let msg = decode_request::<MemberAddRequest>(req).await?;
                        let resp = inner.member_add(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Cluster/MemberRemove" => {
                        let msg = decode_request::<MemberRemoveRequest>(req).await?;
                        let resp = inner.member_remove(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Cluster/MemberUpdate" => {
                        let msg = decode_request::<MemberUpdateRequest>(req).await?;
                        let resp = inner.member_update(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    "/etcdserverpb.Cluster/MemberPromote" => {
                        let msg = decode_request::<MemberPromoteRequest>(req).await?;
                        let resp = inner.member_promote(msg).await.map_err(status_to_http_error)?;
                        Ok(encode_response(resp))
                    }
                    _ => {
                        Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_FOUND)
                            .body(tonic::body::BoxBody::default())
                            .unwrap())
                    }
                }
            })
        }
    }
}

// ============================================================================
// Client stubs
// ============================================================================

pub mod kv_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct KvClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> KvClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}

pub mod watch_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct WatchClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> WatchClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}

pub mod lease_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct LeaseClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> LeaseClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}

pub mod auth_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct AuthClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> AuthClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}

pub mod maintenance_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct MaintenanceClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> MaintenanceClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}

pub mod cluster_client {
    use super::*;
    use tonic::transport::Channel;
    use tonic::client::GrpcService;

    pub struct ClusterClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl<T> ClusterClient<T>
    where
        T: GrpcService<tonic::body::BoxBody>,
        T::Error: Into<tonic::codegen::StdError>,
        T::ResponseBody: http_body::Body<Data = bytes::Bytes> + Send + 'static,
        <T::ResponseBody as http_body::Body>::Error: Into<tonic::codegen::StdError> + Send,
    {
        pub fn new(channel: T) -> Self {
            Self {
                inner: tonic::client::Grpc::new(channel),
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Helper Function: Decode Request
async fn decode_request<T: prost::Message + Default>(
    mut req: http::Request<tonic::body::BoxBody>,
) -> Result<Request<T>, http::Error> {
    use http_body::Body;
    use bytes::Buf;

    let body = req.body_mut();
    let mut buf = Vec::new();
    while let Some(chunk) = body.data().await {
        let chunk = chunk.map_err(|_| http::Error::new(Box::new(std::io::Error::new(
            std::io::ErrorKind::Other,
            "body error",
        ))))?;
        buf.extend_from_slice(chunk.chunk());
    }

    let msg = T::decode(&buf[..]).map_err(|_| http::Error::new(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        "decode error",
    ))))?;

    let (parts, _) = req.into_parts();
    Ok(Request::from_parts(parts, msg))
}

/// Helper function: Encode response
fn encode_response<T: prost::Message>(resp: Response<T>) -> http::Response<tonic::body::BoxBody> {
    use http_body_util::Full;
    use bytes::Bytes;

    let (metadata, msg, _extensions) = resp.into_parts();
    let mut buf = Vec::new();
    msg.encode(&mut buf).unwrap();

    let mut http_resp = http::Response::builder()
        .status(http::StatusCode::OK)
        .header("content-type", "application/grpc")
        .body(tonic::body::BoxBody::new(Full::new(Bytes::from(buf))))
        .unwrap();

    *http_resp.headers_mut() = metadata.into_headers();
    http_resp
}

/// Helper function: Convert Status to http::Error
fn status_to_http_error(status: Status) -> http::Error {
    http::Error::new(Box::new(std::io::Error::new(
        std::io::ErrorKind::Other,
        status.message(),
    )))
}