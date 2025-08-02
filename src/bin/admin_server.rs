use rustmq::admin::api::AdminApi;
use rustmq::controller::ControllerService;
use rustmq::config::ScalingConfig;
use rustmq::Result;
use std::env;
use std::sync::Arc;
use tracing::{info, error};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    let args: Vec<String> = env::args().collect();
    let port = if args.len() > 1 {
        args[1].parse().unwrap_or(8080)
    } else {
        8080
    };
    
    info!("Starting RustMQ Admin API Server on port {}", port);
    
    // Create a mock controller service for demo purposes
    // In production, this would connect to the actual controller cluster
    let scaling_config = ScalingConfig {
        max_concurrent_additions: 3,
        max_concurrent_decommissions: 1,
        rebalance_timeout_ms: 300_000,
        traffic_migration_rate: 0.1,
        health_check_timeout_ms: 30_000,
    };
    
    let peers = vec!["controller-2".to_string(), "controller-3".to_string()];
    let controller = Arc::new(ControllerService::new(
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
    let admin_api = AdminApi::new(controller, port);
    admin_api.start().await?;
    
    Ok(())
}