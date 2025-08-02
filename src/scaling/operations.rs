use super::*;
use crate::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

pub struct PartitionRebalancerImpl {
    operation_progress: Arc<AsyncRwLock<HashMap<String, f64>>>,
}

impl PartitionRebalancerImpl {
    pub fn new() -> Self {
        Self {
            operation_progress: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }

    fn calculate_broker_load_score(&self, broker: &BrokerInfo) -> f64 {
        // Simple load calculation based on multiple factors
        let cpu_weight = 0.3;
        let memory_weight = 0.3;
        let partition_weight = 0.2;
        let message_rate_weight = 0.2;
        
        // Normalize partition count (assume max 1000 partitions per broker)
        let normalized_partitions = (broker.load_metrics.partition_count as f64) / 1000.0;
        
        // Normalize message rate (assume max 100k msg/s per broker)
        let normalized_message_rate = (broker.load_metrics.message_rate as f64) / 100_000.0;
        
        cpu_weight * broker.load_metrics.cpu_usage +
        memory_weight * broker.load_metrics.memory_usage +
        partition_weight * normalized_partitions +
        message_rate_weight * normalized_message_rate
    }

    fn select_partitions_to_move(
        &self,
        from_broker: &BrokerInfo,
        target_load_reduction: f64,
    ) -> Vec<TopicPartition> {
        // In a real implementation, this would query the metadata store
        // For testing, return mock partitions
        let mut partitions = Vec::new();
        let partitions_to_move = (target_load_reduction * from_broker.load_metrics.partition_count as f64) as usize;
        
        for i in 0..partitions_to_move.min(5) { // Limit for testing
            partitions.push(TopicPartition {
                topic: format!("topic-{}", i),
                partition: i as u32,
            });
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
                score_a.partial_cmp(&score_b).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|b| b.id.clone())
    }
}

#[async_trait]
impl PartitionRebalancer for PartitionRebalancerImpl {
    async fn calculate_rebalance_plan(&self, brokers: Vec<BrokerInfo>) -> Result<RebalancePlan> {
        let mut moves = Vec::new();
        let operation_id = Uuid::new_v4().to_string();

        // Calculate average load
        let total_load: f64 = brokers.iter()
            .map(|b| self.calculate_broker_load_score(b))
            .sum();
        let average_load = total_load / brokers.len() as f64;

        // Find brokers that are above average load
        let overloaded_brokers: Vec<&BrokerInfo> = brokers
            .iter()
            .filter(|b| {
                let load = self.calculate_broker_load_score(b);
                load > average_load * 1.1 && matches!(b.status, BrokerStatus::Healthy | BrokerStatus::Draining)
            })
            .collect();

        for overloaded_broker in overloaded_brokers {
            let current_load = self.calculate_broker_load_score(overloaded_broker);
            let target_load_reduction = (current_load - average_load) / current_load;
            
            let partitions_to_move = self.select_partitions_to_move(overloaded_broker, target_load_reduction);
            
            for partition in partitions_to_move {
                if let Some(target_broker) = self.find_best_target_broker(&brokers, &overloaded_broker.id) {
                    moves.push(PartitionMove {
                        topic_partition: partition,
                        from_broker: overloaded_broker.id.clone(),
                        to_broker: target_broker,
                        estimated_bytes: 100 * 1024 * 1024, // 100MB estimate
                    });
                }
            }
        }

        // Estimate duration based on number of moves and data size
        let total_bytes: u64 = moves.iter().map(|m| m.estimated_bytes).sum();
        let estimated_duration = Duration::from_secs(total_bytes / (10 * 1024 * 1024)); // 10MB/s transfer rate

        Ok(RebalancePlan {
            operation_id,
            moves,
            estimated_duration,
        })
    }

    async fn execute_rebalance(&self, plan: RebalancePlan) -> Result<()> {
        let operation_id = plan.operation_id.clone();
        let total_moves = plan.moves.len();
        
        {
            let mut progress_map = self.operation_progress.write().await;
            progress_map.insert(operation_id.clone(), 0.0);
        }

        for (i, partition_move) in plan.moves.iter().enumerate() {
            // Simulate partition move operation
            tracing::info!(
                "Moving partition {}:{} from {} to {}",
                partition_move.topic_partition.topic,
                partition_move.topic_partition.partition,
                partition_move.from_broker,
                partition_move.to_broker
            );

            // Simulate time for partition move
            tokio::time::sleep(Duration::from_millis(100)).await;

            // Update progress
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

// Mock implementation for testing
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
    async fn calculate_rebalance_plan(&self, _brokers: Vec<BrokerInfo>) -> Result<RebalancePlan> {
        let operation_id = Uuid::new_v4().to_string();
        
        // Return empty plan for testing
        Ok(RebalancePlan {
            operation_id,
            moves: vec![],
            estimated_duration: Duration::from_secs(10),
        })
    }

    async fn execute_rebalance(&self, plan: RebalancePlan) -> Result<()> {
        let operation_id = plan.operation_id.clone();
        
        {
            let mut progress_map = self.operation_progress.write().await;
            progress_map.insert(operation_id.clone(), 0.0);
        }

        // Simulate quick rebalance
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
                    cpu_usage: 0.9, // High CPU
                    memory_usage: 0.8,
                    network_io: 1000,
                    partition_count: 100,
                    message_rate: 50000,
                },
            },
            BrokerInfo {
                id: "broker-2".to_string(),
                rack_id: "rack-2".to_string(),
                endpoints: vec!["broker-2:9092".to_string()],
                status: BrokerStatus::Healthy,
                load_metrics: LoadMetrics {
                    cpu_usage: 0.3, // Low CPU
                    memory_usage: 0.4,
                    network_io: 500,
                    partition_count: 20,
                    message_rate: 10000,
                },
            },
        ];

        let plan = rebalancer.calculate_rebalance_plan(brokers).await.unwrap();
        
        // Should generate moves from overloaded broker to underloaded broker
        assert!(!plan.moves.is_empty());
        assert!(plan.moves.iter().all(|m| m.from_broker == "broker-1"));
        assert!(plan.moves.iter().all(|m| m.to_broker == "broker-2"));
    }

    #[tokio::test]
    async fn test_rebalance_execution() {
        let rebalancer = PartitionRebalancerImpl::new();
        
        let plan = RebalancePlan {
            operation_id: "test-op".to_string(),
            moves: vec![
                PartitionMove {
                    topic_partition: TopicPartition {
                        topic: "test-topic".to_string(),
                        partition: 0,
                    },
                    from_broker: "broker-1".to_string(),
                    to_broker: "broker-2".to_string(),
                    estimated_bytes: 100 * 1024 * 1024,
                },
            ],
            estimated_duration: Duration::from_secs(10),
        };

        let result = rebalancer.execute_rebalance(plan).await;
        assert!(result.is_ok());

        let progress = rebalancer.get_rebalance_progress("test-op").await.unwrap();
        assert_eq!(progress, 1.0);
    }
}