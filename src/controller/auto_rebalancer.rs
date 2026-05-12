use crate::controller::service::ControllerService;
use crate::scaling::PartitionRebalancer;
use crate::scaling::operations::PartitionRebalancerImpl;
use std::sync::Arc;
use tokio::time::{Duration, interval};
use tracing::{debug, error, info};

pub struct AutoRebalancer {
    controller_service: Arc<ControllerService>,
    rebalancer: Arc<PartitionRebalancerImpl>,
    last_rebalance_time: Arc<tokio::sync::RwLock<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl AutoRebalancer {
    pub fn new(controller_service: Arc<ControllerService>) -> Self {
        Self {
            controller_service,
            rebalancer: Arc::new(PartitionRebalancerImpl::new()),
            last_rebalance_time: Arc::new(tokio::sync::RwLock::new(None)),
        }
    }

    pub fn start(self) {
        tokio::spawn(async move {
            let mut tick = interval(Duration::from_secs(60));
            info!("AutoRebalancer started with 60s interval");

            loop {
                tick.tick().await;

                if !self.controller_service.is_leader() {
                    continue;
                }

                let enabled = {
                    let config = self.controller_service.scaling_config.read().await;
                    config.auto_rebalance_enabled
                };
                if !enabled {
                    continue;
                }

                if let Err(e) = self.evaluate_and_rebalance().await {
                    error!("AutoRebalancer error: {}", e);
                }
            }
        });
    }

    async fn evaluate_and_rebalance(&self) -> crate::Result<()> {
        let last_rebalance = { *self.last_rebalance_time.read().await };
        let cooldown = {
            let config = self.controller_service.scaling_config.read().await;
            config.rebalance_cooldown_secs
        };

        if let Some(last) = last_rebalance {
            if (chrono::Utc::now() - last).num_seconds() < cooldown as i64 {
                debug!("Skipping rebalance check due to cooldown");
                return Ok(());
            }
        }

        let brokers = self.controller_service.metadata_manager.get_brokers().await;
        let scores = self.controller_service.broker_load_scores.read().await;
        let metrics_map = self.controller_service.broker_metrics.read().await;

        let mut scaling_brokers = Vec::new();
        for broker in &brokers {
            let load_metrics = if let Some(m) = metrics_map.get(&broker.id) {
                crate::scaling::LoadMetrics {
                    cpu_usage: m.cpu_usage,
                    memory_usage: m.memory_usage,
                    disk_usage: m.disk_usage,
                    network_tx_bytes_sec: m.network_tx_bytes_sec,
                    network_rx_bytes_sec: m.network_rx_bytes_sec,
                    partition_count: m.partition_count as usize,
                    leader_partition_count: m.leader_partition_count as usize,
                    message_rate: m.message_rate,
                    consumer_lag_total: m.consumer_lag_total,
                    timestamp: m
                        .timestamp
                        .as_ref()
                        .and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32))
                        .unwrap_or_else(chrono::Utc::now),
                }
            } else {
                crate::scaling::LoadMetrics::default()
            };

            let status = {
                let statuses = self.controller_service.broker_statuses.read().await;
                statuses
                    .get(&broker.id)
                    .cloned()
                    .unwrap_or(crate::scaling::BrokerStatus::Healthy)
            };

            scaling_brokers.push(crate::scaling::BrokerInfo {
                id: broker.id.clone(),
                rack_id: broker.rack_id.clone(),
                endpoints: vec![format!("{}:{}", broker.host, broker.port_quic)],
                status,
                load_metrics,
            });
        }

        let mut trigger = false;

        for broker in &scaling_brokers {
            if broker.load_metrics.partition_count == 0 {
                info!("Trigger: New broker {} with 0 partitions", broker.id);
                trigger = true;
                break;
            }
        }

        if !trigger && !scores.is_empty() {
            let max_score = scores.values().cloned().fold(0.0_f64, f64::max);
            let min_score = scores.values().cloned().fold(f64::INFINITY, f64::min);
            let threshold = {
                let config = self.controller_service.scaling_config.read().await;
                config.imbalance_threshold_ratio
            };
            if min_score > 0.0 && max_score / min_score > threshold {
                info!(
                    "Trigger: Load imbalance (max/min = {:.2})",
                    max_score / min_score
                );
                trigger = true;
            }
        }

        if !trigger {
            for broker in &scaling_brokers {
                if matches!(broker.status, crate::scaling::BrokerStatus::Unhealthy) {
                    info!("Trigger: Unhealthy broker {}", broker.id);
                    trigger = true;
                    break;
                }
            }
        }

        if trigger {
            info!("Auto-rebalance triggered");
            let assignments = self
                .controller_service
                .metadata_manager
                .get_partition_assignments()
                .await;
            let plan = self
                .rebalancer
                .calculate_rebalance_plan(scaling_brokers, assignments.clone())
                .await?;
            if !plan.moves.is_empty() {
                info!("Executing rebalance plan with {} moves", plan.moves.len());
                self.rebalancer.execute_rebalance(plan, assignments).await?;
                let mut time = self.last_rebalance_time.write().await;
                *time = Some(chrono::Utc::now());
            } else {
                debug!("Rebalance plan is empty");
            }
        }

        Ok(())
    }
}
