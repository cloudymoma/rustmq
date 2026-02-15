use crate::{
    Result,
    controller::service::ControllerService,
    error::RustMqError,
    replication::manager::ReplicationManager,
    storage::{Cache, ObjectStorage, WriteAheadLog},
    types::*,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, System};
use tokio::time::timeout;
use tracing::{debug, error, warn};

/// Comprehensive health check service for broker components
/// Provides detailed health status for WAL, cache, object storage, network, and replication
pub struct HealthCheckService {
    /// Broker identifier
    broker_id: BrokerId,
    /// Broker start time for uptime calculation
    start_time: Instant,
    /// WAL reference for health checks
    wal: Option<Arc<dyn WriteAheadLog>>,
    /// Cache reference for health checks  
    cache: Option<Arc<dyn Cache>>,
    /// Object storage reference for health checks
    object_storage: Option<Arc<dyn ObjectStorage>>,
    /// Replication manager for checking replication health
    replication_manager: Option<Arc<ReplicationManager>>,
    /// Controller service for accessing cluster metadata
    controller_service: Option<Arc<ControllerService>>,
    /// System information collector
    system: parking_lot::Mutex<System>,
    /// Health check thresholds
    thresholds: HealthThresholds,
}

/// Health check thresholds for determining component status
#[derive(Debug, Clone)]
pub struct HealthThresholds {
    /// Maximum acceptable WAL latency in milliseconds
    pub wal_max_latency_ms: u32,
    /// Maximum acceptable cache latency in milliseconds
    pub cache_max_latency_ms: u32,
    /// Maximum acceptable object storage latency in milliseconds
    pub object_storage_max_latency_ms: u32,
    /// Maximum acceptable network latency in milliseconds
    pub network_max_latency_ms: u32,
    /// Maximum acceptable CPU usage percentage
    pub cpu_max_usage_percent: f64,
    /// Maximum acceptable memory usage percentage
    pub memory_max_usage_percent: f64,
    /// Maximum acceptable disk usage percentage
    pub disk_max_usage_percent: f64,
    /// Maximum allowed error rate (errors per minute)
    pub max_error_rate: u32,
    /// Maximum acceptable replication lag count for followers
    pub replication_max_lag_count: u64,
    /// Maximum acceptable heartbeat timeout for replication in milliseconds
    pub replication_heartbeat_timeout_ms: u32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            wal_max_latency_ms: 100,
            cache_max_latency_ms: 50,
            object_storage_max_latency_ms: 1000,
            network_max_latency_ms: 200,
            cpu_max_usage_percent: 80.0,
            memory_max_usage_percent: 85.0,
            disk_max_usage_percent: 90.0,
            max_error_rate: 10,
            replication_max_lag_count: 1000,
            replication_heartbeat_timeout_ms: 30000,
        }
    }
}

impl HealthCheckService {
    /// Create a new health check service
    pub fn new(broker_id: BrokerId) -> Self {
        Self {
            broker_id,
            start_time: Instant::now(),
            wal: None,
            cache: None,
            object_storage: None,
            replication_manager: None,
            controller_service: None,
            system: parking_lot::Mutex::new(System::new_all()),
            thresholds: HealthThresholds::default(),
        }
    }

    /// Create health check service with custom thresholds
    pub fn with_thresholds(broker_id: BrokerId, thresholds: HealthThresholds) -> Self {
        Self {
            broker_id,
            start_time: Instant::now(),
            wal: None,
            cache: None,
            object_storage: None,
            replication_manager: None,
            controller_service: None,
            system: parking_lot::Mutex::new(System::new_all()),
            thresholds,
        }
    }

    /// Register WAL for health checks
    pub fn register_wal(&mut self, wal: Arc<dyn WriteAheadLog>) {
        self.wal = Some(wal);
    }

    /// Register cache for health checks
    pub fn register_cache(&mut self, cache: Arc<dyn Cache>) {
        self.cache = Some(cache);
    }

    /// Register object storage for health checks
    pub fn register_object_storage(&mut self, object_storage: Arc<dyn ObjectStorage>) {
        self.object_storage = Some(object_storage);
    }

    /// Register replication manager for health checks
    pub fn register_replication_manager(&mut self, replication_manager: Arc<ReplicationManager>) {
        self.replication_manager = Some(replication_manager);
    }

    /// Register controller service for health checks
    pub fn register_controller_service(&mut self, controller_service: Arc<ControllerService>) {
        self.controller_service = Some(controller_service);
    }

    /// Perform comprehensive health check based on request parameters
    pub async fn perform_health_check(
        &self,
        request: HealthCheckRequest,
    ) -> Result<HealthCheckResponse> {
        let start = Instant::now();
        let check_timeout = Duration::from_millis(request.timeout_ms.unwrap_or(5000) as u64);

        debug!(
            "Starting comprehensive health check for broker: {}",
            self.broker_id
        );

        // Perform component health checks concurrently
        let (wal_health, cache_health, object_storage_health, network_health, replication_health) = tokio::join!(
            self.check_wal_health_with_timeout(request.check_wal, check_timeout),
            self.check_cache_health_with_timeout(request.check_cache, check_timeout),
            self.check_object_storage_health_with_timeout(
                request.check_object_storage,
                check_timeout
            ),
            self.check_network_health_with_timeout(request.check_network, check_timeout),
            self.check_replication_health_with_timeout(request.check_replication, check_timeout)
        );

        // Gather resource usage if requested
        let resource_usage = self.get_resource_usage().await;

        // Calculate overall health status
        let overall_healthy = self.calculate_overall_health(&[
            &wal_health,
            &cache_health,
            &object_storage_health,
            &network_health,
            &replication_health,
        ]);

        // Generate error summary if there are issues
        let error_summary = self.generate_error_summary(&[
            &wal_health,
            &cache_health,
            &object_storage_health,
            &network_health,
            &replication_health,
        ]);

        let uptime = self.start_time.elapsed().as_secs();
        let partition_count = self.get_partition_count();

        let response = HealthCheckResponse {
            overall_healthy,
            broker_id: self.broker_id.clone(),
            timestamp: chrono::Utc::now(),
            uptime_seconds: uptime,
            wal_health,
            cache_health,
            object_storage_health,
            network_health,
            replication_health,
            resource_usage,
            partition_count,
            error_summary,
        };

        let duration = start.elapsed();
        debug!(
            "Health check completed for broker: {} in {:?} - overall_healthy: {}",
            self.broker_id, duration, overall_healthy
        );

        Ok(response)
    }

    /// Check WAL health with timeout
    async fn check_wal_health_with_timeout(
        &self,
        enabled: bool,
        timeout_duration: Duration,
    ) -> ComponentHealth {
        if !enabled {
            return ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("WAL health check disabled".to_string()),
                details: HashMap::new(),
            };
        }

        match timeout(timeout_duration, self.check_wal_health()).await {
            Ok(health) => health,
            Err(_) => ComponentHealth::unhealthy("WAL health check timed out".to_string()),
        }
    }

    /// Check WAL (Write-Ahead Log) health
    async fn check_wal_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut details = HashMap::new();

        if let Some(wal) = &self.wal {
            match wal.get_end_offset().await {
                Ok(end_offset) => {
                    let latency = start.elapsed().as_millis() as u32;
                    details.insert("end_offset".to_string(), end_offset.to_string());
                    details.insert("check_type".to_string(), "end_offset_read".to_string());

                    // Perform a sync operation to verify WAL write capability
                    match wal.sync().await {
                        Ok(_) => {
                            let total_latency = start.elapsed().as_millis() as u32;
                            details
                                .insert("total_latency_ms".to_string(), total_latency.to_string());
                            details.insert("sync_successful".to_string(), "true".to_string());

                            if total_latency > self.thresholds.wal_max_latency_ms {
                                ComponentHealth {
                                    status: HealthStatus::Degraded,
                                    last_check: chrono::Utc::now(),
                                    latency_ms: Some(total_latency),
                                    error_count: 0,
                                    last_error: None,
                                    details,
                                }
                            } else {
                                ComponentHealth {
                                    status: HealthStatus::Healthy,
                                    last_check: chrono::Utc::now(),
                                    latency_ms: Some(total_latency),
                                    error_count: 0,
                                    last_error: None,
                                    details,
                                }
                            }
                        }
                        Err(e) => {
                            error!("WAL sync failed during health check: {}", e);
                            ComponentHealth::unhealthy(format!("WAL sync failed: {}", e))
                        }
                    }
                }
                Err(e) => {
                    error!("WAL end offset read failed during health check: {}", e);
                    ComponentHealth::unhealthy(format!("WAL end offset read failed: {}", e))
                }
            }
        } else {
            ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("WAL not registered".to_string()),
                details,
            }
        }
    }

    /// Check cache health with timeout
    async fn check_cache_health_with_timeout(
        &self,
        enabled: bool,
        timeout_duration: Duration,
    ) -> ComponentHealth {
        if !enabled {
            return ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("Cache health check disabled".to_string()),
                details: HashMap::new(),
            };
        }

        match timeout(timeout_duration, self.check_cache_health()).await {
            Ok(health) => health,
            Err(_) => ComponentHealth::unhealthy("Cache health check timed out".to_string()),
        }
    }

    /// Check cache health
    async fn check_cache_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut details = HashMap::new();

        if let Some(cache) = &self.cache {
            // Test cache with a health check key
            let health_check_key = "health_check_test_key";
            let health_check_value = bytes::Bytes::from("health_check_value");

            match cache.size().await {
                Ok(cache_size) => {
                    details.insert("cache_size".to_string(), cache_size.to_string());

                    // Test cache write
                    match cache
                        .put(health_check_key, health_check_value.clone())
                        .await
                    {
                        Ok(_) => {
                            // Test cache read
                            match cache.get(health_check_key).await {
                                Ok(Some(retrieved_value)) => {
                                    if retrieved_value == health_check_value {
                                        // Clean up test key
                                        let _ = cache.remove(health_check_key).await;

                                        let latency = start.elapsed().as_millis() as u32;
                                        details.insert(
                                            "read_write_test".to_string(),
                                            "successful".to_string(),
                                        );
                                        details
                                            .insert("latency_ms".to_string(), latency.to_string());

                                        if latency > self.thresholds.cache_max_latency_ms {
                                            ComponentHealth::degraded(latency)
                                        } else {
                                            ComponentHealth {
                                                status: HealthStatus::Healthy,
                                                last_check: chrono::Utc::now(),
                                                latency_ms: Some(latency),
                                                error_count: 0,
                                                last_error: None,
                                                details,
                                            }
                                        }
                                    } else {
                                        ComponentHealth::unhealthy(
                                            "Cache data integrity check failed".to_string(),
                                        )
                                    }
                                }
                                Ok(None) => ComponentHealth::unhealthy(
                                    "Cache read returned None for existing key".to_string(),
                                ),
                                Err(e) => {
                                    error!("Cache read failed during health check: {}", e);
                                    ComponentHealth::unhealthy(format!("Cache read failed: {}", e))
                                }
                            }
                        }
                        Err(e) => {
                            error!("Cache write failed during health check: {}", e);
                            ComponentHealth::unhealthy(format!("Cache write failed: {}", e))
                        }
                    }
                }
                Err(e) => {
                    error!("Cache size check failed during health check: {}", e);
                    ComponentHealth::unhealthy(format!("Cache size check failed: {}", e))
                }
            }
        } else {
            ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("Cache not registered".to_string()),
                details,
            }
        }
    }

    /// Check object storage health with timeout
    async fn check_object_storage_health_with_timeout(
        &self,
        enabled: bool,
        timeout_duration: Duration,
    ) -> ComponentHealth {
        if !enabled {
            return ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("Object storage health check disabled".to_string()),
                details: HashMap::new(),
            };
        }

        match timeout(timeout_duration, self.check_object_storage_health()).await {
            Ok(health) => health,
            Err(_) => {
                ComponentHealth::unhealthy("Object storage health check timed out".to_string())
            }
        }
    }

    /// Check object storage health
    async fn check_object_storage_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut details = HashMap::new();

        if let Some(object_storage) = &self.object_storage {
            let health_check_key = format!("health_check_{}", self.broker_id);
            let health_check_data = bytes::Bytes::from("health_check_data");

            // Test object storage connectivity and basic operations
            match object_storage
                .put(&health_check_key, health_check_data.clone())
                .await
            {
                Ok(_) => {
                    // Test read operation
                    match object_storage.get(&health_check_key).await {
                        Ok(retrieved_data) => {
                            if retrieved_data == health_check_data {
                                // Test delete operation
                                match object_storage.delete(&health_check_key).await {
                                    Ok(_) => {
                                        let latency = start.elapsed().as_millis() as u32;
                                        details.insert(
                                            "put_get_delete_test".to_string(),
                                            "successful".to_string(),
                                        );
                                        details
                                            .insert("latency_ms".to_string(), latency.to_string());

                                        if latency > self.thresholds.object_storage_max_latency_ms {
                                            ComponentHealth::degraded(latency)
                                        } else {
                                            ComponentHealth {
                                                status: HealthStatus::Healthy,
                                                last_check: chrono::Utc::now(),
                                                latency_ms: Some(latency),
                                                error_count: 0,
                                                last_error: None,
                                                details,
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            "Object storage delete failed during health check: {}",
                                            e
                                        );
                                        // Still consider healthy if put/get worked
                                        let latency = start.elapsed().as_millis() as u32;
                                        ComponentHealth::degraded(latency)
                                    }
                                }
                            } else {
                                ComponentHealth::unhealthy(
                                    "Object storage data integrity check failed".to_string(),
                                )
                            }
                        }
                        Err(e) => {
                            error!("Object storage get failed during health check: {}", e);
                            ComponentHealth::unhealthy(format!("Object storage get failed: {}", e))
                        }
                    }
                }
                Err(e) => {
                    error!("Object storage put failed during health check: {}", e);
                    ComponentHealth::unhealthy(format!("Object storage put failed: {}", e))
                }
            }
        } else {
            ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("Object storage not registered".to_string()),
                details,
            }
        }
    }

    /// Check network health with timeout
    async fn check_network_health_with_timeout(
        &self,
        enabled: bool,
        timeout_duration: Duration,
    ) -> ComponentHealth {
        if !enabled {
            return ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("Network health check disabled".to_string()),
                details: HashMap::new(),
            };
        }

        match timeout(timeout_duration, self.check_network_health()).await {
            Ok(health) => health,
            Err(_) => ComponentHealth::unhealthy("Network health check timed out".to_string()),
        }
    }

    /// Check network health - production implementation with actual broker connectivity testing
    async fn check_network_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut details = HashMap::new();
        let mut errors = Vec::new();
        let mut is_healthy = true;

        details.insert(
            "check_type".to_string(),
            "production_network_health".to_string(),
        );

        // Test 1: Basic network interface status
        {
            let networks = Networks::new_with_refreshed_list();

            let mut total_in = 0;
            let mut total_out = 0;
            let mut active_interfaces = 0;

            for (interface_name, network) in &networks {
                total_in += network.received();
                total_out += network.transmitted();
                active_interfaces += 1;

                details.insert(
                    format!("interface_{}_received", interface_name),
                    network.received().to_string(),
                );
                details.insert(
                    format!("interface_{}_transmitted", interface_name),
                    network.transmitted().to_string(),
                );
            }

            details.insert("total_received_bytes".to_string(), total_in.to_string());
            details.insert("total_transmitted_bytes".to_string(), total_out.to_string());
            details.insert(
                "active_interfaces".to_string(),
                active_interfaces.to_string(),
            );

            if active_interfaces == 0 {
                is_healthy = false;
                errors.push("No active network interfaces found".to_string());
            }
        }

        // Test 2: Controller connectivity (if controller service is available)
        if let Some(ref controller_service) = self.controller_service {
            match timeout(Duration::from_millis(5000), async {
                // Test controller connectivity by checking cluster metadata
                controller_service.get_cluster_metadata().await
            })
            .await
            {
                Ok(Ok(metadata)) => {
                    details.insert("controller_connectivity".to_string(), "healthy".to_string());
                    details.insert(
                        "cluster_brokers_count".to_string(),
                        metadata.brokers.len().to_string(),
                    );
                    details.insert(
                        "cluster_topics_count".to_string(),
                        metadata.topics.len().to_string(),
                    );

                    // Test 3: Broker-to-broker connectivity
                    let mut reachable_brokers = 0;
                    let mut unreachable_brokers = 0;

                    for broker in &metadata.brokers {
                        // Skip self
                        if broker.id == self.broker_id {
                            continue;
                        }

                        // Test basic TCP connectivity to broker's RPC port
                        match timeout(Duration::from_millis(2000), async {
                            tokio::net::TcpStream::connect((broker.host.as_str(), broker.port_rpc))
                                .await
                        })
                        .await
                        {
                            Ok(Ok(_stream)) => {
                                reachable_brokers += 1;
                                details.insert(
                                    format!("broker_{}_reachable", broker.id),
                                    "true".to_string(),
                                );
                            }
                            Ok(Err(e)) => {
                                unreachable_brokers += 1;
                                details.insert(
                                    format!("broker_{}_reachable", broker.id),
                                    "false".to_string(),
                                );
                                details
                                    .insert(format!("broker_{}_error", broker.id), e.to_string());

                                // Only mark as unhealthy if we can't reach majority of brokers
                                debug!("Failed to connect to broker {}: {}", broker.id, e);
                            }
                            Err(_timeout) => {
                                unreachable_brokers += 1;
                                details.insert(
                                    format!("broker_{}_reachable", broker.id),
                                    "timeout".to_string(),
                                );
                                debug!("Timeout connecting to broker {}", broker.id);
                            }
                        }
                    }

                    details.insert(
                        "reachable_brokers".to_string(),
                        reachable_brokers.to_string(),
                    );
                    details.insert(
                        "unreachable_brokers".to_string(),
                        unreachable_brokers.to_string(),
                    );

                    let total_other_brokers = reachable_brokers + unreachable_brokers;
                    if total_other_brokers > 0 {
                        let reachability_percentage =
                            (reachable_brokers as f64 / total_other_brokers as f64) * 100.0;
                        details.insert(
                            "broker_reachability_percent".to_string(),
                            format!("{:.1}", reachability_percentage),
                        );

                        // Mark as degraded if less than 70% of brokers are reachable
                        if reachability_percentage < 70.0 {
                            is_healthy = false;
                            errors.push(format!(
                                "Low broker reachability: {:.1}% ({}/{} brokers reachable)",
                                reachability_percentage, reachable_brokers, total_other_brokers
                            ));
                        } else if reachability_percentage < 90.0 {
                            // Degraded but not unhealthy if 70-90% reachable
                            errors.push(format!(
                                "Reduced broker reachability: {:.1}% ({}/{} brokers reachable)",
                                reachability_percentage, reachable_brokers, total_other_brokers
                            ));
                        }
                    }
                }
                Ok(Err(e)) => {
                    is_healthy = false;
                    errors.push(format!("Controller communication failed: {}", e));
                    details.insert("controller_connectivity".to_string(), "failed".to_string());
                }
                Err(_timeout) => {
                    is_healthy = false;
                    errors.push("Controller communication timed out".to_string());
                    details.insert("controller_connectivity".to_string(), "timeout".to_string());
                }
            }
        } else {
            // No controller service available - basic network check only
            details.insert("controller_available".to_string(), "false".to_string());
            warn!("Controller service not available for network health checks");
        }

        // Test 4: Port binding test for our own services
        // Test if we can bind to a test port near our configured ports
        let test_port = 19999; // Use a high port for testing
        match tokio::net::TcpListener::bind(("0.0.0.0", test_port)).await {
            Ok(_listener) => {
                details.insert("port_binding_test".to_string(), "successful".to_string());
            }
            Err(e) => {
                // This might not be critical if the port is just in use
                details.insert("port_binding_test".to_string(), "failed".to_string());
                details.insert("port_binding_error".to_string(), e.to_string());
                debug!("Port binding test failed (not critical): {}", e);
            }
        }

        let latency = start.elapsed().as_millis() as u32;
        details.insert("latency_ms".to_string(), latency.to_string());
        details.insert(
            "checks_performed".to_string(),
            "interfaces,controller,brokers,port_binding".to_string(),
        );

        let error_message = if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        };

        // Determine final health status
        let status = if !is_healthy {
            HealthStatus::Unhealthy
        } else if latency > self.thresholds.network_max_latency_ms {
            HealthStatus::Degraded
        } else if errors.len() > 0 {
            HealthStatus::Degraded
        } else {
            HealthStatus::Healthy
        };

        ComponentHealth {
            status,
            last_check: chrono::Utc::now(),
            latency_ms: Some(latency),
            error_count: errors.len() as u32,
            last_error: error_message,
            details,
        }
    }

    /// Check replication health with timeout
    async fn check_replication_health_with_timeout(
        &self,
        enabled: bool,
        timeout_duration: Duration,
    ) -> ComponentHealth {
        if !enabled {
            return ComponentHealth {
                status: HealthStatus::Unknown,
                last_check: chrono::Utc::now(),
                latency_ms: None,
                error_count: 0,
                last_error: Some("Replication health check disabled".to_string()),
                details: HashMap::new(),
            };
        }

        match timeout(timeout_duration, self.check_replication_health()).await {
            Ok(health) => health,
            Err(_) => ComponentHealth::unhealthy("Replication health check timed out".to_string()),
        }
    }

    /// Check replication health - production implementation
    async fn check_replication_health(&self) -> ComponentHealth {
        let start = Instant::now();
        let mut details = HashMap::new();
        let mut errors = Vec::new();
        let mut is_healthy = true;

        details.insert(
            "check_type".to_string(),
            "production_replication_health".to_string(),
        );

        if let Some(ref replication_manager) = self.replication_manager {
            // Check if replication manager is available for each partition
            // Note: In production, there would be multiple replication managers for different partitions

            // Get basic replication metrics
            let log_end_offset = replication_manager.get_log_end_offset();
            let high_watermark = replication_manager.get_high_watermark_sync();
            let leader_epoch = replication_manager.get_leader_epoch();
            let current_leader = replication_manager.get_current_leader();
            let replica_set = replication_manager.get_replica_set();
            let min_isr = replication_manager.get_min_in_sync_replicas();
            let in_sync_replicas = replication_manager.get_in_sync_replicas();
            let topic_partition = replication_manager.get_topic_partition();

            // Record metrics
            details.insert("topic_partition".to_string(), topic_partition.to_string());
            details.insert("log_end_offset".to_string(), log_end_offset.to_string());
            details.insert("high_watermark".to_string(), high_watermark.to_string());
            details.insert("leader_epoch".to_string(), leader_epoch.to_string());
            details.insert(
                "current_leader".to_string(),
                current_leader.clone().unwrap_or("none".to_string()),
            );
            details.insert(
                "replica_set_size".to_string(),
                replica_set.len().to_string(),
            );
            details.insert("min_in_sync_replicas".to_string(), min_isr.to_string());
            details.insert(
                "current_in_sync_replicas".to_string(),
                in_sync_replicas.len().to_string(),
            );
            details.insert("in_sync_replicas".to_string(), in_sync_replicas.join(","));

            // Check 1: In-Sync Replica (ISR) requirements
            if in_sync_replicas.len() < min_isr {
                is_healthy = false;
                errors.push(format!(
                    "Insufficient in-sync replicas: {} < {} required",
                    in_sync_replicas.len(),
                    min_isr
                ));
            }

            // Check 2: Leader/follower lag
            let mut max_lag = 0u64;
            let mut lag_violations = 0;

            for broker_id in &replica_set {
                if Some(broker_id.clone()) != current_leader {
                    if let Some(lag) = replication_manager.get_follower_lag(broker_id) {
                        details.insert(format!("follower_{}_lag", broker_id), lag.to_string());
                        max_lag = max_lag.max(lag);

                        // Check if lag exceeds threshold
                        if lag > self.thresholds.replication_max_lag_count {
                            lag_violations += 1;
                            errors.push(format!(
                                "Follower {} has excessive lag: {} > {} threshold",
                                broker_id, lag, self.thresholds.replication_max_lag_count
                            ));
                        }
                    } else {
                        // No lag information available - potentially concerning
                        details
                            .insert(format!("follower_{}_lag", broker_id), "unknown".to_string());

                        // Check last heartbeat time
                        if let Some(last_heartbeat) =
                            replication_manager.get_last_heartbeat_time(broker_id)
                        {
                            let heartbeat_age = last_heartbeat.elapsed();
                            details.insert(
                                format!("follower_{}_last_heartbeat_ms", broker_id),
                                heartbeat_age.as_millis().to_string(),
                            );

                            // Check if heartbeat is stale
                            if heartbeat_age
                                > Duration::from_millis(
                                    self.thresholds.replication_heartbeat_timeout_ms as u64,
                                )
                            {
                                is_healthy = false;
                                errors.push(format!(
                                    "Follower {} heartbeat is stale: {}ms > {}ms threshold",
                                    broker_id,
                                    heartbeat_age.as_millis(),
                                    self.thresholds.replication_heartbeat_timeout_ms
                                ));
                            }
                        } else {
                            is_healthy = false;
                            errors.push(format!(
                                "No heartbeat information for follower {}",
                                broker_id
                            ));
                        }
                    }
                }
            }

            details.insert("max_follower_lag".to_string(), max_lag.to_string());
            details.insert("lag_violations".to_string(), lag_violations.to_string());

            // Check 3: Replication progress (high watermark should be advancing)
            let replication_lag = log_end_offset.saturating_sub(high_watermark);
            details.insert("replication_lag".to_string(), replication_lag.to_string());

            if replication_lag > self.thresholds.replication_max_lag_count {
                is_healthy = false;
                errors.push(format!(
                    "High watermark lag is excessive: {} > {} threshold",
                    replication_lag, self.thresholds.replication_max_lag_count
                ));
            }

            // Check 4: Overall replication health
            let overall_healthy = replication_manager.is_replication_healthy();
            details.insert(
                "overall_replication_healthy".to_string(),
                overall_healthy.to_string(),
            );

            if !overall_healthy {
                is_healthy = false;
                errors.push("Replication manager reports unhealthy status".to_string());
            }
        } else {
            // No replication manager available
            is_healthy = false;
            errors.push("No replication manager configured".to_string());
            details.insert(
                "replication_manager_available".to_string(),
                "false".to_string(),
            );
        }

        let latency = start.elapsed().as_millis() as u32;
        details.insert("latency_ms".to_string(), latency.to_string());
        details.insert(
            "checks_performed".to_string(),
            "isr,lag,heartbeat,progress,overall".to_string(),
        );

        let error_message = if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        };

        ComponentHealth {
            status: if is_healthy {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy
            },
            last_check: chrono::Utc::now(),
            latency_ms: Some(latency),
            error_count: errors.len() as u32,
            last_error: error_message,
            details,
        }
    }

    /// Get current resource usage statistics
    async fn get_resource_usage(&self) -> ResourceUsage {
        let mut system = self.system.lock();
        system.refresh_all();

        // Get CPU usage
        let cpu_usage = system.global_cpu_info().cpu_usage() as f64;

        // Get memory information
        let memory_total = system.total_memory();
        let memory_used = system.used_memory();

        // Get disk information
        let disks = Disks::new_with_refreshed_list();
        let mut disk_total = 0;
        let mut disk_used = 0;
        for disk in &disks {
            disk_total += disk.total_space();
            disk_used += disk.total_space() - disk.available_space();
        }

        // Get network information
        let networks = Networks::new_with_refreshed_list();
        let mut network_in = 0;
        let mut network_out = 0;
        for (_, network) in &networks {
            network_in += network.received();
            network_out += network.transmitted();
        }

        // Get process information for the current process
        let current_pid = sysinfo::get_current_pid().unwrap_or(sysinfo::Pid::from(0));
        let mut open_fds = 0;
        let mut active_connections = 0;

        if let Some(_process) = system.process(current_pid) {
            // Note: sysinfo doesn't directly provide FD count, this is a placeholder
            open_fds = 0; // Would need platform-specific code to get actual FD count
            active_connections = 0; // Would need netstat or similar to get connection count
        }

        ResourceUsage {
            cpu_usage_percent: cpu_usage,
            memory_usage_bytes: memory_used, // sysinfo 0.30+ returns bytes directly
            memory_total_bytes: memory_total,
            disk_usage_bytes: disk_used,
            disk_total_bytes: disk_total,
            network_in_bytes_per_sec: network_in, // This is cumulative, not per-second
            network_out_bytes_per_sec: network_out,
            open_file_descriptors: open_fds,
            active_connections,
        }
    }

    /// Calculate overall health based on component health statuses
    fn calculate_overall_health(&self, components: &[&ComponentHealth]) -> bool {
        for component in components {
            match component.status {
                HealthStatus::Unhealthy => return false,
                HealthStatus::Unknown => {
                    // Consider unknown as unhealthy if it's due to an error
                    if component.last_error.is_some()
                        && !component.last_error.as_ref().unwrap().contains("disabled")
                    {
                        return false;
                    }
                }
                _ => {}
            }
        }

        // Check resource usage thresholds
        // Note: This is async but we're in a sync context, so we'll skip for now
        // In a real implementation, you'd pass resource usage as a parameter

        true
    }

    /// Generate error summary from component health statuses
    fn generate_error_summary(&self, components: &[&ComponentHealth]) -> Option<String> {
        let mut errors = Vec::new();

        for (i, component) in components.iter().enumerate() {
            let component_name = match i {
                0 => "WAL",
                1 => "Cache",
                2 => "ObjectStorage",
                3 => "Network",
                4 => "Replication",
                _ => "Unknown",
            };

            match component.status {
                HealthStatus::Unhealthy => {
                    if let Some(error) = &component.last_error {
                        errors.push(format!("{}: {}", component_name, error));
                    } else {
                        errors.push(format!("{}: Unhealthy", component_name));
                    }
                }
                HealthStatus::Degraded => {
                    if let Some(latency) = component.latency_ms {
                        errors.push(format!(
                            "{}: Degraded ({}ms latency)",
                            component_name, latency
                        ));
                    } else {
                        errors.push(format!("{}: Degraded", component_name));
                    }
                }
                _ => {}
            }
        }

        if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        }
    }

    /// Get current partition count for this broker - production implementation
    fn get_partition_count(&self) -> u32 {
        if let Some(ref controller_service) = self.controller_service {
            // Get cluster metadata to count partitions assigned to this broker
            match tokio::runtime::Handle::current()
                .block_on(async { controller_service.get_cluster_metadata().await })
            {
                Ok(metadata) => {
                    let mut partition_count = 0u32;

                    // Count partitions where this broker is the leader or a replica
                    for (topic_partition, assignment) in &metadata.partition_assignments {
                        // Count if this broker is the leader
                        if assignment.leader == self.broker_id {
                            partition_count += 1;
                            debug!(
                                "Broker {} is leader for partition {}",
                                self.broker_id, topic_partition
                            );
                        }
                        // Count if this broker is a replica (but not already counted as leader)
                        else if assignment.replicas.contains(&self.broker_id) {
                            partition_count += 1;
                            debug!(
                                "Broker {} is replica for partition {}",
                                self.broker_id, topic_partition
                            );
                        }
                    }

                    debug!(
                        "Broker {} has {} total partitions",
                        self.broker_id, partition_count
                    );
                    partition_count
                }
                Err(e) => {
                    error!("Failed to get cluster metadata for partition count: {}", e);
                    // Return 0 on error, but log the issue
                    0
                }
            }
        } else if let Some(ref replication_manager) = self.replication_manager {
            // Fallback: if no controller service but replication manager exists,
            // we can only count 1 partition that this manager handles
            debug!("Using replication manager fallback for partition count");
            1
        } else {
            // No controller or replication manager available
            warn!("No controller service or replication manager available for partition count");
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::WalConfig;
    use crate::storage::{AlignedBufferPool, DirectIOWal, LocalObjectStorage, LruCache};
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_health_check_service_creation() {
        let service = HealthCheckService::new("test-broker".to_string());
        assert_eq!(service.broker_id, "test-broker");
        assert!(service.wal.is_none());
        assert!(service.cache.is_none());
        assert!(service.object_storage.is_none());
    }

    #[tokio::test]
    async fn test_health_check_with_all_components_disabled() {
        let service = HealthCheckService::new("test-broker".to_string());

        let request = HealthCheckRequest {
            check_wal: false,
            check_cache: false,
            check_object_storage: false,
            check_network: false,
            check_replication: false,
            timeout_ms: Some(1000),
        };

        let response = service.perform_health_check(request).await.unwrap();

        assert_eq!(response.broker_id, "test-broker");
        assert_eq!(response.wal_health.status, HealthStatus::Unknown);
        assert_eq!(response.cache_health.status, HealthStatus::Unknown);
        assert_eq!(response.object_storage_health.status, HealthStatus::Unknown);
        assert_eq!(response.network_health.status, HealthStatus::Unknown);
        assert_eq!(response.replication_health.status, HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_health_check_with_wal() {
        let temp_dir = TempDir::new().unwrap();
        let wal_config = WalConfig {
            path: temp_dir.path().to_path_buf(),
            capacity_bytes: 1024 * 1024,
            fsync_on_write: false,
            segment_size_bytes: 64 * 1024,
            buffer_size: 4096,
            upload_interval_ms: 60_000,
            flush_interval_ms: 1000,
        };

        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 10));
        let wal = Arc::new(DirectIOWal::new(wal_config, buffer_pool).await.unwrap());

        // Use generous thresholds for test environments where I/O latency may be higher
        let thresholds = HealthThresholds {
            wal_max_latency_ms: 5000,
            ..HealthThresholds::default()
        };
        let mut service =
            HealthCheckService::with_thresholds("test-broker".to_string(), thresholds);
        service.register_wal(wal);

        let request = HealthCheckRequest {
            check_wal: true,
            check_cache: false,
            check_object_storage: false,
            check_network: false,
            check_replication: false,
            timeout_ms: Some(5000),
        };

        let response = service.perform_health_check(request).await.unwrap();

        // WAL should be healthy
        assert_eq!(response.wal_health.status, HealthStatus::Healthy);
        assert!(response.wal_health.latency_ms.is_some());
        assert_eq!(response.wal_health.error_count, 0);
    }

    #[tokio::test]
    async fn test_health_check_with_cache() {
        let cache = Arc::new(LruCache::new(1024 * 1024)); // 1MB cache

        let mut service = HealthCheckService::new("test-broker".to_string());
        service.register_cache(cache);

        let request = HealthCheckRequest {
            check_wal: false,
            check_cache: true,
            check_object_storage: false,
            check_network: false,
            check_replication: false,
            timeout_ms: Some(1000),
        };

        let response = service.perform_health_check(request).await.unwrap();

        // Cache should be healthy
        assert_eq!(response.cache_health.status, HealthStatus::Healthy);
        assert!(response.cache_health.latency_ms.is_some());
        assert_eq!(response.cache_health.error_count, 0);
    }

    #[tokio::test]
    async fn test_health_check_with_object_storage() {
        let temp_dir = TempDir::new().unwrap();
        let object_storage: Arc<dyn ObjectStorage> =
            Arc::new(LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap());

        let mut service = HealthCheckService::new("test-broker".to_string());
        service.register_object_storage(object_storage);

        let request = HealthCheckRequest {
            check_wal: false,
            check_cache: false,
            check_object_storage: true,
            check_network: false,
            check_replication: false,
            timeout_ms: Some(1000),
        };

        let response = service.perform_health_check(request).await.unwrap();

        // Object storage should be healthy
        assert_eq!(response.object_storage_health.status, HealthStatus::Healthy);
        assert!(response.object_storage_health.latency_ms.is_some());
        assert_eq!(response.object_storage_health.error_count, 0);
    }

    #[tokio::test]
    async fn test_health_check_network() {
        let service = HealthCheckService::new("test-broker".to_string());

        let request = HealthCheckRequest {
            check_wal: false,
            check_cache: false,
            check_object_storage: false,
            check_network: true,
            check_replication: false,
            timeout_ms: Some(1000),
        };

        let response = service.perform_health_check(request).await.unwrap();

        // Network should be healthy (basic check)
        assert_eq!(response.network_health.status, HealthStatus::Healthy);
        assert!(response.network_health.latency_ms.is_some());
    }

    #[tokio::test]
    async fn test_health_check_timeout() {
        let service = HealthCheckService::new("test-broker".to_string());

        let request = HealthCheckRequest {
            check_wal: true,
            check_cache: true,
            check_object_storage: true,
            check_network: true,
            check_replication: true,
            timeout_ms: Some(1), // Very short timeout
        };

        let response = service.perform_health_check(request).await.unwrap();

        // Some components might timeout, but the overall check should complete
        assert_eq!(response.broker_id, "test-broker");
    }

    #[tokio::test]
    async fn test_resource_usage_collection() {
        let service = HealthCheckService::new("test-broker".to_string());
        let resource_usage = service.get_resource_usage().await;

        // Resource usage should have reasonable values
        assert!(resource_usage.cpu_usage_percent >= 0.0);
        assert!(resource_usage.memory_total_bytes > 0);
        assert!(resource_usage.disk_total_bytes > 0);
    }

    #[tokio::test]
    async fn test_custom_thresholds() {
        let thresholds = HealthThresholds {
            wal_max_latency_ms: 50,
            cache_max_latency_ms: 25,
            object_storage_max_latency_ms: 500,
            network_max_latency_ms: 100,
            cpu_max_usage_percent: 70.0,
            memory_max_usage_percent: 80.0,
            disk_max_usage_percent: 85.0,
            max_error_rate: 5,
            replication_max_lag_count: 500,
            replication_heartbeat_timeout_ms: 15000,
        };

        let service = HealthCheckService::with_thresholds("test-broker".to_string(), thresholds);
        assert_eq!(service.thresholds.wal_max_latency_ms, 50);
        assert_eq!(service.thresholds.cpu_max_usage_percent, 70.0);
    }

    #[tokio::test]
    async fn test_overall_health_calculation() {
        let service = HealthCheckService::new("test-broker".to_string());

        // All healthy components should result in overall healthy
        let healthy_components = vec![
            ComponentHealth::healthy(),
            ComponentHealth::healthy(),
            ComponentHealth::healthy(),
        ];
        let component_refs: Vec<&ComponentHealth> = healthy_components.iter().collect();
        assert!(service.calculate_overall_health(&component_refs));

        // One unhealthy component should result in overall unhealthy
        let mixed_components = vec![
            ComponentHealth::healthy(),
            ComponentHealth::unhealthy("Test error".to_string()),
            ComponentHealth::healthy(),
        ];
        let component_refs: Vec<&ComponentHealth> = mixed_components.iter().collect();
        assert!(!service.calculate_overall_health(&component_refs));
    }

    #[tokio::test]
    async fn test_error_summary_generation() {
        let service = HealthCheckService::new("test-broker".to_string());

        // No errors should return None
        let healthy_components = vec![ComponentHealth::healthy(), ComponentHealth::healthy()];
        let component_refs: Vec<&ComponentHealth> = healthy_components.iter().collect();
        assert!(service.generate_error_summary(&component_refs).is_none());

        // With errors should return summary
        let error_components = vec![
            ComponentHealth::unhealthy("WAL error".to_string()),
            ComponentHealth::degraded(150),
        ];
        let component_refs: Vec<&ComponentHealth> = error_components.iter().collect();
        let summary = service.generate_error_summary(&component_refs);
        assert!(summary.is_some());
        assert!(summary.unwrap().contains("WAL: WAL error"));
    }
}
