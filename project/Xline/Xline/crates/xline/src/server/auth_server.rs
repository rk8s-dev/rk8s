use std::sync::Arc;

use tonic::Status;
use tonic::metadata::MetadataMap;
use tracing::debug;
use utils::hash_password;
// TODO: use our own status type
// use xlinerpc::status::Status;
use xlineapi::{
    command::{Command, CommandResponse, CurpClient, SyncResponse},
    request_validation::RequestValidator,
};

use crate::{
    router::endpoint::EndPoint as RouterEndpoint,
    rpc::{
        AuthDisableRequest, AuthDisableResponse, AuthEnableRequest, AuthEnableResponse,
        AuthRoleAddRequest, AuthRoleAddResponse, AuthRoleDeleteRequest, AuthRoleDeleteResponse,
        AuthRoleGetRequest, AuthRoleGetResponse, AuthRoleGrantPermissionRequest,
        AuthRoleGrantPermissionResponse, AuthRoleListRequest, AuthRoleListResponse,
        AuthRoleRevokePermissionRequest, AuthRoleRevokePermissionResponse, AuthStatusRequest,
        AuthStatusResponse, AuthUserAddRequest, AuthUserAddResponse, AuthUserChangePasswordRequest,
        AuthUserChangePasswordResponse, AuthUserDeleteRequest, AuthUserDeleteResponse,
        AuthUserGetRequest, AuthUserGetResponse, AuthUserGrantRoleRequest,
        AuthUserGrantRoleResponse, AuthUserListRequest, AuthUserListResponse,
        AuthUserRevokeRoleRequest, AuthUserRevokeRoleResponse, AuthenticateRequest,
        AuthenticateResponse, RequestWrapper, ResponseWrapper,
    },
    storage::AuthStore,
};

/// Auth Server
#[derive(Clone)]
pub(crate) struct AuthServer {
    /// Consensus client
    client: Arc<CurpClient>,
    /// Auth Store
    auth_store: Arc<AuthStore>,
}

/// Get token from metadata
pub(crate) fn get_token(metadata: &MetadataMap) -> Option<String> {
    metadata
        .get("token")
        .or_else(|| metadata.get("authorization"))
        .and_then(|v| v.to_str().map(String::from).ok())
}

impl AuthServer {
    /// New `AuthServer`
    pub(crate) fn new(client: Arc<CurpClient>, auth_store: Arc<AuthStore>) -> Self {
        Self { client, auth_store }
    }

    /// Propose request and get result with fast/slow path
    async fn propose<T>(
        &self,
        request: tonic::Request<T>,
    ) -> Result<(CommandResponse, Option<SyncResponse>), Status>
    where
        T: Into<RequestWrapper>,
    {
        let auth_info = self.auth_store.try_get_auth_info_from_request(&request)?;
        let request = request.into_inner().into();
        let cmd = Command::new_with_auth_info(request, auth_info);
        let res = self.client.propose(&cmd, None, false).await??;
        Ok(res)
    }

    /// Propose request and make a response
    async fn handle_req<Req, Res>(
        &self,
        request: tonic::Request<Req>,
    ) -> Result<tonic::Response<Res>, Status>
    where
        Req: Into<RequestWrapper>,
        Res: From<ResponseWrapper>,
    {
        let (cmd_res, sync_res) = self.propose(request).await?;
        let mut res_wrapper = cmd_res.into_inner();
        if let Some(sync_res) = sync_res {
            res_wrapper.update_revision(sync_res.revision());
        }
        Ok(tonic::Response::new(res_wrapper.into()))
    }

    pub(crate) async fn auth_enable(
        &self,
        request: tonic::Request<AuthEnableRequest>,
    ) -> Result<tonic::Response<AuthEnableResponse>, Status> {
        debug!("Receive AuthEnableRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn auth_disable(
        &self,
        request: tonic::Request<AuthDisableRequest>,
    ) -> Result<tonic::Response<AuthDisableResponse>, Status> {
        debug!("Receive AuthDisableRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn auth_status(
        &self,
        request: tonic::Request<AuthStatusRequest>,
    ) -> Result<tonic::Response<AuthStatusResponse>, Status> {
        debug!("Receive AuthStatusRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn authenticate(
        &self,
        request: tonic::Request<AuthenticateRequest>,
    ) -> Result<tonic::Response<AuthenticateResponse>, Status> {
        debug!("Receive AuthenticateRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn user_add(
        &self,
        mut request: tonic::Request<AuthUserAddRequest>,
    ) -> Result<tonic::Response<AuthUserAddResponse>, Status> {
        let user_add_req = request.get_mut();
        debug!("Receive AuthUserAddRequest {}", user_add_req);
        user_add_req.validation()?;
        let hashed_password = hash_password(user_add_req.password.as_bytes())
            .map_err(|err| Status::internal(format!("Failed to hash password: {err}")))?;
        user_add_req.hashed_password = hashed_password;
        user_add_req.password = String::new();
        self.handle_req(request).await
    }

    pub(crate) async fn user_get(
        &self,
        request: tonic::Request<AuthUserGetRequest>,
    ) -> Result<tonic::Response<AuthUserGetResponse>, Status> {
        debug!("Receive AuthUserGetRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn user_list(
        &self,
        request: tonic::Request<AuthUserListRequest>,
    ) -> Result<tonic::Response<AuthUserListResponse>, Status> {
        debug!("Receive AuthUserListRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn user_delete(
        &self,
        request: tonic::Request<AuthUserDeleteRequest>,
    ) -> Result<tonic::Response<AuthUserDeleteResponse>, Status> {
        debug!("Receive AuthUserDeleteRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn user_change_password(
        &self,
        mut request: tonic::Request<AuthUserChangePasswordRequest>,
    ) -> Result<tonic::Response<AuthUserChangePasswordResponse>, Status> {
        debug!("Receive AuthUserChangePasswordRequest {:?}", request);
        let user_change_password_req = request.get_mut();
        let hashed_password = hash_password(user_change_password_req.password.as_bytes())
            .map_err(|err| Status::internal(format!("Failed to hash password: {err}")))?;
        user_change_password_req.hashed_password = hashed_password;
        user_change_password_req.password = String::new();
        self.handle_req(request).await
    }

    pub(crate) async fn user_grant_role(
        &self,
        request: tonic::Request<AuthUserGrantRoleRequest>,
    ) -> Result<tonic::Response<AuthUserGrantRoleResponse>, Status> {
        debug!("Receive AuthUserGrantRoleRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn user_revoke_role(
        &self,
        request: tonic::Request<AuthUserRevokeRoleRequest>,
    ) -> Result<tonic::Response<AuthUserRevokeRoleResponse>, Status> {
        debug!("Receive AuthUserRevokeRoleRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn role_add(
        &self,
        request: tonic::Request<AuthRoleAddRequest>,
    ) -> Result<tonic::Response<AuthRoleAddResponse>, Status> {
        debug!("Receive AuthRoleAddRequest {:?}", request);
        request.get_ref().validation()?;
        self.handle_req(request).await
    }

    pub(crate) async fn role_get(
        &self,
        request: tonic::Request<AuthRoleGetRequest>,
    ) -> Result<tonic::Response<AuthRoleGetResponse>, Status> {
        debug!("Receive AuthRoleGetRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn role_list(
        &self,
        request: tonic::Request<AuthRoleListRequest>,
    ) -> Result<tonic::Response<AuthRoleListResponse>, Status> {
        debug!("Receive AuthRoleListRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn role_delete(
        &self,
        request: tonic::Request<AuthRoleDeleteRequest>,
    ) -> Result<tonic::Response<AuthRoleDeleteResponse>, Status> {
        debug!("Receive AuthRoleDeleteRequest {:?}", request);
        self.handle_req(request).await
    }

    pub(crate) async fn role_grant_permission(
        &self,
        request: tonic::Request<AuthRoleGrantPermissionRequest>,
    ) -> Result<tonic::Response<AuthRoleGrantPermissionResponse>, Status> {
        debug!(
            "Receive AuthRoleGrantPermissionRequest {}",
            request.get_ref()
        );
        request.get_ref().validation()?;
        self.handle_req(request).await
    }

    async fn role_revoke_permission(
        &self,
        request: tonic::Request<AuthRoleRevokePermissionRequest>,
    ) -> Result<tonic::Response<AuthRoleRevokePermissionResponse>, Status> {
        debug!(
            "Receive AuthRoleRevokePermissionRequest {}",
            request.get_ref()
        );
        self.handle_req(request).await
    }
}

pub(crate) struct Server {
    auth_server: Arc<AuthServer>,
}
impl Server {
    pub(crate) fn new(auth_server: AuthServer) -> Self {
        Self {
            auth_server: Arc::new(auth_server),
        }
    }
    pub(crate) fn endpoint(self) -> RouterEndpoint<Arc<AuthServer>> {
        RouterEndpoint::new(self.auth_server)
            .add_unary_fn(
                "/AuthEnable",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthEnableRequest>| async move {
                    this.auth_enable(request).await
                },
            )
            .add_unary_fn(
                "/AuthDisable",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthDisableRequest>| async move {
                    this.auth_disable(request).await
                },
            )
            .add_unary_fn(
                "/AuthStatus",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthStatusRequest>| async move {
                    this.auth_status(request).await
                },
            )
            .add_unary_fn(
                "/Authenticate",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthenticateRequest>| async move {
                    this.authenticate(request).await
                },
            )
            .add_unary_fn(
                "/UserAdd",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserAddRequest>| async move {
                    this.user_add(request).await
                },
            )
            .add_unary_fn(
                "/UserGet",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserGetRequest>| async move {
                    this.user_get(request).await
                },
            )
            .add_unary_fn(
                "/UserList",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserListRequest>| async move {
                    this.user_list(request).await
                },
            )
            .add_unary_fn(
                "/UserDelete",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserDeleteRequest>| async move {
                    this.user_delete(request).await
                },
            )
            .add_unary_fn(
                "/UserChangePassword",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserChangePasswordRequest>| async move {
                    this.user_change_password(request).await
                },
            )
            .add_unary_fn(
                "/UserGrantRole",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserGrantRoleRequest>| async move {
                    this.user_grant_role(request).await
                },
            )
            .add_unary_fn(
                "/UserRevokeRole",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthUserRevokeRoleRequest>| async move {
                    this.user_revoke_role(request).await
                },
            )
            .add_unary_fn(
                "/RoleAdd",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthRoleAddRequest>| async move {
                    this.role_add(request).await
                },
            )
            .add_unary_fn(
                "/RoleGet",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthRoleGetRequest>| async move {
                    this.role_get(request).await
                },
            )
            .add_unary_fn(
                "/RoleList",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthRoleListRequest>| async move {
                    this.role_list(request).await
                },
            )
            .add_unary_fn(
                "/RoleDelete",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthRoleDeleteRequest>| async move {
                    this.role_delete(request).await
                },
            )
            .add_unary_fn(
                "/RoleGrantPermission",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthRoleGrantPermissionRequest>| async move {
                    this.role_grant_permission(request).await
                },
            )
            .add_unary_fn(
                "/RoleRevokePermission",
                move |this: Arc<AuthServer>, request: tonic::Request<AuthRoleRevokePermissionRequest>| async move {
                    this.role_revoke_permission(request).await
                },
            )
    }
}
