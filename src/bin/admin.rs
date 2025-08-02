use rustmq::Result;
use rustmq::controller::{ControllerService, CreateTopicRequest, DeleteTopicRequest, TopicConfig};
use rustmq::config::ScalingConfig;
use std::env;
use std::sync::Arc;
use tracing::{info, error};

/// Create a controller service for admin operations
async fn create_admin_controller() -> Result<Arc<ControllerService>> {
    // Default configuration for admin operations
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };
    
    // Connect to existing controller at default endpoints
    let controller_endpoints = vec![
        "127.0.0.1:9094".to_string(), // Default controller endpoint
    ];
    
    let controller = Arc::new(ControllerService::new(
        "admin-client".to_string(),
        controller_endpoints,
        scaling_config,
    ));
    
    Ok(controller)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    if args.len() < 2 {
        println!("RustMQ Admin Tool");
        println!("Usage: {} <command> [args...]", args[0]);
        println!();
        println!("Commands:");
        println!("  create-topic <name> <partitions> <replication_factor>");
        println!("  list-topics");
        println!("  describe-topic <name>");
        println!("  delete-topic <name>");
        println!("  cluster-health");
        println!("  serve-api [port]  - Start REST API server (default port: 8080)");
        std::process::exit(1);
    }
    
    let command = &args[1];
    
    match command.as_str() {
        "create-topic" => {
            if args.len() != 5 {
                error!("Usage: create-topic <name> <partitions> <replication_factor>");
                std::process::exit(1);
            }
            let topic_name = &args[2];
            let partitions: u32 = args[3].parse().unwrap_or_else(|_| {
                error!("Invalid partition count: {}", args[3]);
                std::process::exit(1);
            });
            let replication_factor: u32 = args[4].parse().unwrap_or_else(|_| {
                error!("Invalid replication factor: {}", args[4]);
                std::process::exit(1);
            });
            
            info!("Creating topic: {} with {} partitions and replication factor {}", 
                  topic_name, partitions, replication_factor);
            
            match create_admin_controller().await {
                Ok(controller) => {
                    let create_request = CreateTopicRequest {
                        name: topic_name.clone(),
                        partitions,
                        replication_factor,
                        config: Some(TopicConfig {
                            retention_ms: Some(86400000), // 24 hours default
                            segment_bytes: Some(1073741824), // 1GB default  
                            compression_type: Some("lz4".to_string()),
                        }),
                    };
                    
                    match controller.create_topic(create_request).await {
                        Ok(response) => {
                            if response.success {
                                println!("✅ Topic '{}' created successfully", topic_name);
                                if let Some(hint) = response.leader_hint {
                                    info!("Leader hint: {}", hint);
                                }
                            } else {
                                if let Some(error) = response.error_message {
                                    error!("❌ Failed to create topic '{}': {}", topic_name, error);
                                    std::process::exit(1);
                                } else {
                                    error!("❌ Failed to create topic '{}': Unknown error", topic_name);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            error!("❌ Error creating topic '{}': {}", topic_name, e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to connect to controller: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "list-topics" => {
            info!("Listing topics");
            
            match create_admin_controller().await {
                Ok(controller) => {
                    match controller.get_cluster_metadata().await {
                        Ok(metadata) => {
                            if metadata.topics.is_empty() {
                                println!("📝 No topics found");
                            } else {
                                println!("📝 Topics ({} total):", metadata.topics.len());
                                println!("{:<30} {:<12} {:<18} {:<20}", "NAME", "PARTITIONS", "REPLICATION", "CREATED");
                                println!("{}", "=".repeat(80));
                                
                                for topic in metadata.topics {
                                    let created = topic.created_at.format("%Y-%m-%d %H:%M:%S UTC");
                                    println!("{:<30} {:<12} {:<18} {:<20}", 
                                        topic.name, 
                                        topic.partitions, 
                                        topic.replication_factor,
                                        created
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!("❌ Failed to list topics: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to connect to controller: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "describe-topic" => {
            if args.len() != 3 {
                error!("Usage: describe-topic <name>");
                std::process::exit(1);
            }
            let topic_name = &args[2];
            info!("Describing topic: {}", topic_name);
            
            match create_admin_controller().await {
                Ok(controller) => {
                    match controller.get_cluster_metadata().await {
                        Ok(metadata) => {
                            if let Some(topic) = metadata.topics.iter().find(|t| t.name == *topic_name) {
                                println!("📊 Topic Details for '{}'", topic_name);
                                println!("{}", "=".repeat(50));
                                println!("Name:                {}", topic.name);
                                println!("Partitions:          {}", topic.partitions);
                                println!("Replication Factor:  {}", topic.replication_factor);
                                println!("Created:             {}", topic.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
                                
                                let config = &topic.config;
                                println!("\n📝 Configuration:");
                                if let Some(retention) = config.retention_ms {
                                    println!("  Retention:         {} ms ({} hours)", retention, retention / 3600000);
                                }
                                if let Some(segment_bytes) = config.segment_bytes {
                                    println!("  Segment Size:      {} bytes ({} MB)", segment_bytes, segment_bytes / 1024 / 1024);
                                }
                                if let Some(compression) = &config.compression_type {
                                    println!("  Compression:       {}", compression);
                                }
                                
                                // Show partition assignments
                                let partitions: Vec<_> = metadata.partition_assignments
                                    .iter()
                                    .filter(|(tp, _)| tp.topic == *topic_name)
                                    .collect();
                                
                                if !partitions.is_empty() {
                                    println!("\n🗂️  Partition Assignments:");
                                    println!("{:<12} {:<15} {:<20} {:<15} {:<10}", 
                                        "PARTITION", "LEADER", "REPLICAS", "IN_SYNC", "EPOCH");
                                    println!("{}", "-".repeat(75));
                                    
                                    for (tp, assignment) in partitions {
                                        let replicas_str = assignment.replicas.join(", ");
                                        let isr_str = assignment.in_sync_replicas.join(", ");
                                        println!("{:<12} {:<15} {:<20} {:<15} {:<10}", 
                                            tp.partition,
                                            &assignment.leader,
                                            replicas_str,
                                            isr_str,
                                            assignment.leader_epoch
                                        );
                                    }
                                } else {
                                    println!("\n🗂️  Partition Assignments: None found");
                                }
                            } else {
                                error!("❌ Topic '{}' not found", topic_name);
                                std::process::exit(1);
                            }
                        }
                        Err(e) => {
                            error!("❌ Failed to describe topic '{}': {}", topic_name, e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to connect to controller: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "delete-topic" => {
            if args.len() != 3 {
                error!("Usage: delete-topic <name>");
                std::process::exit(1);
            }
            let topic_name = &args[2];
            info!("Deleting topic: {}", topic_name);
            
            match create_admin_controller().await {
                Ok(controller) => {
                    let delete_request = DeleteTopicRequest {
                        name: topic_name.clone(),
                    };
                    
                    match controller.delete_topic(delete_request).await {
                        Ok(response) => {
                            if response.success {
                                println!("🗑️  Topic '{}' deleted successfully", topic_name);
                                if let Some(hint) = response.leader_hint {
                                    info!("Leader hint: {}", hint);
                                }
                            } else {
                                if let Some(error) = response.error_message {
                                    error!("❌ Failed to delete topic '{}': {}", topic_name, error);
                                    std::process::exit(1);
                                } else {
                                    error!("❌ Failed to delete topic '{}': Unknown error", topic_name);
                                    std::process::exit(1);
                                }
                            }
                        }
                        Err(e) => {
                            error!("❌ Error deleting topic '{}': {}", topic_name, e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to connect to controller: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "cluster-health" => {
            info!("Checking cluster health");
            
            match create_admin_controller().await {
                Ok(controller) => {
                    match controller.get_cluster_metadata().await {
                        Ok(metadata) => {
                            println!("🏥 Cluster Health Status");
                            println!("{}", "=".repeat(50));
                            
                            // Basic cluster info
                            let raft_info = controller.get_raft_info();
                            println!("Leader:              {}", metadata.leader.as_deref().unwrap_or("none"));
                            println!("Term:                {}", metadata.term);
                            println!("Raft Status:         {}", if raft_info.is_leader { "Leader" } else { "Follower" });
                            
                            // Broker health
                            println!("\n🖥️  Brokers ({} total):", metadata.brokers.len());
                            if metadata.brokers.is_empty() {
                                println!("  ❌ No brokers registered");
                            } else {
                                for broker in &metadata.brokers {
                                    println!("  ✅ {} ({}:{}) - Rack: {}", 
                                        broker.id, 
                                        broker.host, 
                                        broker.port_quic,
                                        broker.rack_id
                                    );
                                }
                            }
                            
                            // Topic health
                            println!("\n📝 Topics ({} total):", metadata.topics.len());
                            if metadata.topics.is_empty() {
                                println!("  ℹ️  No topics created");
                            } else {
                                let mut healthy_topics = 0;
                                for topic in &metadata.topics {
                                    let topic_partitions: Vec<_> = metadata.partition_assignments
                                        .iter()
                                        .filter(|(tp, _)| tp.topic == topic.name)
                                        .collect();
                                    
                                    let unhealthy_partitions = topic_partitions
                                        .iter()
                                        .filter(|(_, assignment)| assignment.leader.is_empty())
                                        .count();
                                    
                                    if unhealthy_partitions == 0 {
                                        healthy_topics += 1;
                                        println!("  ✅ {} ({} partitions, replication {})", 
                                            topic.name, topic.partitions, topic.replication_factor);
                                    } else {
                                        println!("  ⚠️  {} ({}/{} partitions without leader)", 
                                            topic.name, unhealthy_partitions, topic.partitions);
                                    }
                                }
                                
                                println!("\n📊 Topic Health Summary:");
                                println!("  Healthy Topics:      {}/{}", healthy_topics, metadata.topics.len());
                            }
                            
                            // Overall health assessment
                            let has_leader = metadata.leader.is_some();
                            let has_brokers = !metadata.brokers.is_empty();
                            let all_topics_healthy = metadata.topics.iter().all(|topic| {
                                let topic_partitions: Vec<_> = metadata.partition_assignments
                                    .iter()
                                    .filter(|(tp, _)| tp.topic == topic.name)
                                    .collect();
                                topic_partitions.iter().all(|(_, assignment)| !assignment.leader.is_empty())
                            });
                            
                            println!("\n🎯 Overall Cluster Health:");
                            if has_leader && has_brokers && all_topics_healthy {
                                println!("  Status: ✅ HEALTHY");
                                println!("  All systems operational");
                            } else {
                                println!("  Status: ⚠️  DEGRADED");
                                if !has_leader {
                                    println!("  Issue: No cluster leader");
                                }
                                if !has_brokers {
                                    println!("  Issue: No brokers available");
                                }
                                if !all_topics_healthy {
                                    println!("  Issue: Some topics have partitions without leaders");
                                }
                            }
                        }
                        Err(e) => {
                            error!("❌ Failed to get cluster health: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("❌ Failed to connect to controller: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "serve-api" => {
            let port = if args.len() > 2 {
                args[2].parse().unwrap_or(8080)
            } else {
                8080
            };
            
            info!("Starting RustMQ Admin API Server on port {}", port);
            
            // Create a mock controller service for demo purposes
            let scaling_config = rustmq::config::ScalingConfig {
                max_concurrent_additions: 3,
                max_concurrent_decommissions: 1,
                rebalance_timeout_ms: 300_000,
                traffic_migration_rate: 0.1,
                health_check_timeout_ms: 30_000,
            };
            
            let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
            let controller = std::sync::Arc::new(rustmq::controller::ControllerService::new(
                "controller-1".to_string(),
                peers,
                scaling_config,
            ));
            
            // Make this controller the leader for demo purposes
            match controller.start_election().await {
                Ok(true) => info!("Controller became leader"),
                Ok(false) => info!("Controller failed to become leader"),
                Err(e) => error!("Error during leader election: {}", e),
            }
            
            // Add some sample brokers for demo
            let broker1 = rustmq::types::BrokerInfo {
                id: "broker-1".to_string(),
                host: "localhost".to_string(),
                port_quic: 9092,
                port_rpc: 9093,
                rack_id: "rack-1".to_string(),
            };
            
            let broker2 = rustmq::types::BrokerInfo {
                id: "broker-2".to_string(),
                host: "localhost".to_string(),
                port_quic: 9192,
                port_rpc: 9193,
                rack_id: "rack-2".to_string(),
            };
            
            if let Err(e) = controller.register_broker(broker1).await {
                error!("Failed to register broker-1: {}", e);
            }
            
            if let Err(e) = controller.register_broker(broker2).await {
                error!("Failed to register broker-2: {}", e);
            }
            
            info!("Registered sample brokers");
            
            // Start the admin API server
            let admin_api = rustmq::admin::api::AdminApi::new(controller, port);
            admin_api.start().await?;
        }
        _ => {
            error!("Unknown command: {}", command);
            std::process::exit(1);
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustmq::types::BrokerInfo;
    use tokio::test;

    async fn setup_test_controller() -> Arc<ControllerService> {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };
        
        let controller = Arc::new(ControllerService::new(
            "test-admin-controller".to_string(),
            vec![],
            scaling_config,
        ));
        
        // Make controller leader
        controller.start_election().await.unwrap();
        
        // Add test brokers
        let broker1 = BrokerInfo {
            id: "admin-test-broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        let broker2 = BrokerInfo {
            id: "admin-test-broker-2".to_string(),
            host: "localhost".to_string(),
            port_quic: 9192,
            port_rpc: 9193,
            rack_id: "rack-2".to_string(),
        };
        
        controller.register_broker(broker1).await.unwrap();
        controller.register_broker(broker2).await.unwrap();
        
        controller
    }

    #[test]
    async fn test_create_admin_controller() {
        let controller = create_admin_controller().await;
        assert!(controller.is_ok());
        
        let controller = controller.unwrap();
        let raft_info = controller.get_raft_info();
        assert_eq!(raft_info.node_id, "admin-client");
    }

    #[test]
    async fn test_topic_creation_flow() {
        let controller = setup_test_controller().await;
        
        // Test topic creation
        let create_request = CreateTopicRequest {
            name: "admin-test-topic".to_string(),
            partitions: 3,
            replication_factor: 2,
            config: Some(TopicConfig {
                retention_ms: Some(86400000),
                segment_bytes: Some(1073741824),
                compression_type: Some("lz4".to_string()),
            }),
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        // Verify topic was created
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 1);
        assert_eq!(metadata.topics[0].name, "admin-test-topic");
        assert_eq!(metadata.topics[0].partitions, 3);
        assert_eq!(metadata.topics[0].replication_factor, 2);
    }

    #[test]
    async fn test_topic_listing_flow() {
        let controller = setup_test_controller().await;
        
        // Create multiple test topics
        for i in 1..=3 {
            let create_request = CreateTopicRequest {
                name: format!("admin-list-topic-{}", i),
                partitions: i,
                replication_factor: 1,
                config: None,
            };
            
            let response = controller.create_topic(create_request).await.unwrap();
            assert!(response.success);
        }
        
        // Test topic listing
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 3);
        
        // Verify topic names
        let topic_names: Vec<_> = metadata.topics.iter().map(|t| &t.name).collect();
        assert!(topic_names.contains(&&"admin-list-topic-1".to_string()));
        assert!(topic_names.contains(&&"admin-list-topic-2".to_string()));
        assert!(topic_names.contains(&&"admin-list-topic-3".to_string()));
    }

    #[test]
    async fn test_topic_description_flow() {
        let controller = setup_test_controller().await;
        
        // Create a test topic with specific configuration
        let create_request = CreateTopicRequest {
            name: "admin-describe-topic".to_string(),
            partitions: 2,
            replication_factor: 1,
            config: Some(TopicConfig {
                retention_ms: Some(604800000), // 7 days
                segment_bytes: Some(536870912), // 512MB
                compression_type: Some("snappy".to_string()),
            }),
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        // Test topic description
        let metadata = controller.get_cluster_metadata().await.unwrap();
        let topic = metadata.topics.iter().find(|t| t.name == "admin-describe-topic").unwrap();
        
        assert_eq!(topic.name, "admin-describe-topic");
        assert_eq!(topic.partitions, 2);
        assert_eq!(topic.replication_factor, 1);
        
        // Verify configuration
        assert_eq!(topic.config.retention_ms, Some(604800000));
        assert_eq!(topic.config.segment_bytes, Some(536870912));
        assert_eq!(topic.config.compression_type, Some("snappy".to_string()));
        
        // Check partition assignments exist
        let partitions: Vec<_> = metadata.partition_assignments
            .iter()
            .filter(|(tp, _)| tp.topic == "admin-describe-topic")
            .collect();
        assert_eq!(partitions.len(), 2); // Should have 2 partitions
    }

    #[test]
    async fn test_topic_deletion_flow() {
        let controller = setup_test_controller().await;
        
        // Create a test topic
        let create_request = CreateTopicRequest {
            name: "admin-delete-topic".to_string(),
            partitions: 1,
            replication_factor: 1,
            config: None,
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        // Verify topic exists
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 1);
        
        // Test topic deletion
        let delete_request = DeleteTopicRequest {
            name: "admin-delete-topic".to_string(),
        };
        
        let response = controller.delete_topic(delete_request).await.unwrap();
        assert!(response.success);
        
        // Verify topic was deleted
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 0);
    }

    #[test]
    async fn test_cluster_health_flow() {
        let controller = setup_test_controller().await;
        
        // Test cluster health assessment
        let metadata = controller.get_cluster_metadata().await.unwrap();
        let raft_info = controller.get_raft_info();
        
        // Verify basic cluster health
        assert!(metadata.leader.is_some());
        assert!(raft_info.is_leader);
        assert_eq!(metadata.brokers.len(), 2);
        assert!(!metadata.brokers.is_empty());
        
        // Test with topics
        let create_request = CreateTopicRequest {
            name: "health-test-topic".to_string(),
            partitions: 1,
            replication_factor: 1,
            config: None,
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        // Re-check health with topics
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 1);
        
        // Verify all topics are healthy (have leaders)
        let all_topics_healthy = metadata.topics.iter().all(|topic| {
            let topic_partitions: Vec<_> = metadata.partition_assignments
                .iter()
                .filter(|(tp, _)| tp.topic == topic.name)
                .collect();
            topic_partitions.iter().all(|(_, assignment)| !assignment.leader.is_empty())
        });
        assert!(all_topics_healthy);
    }

    #[test]
    async fn test_topic_creation_validation() {
        let controller = setup_test_controller().await;
        
        // Test creating topic with replication factor higher than available brokers
        let create_request = CreateTopicRequest {
            name: "invalid-replication-topic".to_string(),
            partitions: 1,
            replication_factor: 5, // We only have 2 brokers
            config: None,
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(!response.success);
        assert!(response.error_message.is_some());
        assert!(response.error_message.unwrap().contains("Not enough brokers"));
    }

    #[test]
    async fn test_topic_deletion_nonexistent() {
        let controller = setup_test_controller().await;
        
        // Try to delete a topic that doesn't exist
        let delete_request = DeleteTopicRequest {
            name: "nonexistent-topic".to_string(),
        };
        
        let response = controller.delete_topic(delete_request).await.unwrap();
        assert!(!response.success);
        assert!(response.error_message.is_some());
    }

    #[test]
    async fn test_empty_cluster_health() {
        // Test health check with minimal cluster (no topics)
        let controller = setup_test_controller().await;
        
        let metadata = controller.get_cluster_metadata().await.unwrap();
        let raft_info = controller.get_raft_info();
        
        // Cluster should still be considered healthy with leader and brokers
        assert!(metadata.leader.is_some());
        assert!(raft_info.is_leader);
        assert_eq!(metadata.brokers.len(), 2);
        assert_eq!(metadata.topics.len(), 0);
        
        // No topics means all topics are "healthy" (vacuously true)
        let all_topics_healthy = metadata.topics.iter().all(|topic| {
            let topic_partitions: Vec<_> = metadata.partition_assignments
                .iter()
                .filter(|(tp, _)| tp.topic == topic.name)
                .collect();
            topic_partitions.iter().all(|(_, assignment)| !assignment.leader.is_empty())
        });
        assert!(all_topics_healthy);
    }

    #[test]
    async fn test_multiple_topic_operations() {
        let controller = setup_test_controller().await;
        
        // Create multiple topics
        let topics = vec!["multi-op-1", "multi-op-2", "multi-op-3"];
        
        for topic_name in &topics {
            let create_request = CreateTopicRequest {
                name: topic_name.to_string(),
                partitions: 1,
                replication_factor: 1,
                config: None,
            };
            
            let response = controller.create_topic(create_request).await.unwrap();
            assert!(response.success);
        }
        
        // Verify all topics exist
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 3);
        
        // Delete one topic
        let delete_request = DeleteTopicRequest {
            name: "multi-op-2".to_string(),
        };
        
        let response = controller.delete_topic(delete_request).await.unwrap();
        assert!(response.success);
        
        // Verify only 2 topics remain
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 2);
        
        let remaining_topics: Vec<_> = metadata.topics.iter().map(|t| &t.name).collect();
        assert!(remaining_topics.contains(&&"multi-op-1".to_string()));
        assert!(remaining_topics.contains(&&"multi-op-3".to_string()));
        assert!(!remaining_topics.contains(&&"multi-op-2".to_string()));
    }

    #[test]
    async fn test_topic_configuration_options() {
        let controller = setup_test_controller().await;
        
        // Test topic with custom configuration
        let create_request = CreateTopicRequest {
            name: "config-test-topic".to_string(),
            partitions: 1,
            replication_factor: 1,
            config: Some(TopicConfig {
                retention_ms: Some(172800000), // 2 days
                segment_bytes: Some(268435456), // 256MB
                compression_type: Some("gzip".to_string()),
            }),
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        // Verify configuration was applied
        let metadata = controller.get_cluster_metadata().await.unwrap();
        let topic = metadata.topics.iter().find(|t| t.name == "config-test-topic").unwrap();
        
        assert_eq!(topic.config.retention_ms, Some(172800000));
        assert_eq!(topic.config.segment_bytes, Some(268435456));
        assert_eq!(topic.config.compression_type, Some("gzip".to_string()));
        
        // Test topic with default configuration (None)
        let create_request = CreateTopicRequest {
            name: "default-config-topic".to_string(),
            partitions: 1,
            replication_factor: 1,
            config: None,
        };
        
        let response = controller.create_topic(create_request).await.unwrap();
        assert!(response.success);
        
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 2);
    }
}