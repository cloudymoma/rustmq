//! RustMQ Broker - Production-ready message broker with full lifecycle management
//!
//! This module provides the main Broker abstraction that orchestrates all broker components
//! including storage, replication, networking, and background tasks.

use crate::broker::core::{MessageBrokerCore, ProduceRecord, Producer};
use crate::config::Config;
use crate::error::{Result, RustMqError};
use crate::network::grpc_server::BrokerReplicationServiceImpl;
use crate::network::quic_server::{FetchHandler, MetadataHandler, ProduceHandler, QuicServer};
use crate::network::traits::NetworkHandler;
use crate::replication::grpc_client::{
    BrokerEndpoint, GrpcReplicationConfig, GrpcReplicationRpcClient,
};
use crate::replication::manager::ReplicationManager;
use crate::storage::{
    Cache, LocalObjectStorage, LruCache, PartitionStore, RecordLog, SegmentedLog, SegmentedWal,
    UploadManager, UploadManagerImpl,
};
use crate::types::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock as AsyncRwLock, oneshot};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

// Type alias for the concrete broker core.
pub(crate) type BrokerCore = MessageBrokerCore<ReplicationManager, SimpleNetworkHandler>;

/// Main broker instance that manages all broker lifecycle and components
pub struct Broker {
    config: Config,
    state: Arc<AsyncRwLock<BrokerState>>,
    broker_core: Arc<BrokerCore>,
    broker_handler: Arc<BrokerHandler>,
    quic_server: Option<QuicServer>,
    grpc_shutdown_tx: Option<oneshot::Sender<()>>,
    background_tasks: Vec<JoinHandle<()>>,
}

/// Broker lifecycle state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerState {
    /// Broker created but not started
    Created,
    /// Broker is starting up
    Starting,
    /// Broker is running and accepting connections
    Running,
    /// Broker is shutting down
    Stopping,
    /// Broker has stopped
    Stopped,
}

/// Broker health status
#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub is_healthy: bool,
    pub state: BrokerState,
    pub quic_server_running: bool,
    pub grpc_server_running: bool,
    pub background_tasks_running: usize,
}

/// Builder for creating Broker instances with custom configuration
pub struct BrokerBuilder {
    config: Config,
}

impl BrokerBuilder {
    /// Create a new broker builder with the given configuration
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Build the broker instance
    pub async fn build(self) -> Result<Broker> {
        Broker::from_config(self.config).await
    }
}

impl Broker {
    /// Create a new Broker from configuration
    ///
    /// This initializes all broker components including storage, replication,
    /// networking, and background tasks. The broker is created in the `Created`
    /// state and must be started with `start()`.
    ///
    /// # Arguments
    /// * `config` - Broker configuration
    ///
    /// # Returns
    /// A new Broker instance ready to be started
    pub async fn from_config(config: Config) -> Result<Self> {
        info!("Creating RustMQ Broker from configuration");
        info!("Broker ID: {}", config.broker.id);
        info!("Rack ID: {}", config.broker.rack_id);
        info!("QUIC Listen: {}", config.network.quic_listen);
        info!("RPC Listen: {}", config.network.rpc_listen);

        // Validate configuration
        config.validate()?;

        // Create broker handler
        let broker_handler = Arc::new(BrokerHandler::new(config.clone()).await?);

        // Initialize storage components
        info!("Initializing storage components...");
        let wal: Arc<dyn SegmentedLog> = Arc::new(SegmentedWal::new(config.wal.clone()).await?);
        // The hot serving tier lives in the partition index (bounded by the budget
        // below); this cache only accelerates repeated cold (object-storage) reads.
        let cache: Arc<dyn Cache> = Arc::new(LruCache::new(config.cache.read_cache_size_bytes));

        let object_storage = match &config.object_storage.storage_type {
            crate::config::StorageType::Local { path } => {
                std::fs::create_dir_all(path)?;
                Arc::new(crate::storage::StorageBackend::Local(
                    LocalObjectStorage::new(path.clone())?,
                ))
            }
            crate::config::StorageType::Gcs => {
                let mut builder = object_store::gcp::GoogleCloudStorageBuilder::from_env()
                    .with_bucket_name(&config.object_storage.bucket);

                if let Some(creds) = config
                    .object_storage
                    .service_account_path
                    .as_deref()
                    .filter(|s| !s.is_empty())
                {
                    builder = builder.with_service_account_path(creds);
                }

                let store = builder.build().map_err(|e| {
                    tracing::error!("GCS store initialization failed: {}", e);
                    RustMqError::Config("Failed to initialize cloud storage backend".to_string())
                })?;
                Arc::new(crate::storage::StorageBackend::Cloud(
                    crate::storage::cloud_storage::CloudObjectStorage::new(Arc::new(store)),
                ))
            }
            _ => {
                return Err(RustMqError::Config(
                    "Only Local and GCS storage supported in this implementation".to_string(),
                ));
            }
        };

        let upload_manager: Arc<dyn UploadManager> = Arc::new(UploadManagerImpl::new(
            object_storage.clone(),
            config.object_storage.clone(),
        ));

        // Single partition-aware storage engine over the shared WAL. The hot in-memory
        // serving tier is bounded by write_cache_size_bytes; appends apply backpressure
        // (force-seal + tier) when it is full, keeping memory bounded.
        let store = PartitionStore::new(
            wal,
            upload_manager,
            cache,
            config.cache.write_cache_size_bytes,
        )
        .await?;

        // Initialize replication manager
        info!("Initializing production replication manager with gRPC client...");
        let grpc_config = GrpcReplicationConfig::default();
        let rpc_client = Arc::new(GrpcReplicationRpcClient::new(grpc_config));

        // Register this broker's own endpoint for replication
        let self_endpoint = BrokerEndpoint::new(
            config.broker.id.clone(),
            config
                .network
                .quic_listen
                .split(':')
                .next()
                .unwrap_or("localhost")
                .to_string(),
            config
                .network
                .rpc_listen
                .split(':')
                .last()
                .and_then(|s| s.parse().ok())
                .unwrap_or(9093),
        );
        rpc_client.register_broker(self_endpoint);

        let replication_manager = Arc::new(ReplicationManager::new(
            1, // stream_id
            TopicPartition {
                topic: "_global".to_string(),
                partition: 0,
            },
            config.broker.id.clone(),
            1, // leader_epoch
            vec![config.broker.id.clone()],
            config.replication.clone(),
            store.clone() as Arc<dyn RecordLog>,
            rpc_client,
        ));

        // Initialize network handler
        let network_handler = Arc::new(SimpleNetworkHandler::new());

        // Create MessageBrokerCore
        info!("Creating MessageBrokerCore...");
        let broker_core = Arc::new(BrokerCore::new(
            store,
            replication_manager,
            network_handler,
            config.broker.id.clone(),
        ));

        // Update the broker handler with the core
        broker_handler.set_broker_core(broker_core.clone()).await;

        // Initialize QUIC server (but don't start it yet)
        info!("Initializing QUIC server on {}", config.network.quic_listen);
        let quic_server = QuicServer::new(
            config.network.clone(),
            broker_handler.clone(),
            broker_handler.clone(),
            broker_handler.clone(),
            broker_handler.clone(),
        )
        .await?;

        info!("Broker created successfully in Created state");

        Ok(Self {
            config,
            state: Arc::new(AsyncRwLock::new(BrokerState::Created)),
            broker_core,
            broker_handler,
            quic_server: Some(quic_server),
            grpc_shutdown_tx: None,
            background_tasks: Vec::new(),
        })
    }

    /// Start the broker and all its components
    ///
    /// This starts the QUIC server, gRPC replication server, and background tasks.
    /// The broker transitions from `Created` to `Starting` to `Running` state.
    ///
    /// # Errors
    /// Returns an error if the broker is not in the `Created` state or if any
    /// component fails to start.
    pub async fn start(&mut self) -> Result<()> {
        // Validate state transition
        {
            let mut state = self.state.write().await;
            if *state != BrokerState::Created {
                return Err(RustMqError::InvalidOperation(format!(
                    "Cannot start broker in state {:?}",
                    *state
                )));
            }
            *state = BrokerState::Starting;
        }

        info!("Starting RustMQ Broker...");

        // Start QUIC server in background
        let quic_server = self.quic_server.take().ok_or_else(|| {
            RustMqError::InvalidOperation("QUIC server already started".to_string())
        })?;

        let quic_handle = tokio::spawn(async move {
            if let Err(e) = quic_server.start().await {
                error!("QUIC server error: {}", e);
            }
        });
        self.background_tasks.push(quic_handle);

        // Start gRPC replication service
        info!(
            "Starting gRPC replication service on {}",
            self.config.network.rpc_listen
        );
        let grpc_service = Arc::new(BrokerReplicationServiceImpl::new(
            self.config.broker.id.clone(),
        ));

        // Create shutdown channel
        let (grpc_shutdown_tx, grpc_shutdown_rx) = oneshot::channel::<()>();
        self.grpc_shutdown_tx = Some(grpc_shutdown_tx);

        let rpc_listen_addr = self.config.network.rpc_listen.clone();
        let grpc_handle = tokio::spawn(async move {
            let addr = match rpc_listen_addr.parse::<std::net::SocketAddr>() {
                Ok(addr) => addr,
                Err(e) => {
                    error!(
                        "Failed to parse gRPC listen address '{}': {}",
                        rpc_listen_addr, e
                    );
                    return;
                }
            };

            info!("Starting production tonic gRPC server on {}", addr);

            use crate::proto::broker::broker_replication_service_server::BrokerReplicationServiceServer;
            use tonic::transport::Server;

            let svc = BrokerReplicationServiceServer::new((*grpc_service).clone());

            let server_future = Server::builder()
                .timeout(std::time::Duration::from_secs(30))
                .concurrency_limit_per_connection(256)
                .tcp_nodelay(true)
                .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
                .http2_keepalive_interval(Some(std::time::Duration::from_secs(30)))
                .http2_keepalive_timeout(Some(std::time::Duration::from_secs(10)))
                .add_service(svc)
                .serve_with_shutdown(addr, async {
                    grpc_shutdown_rx.await.ok();
                    info!("gRPC server received shutdown signal, stopping gracefully...");
                });

            info!("✅ Production gRPC replication server started on {}", addr);

            if let Err(e) = server_future.await {
                error!("gRPC server error: {}", e);
            } else {
                info!("gRPC server shut down gracefully");
            }
        });
        self.background_tasks.push(grpc_handle);

        // Start replication heartbeat task
        let broker_core_clone = self.broker_core.clone();
        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                // In production, would check follower states and send heartbeats
                let _ = broker_core_clone; // Keep reference alive
                tracing::debug!("Heartbeat check - broker is alive");
            }
        });
        self.background_tasks.push(heartbeat_handle);

        // Start MetricsCollector (Prometheus endpoint + controller heartbeat)
        let metrics = crate::metrics::Metrics::new();
        let controller_endpoint = self
            .config
            .controller
            .endpoints
            .first()
            .cloned()
            .unwrap_or_else(|| "localhost:9094".to_string());
        let admin_port = self
            .config
            .network
            .rpc_listen
            .split(':')
            .last()
            .and_then(|s| s.parse::<u16>().ok())
            .map(|p| p + 550)
            .unwrap_or(9643);
        let collector = crate::broker::metrics_collector::MetricsCollector::new(
            self.broker_core.clone(),
            metrics,
            controller_endpoint,
            self.config.broker.id.clone(),
            self.broker_handler.coordinator_cache.clone(),
        );
        let metrics_handle = collector.start(admin_port);
        self.background_tasks.push(metrics_handle);

        // Recover consumer group state from snapshots and start background tasks
        let snapshot_dir = self.config.wal.path.join("snapshots");
        self.broker_core
            .group_coordinator
            .recover_from_snapshots(&snapshot_dir)
            .await;
        self.broker_core
            .group_coordinator
            .start_snapshot_task(snapshot_dir);
        self.broker_core.group_coordinator.start_reaper_task();

        // Start the storage tiering loop (seal WAL segments → upload to object storage → reclaim).
        let tiering_handle = self.broker_core.get_store().spawn_tiering_task();
        self.background_tasks.push(tiering_handle);

        // Transition to running state
        {
            let mut state = self.state.write().await;
            *state = BrokerState::Running;
        }

        info!("✅ RustMQ Broker started successfully");
        info!("Broker ID: {}", self.config.broker.id);
        info!("Rack ID: {}", self.config.broker.rack_id);
        info!("QUIC Listen: {}", self.config.network.quic_listen);
        info!("RPC Listen: {}", self.config.network.rpc_listen);

        Ok(())
    }

    /// Stop the broker gracefully
    ///
    /// This stops accepting new connections, waits for in-flight requests to complete,
    /// and shuts down all background tasks. The broker transitions from `Running` to
    /// `Stopping` to `Stopped` state.
    ///
    /// # Errors
    /// Returns an error if the broker is not in the `Running` state.
    pub async fn stop(&mut self) -> Result<()> {
        // Validate state transition
        {
            let mut state = self.state.write().await;
            if *state != BrokerState::Running {
                return Err(RustMqError::InvalidOperation(format!(
                    "Cannot stop broker in state {:?}",
                    *state
                )));
            }
            *state = BrokerState::Stopping;
        }

        info!("Shutting down RustMQ Broker gracefully...");

        // Trigger graceful shutdown of gRPC server
        if let Some(grpc_shutdown_tx) = self.grpc_shutdown_tx.take() {
            if grpc_shutdown_tx.send(()).is_err() {
                warn!("Failed to send shutdown signal to gRPC server (already stopped)");
            } else {
                info!("✅ Sent graceful shutdown signal to gRPC server");
            }
        }

        // Give servers time to shut down gracefully
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        // Abort background tasks (QUIC server, heartbeat)
        info!("Stopping background tasks...");
        for handle in self.background_tasks.drain(..) {
            handle.abort();
        }

        // Graceful shutdown - flush WAL, wait for replication, close connections
        info!("Initiating graceful shutdown of storage and replication...");

        // Step 1: Flush WAL to disk (critical for preventing data loss)
        let wal_shutdown_start = std::time::Instant::now();
        if let Err(e) = tokio::time::timeout(
            std::time::Duration::from_secs(5), // 5s timeout for WAL flush
            self.broker_core.get_store().shutdown(),
        )
        .await
        {
            warn!("WAL shutdown timed out after 5s: {:?}", e);
        } else {
            info!(
                "✅ WAL flushed and shut down ({:?})",
                wal_shutdown_start.elapsed()
            );
        }

        // Step 2: Wait for inflight replications (best effort, followers will catch up)
        let replication_shutdown_start = std::time::Instant::now();
        if let Err(e) = tokio::time::timeout(
            std::time::Duration::from_secs(10), // 10s timeout for replication drain
            self.broker_core
                .get_replication_manager()
                .shutdown(std::time::Duration::from_secs(10)),
        )
        .await
        {
            warn!(
                "Replication shutdown timed out after 10s: {:?}. Followers will catch up on restart.",
                e
            );
        } else {
            info!(
                "✅ Replication drained ({:?})",
                replication_shutdown_start.elapsed()
            );
        }

        // Step 3: Close connections gracefully (give clients time to disconnect)
        info!("Waiting for connections to close...");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Transition to stopped state
        {
            let mut state = self.state.write().await;
            *state = BrokerState::Stopped;
        }

        info!("✅ RustMQ Broker shut down successfully");
        Ok(())
    }

    /// Get the current broker state
    pub async fn state(&self) -> BrokerState {
        self.state.read().await.clone()
    }

    /// Initiate graceful drain: notify controller and wait for leadership transfers
    pub async fn drain(&self) -> Result<()> {
        let controller_endpoint = self
            .config
            .controller
            .endpoints
            .first()
            .cloned()
            .unwrap_or_default();
        let endpoint = format!("http://{}", controller_endpoint);

        let request = crate::proto::controller::DrainRequest {
            broker_id: self.config.broker.id.clone(),
        };

        info!("Sending drain request to controller at {}", endpoint);
        match crate::proto::controller::broker_management_service_client::BrokerManagementServiceClient::connect(endpoint).await {
            Ok(mut client) => match client.drain_broker(request).await {
                Ok(_) => info!("Drain request accepted by controller"),
                Err(e) => error!("Failed to send drain request: {}", e),
            },
            Err(e) => error!("Failed to connect to controller for drain: {}", e),
        }

        let start_time = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(300);

        info!("Waiting for leadership transfers to complete...");
        loop {
            let (_total, leader_count) = self.broker_core.get_partition_counts().await;
            if leader_count == 0 {
                info!("All leadership transfers completed");
                break;
            }
            if start_time.elapsed() > timeout {
                warn!(
                    "Timed out waiting for leadership transfers ({} leaders remaining)",
                    leader_count
                );
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }

        Ok(())
    }

    /// Check broker health status
    pub async fn health_check(&self) -> HealthStatus {
        let state = self.state.read().await.clone();

        HealthStatus {
            is_healthy: state == BrokerState::Running,
            state,
            quic_server_running: self.quic_server.is_none(), // None means it's running
            grpc_server_running: self.grpc_shutdown_tx.is_some(),
            background_tasks_running: self.background_tasks.len(),
        }
    }

    /// Wait for the broker to stop (blocks until shutdown)
    ///
    /// This is useful for the main thread to block until the broker receives
    /// a shutdown signal.
    pub async fn wait_for_shutdown(&self) -> Result<()> {
        loop {
            let state = self.state.read().await.clone();
            if state == BrokerState::Stopped {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
        Ok(())
    }

    /// Get the broker configuration
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Get the broker core for advanced operations
    pub fn core(&self) -> &Arc<BrokerCore> {
        &self.broker_core
    }
}

/// Cached coordinator mapping: partition → (broker_id, host, port)
#[derive(Debug, Clone)]
pub struct CoordinatorInfo {
    pub broker_id: BrokerId,
    pub host: String,
    pub port: u16,
}

/// Handler that bridges QUIC server requests to MessageBrokerCore
pub struct BrokerHandler {
    config: Config,
    broker_core: Arc<AsyncRwLock<Option<Arc<BrokerCore>>>>,
    coordinator_cache: Arc<AsyncRwLock<HashMap<u32, CoordinatorInfo>>>,
}

impl BrokerHandler {
    async fn new(config: Config) -> Result<Self> {
        Ok(Self {
            config,
            broker_core: Arc::new(AsyncRwLock::new(None)),
            coordinator_cache: Arc::new(AsyncRwLock::new(HashMap::new())),
        })
    }

    async fn set_broker_core(&self, core: Arc<BrokerCore>) {
        let mut broker_core = self.broker_core.write().await;
        *broker_core = Some(core);
    }

    async fn get_broker_core(&self) -> Result<Arc<BrokerCore>> {
        let broker_core = self.broker_core.read().await;
        broker_core
            .clone()
            .ok_or_else(|| RustMqError::Config("Broker core not initialized".to_string()))
    }
}

#[async_trait]
impl ProduceHandler for BrokerHandler {
    async fn handle_produce(&self, request: ProduceRequest) -> Result<ProduceResponse> {
        let core = self.get_broker_core().await?;
        let producer = core.create_producer();

        let mut results = Vec::new();
        let acks = request.acks.clone();
        let timeout_ms = request.timeout_ms;
        let topic = request.topic.clone();
        let partition_id = request.partition_id;

        for record in request.records {
            let produce_record = ProduceRecord {
                topic: topic.clone(),
                partition: Some(partition_id),
                key: record.key.map(|k| k.to_vec()),
                value: record.value.to_vec(),
                headers: record.headers,
                acks: acks.clone(),
                timeout_ms,
            };

            let result = producer.send(produce_record).await?;
            results.push(result);
        }

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
    async fn handle_fetch(&self, request: FetchRequest) -> Result<FetchResponse> {
        let core = self.get_broker_core().await?;
        let topic_partition = TopicPartition {
            topic: request.topic,
            partition: request.partition_id,
        };

        match core
            .fetch_records(&topic_partition, request.fetch_offset, request.max_bytes)
            .await
        {
            Ok(records) => {
                let high_watermark = core
                    .get_partition_metadata(&topic_partition)
                    .await?
                    .map(|m| m.high_watermark)
                    .unwrap_or(0);

                Ok(FetchResponse {
                    records,
                    high_watermark,
                    error_code: 0,
                    error_message: None,
                })
            }
            Err(e) => {
                tracing::warn!("Fetch failed for {}: {}", topic_partition, e);
                Ok(FetchResponse {
                    records: vec![],
                    high_watermark: 0,
                    error_code: 1, // Generic error
                    error_message: Some("Failed to fetch records".to_string()),
                })
            }
        }
    }
}

#[async_trait]
impl MetadataHandler for BrokerHandler {
    async fn handle_metadata(
        &self,
        _request: crate::network::quic_server::MetadataRequest,
    ) -> Result<crate::network::quic_server::MetadataResponse> {
        Ok(crate::network::quic_server::MetadataResponse {
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

impl Default for SimpleNetworkHandler {
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU16, Ordering};

    // Install rustls crypto provider for tests
    fn ensure_crypto_provider() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    // Use atomic counter to generate unique ports for each test
    static PORT_COUNTER: AtomicU16 = AtomicU16::new(10000);

    fn get_test_config() -> Config {
        let port_base = PORT_COUNTER.fetch_add(10, Ordering::SeqCst);
        let mut config = Config::default();
        config.network.quic_listen = format!("127.0.0.1:{}", port_base);
        config.network.rpc_listen = format!("127.0.0.1:{}", port_base + 1);
        config
    }

    #[tokio::test]
    async fn test_broker_creation() {
        ensure_crypto_provider();
        let config = get_test_config();
        let broker = Broker::from_config(config).await;

        assert!(
            broker.is_ok(),
            "Broker creation should succeed: {:?}",
            broker.err()
        );

        let broker = broker.unwrap();
        let state = broker.state().await;
        assert_eq!(state, BrokerState::Created);
    }

    #[tokio::test]
    async fn test_broker_state_transitions() {
        ensure_crypto_provider();
        let config = get_test_config();
        let mut broker = Broker::from_config(config).await.unwrap();

        // Initial state should be Created
        assert_eq!(broker.state().await, BrokerState::Created);

        // Start the broker
        broker.start().await.unwrap();
        assert_eq!(broker.state().await, BrokerState::Running);

        // Stop the broker
        broker.stop().await.unwrap();
        assert_eq!(broker.state().await, BrokerState::Stopped);
    }

    #[tokio::test]
    async fn test_broker_health_check() {
        ensure_crypto_provider();
        let config = get_test_config();
        let broker = Broker::from_config(config).await.unwrap();

        let health = broker.health_check().await;
        assert_eq!(health.state, BrokerState::Created);
        assert!(!health.is_healthy); // Not healthy until running
    }

    #[tokio::test]
    async fn test_broker_builder() {
        ensure_crypto_provider();
        let config = get_test_config();
        let broker = BrokerBuilder::new(config).build().await;

        assert!(broker.is_ok());
        let broker = broker.unwrap();
        assert_eq!(broker.state().await, BrokerState::Created);
    }

    #[tokio::test]
    async fn test_invalid_state_transitions() {
        ensure_crypto_provider();
        let config = get_test_config();
        let mut broker = Broker::from_config(config).await.unwrap();

        // Try to stop without starting
        let result = broker.stop().await;
        assert!(result.is_err());

        // Start the broker
        broker.start().await.unwrap();

        // Try to start again (should fail)
        let result = broker.start().await;
        assert!(result.is_err());
    }
}

use crate::consumer_group::coordinator::GroupCoordinatorManager;
use crate::network::quic_server::ConsumerGroupHandler;

#[async_trait]
impl ConsumerGroupHandler for BrokerHandler {
    async fn handle_find_coordinator(
        &self,
        request: crate::types::FindCoordinatorRequest,
    ) -> Result<crate::types::FindCoordinatorResponse> {
        let core = self.get_broker_core().await?;
        let partition = core.group_coordinator.partition_for(&request.group_id);

        let cache = self.coordinator_cache.read().await;
        if let Some(info) = cache.get(&partition) {
            return Ok(crate::types::FindCoordinatorResponse {
                coordinator_broker_id: info.broker_id.clone(),
                coordinator_host: info.host.clone(),
                coordinator_port: info.port,
                error_code: 0,
            });
        }
        drop(cache);

        let broker_id = core.broker_id().clone();
        let host = self
            .config
            .network
            .quic_listen
            .split(':')
            .next()
            .unwrap_or("localhost")
            .to_string();
        let port = self
            .config
            .network
            .quic_listen
            .split(':')
            .last()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9092);

        Ok(crate::types::FindCoordinatorResponse {
            coordinator_broker_id: broker_id,
            coordinator_host: host,
            coordinator_port: port,
            error_code: 0,
        })
    }
    async fn handle_join_group(
        &self,
        request: crate::types::JoinGroupRequest,
    ) -> Result<crate::types::JoinGroupResponse> {
        let core = self.get_broker_core().await?;
        core.group_coordinator.handle_join_group(request).await
    }
    async fn handle_consumer_heartbeat(
        &self,
        request: crate::types::ConsumerHeartbeatRequest,
    ) -> Result<crate::types::ConsumerHeartbeatResponse> {
        let core = self.get_broker_core().await?;
        core.group_coordinator.handle_heartbeat(request).await
    }
    async fn handle_commit_offset(
        &self,
        request: crate::types::CommitOffsetRequest,
    ) -> Result<crate::types::CommitOffsetResponse> {
        let core = self.get_broker_core().await?;
        core.group_coordinator.handle_commit_offset(request).await
    }
    async fn handle_fetch_offset(
        &self,
        request: crate::types::FetchOffsetRequest,
    ) -> Result<crate::types::FetchOffsetResponse> {
        let core = self.get_broker_core().await?;
        core.group_coordinator.handle_fetch_offset(request).await
    }
    async fn handle_leave_group(
        &self,
        request: crate::types::LeaveGroupRequest,
    ) -> Result<crate::types::LeaveGroupResponse> {
        let core = self.get_broker_core().await?;
        core.group_coordinator.handle_leave_group(request).await
    }
}
