use async_trait::async_trait;
use chrono::Utc;
use clap::{Arg, Command};
use futures::stream::{self, Stream};
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber;

use rustmq::subscribers::bigquery::{
    config::BigQuerySubscriberConfig,
    service::{BigQuerySubscriberService, RawMessage, RustMqConsumer},
    types::ConnectionStatus,
    error::Result,
};

/// Mock RustMQ consumer implementation for demonstration
/// In a real implementation, this would connect to actual RustMQ brokers
struct MockRustMqConsumer {
    config: BigQuerySubscriberConfig,
}

impl MockRustMqConsumer {
    fn new(config: BigQuerySubscriberConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl RustMqConsumer for MockRustMqConsumer {
    async fn start_consuming(&mut self) -> Result<Box<dyn Stream<Item = Result<RawMessage>> + Unpin + Send>> {
        info!("Starting to consume from topic: {}", self.config.subscription.topic);
        
        // Create a mock stream that generates test messages
        let stream = stream::iter((0..100).map(|i| {
            Ok(RawMessage {
                topic: self.config.subscription.topic.clone(),
                partition: 0,
                offset: i as u64,
                timestamp: Utc::now(),
                key: Some(format!("key-{}", i).into_bytes()),
                value: serde_json::json!({
                    "id": i,
                    "message": format!("Test message {}", i),
                    "timestamp": Utc::now().to_rfc3339(),
                    "data": {
                        "value": i * 10,
                        "category": if i % 2 == 0 { "even" } else { "odd" }
                    }
                }).to_string().into_bytes(),
                headers: {
                    let mut headers = HashMap::new();
                    headers.insert("source".to_string(), "mock".to_string());
                    headers.insert("version".to_string(), "1.0".to_string());
                    headers
                },
            })
        }));
        
        Ok(Box::new(Box::pin(stream)))
    }
    
    async fn commit_offset(&self, topic: &str, partition: u32, offset: u64) -> Result<()> {
        // Mock implementation - in real scenario, this would commit to RustMQ
        tracing::debug!("Committing offset {} for topic {} partition {}", offset, topic, partition);
        Ok(())
    }
    
    async fn health_status(&self) -> ConnectionStatus {
        ConnectionStatus::Connected
    }
}

/// Health check endpoint handler
async fn start_health_server(service: std::sync::Arc<BigQuerySubscriberService>, addr: String) {
    use warp::Filter;
    
    let health = warp::path("health")
        .and(warp::get())
        .map(move || {
            let status = service.health_status();
            warp::reply::json(&status)
        });
    
    let metrics = warp::path("metrics")
        .and(warp::get())
        .map(move || {
            let metrics = service.metrics();
            warp::reply::json(&metrics)
        });
    
    let routes = health.or(metrics);
    
    info!("Starting health server on {}", addr);
    
    let addr: std::net::SocketAddr = addr.parse().expect("Invalid health check address");
    warp::serve(routes).run(addr).await;
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let matches = Command::new("rustmq-bigquery-subscriber")
        .version("0.1.0")
        .author("RustMQ Team")
        .about("BigQuery subscriber for RustMQ")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .required(true)
        )
        .arg(
            Arg::new("project")
                .short('p')
                .long("project")
                .value_name("PROJECT_ID")
                .help("Google Cloud Project ID")
        )
        .arg(
            Arg::new("dataset")
                .short('d')
                .long("dataset")
                .value_name("DATASET")
                .help("BigQuery dataset name")
        )
        .arg(
            Arg::new("table")
                .short('t')
                .long("table")
                .value_name("TABLE")
                .help("BigQuery table name")
        )
        .arg(
            Arg::new("topic")
                .long("topic")
                .value_name("TOPIC")
                .help("RustMQ topic to subscribe to")
        )
        .arg(
            Arg::new("brokers")
                .short('b')
                .long("brokers")
                .value_name("BROKERS")
                .help("Comma-separated list of RustMQ broker endpoints")
        )
        .get_matches();
    
    // Load configuration
    let config_path = matches.get_one::<String>("config").unwrap();
    let mut config = load_config(config_path).await?;
    
    // Override configuration with command line arguments
    if let Some(project) = matches.get_one::<String>("project") {
        config.project_id = project.clone();
    }
    
    if let Some(dataset) = matches.get_one::<String>("dataset") {
        config.dataset = dataset.clone();
    }
    
    if let Some(table) = matches.get_one::<String>("table") {
        config.table = table.clone();
    }
    
    if let Some(topic) = matches.get_one::<String>("topic") {
        config.subscription.topic = topic.clone();
    }
    
    if let Some(brokers) = matches.get_one::<String>("brokers") {
        config.subscription.broker_endpoints = brokers
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
    }
    
    // Validate required configuration
    if config.project_id.is_empty() {
        error!("Project ID is required");
        std::process::exit(1);
    }
    
    if config.dataset.is_empty() {
        error!("Dataset name is required");
        std::process::exit(1);
    }
    
    if config.table.is_empty() {
        error!("Table name is required");
        std::process::exit(1);
    }
    
    if config.subscription.topic.is_empty() {
        error!("Topic name is required");
        std::process::exit(1);
    }
    
    info!("Starting BigQuery subscriber");
    info!("Project: {}", config.project_id);
    info!("Dataset: {}", config.dataset);
    info!("Table: {}", config.table);
    info!("Topic: {}", config.subscription.topic);
    info!("Brokers: {:?}", config.subscription.broker_endpoints);
    
    // Create BigQuery subscriber service
    let mut service = BigQuerySubscriberService::new(config.clone()).await?;
    let service_arc = std::sync::Arc::new(service);
    
    // Start health check server if enabled
    let health_server_task = if config.monitoring.enable_health_check {
        let service_clone = service_arc.clone();
        let addr = config.monitoring.health_check_addr.clone();
        Some(tokio::spawn(async move {
            start_health_server(service_clone, addr).await;
        }))
    } else {
        None
    };
    
    // Create RustMQ consumer
    let consumer = MockRustMqConsumer::new(config.clone());
    
    // Handle shutdown signals
    let shutdown_signal = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        info!("Received shutdown signal");
    };
    
    // Start the subscriber service
    let service_task = {
        let mut service = std::sync::Arc::try_unwrap(service_arc)
            .map_err(|_| "Failed to unwrap service Arc")?;
        tokio::spawn(async move {
            if let Err(e) = service.start(consumer).await {
                error!("Subscriber service failed: {}", e);
            }
        })
    };
    
    // Wait for shutdown signal or service completion
    tokio::select! {
        _ = shutdown_signal => {
            info!("Initiating graceful shutdown");
        }
        result = service_task => {
            match result {
                Ok(_) => info!("Subscriber service completed"),
                Err(e) => error!("Subscriber service panicked: {}", e),
            }
        }
    }
    
    // Cancel health server if running
    if let Some(health_task) = health_server_task {
        health_task.abort();
    }
    
    info!("BigQuery subscriber stopped");
    Ok(())
}

/// Load configuration from file
async fn load_config(path: &str) -> Result<BigQuerySubscriberConfig> {
    let content = tokio::fs::read_to_string(path).await
        .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read config file: {}", e)))?;
    
    // Try TOML first, then JSON
    if path.ends_with(".toml") {
        toml::from_str(&content)
            .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to parse TOML config: {}", e)))
    } else {
        serde_json::from_str(&content)
            .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to parse JSON config: {}", e)))
    }
    .map_err(Into::into)
}