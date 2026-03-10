use std::sync::Arc;

use curp::{
    members::ClusterInfo,
    rpc::{
        ConfChange,
        ConfChangeType::{Add, AddLearner, Promote, Remove, Update},
    },
};
use itertools::Itertools;
use tracing::debug;
use xlinerpc::{Request, Response as XlineResponse, Status};
use utils::timestamp;
use xlineapi::{
    Cluster as GeneratedCluster, Member, MemberAddRequest, MemberAddResponse, MemberListRequest, MemberListResponse,
    MemberPromoteRequest, MemberPromoteResponse, MemberRemoveRequest, MemberRemoveResponse,
    MemberUpdateRequest, MemberUpdateResponse, command::CurpClient,
};

/// unified enum representing every supported cluster RPC request
enum ClusterRequest {
    Add(MemberAddRequest),
    Remove(MemberRemoveRequest),
    Update(MemberUpdateRequest),
    List(MemberListRequest),
    Promote(MemberPromoteRequest),
}

/// unified enum representing every supported cluster RPC response
enum ClusterResponse {
    Add(MemberAddResponse),
    Remove(MemberRemoveResponse),
    Update(MemberUpdateResponse),
    List(MemberListResponse),
    Promote(MemberPromoteResponse),
}

// conversions so that `handle_req` can remain generic
impl From<MemberAddRequest> for ClusterRequest {
    fn from(r: MemberAddRequest) -> Self {
        ClusterRequest::Add(r)
    }
}
impl From<MemberRemoveRequest> for ClusterRequest {
    fn from(r: MemberRemoveRequest) -> Self {
        ClusterRequest::Remove(r)
    }
}
impl From<MemberUpdateRequest> for ClusterRequest {
    fn from(r: MemberUpdateRequest) -> Self {
        ClusterRequest::Update(r)
    }
}
impl From<MemberListRequest> for ClusterRequest {
    fn from(r: MemberListRequest) -> Self {
        ClusterRequest::List(r)
    }
}
impl From<MemberPromoteRequest> for ClusterRequest {
    fn from(r: MemberPromoteRequest) -> Self {
        ClusterRequest::Promote(r)
    }
}

impl From<ClusterResponse> for MemberAddResponse {
    fn from(r: ClusterResponse) -> Self {
        if let ClusterResponse::Add(resp) = r {
            resp
        } else {
            unreachable!("wrong response type for MemberAddResponse")
        }
    }
}
impl From<ClusterResponse> for MemberRemoveResponse {
    fn from(r: ClusterResponse) -> Self {
        if let ClusterResponse::Remove(resp) = r {
            resp
        } else {
            unreachable!("wrong response type for MemberRemoveResponse")
        }
    }
}
impl From<ClusterResponse> for MemberUpdateResponse {
    fn from(r: ClusterResponse) -> Self {
        if let ClusterResponse::Update(resp) = r {
            resp
        } else {
            unreachable!("wrong response type for MemberUpdateResponse")
        }
    }
}
impl From<ClusterResponse> for MemberListResponse {
    fn from(r: ClusterResponse) -> Self {
        if let ClusterResponse::List(resp) = r {
            resp
        } else {
            unreachable!("wrong response type for MemberListResponse")
        }
    }
}
impl From<ClusterResponse> for MemberPromoteResponse {
    fn from(r: ClusterResponse) -> Self {
        if let ClusterResponse::Promote(resp) = r {
            resp
        } else {
            unreachable!("wrong response type for MemberPromoteResponse")
        }
    }
}


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


    /// generic handler for typed cluster requests; called by the gRPC
    /// methods so that all of them go through one place.
    async fn handle_req(
        &self,
        request: Request<ClusterRequest>,
    ) -> Result<XlineResponse<ClusterResponse>, Status> {
        let (req, _) = request.into_parts();
        match req {
            ClusterRequest::Add(mut req) => {
                let change_type = if req.is_learner {
                    i32::from(AddLearner)
                } else {
                    i32::from(Add)
                };
                let peer_url_ls = req.peer_ur_ls.into_iter().sorted().collect_vec();
                let node_id =
                    ClusterInfo::calculate_member_id(peer_url_ls.clone(), "", Some(timestamp()));
                let members = self
                    .propose_conf_change(vec![ConfChange { change_type, node_id, address: peer_url_ls }])
                    .await?;
                let resp = MemberAddResponse {
                    header: Some(self.header_gen.gen_header()),
                    member: members.iter().find(|m| m.id == node_id).cloned(),
                    members,
                };
                Ok(XlineResponse::new(ClusterResponse::Add(resp)))
            }
            ClusterRequest::Remove(req) => {
                let members = self
                    .propose_conf_change(vec![ConfChange {
                        change_type: i32::from(Remove),
                        node_id: req.id,
                        address: vec![],
                    }])
                    .await?;
                let resp = MemberRemoveResponse {
                    header: Some(self.header_gen.gen_header()),
                    members,
                };
                Ok(XlineResponse::new(ClusterResponse::Remove(resp)))
            }
            ClusterRequest::Update(req) => {
                let members = self
                    .propose_conf_change(vec![ConfChange {
                        change_type: i32::from(Update),
                        node_id: req.id,
                        address: req.peer_ur_ls,
                    }])
                    .await?;
                let resp = MemberUpdateResponse {
                    header: Some(self.header_gen.gen_header()),
                    members,
                };
                Ok(XlineResponse::new(ClusterResponse::Update(resp)))
            }
            ClusterRequest::List(req) => {
                let header = self.header_gen.gen_header();
                let members = self
                    .client
                    .fetch_cluster(req.linearizable)
                    .await?
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
                Ok(XlineResponse::new(ClusterResponse::List(resp)))
            }
            ClusterRequest::Promote(req) => {
                let members = self
                    .propose_conf_change(vec![ConfChange {
                        change_type: i32::from(Promote),
                        node_id: req.id,
                        address: vec![],
                    }])
                    .await?;
                let resp = MemberPromoteResponse {
                    header: Some(self.header_gen.gen_header()),
                    members,
                };
                Ok(XlineResponse::new(ClusterResponse::Promote(resp)))
            }
        }
    }
}

#[async_trait::async_trait]
impl GeneratedCluster for ClusterServer {
    async fn member_add(
        &self,
        request: Request<MemberAddRequest>,
    ) -> Result<XlineResponse<MemberAddResponse>, Status> {
        debug!("Receive MemberAddRequest {:?}", request.get_ref());
        self.handle_req(request).await
    }

    async fn member_remove(
        &self,
        request: Request<MemberRemoveRequest>,
    ) -> Result<XlineResponse<MemberRemoveResponse>, Status> {
        debug!("Receive MemberRemoveRequest {:?}", request.get_ref());
        self.handle_req(request).await
    }

    async fn member_update(
        &self,
        request: Request<MemberUpdateRequest>,
    ) -> Result<XlineResponse<MemberUpdateResponse>, Status> {
        debug!("Receive MemberUpdateRequest {:?}", request.get_ref());
        self.handle_req(request).await
    }

    async fn member_list(
        &self,
        request: Request<MemberListRequest>,
    ) -> Result<XlineResponse<MemberListResponse>, Status> {
        debug!("Receive MemberListRequest {:?}", request.get_ref());
        self.handle_req(request).await
    }

    async fn member_promote(
        &self,
        request: Request<MemberPromoteRequest>,
    ) -> Result<XlineResponse<MemberPromoteResponse>, Status> {
        debug!("Receive MemberPromoteRequest {:?}", request.get_ref());
        self.handle_req(request).await
    }
}
