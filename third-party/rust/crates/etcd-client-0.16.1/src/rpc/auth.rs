//! Etcd Auth RPC.

pub use crate::rpc::pb::authpb::permission::Type as PermissionType;

use crate::auth::AuthService;
use crate::error::Result;
use crate::intercept::InterceptedChannel;
use crate::lock::RwLockExt;
use crate::rpc::pb::authpb::{Permission as PbPermission, UserAddOptions as PbUserAddOptions};
use crate::rpc::pb::etcdserverpb::auth_client::AuthClient as PbAuthClient;
use crate::rpc::pb::etcdserverpb::{
    AuthDisableRequest as PbAuthDisableRequest, AuthDisableResponse as PbAuthDisableResponse,
    AuthEnableRequest as PbAuthEnableRequest, AuthEnableResponse as PbAuthEnableResponse,
    AuthRoleAddRequest as PbAuthRoleAddRequest, AuthRoleAddResponse as PbAuthRoleAddResponse,
    AuthRoleDeleteRequest as PbAuthRoleDeleteRequest,
    AuthRoleDeleteResponse as PbAuthRoleDeleteResponse, AuthRoleGetRequest as PbAuthRoleGetRequest,
    AuthRoleGetResponse as PbAuthRoleGetResponse,
    AuthRoleGrantPermissionRequest as PbAuthRoleGrantPermissionRequest,
    AuthRoleGrantPermissionResponse as PbAuthRoleGrantPermissionResponse,
    AuthRoleListRequest as PbAuthRoleListRequest, AuthRoleListResponse as PbAuthRoleListResponse,
    AuthRoleRevokePermissionRequest as PbAuthRoleRevokePermissionRequest,
    AuthRoleRevokePermissionResponse as PbAuthRoleRevokePermissionResponse,
    AuthUserAddRequest as PbAuthUserAddRequest, AuthUserAddResponse as PbAuthUserAddResponse,
    AuthUserChangePasswordRequest as PbAuthUserChangePasswordRequest,
    AuthUserChangePasswordResponse as PbAuthUserChangePasswordResponse,
    AuthUserDeleteRequest as PbAuthUserDeleteRequest,
    AuthUserDeleteResponse as PbAuthUserDeleteResponse, AuthUserGetRequest as PbAuthUserGetRequest,
    AuthUserGetResponse as PbAuthUserGetResponse,
    AuthUserGrantRoleRequest as PbAuthUserGrantRoleRequest,
    AuthUserGrantRoleResponse as PbAuthUserGrantRoleResponse,
    AuthUserListRequest as PbAuthUserListRequest, AuthUserListResponse as PbAuthUserListResponse,
    AuthUserRevokeRoleRequest as PbAuthUserRevokeRoleRequest,
    AuthUserRevokeRoleResponse as PbAuthUserRevokeRoleResponse,
    AuthenticateRequest as PbAuthenticateRequest, AuthenticateResponse as PbAuthenticateResponse,
};
use crate::rpc::ResponseHeader;
use crate::rpc::{get_prefix, KeyRange};
use http::HeaderValue;
use std::sync::RwLock;
use std::{string::String, sync::Arc};
use tonic::{IntoRequest, Request};

/// Client for Auth operations.
#[derive(Clone)]
pub struct AuthClient {
    inner: PbAuthClient<AuthService<InterceptedChannel>>,
    auth_token: Arc<RwLock<Option<HeaderValue>>>,
}

impl AuthClient {
    /// Creates an auth client.
    #[inline]
    pub(crate) fn new(
        channel: InterceptedChannel,
        auth_token: Arc<RwLock<Option<HeaderValue>>>,
    ) -> Self {
        let inner = PbAuthClient::new(AuthService::new(channel, auth_token.clone()));
        Self { inner, auth_token }
    }

    /// Sets client-side authentication.
    pub async fn set_client_auth(&mut self, name: String, password: String) -> Result<()> {
        let resp = self.authenticate(name, password).await?;
        self.auth_token
            .write_unpoisoned()
            .replace(resp.token().parse()?);
        Ok(())
    }

    /// Removes client-side authentication.
    pub fn remove_client_auth(&mut self) {
        self.auth_token.write_unpoisoned().take();
    }

    /// Enables authentication for the etcd cluster.
    #[inline]
    pub async fn auth_enable(&mut self) -> Result<AuthEnableResponse> {
        let resp = self
            .inner
            .auth_enable(AuthEnableOptions::new())
            .await?
            .into_inner();
        Ok(AuthEnableResponse::new(resp))
    }

    /// Disables authentication for the etcd cluster.
    #[inline]
    pub async fn auth_disable(&mut self) -> Result<AuthDisableResponse> {
        let resp = self
            .inner
            .auth_disable(AuthDisableOptions::new())
            .await?
            .into_inner();
        Ok(AuthDisableResponse::new(resp))
    }

    /// Sends an authenticate request.
    /// Note that this does not set or update client-side authentication settings.
    /// Call [`set_client_auth`] to set or update client-side authentication.
    #[inline]
    pub async fn authenticate(
        &mut self,
        name: String,
        password: String,
    ) -> Result<AuthenticateResponse> {
        let resp = self
            .inner
            .authenticate(AuthenticateOptions::new().with_user(name, password))
            .await?
            .into_inner();
        Ok(AuthenticateResponse::new(resp))
    }

    /// Adds role
    #[inline]
    pub async fn role_add(&mut self, name: impl Into<String>) -> Result<RoleAddResponse> {
        let resp = self
            .inner
            .role_add(RoleAddOptions::new(name.into()))
            .await?
            .into_inner();
        Ok(RoleAddResponse::new(resp))
    }

    /// Deletes role
    #[inline]
    pub async fn role_delete(&mut self, name: impl Into<String>) -> Result<RoleDeleteResponse> {
        let resp = self
            .inner
            .role_delete(RoleDeleteOptions::new(name.into()))
            .await?
            .into_inner();
        Ok(RoleDeleteResponse::new(resp))
    }

    /// Gets role
    #[inline]
    pub async fn role_get(&mut self, name: impl Into<String>) -> Result<RoleGetResponse> {
        let resp = self
            .inner
            .role_get(RoleGetOptions::new(name.into()))
            .await?
            .into_inner();
        Ok(RoleGetResponse::new(resp))
    }

    /// Lists role
    #[inline]
    pub async fn role_list(&mut self) -> Result<RoleListResponse> {
        let resp = self
            .inner
            .role_list(AuthRoleListOptions {})
            .await?
            .into_inner();
        Ok(RoleListResponse::new(resp))
    }

    /// Grants role permission
    #[inline]
    pub async fn role_grant_permission(
        &mut self,
        name: impl Into<String>,
        perm: Permission,
    ) -> Result<RoleGrantPermissionResponse> {
        let resp = self
            .inner
            .role_grant_permission(RoleGrantPermissionOptions::new(name.into(), perm))
            .await?
            .into_inner();
        Ok(RoleGrantPermissionResponse::new(resp))
    }

    /// Revokes role permission
    #[inline]
    pub async fn role_revoke_permission(
        &mut self,
        name: impl Into<String>,
        key: impl Into<Vec<u8>>,
        options: Option<RoleRevokePermissionOptions>,
    ) -> Result<RoleRevokePermissionResponse> {
        let resp = self
            .inner
            .role_revoke_permission(
                options
                    .unwrap_or_default()
                    .with_name(name.into())
                    .with_key(key.into()),
            )
            .await?
            .into_inner();
        Ok(RoleRevokePermissionResponse::new(resp))
    }

    /// Adds user
    #[inline]
    pub async fn user_add(
        &mut self,
        name: impl Into<String>,
        password: impl Into<String>,
        options: Option<UserAddOptions>,
    ) -> Result<UserAddResponse> {
        let resp = self
            .inner
            .user_add(
                options
                    .unwrap_or_default()
                    .with_name(name.into())
                    .with_pwd(password.into()),
            )
            .await?
            .into_inner();
        Ok(UserAddResponse::new(resp))
    }

    /// Gets user
    #[inline]
    pub async fn user_get(&mut self, name: impl Into<String>) -> Result<UserGetResponse> {
        let resp = self
            .inner
            .user_get(UserGetOptions::new(name.into()))
            .await?
            .into_inner();
        Ok(UserGetResponse::new(resp))
    }

    /// Lists user
    #[inline]
    pub async fn user_list(&mut self) -> Result<UserListResponse> {
        let resp = self
            .inner
            .user_list(AuthUserListOptions {})
            .await?
            .into_inner();
        Ok(UserListResponse::new(resp))
    }

    /// Deletes user
    #[inline]
    pub async fn user_delete(&mut self, name: impl Into<String>) -> Result<UserDeleteResponse> {
        let resp = self
            .inner
            .user_delete(UserDeleteOptions::new(name.into()))
            .await?
            .into_inner();
        Ok(UserDeleteResponse::new(resp))
    }

    /// Change user's password
    #[inline]
    pub async fn user_change_password(
        &mut self,
        name: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<UserChangePasswordResponse> {
        let resp = self
            .inner
            .user_change_password(UserChangePasswordOptions::new(name.into(), password.into()))
            .await?
            .into_inner();
        Ok(UserChangePasswordResponse::new(resp))
    }

    /// Grant role for an user
    #[inline]
    pub async fn user_grant_role(
        &mut self,
        name: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<UserGrantRoleResponse> {
        let resp = self
            .inner
            .user_grant_role(UserGrantRoleOptions::new(name.into(), role.into()))
            .await?
            .into_inner();
        Ok(UserGrantRoleResponse::new(resp))
    }

    /// Revoke role for an user
    #[inline]
    pub async fn user_revoke_role(
        &mut self,
        name: impl Into<String>,
        role: impl Into<String>,
    ) -> Result<UserRevokeRoleResponse> {
        let resp = self
            .inner
            .user_revoke_role(UserRevokeRoleOptions::new(name.into(), role.into()))
            .await?
            .into_inner();
        Ok(UserRevokeRoleResponse::new(resp))
    }
}

/// Options for `AuthEnable` operation.
#[derive(Debug, Default, Clone)]
pub struct AuthEnableOptions(PbAuthEnableRequest);

impl AuthEnableOptions {
    /// Creates a `AuthEnableOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbAuthEnableRequest {})
    }
}

impl From<AuthEnableOptions> for PbAuthEnableRequest {
    #[inline]
    fn from(options: AuthEnableOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthEnableRequest> for AuthEnableOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthEnableRequest> {
        Request::new(self.into())
    }
}

/// Response for `AuthEnable` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct AuthEnableResponse(PbAuthEnableResponse);

impl AuthEnableResponse {
    /// Creates a new `AuthEnableResponse` from pb auth response.
    #[inline]
    const fn new(resp: PbAuthEnableResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for `AuthDisable` operation.
#[derive(Debug, Default, Clone)]
pub struct AuthDisableOptions(PbAuthDisableRequest);

impl AuthDisableOptions {
    /// Creates a `AuthDisableOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbAuthDisableRequest {})
    }
}

impl From<AuthDisableOptions> for PbAuthDisableRequest {
    #[inline]
    fn from(options: AuthDisableOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthDisableRequest> for AuthDisableOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthDisableRequest> {
        Request::new(self.into())
    }
}

/// Response for `AuthDisable` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct AuthDisableResponse(PbAuthDisableResponse);

impl AuthDisableResponse {
    /// Creates a new `AuthDisableResponse` from pb auth response.
    #[inline]
    const fn new(resp: PbAuthDisableResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for `Authenticate` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct AuthenticateOptions(PbAuthenticateRequest);

impl AuthenticateOptions {
    /// Sets user's name and password.
    #[inline]
    fn with_user(mut self, name: String, password: String) -> Self {
        self.0.name = name;
        self.0.password = password;
        self
    }

    /// Creates a `AuthenticateOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbAuthenticateRequest {
            name: String::new(),
            password: String::new(),
        })
    }
}

impl From<AuthenticateOptions> for PbAuthenticateRequest {
    #[inline]
    fn from(options: AuthenticateOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthenticateRequest> for AuthenticateOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthenticateRequest> {
        Request::new(self.into())
    }
}

/// Response for `Authenticate` operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct AuthenticateResponse(PbAuthenticateResponse);

impl AuthenticateResponse {
    /// Creates a new `AuthenticateResponse` from pb auth response.
    #[inline]
    const fn new(resp: PbAuthenticateResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[allow(dead_code)]
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[allow(dead_code)]
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// An authorized token that can be used in succeeding RPCs
    #[inline]
    pub fn token(&self) -> &str {
        &self.0.token
    }
}

/// Options for `RoleAddOptions` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct RoleAddOptions(PbAuthRoleAddRequest);

impl RoleAddOptions {
    /// Creates a `RoleAddOptions`.
    #[inline]
    pub fn new(name: String) -> Self {
        Self(PbAuthRoleAddRequest { name })
    }
}

impl From<RoleAddOptions> for PbAuthRoleAddRequest {
    #[inline]
    fn from(options: RoleAddOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthRoleAddRequest> for RoleAddOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthRoleAddRequest> {
        Request::new(self.into())
    }
}

/// Response for role add operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RoleAddResponse(PbAuthRoleAddResponse);

impl RoleAddResponse {
    /// Creates a new `RoleAddResponse` from pb role add response.
    #[inline]
    const fn new(resp: PbAuthRoleAddResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for delete role operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct RoleDeleteOptions(PbAuthRoleDeleteRequest);

impl RoleDeleteOptions {
    /// Creates a `RoleDeleteOptions` to delete role.
    #[inline]
    pub fn new(name: String) -> Self {
        Self(PbAuthRoleDeleteRequest { role: name })
    }
}

impl From<RoleDeleteOptions> for PbAuthRoleDeleteRequest {
    #[inline]
    fn from(options: RoleDeleteOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthRoleDeleteRequest> for RoleDeleteOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthRoleDeleteRequest> {
        Request::new(self.into())
    }
}

/// Response for delete role operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RoleDeleteResponse(PbAuthRoleDeleteResponse);

impl RoleDeleteResponse {
    /// Creates a new `RoleDeleteResponse` from pb role delete response.
    #[inline]
    const fn new(resp: PbAuthRoleDeleteResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for get role operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct RoleGetOptions(PbAuthRoleGetRequest);

impl RoleGetOptions {
    /// Creates a `RoleGetOptions` to get role.
    #[inline]
    pub fn new(name: String) -> Self {
        Self(PbAuthRoleGetRequest { role: name })
    }
}

impl From<RoleGetOptions> for PbAuthRoleGetRequest {
    #[inline]
    fn from(options: RoleGetOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthRoleGetRequest> for RoleGetOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthRoleGetRequest> {
        Request::new(self.into())
    }
}

/// Role access permission.
#[derive(Debug, Clone)]
pub struct Permission {
    inner: PbPermission,
    with_prefix: bool,
    with_from_key: bool,
}

impl Permission {
    /// Creates a permission with operation type and key
    #[inline]
    pub fn new(perm_type: PermissionType, key: impl Into<Vec<u8>>) -> Self {
        Self {
            inner: PbPermission {
                perm_type: perm_type.into(),
                key: key.into(),
                range_end: Vec::new(),
            },
            with_prefix: false,
            with_from_key: false,
        }
    }

    /// Creates a read permission with key
    #[inline]
    pub fn read(key: impl Into<Vec<u8>>) -> Self {
        Permission::new(PermissionType::Read, key)
    }

    /// Creates a write permission with key
    #[inline]
    pub fn write(key: impl Into<Vec<u8>>) -> Self {
        Permission::new(PermissionType::Write, key)
    }

    /// Creates a read write permission with key
    #[inline]
    pub fn read_write(key: impl Into<Vec<u8>>) -> Self {
        Permission::new(PermissionType::Readwrite, key)
    }

    /// Sets range end for the permission
    #[inline]
    pub fn with_range_end(mut self, range_end: impl Into<Vec<u8>>) -> Self {
        self.inner.range_end = range_end.into();
        self.with_prefix = false;
        self.with_from_key = false;
        self
    }

    /// Sets the permission with all keys >= key.
    #[inline]
    pub fn with_from_key(mut self) -> Self {
        self.with_from_key = true;
        self.with_prefix = false;
        self
    }

    /// Sets the permission with all keys prefixed with key.
    #[inline]
    pub fn with_prefix(mut self) -> Self {
        self.with_prefix = true;
        self.with_from_key = false;
        self
    }

    /// Sets the permission with all keys.
    #[inline]
    pub fn with_all_keys(mut self) -> Self {
        self.inner.key.clear();
        self.with_from_key()
    }

    /// The key in bytes. An empty key is not allowed.
    #[inline]
    pub fn key(&self) -> &[u8] {
        &self.inner.key
    }

    /// The range end in bytes. maybe empty
    #[inline]
    pub fn range_end(&self) -> &[u8] {
        &self.inner.range_end
    }

    /// The key in string. An empty key is not allowed.
    #[inline]
    pub fn key_str(&self) -> Result<&str> {
        std::str::from_utf8(self.key()).map_err(From::from)
    }

    /// The key in string. An empty key is not allowed.
    ///
    /// # Safety
    /// This function is unsafe because it does not check that the bytes of the key are valid UTF-8.
    /// If this constraint is violated, undefined behavior results,
    /// as the rest of Rust assumes that [`&str`]s are valid UTF-8.
    #[inline]
    pub unsafe fn key_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(self.key())
    }

    /// The range end in string.
    #[inline]
    pub fn range_end_str(&self) -> Result<&str> {
        std::str::from_utf8(self.range_end()).map_err(From::from)
    }

    /// The range end in string.
    ///
    /// # Safety
    /// This function is unsafe because it does not check that the bytes of the key are valid UTF-8.
    /// If this constraint is violated, undefined behavior results,
    /// as the rest of Rust assumes that [`&str`]s are valid UTF-8.
    #[inline]
    pub unsafe fn range_end_str_unchecked(&self) -> &str {
        std::str::from_utf8_unchecked(self.key())
    }

    /// Gets the operation type of permission.
    #[inline]
    pub const fn get_type(&self) -> i32 {
        self.inner.perm_type
    }

    /// Indicates whether permission is with keys >= key.
    #[inline]
    pub const fn is_from_key(&self) -> bool {
        self.with_from_key
    }

    /// Indicates whether permission is with all keys prefixed with key.
    #[inline]
    pub const fn is_prefix(&self) -> bool {
        self.with_prefix
    }
}

impl PartialEq for Permission {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        if (self.with_prefix == other.with_prefix)
            && (self.with_from_key == other.with_from_key)
            && (self.inner.perm_type == other.inner.perm_type)
        {
            if self.inner.key == other.inner.key {
                true
            } else {
                (self.inner.key.is_empty() && other.inner.key == [b'\0'])
                    || (self.inner.key == [b'\0'] && other.inner.key.is_empty())
            }
        } else {
            false
        }
    }
}

impl From<&PbPermission> for Permission {
    #[inline]
    fn from(src: &PbPermission) -> Self {
        let mut perm = Permission {
            inner: PbPermission {
                perm_type: src.perm_type,
                key: src.key.clone(),
                range_end: src.range_end.clone(),
            },
            with_from_key: false,
            with_prefix: false,
        };

        if perm.inner.range_end == [b'\0'] {
            perm.with_from_key = true;
        } else if !perm.inner.range_end.is_empty() {
            let prefix = get_prefix(&perm.inner.key);
            if prefix == perm.inner.range_end {
                perm.with_prefix = true;
            }
        }
        perm
    }
}

impl From<Permission> for PbPermission {
    #[inline]
    fn from(mut perm: Permission) -> Self {
        let mut key_range = KeyRange::new();
        key_range.with_key(perm.inner.key);
        key_range.with_range(perm.inner.range_end);
        if perm.with_prefix {
            key_range.with_prefix();
        } else if perm.with_from_key {
            key_range.with_from_key();
        }
        let (key, range_end) = key_range.build();
        perm.inner.key = key;
        perm.inner.range_end = range_end;
        perm.inner
    }
}

/// Response for get role operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RoleGetResponse(PbAuthRoleGetResponse);

impl RoleGetResponse {
    /// Creates a new `RoleGetResponse` from pb role get response.
    #[inline]
    const fn new(resp: PbAuthRoleGetResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// The list of permissions by the `Get` request.
    #[inline]
    pub fn permissions(&self) -> Vec<Permission> {
        let mut perms = Vec::new();
        for p in &self.0.perm {
            perms.push(p.into());
        }
        perms
    }
}

/// Options for list role operation.
use PbAuthRoleListRequest as AuthRoleListOptions;

/// Response for list role operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RoleListResponse(PbAuthRoleListResponse);

impl RoleListResponse {
    /// Creates a new `RoleListResponse` from pb role list response.
    #[inline]
    const fn new(resp: PbAuthRoleListResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Gets roles in response.
    #[inline]
    pub fn roles(&self) -> &[String] {
        self.0.roles.as_slice()
    }
}

/// Options for grant role permission operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct RoleGrantPermissionOptions(PbAuthRoleGrantPermissionRequest);

impl RoleGrantPermissionOptions {
    /// Creates a "RoleGrantPermissionOptions" to grant role permission
    #[inline]
    pub fn new(name: String, perm: Permission) -> Self {
        Self(PbAuthRoleGrantPermissionRequest {
            name,
            perm: Some(perm.into()),
        })
    }
}

impl From<RoleGrantPermissionOptions> for PbAuthRoleGrantPermissionRequest {
    #[inline]
    fn from(options: RoleGrantPermissionOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthRoleGrantPermissionRequest> for RoleGrantPermissionOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthRoleGrantPermissionRequest> {
        Request::new(self.into())
    }
}

/// Response for grant role permission operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RoleGrantPermissionResponse(PbAuthRoleGrantPermissionResponse);

impl RoleGrantPermissionResponse {
    /// Creates a new `RoleGrantPermissionResponse` from pb role grant permission response.
    #[inline]
    const fn new(resp: PbAuthRoleGrantPermissionResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for grant role permission operation.
#[derive(Debug, Default, Clone)]
pub struct RoleRevokePermissionOptions {
    req: PbAuthRoleRevokePermissionRequest,
    key_range: KeyRange,
}

impl RoleRevokePermissionOptions {
    /// Create a new `RoleRevokePermissionOption` from pb role revoke permission.
    #[inline]
    pub const fn new() -> Self {
        Self {
            req: PbAuthRoleRevokePermissionRequest {
                role: String::new(),
                key: Vec::new(),
                range_end: Vec::new(),
            },
            key_range: KeyRange::new(),
        }
    }

    /// Sets name.
    #[inline]
    fn with_name(mut self, name: String) -> Self {
        self.req.role = name;
        self
    }

    /// Sets key.
    #[inline]
    fn with_key(mut self, key: impl Into<Vec<u8>>) -> Self {
        self.key_range.with_key(key);
        self
    }

    /// Specifies the range end.
    /// `end_key` must be lexicographically greater than start key.
    #[inline]
    pub fn with_range_end(mut self, range_end: impl Into<Vec<u8>>) -> Self {
        self.key_range.with_range(range_end);
        self
    }

    /// Sets all keys prefixed with key.
    #[inline]
    pub fn with_prefix(mut self) -> Self {
        self.key_range.with_prefix();
        self
    }

    /// Sets all keys >= key.
    #[inline]
    pub fn with_from_key(mut self) -> Self {
        self.key_range.with_from_key();
        self
    }

    /// Sets all keys.
    #[inline]
    pub fn with_all_keys(mut self) -> Self {
        self.key_range.with_all_keys();
        self
    }
}

impl From<RoleRevokePermissionOptions> for PbAuthRoleRevokePermissionRequest {
    #[inline]
    fn from(mut option: RoleRevokePermissionOptions) -> Self {
        let (key, range_end) = option.key_range.build();
        option.req.key = key;
        option.req.range_end = range_end;
        option.req
    }
}

impl IntoRequest<PbAuthRoleRevokePermissionRequest> for RoleRevokePermissionOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthRoleRevokePermissionRequest> {
        Request::new(self.into())
    }
}

/// Response for revoke role permission operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct RoleRevokePermissionResponse(PbAuthRoleRevokePermissionResponse);

impl RoleRevokePermissionResponse {
    /// Creates a new `RoleRevokePermissionResponse` from pb role revoke permission response.
    #[inline]
    const fn new(resp: PbAuthRoleRevokePermissionResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for `UserAdd` operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UserAddOptions(PbAuthUserAddRequest);

impl UserAddOptions {
    /// Creates a `UserAddOptions`.
    #[inline]
    pub const fn new() -> Self {
        Self(PbAuthUserAddRequest {
            name: String::new(),
            password: String::new(),
            options: Some(PbUserAddOptions { no_password: false }),
        })
    }

    /// Set name.
    #[inline]
    fn with_name(mut self, name: impl Into<String>) -> Self {
        self.0.name = name.into();
        self
    }

    /// Set password.
    #[inline]
    fn with_pwd(mut self, password: impl Into<String>) -> Self {
        self.0.password = password.into();
        self
    }

    /// Set no password.
    #[inline]
    pub const fn with_no_pwd(mut self) -> Self {
        self.0.options = Some(PbUserAddOptions { no_password: true });
        self
    }
}

impl From<UserAddOptions> for PbAuthUserAddRequest {
    #[inline]
    fn from(options: UserAddOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthUserAddRequest> for UserAddOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthUserAddRequest> {
        Request::new(self.into())
    }
}

/// Response for use add operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserAddResponse(PbAuthUserAddResponse);

impl UserAddResponse {
    /// Creates a new `UserAddReqResponse` from pb user add response.
    #[inline]
    const fn new(resp: PbAuthUserAddResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for get user operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UserGetOptions(PbAuthUserGetRequest);

impl UserGetOptions {
    /// Creates a `UserGetOptions` to get user.
    #[inline]
    pub fn new(name: String) -> Self {
        Self(PbAuthUserGetRequest { name })
    }
}

impl From<UserGetOptions> for PbAuthUserGetRequest {
    #[inline]
    fn from(options: UserGetOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthUserGetRequest> for UserGetOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthUserGetRequest> {
        Request::new(self.into())
    }
}

/// Response for get user operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserGetResponse(PbAuthUserGetResponse);

impl UserGetResponse {
    /// Creates a new `UserGetResponse` from pb user get response.
    #[inline]
    const fn new(resp: PbAuthUserGetResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Gets roles of the user in response.
    #[inline]
    pub fn roles(&self) -> &[String] {
        &self.0.roles
    }
}

/// Options for list user operation.
use PbAuthUserListRequest as AuthUserListOptions;

/// Response for list user operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserListResponse(PbAuthUserListResponse);

impl UserListResponse {
    /// Creates a new `UserListResponse` from pb user list response.
    #[inline]
    const fn new(resp: PbAuthUserListResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }

    /// Gets users in response.
    #[inline]
    pub fn users(&self) -> &[String] {
        &self.0.users
    }
}

/// Options for delete user operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UserDeleteOptions(PbAuthUserDeleteRequest);

impl UserDeleteOptions {
    /// Creates a `UserDeleteOptions` to delete user.
    #[inline]
    pub fn new(name: String) -> Self {
        Self(PbAuthUserDeleteRequest { name })
    }
}

impl From<UserDeleteOptions> for PbAuthUserDeleteRequest {
    #[inline]
    fn from(options: UserDeleteOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthUserDeleteRequest> for UserDeleteOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthUserDeleteRequest> {
        Request::new(self.into())
    }
}

/// Response for delete user operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserDeleteResponse(PbAuthUserDeleteResponse);

impl UserDeleteResponse {
    /// Creates a new `UserDeleteResponse` from pb user delete response.
    #[inline]
    const fn new(resp: PbAuthUserDeleteResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for change user's password operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UserChangePasswordOptions(PbAuthUserChangePasswordRequest);

impl UserChangePasswordOptions {
    /// Creates a `UserChangePasswordOptions` to change user's password.
    #[inline]
    pub fn new(name: String, new_password: String) -> Self {
        Self(PbAuthUserChangePasswordRequest {
            name,
            password: new_password,
        })
    }
}

impl From<UserChangePasswordOptions> for PbAuthUserChangePasswordRequest {
    #[inline]
    fn from(options: UserChangePasswordOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthUserChangePasswordRequest> for UserChangePasswordOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthUserChangePasswordRequest> {
        Request::new(self.into())
    }
}

/// Response for change user's password operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserChangePasswordResponse(PbAuthUserChangePasswordResponse);

impl UserChangePasswordResponse {
    /// Creates a new `UserChangePasswordResponse` from pb user change password response.
    #[inline]
    const fn new(resp: PbAuthUserChangePasswordResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for grant role for an user operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UserGrantRoleOptions(PbAuthUserGrantRoleRequest);

impl UserGrantRoleOptions {
    /// Creates a `UserGrantRoleOptions` to grant role for an user.
    #[inline]
    pub fn new(name: String, role: String) -> Self {
        Self(PbAuthUserGrantRoleRequest { user: name, role })
    }
}

impl From<UserGrantRoleOptions> for PbAuthUserGrantRoleRequest {
    #[inline]
    fn from(options: UserGrantRoleOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthUserGrantRoleRequest> for UserGrantRoleOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthUserGrantRoleRequest> {
        Request::new(self.into())
    }
}

/// Response for grant role for an user operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserGrantRoleResponse(PbAuthUserGrantRoleResponse);

impl UserGrantRoleResponse {
    /// Creates a new `UserGrantRoleResponse` from pb user grant role response.
    #[inline]
    const fn new(resp: PbAuthUserGrantRoleResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}

/// Options for revoke role for an user operation.
#[derive(Debug, Default, Clone)]
#[repr(transparent)]
pub struct UserRevokeRoleOptions(PbAuthUserRevokeRoleRequest);

impl UserRevokeRoleOptions {
    /// Creates a `UserRevokeRoleOptions` to revoke role for an user.
    #[inline]
    pub fn new(name: String, role: String) -> Self {
        Self(PbAuthUserRevokeRoleRequest { name, role })
    }
}

impl From<UserRevokeRoleOptions> for PbAuthUserRevokeRoleRequest {
    #[inline]
    fn from(options: UserRevokeRoleOptions) -> Self {
        options.0
    }
}

impl IntoRequest<PbAuthUserRevokeRoleRequest> for UserRevokeRoleOptions {
    #[inline]
    fn into_request(self) -> Request<PbAuthUserRevokeRoleRequest> {
        Request::new(self.into())
    }
}

/// Response for revoke role for an user operation.
#[cfg_attr(feature = "pub-response-field", visible::StructFields(pub))]
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct UserRevokeRoleResponse(PbAuthUserRevokeRoleResponse);

impl UserRevokeRoleResponse {
    /// Creates a new `UserRevokeRoleResponse` from pb user revoke role response.
    #[inline]
    const fn new(resp: PbAuthUserRevokeRoleResponse) -> Self {
        Self(resp)
    }

    /// Gets response header.
    #[inline]
    pub fn header(&self) -> Option<&ResponseHeader> {
        self.0.header.as_ref().map(From::from)
    }

    /// Takes the header out of the response, leaving a [`None`] in its place.
    #[inline]
    pub fn take_header(&mut self) -> Option<ResponseHeader> {
        self.0.header.take().map(ResponseHeader::new)
    }
}
