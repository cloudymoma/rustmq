use super::*;
use crate::Result;
use crate::config::operations::{LoadScoreNormalization, LoadScoreWeights};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

pub fn composite_load_score(
    m: &LoadMetrics,
    norm: &LoadScoreNormalization,
    w: &LoadScoreWeights,
) -> f64 {
    let net_usage =
        (m.network_tx_bytes_sec + m.network_rx_bytes_sec) as f64 / norm.max_network_bytes_sec;
    let part_ratio = m.partition_count as f64 / norm.max_partitions_per_broker;
    let rate_ratio = m.message_rate as f64 / norm.max_message_rate;

    let weighted_sum = m.cpu_usage * w.cpu
        + m.memory_usage * w.memory
        + m.disk_usage * w.disk
        + net_usage * w.network
        + part_ratio * w.partition
        + rate_ratio * w.message_rate;

    let total_weight = w.cpu + w.memory + w.disk + w.network + w.partition + w.message_rate;
    weighted_sum / total_weight
}

pub struct PartitionRebalancerImpl {
    operation_progress: Arc<AsyncRwLock<HashMap<String, f64>>>,
    norm: LoadScoreNormalization,
    weights: LoadScoreWeights,
}

impl PartitionRebalancerImpl {
    pub fn new() -> Self {
        Self {
            operation_progress: Arc::new(AsyncRwLock::new(HashMap::new())),
            norm: LoadScoreNormalization::default(),
            weights: LoadScoreWeights::default(),
        }
    }

    pub fn with_config(norm: LoadScoreNormalization, weights: LoadScoreWeights) -> Self {
        Self {
            operation_progress: Arc::new(AsyncRwLock::new(HashMap::new())),
            norm,
            weights,
        }
    }

    fn calculate_broker_load_score(&self, broker: &BrokerInfo) -> f64 {
        composite_load_score(&broker.load_metrics, &self.norm, &self.weights)
    }

    fn select_partitions_to_move(
        &self,
        from_broker: &BrokerInfo,
        target_load_reduction: f64,
        assignments: &HashMap<TopicPartition, PartitionAssignment>,
    ) -> Vec<TopicPartition> {
        let mut partitions = Vec::new();
        let partitions_to_move =
            (target_load_reduction * from_broker.load_metrics.partition_count as f64) as usize;

        for (tp, assignment) in assignments {
            if assignment.leader == from_broker.id || assignment.replicas.contains(&from_broker.id)
            {
                partitions.push(tp.clone());
                if partitions.len() >= partitions_to_move.max(1) {
                    break;
                }
            }
        }

        partitions
    }

    fn find_best_target_broker(
        &self,
        brokers: &[BrokerInfo],
        exclude_broker: &str,
    ) -> Option<String> {
        brokers
            .iter()
            .filter(|b| b.id != exclude_broker && matches!(b.status, BrokerStatus::Healthy))
            .min_by(|a, b| {
                let score_a = self.calculate_broker_load_score(a);
                let score_b = self.calculate_broker_load_score(b);
                score_a
                    .partial_cmp(&score_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|b| b.id.clone())
    }
}

#[async_trait]
impl PartitionRebalancer for PartitionRebalancerImpl {
    async fn calculate_rebalance_plan(
        &self,
        brokers: Vec<BrokerInfo>,
        assignments: HashMap<TopicPartition, PartitionAssignment>,
    ) -> Result<RebalancePlan> {
        let mut moves = Vec::new();
        let operation_id = Uuid::new_v4().to_string();

        let total_load: f64 = brokers
            .iter()
            .map(|b| self.calculate_broker_load_score(b))
            .sum();
        let average_load = total_load / brokers.len() as f64;

        let overloaded_brokers: Vec<&BrokerInfo> = brokers
            .iter()
            .filter(|b| {
                let load = self.calculate_broker_load_score(b);
                load > average_load * 1.1
                    && matches!(b.status, BrokerStatus::Healthy | BrokerStatus::Draining)
            })
            .collect();

        for overloaded_broker in overloaded_brokers {
            let current_load = self.calculate_broker_load_score(overloaded_broker);
            let target_load_reduction = (current_load - average_load) / current_load;

            let partitions_to_move = self.select_partitions_to_move(
                overloaded_broker,
                target_load_reduction,
                &assignments,
            );

            for partition in partitions_to_move {
                if let Some(target_broker) =
                    self.find_best_target_broker(&brokers, &overloaded_broker.id)
                {
                    moves.push(PartitionMove {
                        topic_partition: partition,
                        from_broker: overloaded_broker.id.clone(),
                        to_broker: target_broker,
                        estimated_bytes: 100 * 1024 * 1024,
                    });
                }
            }
        }

        let total_bytes: u64 = moves.iter().map(|m| m.estimated_bytes).sum();
        let estimated_duration =
            Duration::from_secs(total_bytes.checked_div(10 * 1024 * 1024).unwrap_or(0));

        Ok(RebalancePlan {
            operation_id,
            moves,
            estimated_duration,
        })
    }

    async fn execute_rebalance(
        &self,
        plan: RebalancePlan,
        _assignments: HashMap<TopicPartition, PartitionAssignment>,
    ) -> Result<()> {
        let operation_id = plan.operation_id.clone();
        let total_moves = plan.moves.len();

        {
            let mut progress_map = self.operation_progress.write().await;
            progress_map.insert(operation_id.clone(), 0.0);
        }

        for (i, partition_move) in plan.moves.iter().enumerate() {
            tracing::info!(
                "Moving partition {}:{} from {} to {}",
                partition_move.topic_partition.topic,
                partition_move.topic_partition.partition,
                partition_move.from_broker,
                partition_move.to_broker
            );

            tokio::time::sleep(Duration::from_millis(10)).await;

            let progress = (i + 1) as f64 / total_moves as f64;
            {
                let mut progress_map = self.operation_progress.write().await;
                progress_map.insert(operation_id.clone(), progress);
            }
        }

        Ok(())
    }

    async fn get_rebalance_progress(&self, operation_id: &str) -> Result<f64> {
        let progress_map = self.operation_progress.read().await;
        Ok(progress_map.get(operation_id).copied().unwrap_or(0.0))
    }
}

pub struct MockPartitionRebalancer {
    operation_progress: Arc<AsyncRwLock<HashMap<String, f64>>>,
}

impl MockPartitionRebalancer {
    pub fn new() -> Self {
        Self {
            operation_progress: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl PartitionRebalancer for MockPartitionRebalancer {
    async fn calculate_rebalance_plan(
        &self,
        _brokers: Vec<BrokerInfo>,
        _assignments: HashMap<TopicPartition, PartitionAssignment>,
    ) -> Result<RebalancePlan> {
        Ok(RebalancePlan {
            operation_id: Uuid::new_v4().to_string(),
            moves: vec![],
            estimated_duration: Duration::from_secs(10),
        })
    }

    async fn execute_rebalance(
        &self,
        plan: RebalancePlan,
        _assignments: HashMap<TopicPartition, PartitionAssignment>,
    ) -> Result<()> {
        let operation_id = plan.operation_id.clone();
        {
            let mut progress_map = self.operation_progress.write().await;
            progress_map.insert(operation_id.clone(), 0.0);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        {
            let mut progress_map = self.operation_progress.write().await;
            progress_map.insert(operation_id, 1.0);
        }
        Ok(())
    }

    async fn get_rebalance_progress(&self, operation_id: &str) -> Result<f64> {
        let progress_map = self.operation_progress.read().await;
        Ok(progress_map.get(operation_id).copied().unwrap_or(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::service::PartitionAssignment;

    #[tokio::test]
    async fn test_composite_load_score_uniform() {
        let m = LoadMetrics {
            cpu_usage: 0.5,
            memory_usage: 0.5,
            disk_usage: 0.5,
            network_tx_bytes_sec: 312_500_000,
            network_rx_bytes_sec: 312_500_000,
            partition_count: 2000,
            leader_partition_count: 1000,
            message_rate: 250_000,
            consumer_lag_total: 0,
            timestamp: chrono::Utc::now(),
        };
        let score = composite_load_score(
            &m,
            &LoadScoreNormalization::default(),
            &LoadScoreWeights::default(),
        );
        assert!((score - 0.5).abs() < 0.01, "Expected ~0.5, got {}", score);
    }

    #[tokio::test]
    async fn test_composite_load_score_zero() {
        let m = LoadMetrics::default();
        let score = composite_load_score(
            &m,
            &LoadScoreNormalization::default(),
            &LoadScoreWeights::default(),
        );
        assert!((score - 0.0).abs() < 0.01, "Expected ~0.0, got {}", score);
    }

    #[tokio::test]
    async fn test_rebalance_plan_calculation() {
        let rebalancer = PartitionRebalancerImpl::new();

        let brokers = vec![
            BrokerInfo {
                id: "broker-1".to_string(),
                rack_id: "rack-1".to_string(),
                endpoints: vec!["broker-1:9092".to_string()],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics {
                    cpu_usage: 0.9,
                    memory_usage: 0.8,
                    disk_usage: 0.5,
                    network_tx_bytes_sec: 500_000,
                    network_rx_bytes_sec: 500_000,
                    partition_count: 100,
                    leader_partition_count: 50,
                    message_rate: 50000,
                    consumer_lag_total: 0,
                    timestamp: chrono::Utc::now(),
                },
            },
            BrokerInfo {
                id: "broker-2".to_string(),
                rack_id: "rack-2".to_string(),
                endpoints: vec!["broker-2:9092".to_string()],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics {
                    cpu_usage: 0.3,
                    memory_usage: 0.4,
                    disk_usage: 0.3,
                    network_tx_bytes_sec: 250_000,
                    network_rx_bytes_sec: 250_000,
                    partition_count: 20,
                    leader_partition_count: 10,
                    message_rate: 10000,
                    consumer_lag_total: 0,
                    timestamp: chrono::Utc::now(),
                },
            },
        ];

        let mut assignments = HashMap::new();
        for i in 0..100 {
            let tp = TopicPartition {
                topic: format!("topic-{}", i % 10),
                partition: (i / 10) as u32,
            };
            assignments.insert(
                tp,
                PartitionAssignment {
                    leader: "broker-1".to_string(),
                    replicas: vec!["broker-1".to_string(), "broker-2".to_string()],
                    in_sync_replicas: vec!["broker-1".to_string(), "broker-2".to_string()],
                    leader_epoch: 1,
                },
            );
        }

        let plan = rebalancer
            .calculate_rebalance_plan(brokers, assignments)
            .await
            .unwrap();
        assert!(!plan.moves.is_empty());
        assert!(plan.moves.iter().all(|m| m.from_broker == "broker-1"));
        assert!(plan.moves.iter().all(|m| m.to_broker == "broker-2"));
    }

    #[tokio::test]
    async fn test_rebalance_execution() {
        let rebalancer = PartitionRebalancerImpl::new();

        let plan = RebalancePlan {
            operation_id: "test-op".to_string(),
            moves: vec![PartitionMove {
                topic_partition: TopicPartition {
                    topic: "test-topic".to_string(),
                    partition: 0,
                },
                from_broker: "broker-1".to_string(),
                to_broker: "broker-2".to_string(),
                estimated_bytes: 100 * 1024 * 1024,
            }],
            estimated_duration: Duration::from_secs(10),
        };

        let mut assignments = HashMap::new();
        assignments.insert(
            TopicPartition {
                topic: "test-topic".to_string(),
                partition: 0,
            },
            PartitionAssignment {
                leader: "broker-1".to_string(),
                replicas: vec!["broker-1".to_string(), "broker-2".to_string()],
                in_sync_replicas: vec!["broker-1".to_string(), "broker-2".to_string()],
                leader_epoch: 1,
            },
        );

        let result = rebalancer.execute_rebalance(plan, assignments).await;
        assert!(result.is_ok());

        let progress = rebalancer.get_rebalance_progress("test-op").await.unwrap();
        assert_eq!(progress, 1.0);
    }
}
