use std::sync::Arc;

use utils::hash_password;
use tracing::debug;

use xlinerpc::{Request, Response as XlineResponse, Status};
use xlineapi::{
    command::{Command, CommandResponse, CurpClient, SyncResponse},
    request_validation::RequestValidator,
    Auth as GeneratedAuth,
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
        let (data, _) = request.into_parts();
        let request = data.into();
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
        let (mut res_wrapper, _) = cmd_res.into_parts();
        if let Some(sync_res) = sync_res {
            res_wrapper.update_revision(sync_res.revision());
        }
        Ok(XlineResponse::new(res_wrapper.into()))
    }
}

impl AuthServer {
    /// convert tonic request into xlinerpc request copying metadata
    fn tonic_to_xline<Req>(&self, request: TonicRequest<Req>) -> Request<Req> {
        let (body, metadata) = request.into_parts();
        let mut xreq = Request::from_data(body);
        for (key, value) in metadata.iter() {
            xreq
                .meta_mut()
                .insert(key.as_bytes().to_vec(), value.as_bytes().to_vec());
        }
        xreq
    }

    /// helper bridging tonic -> xlinerpc and back again
    async fn handle_req_tonic<Req, Res>(
        &self,
        request: TonicRequest<Req>,
    ) -> Result<TonicResponse<Res>, TonicStatus>
    where
        Req: Into<RequestWrapper> + Send + 'static,
        Res: From<ResponseWrapper> + Send + 'static,
    {
        let xreq = self.tonic_to_xline(request);
        let xresp = self
            .handle_req(xreq)
            .await
            .map_err(|e| TonicStatus::internal(e.to_string()))?;
        let (res_data, _) = xresp.into_parts();
        Ok(TonicResponse::new(res_data))
    }
}

#[async_trait::async_trait]
impl GeneratedAuth for AuthServer {
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
        request: Request<AuthUserAddRequest>,
    ) -> Result<XlineResponse<AuthUserAddResponse>, Status> {
        let mut xreq = self.tonic_to_xline(request);
        let user_add_req = xreq.data_mut();
        debug!("Receive AuthUserAddRequest {}", user_add_req);
        user_add_req.validation()?;
        let hashed_password = hash_password(user_add_req.password.as_bytes())
            .map_err(|err| TonicStatus::internal(format!("Failed to hash password: {err}")))?;
        user_add_req.hashed_password = hashed_password;
        user_add_req.password = String::new();
        let xresp = self
            .handle_req(xreq)
            .await
            .map_err(TonicStatus::from)?;
        let (res_data, _) = xresp.into_parts();
        Ok(TonicResponse::new(res_data))
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
        request: Request<AuthUserChangePasswordRequest>,
    ) -> Result<XlineResponse<AuthUserChangePasswordResponse>, Status> {
        debug!("Receive AuthUserChangePasswordRequest {:?}", request);
        let mut xreq = self.tonic_to_xline(request);
        let user_change_password_req = xreq.data_mut();
        let hashed_password = hash_password(user_change_password_req.password.as_bytes())
            .map_err(|err| TonicStatus::internal(format!("Failed to hash password: {err}")))?;
        user_change_password_req.hashed_password = hashed_password;
        user_change_password_req.password = String::new();
        let xresp = self
            .handle_req(xreq)
            .await
            .map_err(|e| TonicStatus::internal(e.to_string()))?;
        let (res_data, _) = xresp.into_parts();
        Ok(TonicResponse::new(res_data))
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
        let mut xreq = self.tonic_to_xline(request);
        xreq.data().validation()?;
        let xresp = self
            .handle_req(xreq)
            .await
            .map_err(|e| TonicStatus::internal(e.to_string()))?;
        let (res_data, _) = xresp.into_parts();
        Ok(TonicResponse::new(res_data))
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
        let mut xreq = self.tonic_to_xline(request);
        xreq.data().validation()?;
        let xresp = self
            .handle_req(xreq)
            .await
            .map_err(|e| TonicStatus::internal(e.to_string()))?;
        let (res_data, _) = xresp.into_parts();
        Ok(TonicResponse::new(res_data))
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
