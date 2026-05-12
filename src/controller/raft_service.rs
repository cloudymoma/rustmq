use crate::controller::{NodeId, RustMqNode, RustMqTypeConfig};
use crate::proto::controller::raft_service_server::RaftService;
use crate::proto::controller::{
    SimpleAppendEntriesRequest, SimpleAppendEntriesResponse, SimpleInstallSnapshotRequest,
    SimpleInstallSnapshotResponse, SimpleVoteRequest, SimpleVoteResponse,
};
use openraft::Raft;
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use tonic::{Request, Response, Status};
use tracing::debug;

/// gRPC service implementation for OpenRaft communication
pub struct RustMqRaftService {
    raft: Raft<RustMqTypeConfig>,
}

impl RustMqRaftService {
    pub fn new(raft: Raft<RustMqTypeConfig>) -> Self {
        Self { raft }
    }
}

#[tonic::async_trait]
impl RaftService for RustMqRaftService {
    async fn vote(
        &self,
        request: Request<SimpleVoteRequest>,
    ) -> Result<Response<SimpleVoteResponse>, Status> {
        let req = request.into_inner();
        debug!("Received vote request from node {}", req.candidate_id);

        let last_log_id: Option<openraft::LogId<NodeId>> = if req.last_log_id.is_empty() {
            None
        } else {
            Some(bincode::deserialize(&req.last_log_id).map_err(|e| {
                Status::internal(format!("Failed to deserialize last_log_id: {}", e))
            })?)
        };

        // Pass to OpenRaft
        let resp = self
            .raft
            .vote(VoteRequest {
                vote: openraft::Vote::new(req.term, req.candidate_id),
                last_log_id,
            })
            .await
            .map_err(|e| Status::internal(format!("Raft vote error: {}", e)))?;

        // Serialize the response
        let reply = SimpleVoteResponse {
            term: resp.vote.leader_id.term,
            vote_granted: resp.vote_granted,
        };

        Ok(Response::new(reply))
    }

    async fn append_entries(
        &self,
        request: Request<SimpleAppendEntriesRequest>,
    ) -> Result<Response<SimpleAppendEntriesResponse>, Status> {
        let req = request.into_inner();

        let prev_log_id: Option<openraft::LogId<NodeId>> = if req.prev_log_id.is_empty() {
            None
        } else {
            Some(bincode::deserialize(&req.prev_log_id).map_err(|e| {
                Status::internal(format!("Failed to deserialize prev_log_id: {}", e))
            })?)
        };

        let entries: Vec<openraft::Entry<RustMqTypeConfig>> = bincode::deserialize(&req.entries)
            .map_err(|e| Status::internal(format!("Failed to deserialize entries: {}", e)))?;

        // Pass to OpenRaft
        let resp = self
            .raft
            .append_entries(AppendEntriesRequest {
                vote: openraft::Vote {
                    leader_id: openraft::CommittedLeaderId::new(req.term, req.leader_id),
                    committed: true,
                },
                prev_log_id,
                entries,
                leader_commit: if req.leader_commit > 0 {
                    Some(openraft::LogId::new(
                        openraft::CommittedLeaderId::new(
                            req.leader_commit_term,
                            req.leader_commit_node_id,
                        ),
                        req.leader_commit,
                    ))
                } else {
                    None
                },
            })
            .await
            .map_err(|e| Status::internal(format!("Raft append_entries error: {}", e)))?;

        // Serialize the response
        let mut reply = SimpleAppendEntriesResponse {
            term: req.term,
            success: false,
            conflict_log_id: Vec::new().into(),
        };

        match resp {
            AppendEntriesResponse::Success => {
                reply.success = true;
            }
            AppendEntriesResponse::PartialSuccess(conflict) => {
                reply.success = false;
                reply.conflict_log_id = bincode::serialize(&conflict).unwrap_or_default().into();
            }
            AppendEntriesResponse::HigherVote(vote) => {
                reply.term = vote.leader_id.term;
                reply.success = false;
            }
            AppendEntriesResponse::Conflict => {
                reply.success = false;
            }
        }

        Ok(Response::new(reply))
    }

    async fn install_snapshot(
        &self,
        request: Request<SimpleInstallSnapshotRequest>,
    ) -> Result<Response<SimpleInstallSnapshotResponse>, Status> {
        let req = request.into_inner();

        let meta: openraft::SnapshotMeta<NodeId, RustMqNode> = bincode::deserialize(&req.meta)
            .map_err(|e| Status::internal(format!("Failed to deserialize snapshot meta: {}", e)))?;

        // Pass to OpenRaft
        let resp = self
            .raft
            .install_snapshot(InstallSnapshotRequest {
                vote: openraft::Vote::new(req.term, req.leader_id),
                meta,
                offset: req.offset,
                data: req.data.to_vec(),
                done: req.done,
            })
            .await
            .map_err(|e| Status::internal(format!("Raft install_snapshot error: {}", e)))?;

        // Serialize the response
        let reply = SimpleInstallSnapshotResponse {
            term: resp.vote.leader_id.term,
        };

        Ok(Response::new(reply))
    }
}
