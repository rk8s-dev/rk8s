use std::sync::Arc;

use utils::hash_password;
use tracing::debug;

use xlinerpc::{Request, Response as XlineResponse, Status};
use xlineapi::{
    command::{Command, CommandResponse, CurpClient, SyncResponse},
    request_validation::RequestValidator,
    Auth as GeneratedAuth,
};
use tonic::{Request as TonicRequest, Response as TonicResponse, Status as TonicStatus};


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
        request: TonicRequest<AuthEnableRequest>,
    ) -> Result<TonicResponse<AuthEnableResponse>, TonicStatus> {
        debug!("Receive AuthEnableRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn auth_disable(
        &self,
        request: TonicRequest<AuthDisableRequest>,
    ) -> Result<TonicResponse<AuthDisableResponse>, TonicStatus> {
        debug!("Receive AuthDisableRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn auth_status(
        &self,
        request: TonicRequest<AuthStatusRequest>,
    ) -> Result<TonicResponse<AuthStatusResponse>, TonicStatus> {
        debug!("Receive AuthStatusRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn authenticate(
        &self,
        request: TonicRequest<AuthenticateRequest>,
    ) -> Result<TonicResponse<AuthenticateResponse>, TonicStatus> {
        debug!("Receive AuthenticateRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn user_add(
        &self,
        request: TonicRequest<AuthUserAddRequest>,
    ) -> Result<TonicResponse<AuthUserAddResponse>, TonicStatus> {
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
        request: TonicRequest<AuthUserGetRequest>,
    ) -> Result<TonicResponse<AuthUserGetResponse>, TonicStatus> {
        debug!("Receive AuthUserGetRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn user_list(
        &self,
        request: TonicRequest<AuthUserListRequest>,
    ) -> Result<TonicResponse<AuthUserListResponse>, TonicStatus> {
        debug!("Receive AuthUserListRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn user_delete(
        &self,
        request: TonicRequest<AuthUserDeleteRequest>,
    ) -> Result<TonicResponse<AuthUserDeleteResponse>, TonicStatus> {
        debug!("Receive AuthUserDeleteRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn user_change_password(
        &self,
        request: TonicRequest<AuthUserChangePasswordRequest>,
    ) -> Result<TonicResponse<AuthUserChangePasswordResponse>, TonicStatus> {
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
        request: TonicRequest<AuthUserGrantRoleRequest>,
    ) -> Result<TonicResponse<AuthUserGrantRoleResponse>, TonicStatus> {
        debug!("Receive AuthUserGrantRoleRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn user_revoke_role(
        &self,
        request: TonicRequest<AuthUserRevokeRoleRequest>,
    ) -> Result<TonicResponse<AuthUserRevokeRoleResponse>, TonicStatus> {
        debug!("Receive AuthUserRevokeRoleRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn role_add(
        &self,
        request: TonicRequest<AuthRoleAddRequest>,
    ) -> Result<TonicResponse<AuthRoleAddResponse>, TonicStatus> {
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
        request: TonicRequest<AuthRoleGetRequest>,
    ) -> Result<TonicResponse<AuthRoleGetResponse>, TonicStatus> {
        debug!("Receive AuthRoleGetRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn role_list(
        &self,
        request: TonicRequest<AuthRoleListRequest>,
    ) -> Result<TonicResponse<AuthRoleListResponse>, TonicStatus> {
        debug!("Receive AuthRoleListRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn role_delete(
        &self,
        request: TonicRequest<AuthRoleDeleteRequest>,
    ) -> Result<TonicResponse<AuthRoleDeleteResponse>, TonicStatus> {
        debug!("Receive AuthRoleDeleteRequest {:?}", request);
        self.handle_req_tonic(request).await
    }

    async fn role_grant_permission(
        &self,
        request: TonicRequest<AuthRoleGrantPermissionRequest>,
    ) -> Result<TonicResponse<AuthRoleGrantPermissionResponse>, TonicStatus> {
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
        request: TonicRequest<AuthRoleRevokePermissionRequest>,
    ) -> Result<TonicResponse<AuthRoleRevokePermissionResponse>, TonicStatus> {
        debug!(
            "Receive AuthRoleRevokePermissionRequest {}",
            request.get_ref()
        );
        self.handle_req_tonic(request).await
    }
}
