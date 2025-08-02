use rustmq::{Config, Result};
use rustmq::broker::core::{MessageBrokerCore, ProduceRecord, Producer};
use rustmq::storage::{DirectIOWal, LocalObjectStorage, LruCache, UploadManagerImpl, AlignedBufferPool};
use rustmq::replication::manager::{ReplicationManager, MockReplicationRpcClient};
use rustmq::network::quic_server::{QuicServer, ProduceHandler, FetchHandler, MetadataHandler};
use rustmq::network::grpc_server::BrokerReplicationService;
use rustmq::network::traits::NetworkHandler;
use rustmq::types::*;
use std::env;
use std::sync::Arc;
use tokio::signal;
use tracing::{info, error};
use async_trait::async_trait;

// Type aliases to make the broker core type more manageable
type BrokerCore = MessageBrokerCore<
    DirectIOWal,
    LocalObjectStorage,
    LruCache,
    ReplicationManager,
    SimpleNetworkHandler,
>;

/// Handler that bridges QUIC server requests to MessageBrokerCore
pub struct BrokerHandler {
    config: Config,
    broker_core: Arc<tokio::sync::RwLock<Option<Arc<BrokerCore>>>>,
}

impl BrokerHandler {
    async fn new(config: Config) -> Result<Self> {
        Ok(Self {
            config,
            broker_core: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }
    
    async fn set_broker_core(&self, core: Arc<BrokerCore>) {
        let mut broker_core = self.broker_core.write().await;
        *broker_core = Some(core);
    }
    
    async fn get_broker_core(&self) -> Result<Arc<BrokerCore>> {
        let broker_core = self.broker_core.read().await;
        broker_core.clone().ok_or_else(|| rustmq::RustMqError::Config("Broker core not initialized".to_string()))
    }
}

#[async_trait]
impl ProduceHandler for BrokerHandler {
    async fn handle_produce(&self, request: ProduceRequest) -> Result<ProduceResponse> {
        let core = self.get_broker_core().await?;
        let producer = core.create_producer();
        
        let mut results = Vec::new();
        let acks = request.acks.clone(); // Clone once outside the loop
        let timeout_ms = request.timeout_ms;
        let topic = request.topic.clone();
        let partition_id = request.partition_id;
        
        for record in request.records {
            let produce_record = ProduceRecord {
                topic: topic.clone(),
                partition: Some(partition_id),
                key: record.key,
                value: record.value,
                headers: record.headers,
                acks: acks.clone(),
                timeout_ms,
            };
            
            let result = producer.send(produce_record).await?;
            results.push(result);
        }
        
        // Return the offset of the first record (simplified)
        let offset = results.first().map(|r| r.offset).unwrap_or(0);
        
        Ok(ProduceResponse {
            offset,
            error_code: 0,
            error_message: None,
        })
    }
}

#[async_trait]
impl FetchHandler for BrokerHandler {
    async fn handle_fetch(&self, _request: FetchRequest) -> Result<FetchResponse> {
        let _core = self.get_broker_core().await?;
        
        // Simplified implementation - in real implementation would fetch from core
        Ok(FetchResponse {
            records: vec![],
            high_watermark: 0,
            error_code: 0,
            error_message: None,
        })
    }
}

#[async_trait]
impl MetadataHandler for BrokerHandler {
    async fn handle_metadata(&self, _request: rustmq::network::quic_server::MetadataRequest) -> Result<rustmq::network::quic_server::MetadataResponse> {
        Ok(rustmq::network::quic_server::MetadataResponse {
            brokers: vec![BrokerInfo {
                id: self.config.broker.id.clone(),
                host: "localhost".to_string(),
                port_quic: 9092,
                port_rpc: 9093,
                rack_id: self.config.broker.rack_id.clone(),
            }],
            topics_metadata: vec![],
        })
    }
}

/// Simple network handler implementation
pub struct SimpleNetworkHandler;

impl SimpleNetworkHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl NetworkHandler for SimpleNetworkHandler {
    async fn send_request(&self, _broker_id: &BrokerId, _request: Vec<u8>) -> Result<Vec<u8>> {
        // Simplified implementation - in real implementation would send over gRPC
        Ok(vec![])
    }
    
    async fn broadcast(&self, _brokers: &[BrokerId], _request: Vec<u8>) -> Result<()> {
        // Simplified implementation - in real implementation would broadcast over gRPC
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let default_config = "config/broker.toml".to_string();
    let config_path = args.get(2).unwrap_or(&default_config);
    
    info!("Starting RustMQ Broker");
    info!("Loading configuration from: {}", config_path);
    
    // Load configuration
    let config = match Config::from_file(config_path) {
        Ok(config) => {
            config.validate()?;
            config
        }
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };
    
    info!("Broker ID: {}", config.broker.id);
    info!("Rack ID: {}", config.broker.rack_id);
    info!("QUIC Listen: {}", config.network.quic_listen);
    info!("RPC Listen: {}", config.network.rpc_listen);
    
    // Create concrete handler implementations that bridge network layer to MessageBrokerCore
    let broker_handler = Arc::new(BrokerHandler::new(config.clone()).await?);
    
    // Initialize storage components
    info!("Initializing storage components...");
    let buffer_pool = Arc::new(AlignedBufferPool::new(config.wal.buffer_size, 10));
    let wal = Arc::new(DirectIOWal::new(config.wal.clone(), buffer_pool).await?);
    let cache = Arc::new(LruCache::new(config.cache.write_cache_size_bytes + config.cache.read_cache_size_bytes));
    
    let object_storage = match &config.object_storage.storage_type {
        rustmq::config::StorageType::Local { path } => {
            std::fs::create_dir_all(path)?;
            Arc::new(LocalObjectStorage::new(path.clone())?)
        },
        _ => return Err(rustmq::RustMqError::Config("Only Local storage supported in this implementation".to_string())),
    };
    
    let _upload_manager = Arc::new(UploadManagerImpl::new(object_storage.clone(), config.object_storage.clone()));
    
    // Initialize replication manager
    info!("Initializing replication manager...");
    let rpc_client = Arc::new(MockReplicationRpcClient);
    let replication_manager = Arc::new(ReplicationManager::new(
        1, // stream_id
        TopicPartition { topic: "_global".to_string(), partition: 0 },
        config.broker.id.clone(),
        1, // leader_epoch
        vec![config.broker.id.clone()],
        config.replication.clone(),
        wal.clone(),
        rpc_client,
    ));
    
    // Initialize network handler
    let network_handler = Arc::new(SimpleNetworkHandler::new());
    
    // Create MessageBrokerCore
    info!("Creating MessageBrokerCore...");
    let broker_core = Arc::new(MessageBrokerCore::new(
        wal,
        object_storage,
        cache,
        replication_manager,
        network_handler,
        config.broker.id.clone(),
    ));
    
    // Update the broker handler with the core
    broker_handler.set_broker_core(broker_core.clone()).await;
    
    // Initialize QUIC server
    info!("Starting QUIC server on {}", config.network.quic_listen);
    let quic_server = QuicServer::new(
        config.network.clone(),
        broker_handler.clone(),
        broker_handler.clone(),
        broker_handler.clone(),
    ).await?;
    
    // Initialize gRPC replication service
    info!("Starting gRPC replication service on {}", config.network.rpc_listen);
    let grpc_service = Arc::new(BrokerReplicationService::new(config.broker.id.clone()));
    
    // Start background services
    info!("Starting background services...");
    
    // Start QUIC server in background
    let quic_handle = {
        let quic_server = quic_server;
        tokio::spawn(async move {
            if let Err(e) = quic_server.start().await {
                error!("QUIC server error: {}", e);
            }
        })
    };
    
    // Start gRPC server in background (simplified - in real implementation would use tonic)
    let grpc_handle = {
        let _grpc_service = grpc_service;
        let rpc_listen = config.network.rpc_listen.clone();
        tokio::spawn(async move {
            info!("gRPC service ready on {}", rpc_listen);
            // In a real implementation, this would start a tonic gRPC server
            // For now, we'll just keep it alive
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            }
        })
    };
    
    // Start replication heartbeat task
    let heartbeat_handle = {
        let _broker_core_clone = broker_core.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                // Send heartbeats to followers - simplified implementation
                // In a real broker, this would check follower states and send heartbeats
                info!("Heartbeat check - broker is alive");
            }
        })
    };
    
    info!("RustMQ Broker started successfully");
    info!("Broker ID: {}", config.broker.id);
    info!("Rack ID: {}", config.broker.rack_id);
    info!("QUIC Listen: {}", config.network.quic_listen);
    info!("RPC Listen: {}", config.network.rpc_listen);
    
    // Wait for shutdown signal
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received shutdown signal");
        }
        _ = quic_handle => {
            error!("QUIC server terminated unexpectedly");
        }
        _ = grpc_handle => {
            error!("gRPC server terminated unexpectedly");
        }
        _ = heartbeat_handle => {
            error!("Heartbeat task terminated unexpectedly");
        }
    }
    
    info!("Shutting down RustMQ Broker gracefully...");
    
    // Graceful shutdown would happen here - flush WAL, close connections, etc.
    // For now we'll just exit cleanly
    Ok(())
}