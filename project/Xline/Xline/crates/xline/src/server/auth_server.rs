use std::sync::Arc;

use utils::hash_password;
use tracing::debug;
use xlinerpc::{Request, Response as XlineResponse, Status};
use xlineapi::{
    command::{Command, CommandResponse, CurpClient, SyncResponse},
    request_validation::RequestValidator,
};

use crate::{
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

/// Auth service trait
#[async_trait::async_trait]
pub trait Auth {
    /// AuthEnable enables authentication.
    async fn auth_enable(
        &self,
        request: Request<AuthEnableRequest>,
    ) -> Result<XlineResponse<AuthEnableResponse>, Status>;

    /// AuthDisable disables authentication.
    async fn auth_disable(
        &self,
        request: Request<AuthDisableRequest>,
    ) -> Result<XlineResponse<AuthDisableResponse>, Status>;

    /// AuthStatus gets the authentication status.
    async fn auth_status(
        &self,
        request: Request<AuthStatusRequest>,
    ) -> Result<XlineResponse<AuthStatusResponse>, Status>;

    /// Authenticate processes an authenticate request.
    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<XlineResponse<AuthenticateResponse>, Status>;

    /// UserAdd adds a new user.
    async fn user_add(
        &self,
        request: Request<AuthUserAddRequest>,
    ) -> Result<XlineResponse<AuthUserAddResponse>, Status>;

    /// UserGet gets detailed user information.
    async fn user_get(
        &self,
        request: Request<AuthUserGetRequest>,
    ) -> Result<XlineResponse<AuthUserGetResponse>, Status>;

    /// UserList gets a list of all users.
    async fn user_list(
        &self,
        request: Request<AuthUserListRequest>,
    ) -> Result<XlineResponse<AuthUserListResponse>, Status>;

    /// UserDelete deletes a user.
    async fn user_delete(
        &self,
        request: Request<AuthUserDeleteRequest>,
    ) -> Result<XlineResponse<AuthUserDeleteResponse>, Status>;

    /// UserChangePassword changes the password of a user.
    async fn user_change_password(
        &self,
        request: Request<AuthUserChangePasswordRequest>,
    ) -> Result<XlineResponse<AuthUserChangePasswordResponse>, Status>;

    /// UserGrantRole grants a role to a user.
    async fn user_grant_role(
        &self,
        request: Request<AuthUserGrantRoleRequest>,
    ) -> Result<XlineResponse<AuthUserGrantRoleResponse>, Status>;

    /// UserRevokeRole revokes a role from a user.
    async fn user_revoke_role(
        &self,
        request: Request<AuthUserRevokeRoleRequest>,
    ) -> Result<XlineResponse<AuthUserRevokeRoleResponse>, Status>;

    /// RoleAdd adds a new role.
    async fn role_add(
        &self,
        request: Request<AuthRoleAddRequest>,
    ) -> Result<XlineResponse<AuthRoleAddResponse>, Status>;

    /// RoleGet gets detailed role information.
    async fn role_get(
        &self,
        request: Request<AuthRoleGetRequest>,
    ) -> Result<XlineResponse<AuthRoleGetResponse>, Status>;

    /// RoleList gets a list of all roles.
    async fn role_list(
        &self,
        request: Request<AuthRoleListRequest>,
    ) -> Result<XlineResponse<AuthRoleListResponse>, Status>;

    /// RoleDelete deletes a role.
    async fn role_delete(
        &self,
        request: Request<AuthRoleDeleteRequest>,
    ) -> Result<XlineResponse<AuthRoleDeleteResponse>, Status>;

    /// RoleGrantPermission grants a permission to a role.
    async fn role_grant_permission(
        &self,
        request: Request<AuthRoleGrantPermissionRequest>,
    ) -> Result<XlineResponse<AuthRoleGrantPermissionResponse>, Status>;

    /// RoleRevokePermission revokes a permission from a role.
    async fn role_revoke_permission(
        &self,
        request: Request<AuthRoleRevokePermissionRequest>,
    ) -> Result<XlineResponse<AuthRoleRevokePermissionResponse>, Status>;
}

/// Auth Server
pub(crate) struct AuthServer {
    /// Consensus client
    client: Arc<CurpClient>,
    /// Auth Store
    auth_store: Arc<AuthStore>,
}

/// Get token from metadata
pub(crate) fn get_token(metadata: &xlinerpc::MetaData) -> Option<String> {
    let token_key = b"token";
    let auth_key = b"authorization";
    metadata
        .get(token_key)
        .or_else(|| metadata.get(auth_key))
        .and_then(|v| String::from_utf8(v.clone()).ok())
}

impl AuthServer {
    /// New `AuthServer`
    pub(crate) fn new(client: Arc<CurpClient>, auth_store: Arc<AuthStore>) -> Self {
        Self { client, auth_store }
    }

    /// Propose request and get result with fast/slow path
    async fn propose<T>(
        &self,
        request: Request<T>,
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
        request: Request<Req>,
    ) -> Result<XlineResponse<Res>, Status>
    where
        Req: Into<RequestWrapper>,
        Res: From<ResponseWrapper>,
    {
        let (cmd_res, sync_res) = self.propose(request).await?;
        let mut res_wrapper = cmd_res.into_inner();
        if let Some(sync_res) = sync_res {
            res_wrapper.update_revision(sync_res.revision());
        }
        Ok(XlineResponse::new(res_wrapper.into()))
    }
}

#[async_trait::async_trait]
impl Auth for AuthServer {
    async fn auth_enable(
        &self,
        request: Request<AuthEnableRequest>,
    ) -> Result<XlineResponse<AuthEnableResponse>, Status> {
        debug!("Receive AuthEnableRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn auth_disable(
        &self,
        request: Request<AuthDisableRequest>,
    ) -> Result<XlineResponse<AuthDisableResponse>, Status> {
        debug!("Receive AuthDisableRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn auth_status(
        &self,
        request: Request<AuthStatusRequest>,
    ) -> Result<XlineResponse<AuthStatusResponse>, Status> {
        debug!("Receive AuthStatusRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn authenticate(
        &self,
        request: Request<AuthenticateRequest>,
    ) -> Result<XlineResponse<AuthenticateResponse>, Status> {
        debug!("Receive AuthenticateRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn user_add(
        &self,
        mut request: Request<AuthUserAddRequest>,
    ) -> Result<XlineResponse<AuthUserAddResponse>, Status> {
        let user_add_req = request.get_mut();
        debug!("Receive AuthUserAddRequest {}", user_add_req);
        user_add_req.validation()?;
        let hashed_password = hash_password(user_add_req.password.as_bytes())
            .map_err(|err| Status::internal(format!("Failed to hash password: {err}")))?;
        user_add_req.hashed_password = hashed_password;
        user_add_req.password = String::new();
        self.handle_req(request).await
    }

    async fn user_get(
        &self,
        request: Request<AuthUserGetRequest>,
    ) -> Result<XlineResponse<AuthUserGetResponse>, Status> {
        debug!("Receive AuthUserGetRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn user_list(
        &self,
        request: Request<AuthUserListRequest>,
    ) -> Result<XlineResponse<AuthUserListResponse>, Status> {
        debug!("Receive AuthUserListRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn user_delete(
        &self,
        request: Request<AuthUserDeleteRequest>,
    ) -> Result<XlineResponse<AuthUserDeleteResponse>, Status> {
        debug!("Receive AuthUserDeleteRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn user_change_password(
        &self,
        mut request: Request<AuthUserChangePasswordRequest>,
    ) -> Result<XlineResponse<AuthUserChangePasswordResponse>, Status> {
        debug!("Receive AuthUserChangePasswordRequest {:?}", request);
        let user_change_password_req = request.get_mut();
        let hashed_password = hash_password(user_change_password_req.password.as_bytes())
            .map_err(|err| Status::internal(format!("Failed to hash password: {err}")))?;
        user_change_password_req.hashed_password = hashed_password;
        user_change_password_req.password = String::new();
        self.handle_req(request).await
    }

    async fn user_grant_role(
        &self,
        request: Request<AuthUserGrantRoleRequest>,
    ) -> Result<XlineResponse<AuthUserGrantRoleResponse>, Status> {
        debug!("Receive AuthUserGrantRoleRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn user_revoke_role(
        &self,
        request: Request<AuthUserRevokeRoleRequest>,
    ) -> Result<XlineResponse<AuthUserRevokeRoleResponse>, Status> {
        debug!("Receive AuthUserRevokeRoleRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn role_add(
        &self,
        request: Request<AuthRoleAddRequest>,
    ) -> Result<XlineResponse<AuthRoleAddResponse>, Status> {
        debug!("Receive AuthRoleAddRequest {:?}", request);
        request.get_ref().validation()?;
        self.handle_req(request).await
    }

    async fn role_get(
        &self,
        request: Request<AuthRoleGetRequest>,
    ) -> Result<XlineResponse<AuthRoleGetResponse>, Status> {
        debug!("Receive AuthRoleGetRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn role_list(
        &self,
        request: Request<AuthRoleListRequest>,
    ) -> Result<XlineResponse<AuthRoleListResponse>, Status> {
        debug!("Receive AuthRoleListRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn role_delete(
        &self,
        request: Request<AuthRoleDeleteRequest>,
    ) -> Result<XlineResponse<AuthRoleDeleteResponse>, Status> {
        debug!("Receive AuthRoleDeleteRequest {:?}", request);
        self.handle_req(request).await
    }

    async fn role_grant_permission(
        &self,
        request: Request<AuthRoleGrantPermissionRequest>,
    ) -> Result<XlineResponse<AuthRoleGrantPermissionResponse>, Status> {
        debug!(
            "Receive AuthRoleGrantPermissionRequest {}",
            request.get_ref()
        );
        request.get_ref().validation()?;
        self.handle_req(request).await
    }

    async fn role_revoke_permission(
        &self,
        request: Request<AuthRoleRevokePermissionRequest>,
    ) -> Result<XlineResponse<AuthRoleRevokePermissionResponse>, Status> {
        debug!(
            "Receive AuthRoleRevokePermissionRequest {}",
            request.get_ref()
        );
        self.handle_req(request).await
    }
}
