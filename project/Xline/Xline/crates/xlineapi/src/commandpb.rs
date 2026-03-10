//! Command protocol buffer types (manually implemented)

use prost::Message;
use serde::{Deserialize, Serialize};

// ============================================================================
// Command
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct Command {
    #[prost(oneof = "command::Request", tags = "1")]
    pub request: ::core::option::Option<command::Request>,
}

pub mod command {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Request {
        #[prost(message, tag = "1")]
        RequestWrapper(super::RequestWrapper),
    }
}

// ============================================================================
// RequestWrapper
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct RequestWrapper {
    #[prost(oneof = "request_wrapper::Request", tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25")]
    pub request: ::core::option::Option<request_wrapper::Request>,
}

pub mod request_wrapper {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Request {
        #[prost(message, tag = "1")]
        RangeRequest(super::super::etcdserverpb::RangeRequest),
        #[prost(message, tag = "2")]
        PutRequest(super::super::etcdserverpb::PutRequest),
        #[prost(message, tag = "3")]
        DeleteRangeRequest(super::super::etcdserverpb::DeleteRangeRequest),
        #[prost(message, tag = "4")]
        TxnRequest(super::super::etcdserverpb::TxnRequest),
        #[prost(message, tag = "5")]
        CompactionRequest(super::super::etcdserverpb::CompactionRequest),
        #[prost(message, tag = "6")]
        AuthEnableRequest(super::super::etcdserverpb::AuthEnableRequest),
        #[prost(message, tag = "7")]
        AuthDisableRequest(super::super::etcdserverpb::AuthDisableRequest),
        #[prost(message, tag = "8")]
        AuthStatusRequest(super::super::etcdserverpb::AuthStatusRequest),
        #[prost(message, tag = "9")]
        AuthRoleAddRequest(super::super::etcdserverpb::AuthRoleAddRequest),
        #[prost(message, tag = "10")]
        AuthRoleDeleteRequest(super::super::etcdserverpb::AuthRoleDeleteRequest),
        #[prost(message, tag = "11")]
        AuthRoleGetRequest(super::super::etcdserverpb::AuthRoleGetRequest),
        #[prost(message, tag = "12")]
        AuthRoleGrantPermissionRequest(super::super::etcdserverpb::AuthRoleGrantPermissionRequest),
        #[prost(message, tag = "13")]
        AuthRoleListRequest(super::super::etcdserverpb::AuthRoleListRequest),
        #[prost(message, tag = "14")]
        AuthRoleRevokePermissionRequest(super::super::etcdserverpb::AuthRoleRevokePermissionRequest),
        #[prost(message, tag = "15")]
        AuthUserAddRequest(super::super::etcdserverpb::AuthUserAddRequest),
        #[prost(message, tag = "16")]
        AuthUserChangePasswordRequest(super::super::etcdserverpb::AuthUserChangePasswordRequest),
        #[prost(message, tag = "17")]
        AuthUserDeleteRequest(super::super::etcdserverpb::AuthUserDeleteRequest),
        #[prost(message, tag = "18")]
        AuthUserGetRequest(super::super::etcdserverpb::AuthUserGetRequest),
        #[prost(message, tag = "19")]
        AuthUserGrantRoleRequest(super::super::etcdserverpb::AuthUserGrantRoleRequest),
        #[prost(message, tag = "20")]
        AuthUserListRequest(super::super::etcdserverpb::AuthUserListRequest),
        #[prost(message, tag = "21")]
        AuthUserRevokeRoleRequest(super::super::etcdserverpb::AuthUserRevokeRoleRequest),
        #[prost(message, tag = "22")]
        AuthenticateRequest(super::super::etcdserverpb::AuthenticateRequest),
        #[prost(message, tag = "23")]
        LeaseGrantRequest(super::super::etcdserverpb::LeaseGrantRequest),
        #[prost(message, tag = "24")]
        LeaseRevokeRequest(super::super::etcdserverpb::LeaseRevokeRequest),
        #[prost(message, tag = "25")]
        LeaseLeasesRequest(super::super::etcdserverpb::LeaseLeasesRequest),
        #[prost(message, tag = "26")]
        AlarmRequest(super::super::etcdserverpb::AlarmRequest),
    }
}

// ============================================================================
// AuthInfo
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct AuthInfo {
    #[prost(string, tag = "1")]
    pub username: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "2")]
    pub roles: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}

// ============================================================================
// CommandResponse
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct CommandResponse {
    #[prost(oneof = "command_response::Response", tags = "1")]
    pub response: ::core::option::Option<command_response::Response>,
}

pub mod command_response {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Response {
        #[prost(message, tag = "1")]
        ResponseWrapper(super::ResponseWrapper),
    }
}

// ============================================================================
// ResponseWrapper
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct ResponseWrapper {
    #[prost(oneof = "response_wrapper::Response", tags = "1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25")]
    pub response: ::core::option::Option<response_wrapper::Response>,
}

pub mod response_wrapper {
    use super::*;
    #[derive(Clone, PartialEq, ::prost::Oneof, Serialize, Deserialize)]
    pub enum Response {
        #[prost(message, tag = "1")]
        RangeResponse(super::super::etcdserverpb::RangeResponse),
        #[prost(message, tag = "2")]
        PutResponse(super::super::etcdserverpb::PutResponse),
        #[prost(message, tag = "3")]
        DeleteRangeResponse(super::super::etcdserverpb::DeleteRangeResponse),
        #[prost(message, tag = "4")]
        TxnResponse(super::super::etcdserverpb::TxnResponse),
        #[prost(message, tag = "5")]
        CompactionResponse(super::super::etcdserverpb::CompactionResponse),
        #[prost(message, tag = "6")]
        AuthEnableResponse(super::super::etcdserverpb::AuthEnableResponse),
        #[prost(message, tag = "7")]
        AuthDisableResponse(super::super::etcdserverpb::AuthDisableResponse),
        #[prost(message, tag = "8")]
        AuthStatusResponse(super::super::etcdserverpb::AuthStatusResponse),
        #[prost(message, tag = "9")]
        AuthRoleAddResponse(super::super::etcdserverpb::AuthRoleAddResponse),
        #[prost(message, tag = "10")]
        AuthRoleDeleteResponse(super::super::etcdserverpb::AuthRoleDeleteResponse),
        #[prost(message, tag = "11")]
        AuthRoleGetResponse(super::super::etcdserverpb::AuthRoleGetResponse),
        #[prost(message, tag = "12")]
        AuthRoleGrantPermissionResponse(super::super::etcdserverpb::AuthRoleGrantPermissionResponse),
        #[prost(message, tag = "13")]
        AuthRoleListResponse(super::super::etcdserverpb::AuthRoleListResponse),
        #[prost(message, tag = "14")]
        AuthRoleRevokePermissionResponse(super::super::etcdserverpb::AuthRoleRevokePermissionResponse),
        #[prost(message, tag = "15")]
        AuthUserAddResponse(super::super::etcdserverpb::AuthUserAddResponse),
        #[prost(message, tag = "16")]
        AuthUserChangePasswordResponse(super::super::etcdserverpb::AuthUserChangePasswordResponse),
        #[prost(message, tag = "17")]
        AuthUserDeleteResponse(super::super::etcdserverpb::AuthUserDeleteResponse),
        #[prost(message, tag = "18")]
        AuthUserGetResponse(super::super::etcdserverpb::AuthUserGetResponse),
        #[prost(message, tag = "19")]
        AuthUserGrantRoleResponse(super::super::etcdserverpb::AuthUserGrantRoleResponse),
        #[prost(message, tag = "20")]
        AuthUserListResponse(super::super::etcdserverpb::AuthUserListResponse),
        #[prost(message, tag = "21")]
        AuthUserRevokeRoleResponse(super::super::etcdserverpb::AuthUserRevokeRoleResponse),
        #[prost(message, tag = "22")]
        AuthenticateResponse(super::super::etcdserverpb::AuthenticateResponse),
        #[prost(message, tag = "23")]
        LeaseGrantResponse(super::super::etcdserverpb::LeaseGrantResponse),
        #[prost(message, tag = "24")]
        LeaseRevokeResponse(super::super::etcdserverpb::LeaseRevokeResponse),
        #[prost(message, tag = "25")]
        LeaseLeasesResponse(super::super::etcdserverpb::LeaseLeasesResponse),
        #[prost(message, tag = "26")]
        AlarmResponse(super::super::etcdserverpb::AlarmResponse),
    }
}

// ============================================================================
// KeyRange
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize)]
pub struct KeyRange {
    #[prost(bytes = "vec", tag = "1")]
    pub key: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "2")]
    pub range_end: ::prost::alloc::vec::Vec<u8>,
}

// ============================================================================
// SyncResponse
// ============================================================================

#[derive(Clone, PartialEq, ::prost::Message, Serialize, Deserialize, Default)]
pub struct SyncResponse {
    #[prost(int64, tag = "1")]
    pub revision: i64,
}