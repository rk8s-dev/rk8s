use std::sync::Arc;

use curp::{
    members::ClusterInfo,
    rpc::{
        ConfChange,
        ConfChangeType::{Add, AddLearner, Promote, Remove, Update},
    },
};
use itertools::Itertools;
use xlinerpc::{Request, Status};
use utils::timestamp;
use xlineapi::{
    Cluster as GeneratedCluster, Member, MemberAddRequest, MemberAddResponse, MemberListRequest, MemberListResponse,
    MemberPromoteRequest, MemberPromoteResponse, MemberRemoveRequest, MemberRemoveResponse,
    MemberUpdateRequest, MemberUpdateResponse, command::CurpClient,
};

use tonic::{Request as TonicRequest, Response as TonicResponse, Status as TonicStatus};

use crate::header_gen::HeaderGenerator;

/// Cluster Server
pub(crate) struct ClusterServer {
    /// Consensus client
    client: Arc<CurpClient>,
    /// Header generator
    header_gen: Arc<HeaderGenerator>,
}

impl ClusterServer {
    /// New `ClusterServer`
    pub(crate) fn new(client: Arc<CurpClient>, header_gen: Arc<HeaderGenerator>) -> Self {
        Self { client, header_gen }
    }

    /// Send propose conf change request
    async fn propose_conf_change(&self, changes: Vec<ConfChange>) -> Result<Vec<Member>, Status> {
        Ok(self
            .client
            .propose_conf_change(changes)
            .await?
            .into_iter()
            .map(|member| Member {
                id: member.id,
                name: member.name.clone(),
                peer_ur_ls: member.peer_urls.clone(),
                client_ur_ls: member.client_urls.clone(),
                is_learner: member.is_learner,
            })
            .collect())
    }

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
        Req: Into<MemberAddRequest> + Into<MemberRemoveRequest> + Into<MemberUpdateRequest> + Into<MemberListRequest> + Into<MemberPromoteRequest> + Send + 'static,
        Res: From<MemberAddResponse> + From<MemberRemoveResponse> + From<MemberUpdateResponse> + From<MemberListResponse> + From<MemberPromoteResponse> + Send + 'static,
    {
        let xreq = self.tonic_to_xline(request);
        let xresp = self
            .handle_req(xreq)
            .await
            .map_err(|e| TonicStatus::from(e))?;
        let (res_data, _) = xresp.into_parts();
        Ok(TonicResponse::new(res_data))
    }
}

#[async_trait::async_trait]
impl GeneratedCluster for ClusterServer {
    async fn member_add(
        &self,
        request: TonicRequest<MemberAddRequest>,
    ) -> Result<TonicResponse<MemberAddResponse>, TonicStatus> {
        let xreq = self.tonic_to_xline(request);
        let (req, _) = xreq.into_parts();
        let change_type = if req.is_learner {
            i32::from(AddLearner)
        } else {
            i32::from(Add)
        };
        let peer_url_ls = req.peer_ur_ls.into_iter().sorted().collect_vec();
        // calculate node id based on addresses and current timestamp
        let node_id = ClusterInfo::calculate_member_id(peer_url_ls.clone(), "", Some(timestamp()));
        let members = self
            .propose_conf_change(vec![ConfChange {
                change_type,
                node_id,
                address: peer_url_ls,
            }])
            .await
            .map_err(TonicStatus::from)?;
        let resp = MemberAddResponse {
            header: Some(self.header_gen.gen_header()),
            member: members.iter().find(|m| m.id == node_id).cloned(),
            members,
        };
        Ok(TonicResponse::new(resp))
    }

    async fn member_remove(
        &self,
        request: TonicRequest<MemberRemoveRequest>,
    ) -> Result<TonicResponse<MemberRemoveResponse>, TonicStatus> {
        let xreq = self.tonic_to_xline(request);
        let (req, _) = xreq.into_parts();
        let members = self
            .propose_conf_change(vec![ConfChange {
                change_type: i32::from(Remove),
                node_id: req.id,
                address: vec![],
            }])
            .await
            .map_err(TonicStatus::from)?;
        let resp = MemberRemoveResponse {
            header: Some(self.header_gen.gen_header()),
            members,
        };
        Ok(TonicResponse::new(resp))
    }

    async fn member_update(
        &self,
        request: TonicRequest<MemberUpdateRequest>,
    ) -> Result<TonicResponse<MemberUpdateResponse>, TonicStatus> {
        let xreq = self.tonic_to_xline(request);
        let (req, _) = xreq.into_parts();
        let members = self
            .propose_conf_change(vec![ConfChange {
                change_type: i32::from(Update),
                node_id: req.id,
                address: req.peer_ur_ls,
            }])
            .await
            .map_err(TonicStatus::from)?;
        let resp = MemberUpdateResponse {
            header: Some(self.header_gen.gen_header()),
            members,
        };
        Ok(TonicResponse::new(resp))
    }

    async fn member_list(
        &self,
        request: TonicRequest<MemberListRequest>,
    ) -> Result<TonicResponse<MemberListResponse>, TonicStatus> {
        let xreq = self.tonic_to_xline(request);
        let (req, _) = xreq.into_parts();
        let header = self.header_gen.gen_header();
        let members = self
            .client
            .fetch_cluster(req.linearizable)
            .await
            .map_err(TonicStatus::from)?
            .members;
        let resp = MemberListResponse {
            header: Some(header),
            members: members
                .into_iter()
                .map(|member| Member {
                    id: member.id,
                    name: member.name,
                    peer_ur_ls: member.peer_urls,
                    client_ur_ls: member.client_urls,
                    is_learner: member.is_learner,
                })
                .collect(),
        };
        Ok(TonicResponse::new(resp))
    }

    async fn member_promote(
        &self,
        request: TonicRequest<MemberPromoteRequest>,
    ) -> Result<TonicResponse<MemberPromoteResponse>, TonicStatus> {
        let xreq = self.tonic_to_xline(request);
        let (req, _) = xreq.into_parts();
        let members = self
            .propose_conf_change(vec![ConfChange {
                change_type: i32::from(Promote),
                node_id: req.id,
                address: vec![],
            }])
            .await
            .map_err(TonicStatus::from)?;
        let resp = MemberPromoteResponse {
            header: Some(self.header_gen.gen_header()),
            members,
        };
        Ok(TonicResponse::new(resp))
    }
}
