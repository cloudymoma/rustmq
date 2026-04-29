use std::sync::Arc;
use tonic::{Request, Response, Status};
use crate::proto::controller::broker_management_service_server::BrokerManagementService;
use crate::proto::controller::{BrokerHeartbeatRequest, BrokerHeartbeatResponse};
use crate::controller::service::ControllerService;
use tracing::debug;

pub struct BrokerManagementServiceImpl {
    controller_service: Arc<ControllerService>,
}

impl BrokerManagementServiceImpl {
    pub fn new(controller_service: Arc<ControllerService>) -> Self {
        Self { controller_service }
    }
}

#[tonic::async_trait]
impl BrokerManagementService for BrokerManagementServiceImpl {
    async fn broker_heartbeat(
        &self,
        request: Request<BrokerHeartbeatRequest>,
    ) -> Result<Response<BrokerHeartbeatResponse>, Status> {
        let req = request.into_inner();
        let broker_id = req.broker_id;
        let metrics = req
            .metrics
            .ok_or_else(|| Status::invalid_argument("Missing metrics"))?;

        let age = {
            let mut heartbeats = self.controller_service.broker_heartbeats.write().await;
            let prev = heartbeats.get(&broker_id).copied();
            heartbeats.insert(broker_id.clone(), chrono::Utc::now());
            prev.map(|p| (chrono::Utc::now() - p).num_seconds().max(0) as u64)
                .unwrap_or(0)
        };

        {
            let mut bm = self.controller_service.broker_metrics.write().await;
            bm.insert(broker_id.clone(), metrics.clone());
        }

        let load_metrics = crate::scaling::LoadMetrics {
            cpu_usage: metrics.cpu_usage,
            memory_usage: metrics.memory_usage,
            disk_usage: metrics.disk_usage,
            network_tx_bytes_sec: metrics.network_tx_bytes_sec,
            network_rx_bytes_sec: metrics.network_rx_bytes_sec,
            partition_count: metrics.partition_count as usize,
            leader_partition_count: metrics.leader_partition_count as usize,
            message_rate: metrics.message_rate,
            consumer_lag_total: metrics.consumer_lag_total,
            timestamp: metrics
                .timestamp
                .as_ref()
                .and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32))
                .unwrap_or_else(chrono::Utc::now),
        };

        let config = self.controller_service.scaling_config.read().await;
        let score = crate::scaling::operations::composite_load_score(
            &load_metrics,
            &config.load_score_normalization,
            &config.load_score_weights,
        );

        {
            let mut scores = self.controller_service.broker_load_scores.write().await;
            let prev_score = scores.get(&broker_id).copied().unwrap_or(score);
            let alpha = config.ewma_alpha;
            let ewma_score = alpha * score + (1.0 - alpha) * prev_score;
            scores.insert(broker_id.clone(), ewma_score);
            debug!("EWMA score for broker {}: {:.4}", broker_id, ewma_score);
        }

        Ok(Response::new(BrokerHeartbeatResponse {
            success: true,
            error_message: String::new(),
            heartbeat_age_seconds: age,
        }))
    }

    async fn drain_broker(
        &self,
        request: Request<crate::proto::controller::DrainRequest>,
    ) -> Result<Response<crate::proto::controller::DrainResponse>, Status> {
        let broker_id = request.into_inner().broker_id;

        {
            let mut statuses = self.controller_service.broker_statuses.write().await;
            statuses.insert(broker_id.clone(), crate::scaling::BrokerStatus::Draining);
            tracing::info!("Marked broker {} as Draining", broker_id);
        }

        let assignments = self
            .controller_service
            .metadata_manager
            .get_partition_assignments()
            .await;
        let scores = self.controller_service.broker_load_scores.read().await;

        for (tp, assignment) in assignments {
            if assignment.leader == broker_id {
                let mut best = None;
                let mut lowest = f64::INFINITY;
                for isr in &assignment.in_sync_replicas {
                    if isr == &broker_id {
                        continue;
                    }
                    let s = scores.get(isr).copied().unwrap_or(0.0);
                    if s < lowest {
                        lowest = s;
                        best = Some(isr.clone());
                    }
                }
                if let Some(new_leader) = best {
                    tracing::info!("Transferring leadership of {} from {} to {}", tp, broker_id, new_leader);
                    let req = crate::types::TransferLeadershipRequest {
                        topic_partition: tp.clone(),
                        current_leader_id: broker_id.clone(),
                        current_leader_epoch: assignment.leader_epoch,
                        new_leader_id: new_leader,
                    };
                    if let Err(e) = self.controller_service.transfer_partition_leadership(&broker_id, req).await {
                        tracing::error!("Failed to transfer leadership for {}: {}", tp, e);
                    }
                }
            }
        }

        Ok(Response::new(crate::proto::controller::DrainResponse {
            success: true,
            error_message: String::new(),
        }))
    }
}
