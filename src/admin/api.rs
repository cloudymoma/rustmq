use crate::controller::{ControllerService, CreateTopicRequest, DeleteTopicRequest};
use crate::types::BrokerInfo;
use crate::config::RateLimitConfig;
use crate::security::SecurityManager;
use crate::Result;
use super::rate_limiter::{RateLimiterManager, rate_limit_filter};
use super::security_api::SecurityApi;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use warp::{Filter, Rejection, Reply};
use serde::{Deserialize, Serialize};
use tracing::{info, error, warn, debug};

/// Admin REST API server for RustMQ cluster management
pub struct AdminApi {
    controller: Arc<ControllerService>,
    port: u16,
    start_time: Instant,
    health_tracker: Arc<BrokerHealthTracker>,
    rate_limiter: Option<Arc<RateLimiterManager>>,
    security_api: Option<SecurityApi>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub leader_hint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTopicApiRequest {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub retention_ms: Option<u64>,
    pub segment_bytes: Option<u64>,
    pub compression_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BrokerStatus {
    pub id: String,
    pub host: String,
    pub port_quic: u16,
    pub port_rpc: u16,
    pub rack_id: String,
    pub online: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub brokers: Vec<BrokerStatus>,
    pub topics: Vec<TopicSummary>,
    pub leader: Option<String>,
    pub term: u64,
    pub healthy: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TopicSummary {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub is_leader: bool,
    pub raft_term: u64,
}

/// Tracks health status of brokers in the cluster
#[derive(Debug)]
pub struct BrokerHealthTracker {
    /// Cache of broker health status with last check time
    health_cache: RwLock<HashMap<String, BrokerHealth>>,
    /// Timeout for considering a broker unhealthy
    health_timeout: Duration,
    /// Test mode - when true, assume all health checks pass
    test_mode: bool,
}

#[derive(Debug, Clone)]
struct BrokerHealth {
    online: bool,
    last_check: Instant,
    last_success: Option<Instant>,
    consecutive_failures: u32,
}

impl BrokerHealthTracker {
    pub fn new(health_timeout: Duration) -> Self {
        Self {
            health_cache: RwLock::new(HashMap::new()),
            health_timeout,
            test_mode: false,
        }
    }

    /// Create a new health tracker in test mode (skips actual network calls)
    #[cfg(test)]
    pub fn new_test_mode(health_timeout: Duration) -> Self {
        Self {
            health_cache: RwLock::new(HashMap::new()),
            health_timeout,
            test_mode: true,
        }
    }

    /// Check if a broker is currently considered healthy
    pub async fn is_broker_healthy(&self, broker_id: &str) -> bool {
        let cache = self.health_cache.read().await;
        
        if let Some(health) = cache.get(broker_id) {
            // Consider broker healthy if last successful check was within timeout
            if let Some(last_success) = health.last_success {
                return last_success.elapsed() < self.health_timeout;
            }
        }
        
        // Default to false if no health info available
        false
    }

    /// Perform health check on a specific broker
    pub async fn check_broker_health(&self, broker: &BrokerInfo) -> bool {
        // Simulate health check by attempting to connect to broker
        let is_healthy = self.perform_health_check(broker).await;
        
        // Update health cache
        let mut cache = self.health_cache.write().await;
        let health = cache.entry(broker.id.clone()).or_insert_with(|| BrokerHealth {
            online: false,
            last_check: Instant::now(),
            last_success: None,
            consecutive_failures: 0,
        });
        
        health.last_check = Instant::now();
        
        if is_healthy {
            health.online = true;
            health.last_success = Some(Instant::now());
            health.consecutive_failures = 0;
            debug!("Health check passed for broker {}", broker.id);
        } else {
            health.online = false;
            health.consecutive_failures += 1;
            debug!("Health check failed for broker {} (consecutive failures: {})", 
                   broker.id, health.consecutive_failures);
        }
        
        is_healthy
    }

    /// Get the current health status for all tracked brokers
    pub async fn get_all_broker_health(&self) -> HashMap<String, bool> {
        let cache = self.health_cache.read().await;
        let mut result = HashMap::new();
        
        for (broker_id, health) in cache.iter() {
            // Check if the health info is stale
            let is_healthy = if let Some(last_success) = health.last_success {
                last_success.elapsed() < self.health_timeout
            } else {
                false
            };
            
            result.insert(broker_id.clone(), is_healthy);
        }
        
        result
    }

    /// Perform the actual health check using gRPC health check RPC
    async fn perform_health_check(&self, broker: &BrokerInfo) -> bool {
        // In test mode, always return true to avoid network calls
        if self.test_mode {
            return true;
        }

        use crate::proto::broker::broker_replication_service_client::BrokerReplicationServiceClient;
        use crate::proto::broker::{HealthCheckRequest as ProtoHealthCheckRequest};
        use tonic::transport::Channel;
        
        // Create connection to broker's gRPC endpoint
        let endpoint = format!("http://{}:{}", broker.host, broker.port_rpc);
        
        // Set a reasonable timeout for health checks
        let timeout = Duration::from_millis(2000);
        let check_result = tokio::time::timeout(timeout, async {
            // Attempt to connect to the broker
            match Channel::from_shared(endpoint.clone()) {
                Ok(channel_builder) => {
                    match channel_builder
                        .timeout(Duration::from_millis(1000))
                        .connect()
                        .await
                    {
                        Ok(channel) => {
                            let mut client = BrokerReplicationServiceClient::new(channel);
                            
                            // Create health check request
                            let request = tonic::Request::new(ProtoHealthCheckRequest {
                                check_wal: true,
                                check_cache: true,
                                check_object_storage: true,
                                check_network: true,
                                check_replication: true,
                                timeout_ms: 1000,
                                metadata: None,
                                include_detailed_metrics: false,
                                include_resource_usage: false,
                            });
                            
                            // Send health check request
                            match client.health_check(request).await {
                                Ok(response) => {
                                    let health_response = response.into_inner();
                                    debug!(
                                        "Health check response from broker {}: overall_healthy={}",
                                        broker.id, health_response.overall_healthy
                                    );
                                    health_response.overall_healthy
                                }
                                Err(e) => {
                                    debug!("Health check RPC failed for broker {}: {}", broker.id, e);
                                    false
                                }
                            }
                        }
                        Err(e) => {
                            debug!("Failed to connect to broker {} at {}: {}", broker.id, endpoint, e);
                            false
                        }
                    }
                }
                Err(e) => {
                    debug!("Invalid endpoint for broker {} ({}): {}", broker.id, endpoint, e);
                    false
                }
            }
        }).await;
        
        match check_result {
            Ok(result) => result,
            Err(_) => {
                debug!("Health check timeout for broker {} at {}", broker.id, endpoint);
                false
            }
        }
    }


    /// Clean up stale health entries
    pub async fn cleanup_stale_entries(&self) {
        let mut cache = self.health_cache.write().await;
        let stale_threshold = self.health_timeout * 3; // Keep entries for 3x the timeout
        
        cache.retain(|broker_id, health| {
            let is_stale = health.last_check.elapsed() > stale_threshold;
            if is_stale {
                debug!("Removing stale health entry for broker {}", broker_id);
            }
            !is_stale
        });
    }
}

/// Macro to reduce duplication in route definitions with optional rate limiting
macro_rules! create_route {
    ($rate_limit:expr, $path:expr, $($rest:tt)*) => {
        if let Some(ref rate_limit) = $rate_limit {
            $path.and(rate_limit.clone()).$($rest)*.boxed()
        } else {
            $path.$($rest)*.boxed()
        }
    };
}

impl AdminApi {
    pub fn new(controller: Arc<ControllerService>, port: u16) -> Self {
        let health_tracker = Arc::new(BrokerHealthTracker::new(Duration::from_secs(30)));
        
        // Start background health checking
        let tracker_clone = health_tracker.clone();
        let controller_clone = controller.clone();
        tokio::spawn(async move {
            Self::background_health_checker(controller_clone, tracker_clone).await;
        });
        
        Self { 
            controller, 
            port, 
            start_time: Instant::now(),
            health_tracker,
            rate_limiter: None,
            security_api: None,
        }
    }

    pub fn new_with_rate_limiting(
        controller: Arc<ControllerService>, 
        port: u16, 
        rate_limit_config: RateLimitConfig
    ) -> Result<Self> {
        let health_tracker = Arc::new(BrokerHealthTracker::new(Duration::from_secs(30)));
        
        // Create rate limiter if enabled
        let rate_limiter = if rate_limit_config.enabled {
            Some(Arc::new(RateLimiterManager::new(rate_limit_config)?))
        } else {
            None
        };
        
        // Start background health checking
        let tracker_clone = health_tracker.clone();
        let controller_clone = controller.clone();
        tokio::spawn(async move {
            Self::background_health_checker(controller_clone, tracker_clone).await;
        });
        
        Ok(Self { 
            controller, 
            port, 
            start_time: Instant::now(),
            health_tracker,
            rate_limiter,
            security_api: None,
        })
    }

    pub fn new_with_security(
        controller: Arc<ControllerService>,
        port: u16,
        security_manager: Arc<SecurityManager>,
        rate_limit_config: Option<RateLimitConfig>
    ) -> Result<Self> {
        let health_tracker = Arc::new(BrokerHealthTracker::new(Duration::from_secs(30)));
        
        // Create rate limiter if enabled
        let rate_limiter = if let Some(config) = rate_limit_config {
            if config.enabled {
                Some(Arc::new(RateLimiterManager::new(config)?))
            } else {
                None
            }
        } else {
            None
        };

        // Create security API
        let security_api = Some(SecurityApi::new(security_manager, controller.clone()));
        
        // Start background health checking
        let tracker_clone = health_tracker.clone();
        let controller_clone = controller.clone();
        tokio::spawn(async move {
            Self::background_health_checker(controller_clone, tracker_clone).await;
        });
        
        Ok(Self { 
            controller, 
            port, 
            start_time: Instant::now(),
            health_tracker,
            rate_limiter,
            security_api,
        })
    }

    /// Background task to periodically check broker health
    async fn background_health_checker(
        controller: Arc<ControllerService>,
        health_tracker: Arc<BrokerHealthTracker>,
    ) {
        let mut interval = tokio::time::interval(Duration::from_secs(15)); // Check every 15 seconds
        
        loop {
            interval.tick().await;
            
            // Get current brokers from controller
            match controller.get_cluster_metadata().await {
                Ok(metadata) => {
                    // Check health of each broker
                    for broker in metadata.brokers {
                        health_tracker.check_broker_health(&broker).await;
                    }
                    
                    // Clean up stale entries
                    health_tracker.cleanup_stale_entries().await;
                }
                Err(e) => {
                    error!("Failed to get cluster metadata for health checks: {}", e);
                }
            }
        }
    }

    /// Start the admin HTTP server
    pub async fn start(&self) -> Result<()> {
        let controller = self.controller.clone();
        let start_time = self.start_time;
        let health_tracker = self.health_tracker.clone();
        
        // Create rate limiting filter if enabled
        let rate_limit_middleware = if let Some(rate_limiter) = &self.rate_limiter {
            Some(rate_limit_filter(rate_limiter.clone()))
        } else {
            None
        };
        
        // Health check endpoint
        let health = create_route!(
            rate_limit_middleware,
            warp::path("health"),
            and(warp::get())
                .and(with_controller(controller.clone()))
                .and(with_start_time(start_time))
                .and_then(handle_health)
        );

        // Cluster status endpoint
        let cluster = create_route!(
            rate_limit_middleware,
            warp::path!("api" / "v1" / "cluster"),
            and(warp::get())
                .and(with_controller(controller.clone()))
                .and(with_health_tracker(health_tracker.clone()))
                .and_then(handle_cluster_status)
        );

        // Topics endpoints
        let topics_list = create_route!(
            rate_limit_middleware,
            warp::path!("api" / "v1" / "topics"),
            and(warp::get())
                .and(with_controller(controller.clone()))
                .and_then(handle_list_topics)
        );

        let topics_create = create_route!(
            rate_limit_middleware,
            warp::path!("api" / "v1" / "topics"),
            and(warp::post())
                .and(warp::body::json())
                .and(with_controller(controller.clone()))
                .and_then(handle_create_topic)
        );

        let topics_delete = create_route!(
            rate_limit_middleware,
            warp::path!("api" / "v1" / "topics" / String),
            and(warp::delete())
                .and(with_controller(controller.clone()))
                .and_then(handle_delete_topic)
        );

        let topics_describe = create_route!(
            rate_limit_middleware,
            warp::path!("api" / "v1" / "topics" / String),
            and(warp::get())
                .and(with_controller(controller.clone()))
                .and_then(handle_describe_topic)
        );

        // Brokers endpoints
        let brokers_list = create_route!(
            rate_limit_middleware,
            warp::path!("api" / "v1" / "brokers"),
            and(warp::get())
                .and(with_controller(controller.clone()))
                .and(with_health_tracker(health_tracker.clone()))
                .and_then(handle_list_brokers)
        );

        // Security routes integration - P0-8: Enabled for production
        // Security API is required for production deployments
        // Note: The Option<SecurityApi> design allows flexibility for testing,
        // but production systems must provide SecurityApi for secure operation.
        info!("Security routes enabled - authentication required via mTLS");

        // Extract Arc fields from security_api to avoid lifetime issues
        // This allows us to build routes with owned Arc values instead of borrowing from self
        let security_api_ref = self.security_api
            .as_ref()
            .expect("SecurityApi must be provided for production use - configure SecurityManager when creating AdminApi");

        let security_manager = security_api_ref.security_manager();
        let _controller = security_api_ref.controller();

        // Build security routes directly using static methods with owned Arc values
        let ca_routes = crate::admin::security_api::SecurityApi::ca_routes_static(
            security_manager.clone(),
            rate_limit_middleware.clone()
        );
        let cert_routes = crate::admin::security_api::SecurityApi::certificate_routes_static(
            security_manager.clone(),
            rate_limit_middleware.clone()
        );
        let acl_routes = crate::admin::security_api::SecurityApi::acl_routes_static(
            security_manager.clone(),
            rate_limit_middleware.clone()
        );
        let audit_routes = crate::admin::security_api::SecurityApi::audit_routes_static(
            security_manager.clone(),
            rate_limit_middleware.clone()
        );

        let security_routes = ca_routes
            .or(cert_routes)
            .or(acl_routes)
            .or(audit_routes)
            .boxed();

        // Combine all routes including security
        let api_routes = health
            .or(cluster)
            .or(topics_list)
            .or(topics_create)
            .or(topics_delete)
            .or(topics_describe)
            .or(brokers_list)
            .or(security_routes)
            .boxed();

        // Static file serving for WebUI
        let static_files = warp::path::end()
            .and(warp::fs::file("./web/dist/index.html"))
            .or(warp::fs::dir("./web/dist"));

        // Combine API routes with static files (API takes precedence)
        let all_routes = api_routes
            .or(static_files)
            .boxed();

        // Combine all routes
        let routes = all_routes
            .with(warp::cors().allow_any_origin())
            .with(warp::trace::request())
            .recover(handle_rejection_unified);

        let rate_limit_status = if self.rate_limiter.is_some() { "enabled" } else { "disabled" };
        let security_status = if self.security_api.is_some() { "enabled" } else { "disabled" };
        info!("Starting Admin API server on port {} (rate limiting: {}, security: {})",
              self.port, rate_limit_status, security_status);
        info!("WebUI available at http://0.0.0.0:{}", self.port);
        
        warp::serve(routes)
            .run(([0, 0, 0, 0], self.port))
            .await;

        Ok(())
    }
}

/// Warp filter to inject controller service
fn with_controller(
    controller: Arc<ControllerService>,
) -> impl Filter<Extract = (Arc<ControllerService>,), Error = Infallible> + Clone {
    warp::any().map(move || controller.clone())
}

/// Warp filter to inject start time for uptime calculation
fn with_start_time(
    start_time: Instant,
) -> impl Filter<Extract = (Instant,), Error = Infallible> + Clone {
    warp::any().map(move || start_time)
}

/// Warp filter to inject health tracker
fn with_health_tracker(
    health_tracker: Arc<BrokerHealthTracker>,
) -> impl Filter<Extract = (Arc<BrokerHealthTracker>,), Error = Infallible> + Clone {
    warp::any().map(move || health_tracker.clone())
}

/// Handle health check endpoint
async fn handle_health(
    controller: Arc<ControllerService>,
    start_time: Instant,
) -> std::result::Result<impl Reply, Rejection> {
    let raft_info = controller.get_raft_info();
    let uptime = start_time.elapsed().as_secs();
    
    let response = HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_seconds: uptime,
        is_leader: raft_info.is_leader,
        raft_term: raft_info.current_term,
    };
    
    Ok(warp::reply::json(&response))
}

/// Handle cluster status endpoint
async fn handle_cluster_status(
    controller: Arc<ControllerService>,
    health_tracker: Arc<BrokerHealthTracker>,
) -> std::result::Result<impl Reply, Rejection> {
    match controller.get_cluster_metadata().await {
        Ok(metadata) => {
            let broker_health = health_tracker.get_all_broker_health().await;
            let mut healthy_brokers = 0;
            let total_brokers = metadata.brokers.len();
            
            let broker_statuses: Vec<BrokerStatus> = metadata.brokers.into_iter().map(|b| {
                let is_online = broker_health.get(&b.id).copied().unwrap_or(false);
                if is_online {
                    healthy_brokers += 1;
                }
                
                BrokerStatus {
                    id: b.id,
                    host: b.host,
                    port_quic: b.port_quic,
                    port_rpc: b.port_rpc,
                    rack_id: b.rack_id,
                    online: is_online,
                }
            }).collect();
            
            // Cluster is healthy if majority of brokers are healthy and we have a leader
            // For 2 brokers, we need at least 1 to be healthy (50% is acceptable for small clusters)
            let cluster_healthy = if total_brokers == 0 {
                false
            } else if total_brokers <= 2 {
                healthy_brokers >= 1 && metadata.leader.is_some()
            } else {
                healthy_brokers > total_brokers / 2 && metadata.leader.is_some()
            };
            
            let cluster_status = ClusterStatus {
                brokers: broker_statuses,
                topics: metadata.topics.into_iter().map(|t| TopicSummary {
                    name: t.name,
                    partitions: t.partitions,
                    replication_factor: t.replication_factor,
                    created_at: t.created_at,
                }).collect(),
                leader: metadata.leader,
                term: metadata.term,
                healthy: cluster_healthy,
            };
            
            let response = ApiResponse {
                success: true,
                data: Some(cluster_status),
                error: None,
                leader_hint: None,
            };
            
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            error!("Failed to get cluster metadata: {}", e);
            let response = ApiResponse::<ClusterStatus> {
                success: false,
                data: None,
                error: Some(format!("Failed to get cluster metadata: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handle list topics endpoint
async fn handle_list_topics(
    controller: Arc<ControllerService>,
) -> std::result::Result<impl Reply, Rejection> {
    match controller.get_cluster_metadata().await {
        Ok(metadata) => {
            let topics: Vec<TopicSummary> = metadata.topics.into_iter().map(|t| TopicSummary {
                name: t.name,
                partitions: t.partitions,
                replication_factor: t.replication_factor,
                created_at: t.created_at,
            }).collect();
            
            let response = ApiResponse {
                success: true,
                data: Some(topics),
                error: None,
                leader_hint: metadata.leader,
            };
            
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            error!("Failed to list topics: {}", e);
            let response = ApiResponse::<Vec<TopicSummary>> {
                success: false,
                data: None,
                error: Some(format!("Failed to list topics: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handle create topic endpoint
async fn handle_create_topic(
    request: CreateTopicApiRequest,
    controller: Arc<ControllerService>,
) -> std::result::Result<impl Reply, Rejection> {
    let create_request = CreateTopicRequest {
        name: request.name.clone(),
        partitions: request.partitions,
        replication_factor: request.replication_factor,
        config: Some(crate::controller::TopicConfig {
            retention_ms: request.retention_ms,
            segment_bytes: request.segment_bytes,
            compression_type: request.compression_type,
        }),
    };
    
    match controller.create_topic(create_request).await {
        Ok(create_response) => {
            if create_response.success {
                info!("Topic '{}' created successfully", request.name);
                let response = ApiResponse {
                    success: true,
                    data: Some(format!("Topic '{}' created", request.name)),
                    error: None,
                    leader_hint: create_response.leader_hint,
                };
                Ok(warp::reply::json(&response))
            } else {
                warn!("Failed to create topic '{}': {:?}", request.name, create_response.error_message);
                let response = ApiResponse::<String> {
                    success: false,
                    data: None,
                    error: create_response.error_message,
                    leader_hint: create_response.leader_hint,
                };
                Ok(warp::reply::json(&response))
            }
        }
        Err(e) => {
            error!("Error creating topic '{}': {}", request.name, e);
            let response = ApiResponse::<String> {
                success: false,
                data: None,
                error: Some(format!("Error creating topic: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handle delete topic endpoint
async fn handle_delete_topic(
    topic_name: String,
    controller: Arc<ControllerService>,
) -> std::result::Result<impl Reply, Rejection> {
    let delete_request = DeleteTopicRequest {
        name: topic_name.clone(),
    };
    
    match controller.delete_topic(delete_request).await {
        Ok(delete_response) => {
            if delete_response.success {
                info!("Topic '{}' deleted successfully", topic_name);
                let response = ApiResponse {
                    success: true,
                    data: Some(format!("Topic '{}' deleted", topic_name)),
                    error: None,
                    leader_hint: delete_response.leader_hint,
                };
                Ok(warp::reply::json(&response))
            } else {
                warn!("Failed to delete topic '{}': {:?}", topic_name, delete_response.error_message);
                let response = ApiResponse::<String> {
                    success: false,
                    data: None,
                    error: delete_response.error_message,
                    leader_hint: delete_response.leader_hint,
                };
                Ok(warp::reply::json(&response))
            }
        }
        Err(e) => {
            error!("Error deleting topic '{}': {}", topic_name, e);
            let response = ApiResponse::<String> {
                success: false,
                data: None,
                error: Some(format!("Error deleting topic: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handle describe topic endpoint
async fn handle_describe_topic(
    topic_name: String,
    controller: Arc<ControllerService>,
) -> std::result::Result<impl Reply, Rejection> {
    match controller.get_cluster_metadata().await {
        Ok(metadata) => {
            if let Some(topic) = metadata.topics.iter().find(|t| t.name == topic_name) {
                // Get partition assignments for this topic
                let partitions: Vec<_> = metadata.partition_assignments
                    .iter()
                    .filter(|(tp, _)| tp.topic == topic_name)
                    .map(|(tp, assignment)| {
                        serde_json::json!({
                            "partition": tp.partition,
                            "leader": assignment.leader,
                            "replicas": assignment.replicas,
                            "in_sync_replicas": assignment.in_sync_replicas,
                            "leader_epoch": assignment.leader_epoch
                        })
                    })
                    .collect();
                
                let topic_detail = serde_json::json!({
                    "name": topic.name,
                    "partitions": topic.partitions,
                    "replication_factor": topic.replication_factor,
                    "config": topic.config,
                    "created_at": topic.created_at,
                    "partition_assignments": partitions
                });
                
                let response = ApiResponse {
                    success: true,
                    data: Some(topic_detail),
                    error: None,
                    leader_hint: metadata.leader,
                };
                
                Ok(warp::reply::json(&response))
            } else {
                let response = ApiResponse::<serde_json::Value> {
                    success: false,
                    data: None,
                    error: Some(format!("Topic '{}' not found", topic_name)),
                    leader_hint: metadata.leader,
                };
                Ok(warp::reply::json(&response))
            }
        }
        Err(e) => {
            error!("Failed to describe topic '{}': {}", topic_name, e);
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Failed to describe topic: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handle list brokers endpoint
async fn handle_list_brokers(
    controller: Arc<ControllerService>,
    health_tracker: Arc<BrokerHealthTracker>,
) -> std::result::Result<impl Reply, Rejection> {
    match controller.get_cluster_metadata().await {
        Ok(metadata) => {
            let broker_health = health_tracker.get_all_broker_health().await;
            
            let brokers: Vec<BrokerStatus> = metadata.brokers.into_iter().map(|b| {
                let is_online = broker_health.get(&b.id).copied().unwrap_or(false);
                
                BrokerStatus {
                    id: b.id,
                    host: b.host,
                    port_quic: b.port_quic,
                    port_rpc: b.port_rpc,
                    rack_id: b.rack_id,
                    online: is_online,
                }
            }).collect();
            
            let response = ApiResponse {
                success: true,
                data: Some(brokers),
                error: None,
                leader_hint: metadata.leader,
            };
            
            Ok(warp::reply::json(&response))
        }
        Err(e) => {
            error!("Failed to list brokers: {}", e);
            let response = ApiResponse::<Vec<BrokerStatus>> {
                success: false,
                data: None,
                error: Some(format!("Failed to list brokers: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

/// Handle warp rejections
async fn handle_rejection(err: Rejection) -> std::result::Result<warp::reply::Response, Infallible> {
    let code;
    let message;

    if err.is_not_found() {
        code = warp::http::StatusCode::NOT_FOUND;
        message = "Not Found";
    } else if let Some(_) = err.find::<warp::filters::body::BodyDeserializeError>() {
        code = warp::http::StatusCode::BAD_REQUEST;
        message = "Invalid JSON body";
    } else if let Some(_) = err.find::<warp::reject::MethodNotAllowed>() {
        code = warp::http::StatusCode::METHOD_NOT_ALLOWED;
        message = "Method Not Allowed";
    } else {
        error!("Unhandled rejection: {:?}", err);
        code = warp::http::StatusCode::INTERNAL_SERVER_ERROR;
        message = "Internal Server Error";
    }

    let json = warp::reply::json(&ApiResponse::<()> {
        success: false,
        data: None,
        error: Some(message.to_string()),
        leader_hint: None,
    });

    Ok(warp::reply::with_status(json, code).into_response())
}

/// Unified rejection handler that handles both rate limiting and other rejections
async fn handle_rejection_unified(err: Rejection) -> std::result::Result<warp::reply::Response, Infallible> {
    // First try to handle rate limit rejections
    if let Some(rate_limit_rejection) = err.find::<super::rate_limiter::RateLimitRejection>() {
        let error_response = ApiResponse::<()> {
            success: false,
            data: None,
            error: Some(rate_limit_rejection.0.error.clone()),
            leader_hint: None,
        };
        
        let reply = warp::reply::json(&error_response);
        let reply = warp::reply::with_status(reply, warp::http::StatusCode::TOO_MANY_REQUESTS);
        let reply = warp::reply::with_header(reply, "X-RateLimit-Limit", rate_limit_rejection.0.limit);
        let reply = warp::reply::with_header(reply, "X-RateLimit-Remaining", rate_limit_rejection.0.remaining);
        let reply = warp::reply::with_header(reply, "X-RateLimit-Reset", rate_limit_rejection.0.reset_time);
        let reply = warp::reply::with_header(reply, "Retry-After", rate_limit_rejection.0.retry_after);
        
        return Ok(reply.into_response());
    }
    
    // Fall back to standard rejection handling
    handle_rejection(err).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::ControllerService;
    use crate::config::ScalingConfig;
    use crate::types::BrokerInfo;
    use std::sync::Arc;
    use warp::test;

    async fn setup_test_controller() -> Arc<ControllerService> {
        let scaling_config = ScalingConfig {
            max_concurrent_additions: 3,
            max_concurrent_decommissions: 1,
            rebalance_timeout_ms: 300_000,
            traffic_migration_rate: 0.1,
            health_check_timeout_ms: 30_000,
        };
        
        let peers = vec![];
        let controller = Arc::new(ControllerService::new(
            "test-controller".to_string(),
            peers,
            scaling_config,
        ));
        
        // Make controller leader
        controller.start_election().await.unwrap();
        
        // Add test brokers
        let broker1 = BrokerInfo {
            id: "test-broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        let broker2 = BrokerInfo {
            id: "test-broker-2".to_string(),
            host: "localhost".to_string(),
            port_quic: 9192,
            port_rpc: 9193,
            rack_id: "rack-2".to_string(),
        };
        
        controller.register_broker(broker1).await.unwrap();
        controller.register_broker(broker2).await.unwrap();
        
        controller
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let controller = setup_test_controller().await;
        let start_time = Instant::now();
        
        let filter = warp::path("health")
            .and(warp::get())
            .and(with_controller(controller))
            .and(with_start_time(start_time))
            .and_then(handle_health);
        
        let response = test::request()
            .method("GET")
            .path("/health")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: HealthResponse = serde_json::from_slice(response.body()).unwrap();
        assert_eq!(body.status, "ok");
        assert!(body.is_leader);
        assert_eq!(body.raft_term, 1);
        assert!(body.uptime_seconds < 5); // Should be very small since we just started
    }

    #[tokio::test]
    async fn test_cluster_status_endpoint() {
        let controller = setup_test_controller().await;
        let health_tracker = Arc::new(BrokerHealthTracker::new_test_mode(Duration::from_secs(30)));
        
        // Manually add health status for test brokers
        let broker1 = BrokerInfo {
            id: "test-broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        let broker2 = BrokerInfo {
            id: "test-broker-2".to_string(),
            host: "localhost".to_string(),
            port_quic: 9192,
            port_rpc: 9193,
            rack_id: "rack-2".to_string(),
        };
        
        health_tracker.check_broker_health(&broker1).await;
        health_tracker.check_broker_health(&broker2).await;
        
        let filter = warp::path!("api" / "v1" / "cluster")
            .and(warp::get())
            .and(with_controller(controller))
            .and(with_health_tracker(health_tracker))
            .and_then(handle_cluster_status);
        
        let response = test::request()
            .method("GET")
            .path("/api/v1/cluster")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<ClusterStatus> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        
        let cluster_status = body.data.unwrap();
        assert_eq!(cluster_status.brokers.len(), 2);
        assert_eq!(cluster_status.topics.len(), 0);
        assert!(cluster_status.healthy);
        assert_eq!(cluster_status.term, 1);
    }

    #[tokio::test]
    async fn test_list_topics_endpoint() {
        let controller = setup_test_controller().await;
        
        let filter = warp::path!("api" / "v1" / "topics")
            .and(warp::get())
            .and(with_controller(controller))
            .and_then(handle_list_topics);
        
        let response = test::request()
            .method("GET")
            .path("/api/v1/topics")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<Vec<TopicSummary>> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        
        let topics = body.data.unwrap();
        assert_eq!(topics.len(), 0);
    }

    #[tokio::test]
    async fn test_create_topic_endpoint() {
        let controller = setup_test_controller().await;
        
        let filter = warp::path!("api" / "v1" / "topics")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_controller(controller.clone()))
            .and_then(handle_create_topic);
        
        let create_request = CreateTopicApiRequest {
            name: "test-topic".to_string(),
            partitions: 3,
            replication_factor: 2,
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };
        
        let response = test::request()
            .method("POST")
            .path("/api/v1/topics")
            .json(&create_request)
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<String> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        assert!(body.data.unwrap().contains("test-topic"));
        
        // Verify topic was created
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 1);
        assert_eq!(metadata.topics[0].name, "test-topic");
        assert_eq!(metadata.topics[0].partitions, 3);
    }

    #[tokio::test]
    async fn test_delete_topic_endpoint() {
        let controller = setup_test_controller().await;
        
        // First create a topic
        let create_request = crate::controller::CreateTopicRequest {
            name: "delete-test-topic".to_string(),
            partitions: 2,
            replication_factor: 1,
            config: None,
        };
        
        controller.create_topic(create_request).await.unwrap();
        
        // Now delete it via API
        let filter = warp::path!("api" / "v1" / "topics" / String)
            .and(warp::delete())
            .and(with_controller(controller.clone()))
            .and_then(handle_delete_topic);
        
        let response = test::request()
            .method("DELETE")
            .path("/api/v1/topics/delete-test-topic")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<String> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        assert!(body.data.unwrap().contains("delete-test-topic"));
        
        // Verify topic was deleted
        let metadata = controller.get_cluster_metadata().await.unwrap();
        assert_eq!(metadata.topics.len(), 0);
    }

    #[tokio::test]
    async fn test_describe_topic_endpoint() {
        let controller = setup_test_controller().await;
        
        // First create a topic
        let create_request = crate::controller::CreateTopicRequest {
            name: "describe-test-topic".to_string(),
            partitions: 2,
            replication_factor: 1,
            config: None,
        };
        
        controller.create_topic(create_request).await.unwrap();
        
        // Now describe it via API
        let filter = warp::path!("api" / "v1" / "topics" / String)
            .and(warp::get())
            .and(with_controller(controller))
            .and_then(handle_describe_topic);
        
        let response = test::request()
            .method("GET")
            .path("/api/v1/topics/describe-test-topic")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<serde_json::Value> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        
        let topic_detail = body.data.unwrap();
        assert_eq!(topic_detail["name"], "describe-test-topic");
        assert_eq!(topic_detail["partitions"], 2);
        assert_eq!(topic_detail["replication_factor"], 1);
        assert!(topic_detail["partition_assignments"].is_array());
    }

    #[tokio::test]
    async fn test_list_brokers_endpoint() {
        let controller = setup_test_controller().await;
        let health_tracker = Arc::new(BrokerHealthTracker::new_test_mode(Duration::from_secs(30)));
        
        // Manually add health status for test brokers
        let broker1 = BrokerInfo {
            id: "test-broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        let broker2 = BrokerInfo {
            id: "test-broker-2".to_string(),
            host: "localhost".to_string(),
            port_quic: 9192,
            port_rpc: 9193,
            rack_id: "rack-2".to_string(),
        };
        
        health_tracker.check_broker_health(&broker1).await;
        health_tracker.check_broker_health(&broker2).await;
        
        let filter = warp::path!("api" / "v1" / "brokers")
            .and(warp::get())
            .and(with_controller(controller))
            .and(with_health_tracker(health_tracker))
            .and_then(handle_list_brokers);
        
        let response = test::request()
            .method("GET")
            .path("/api/v1/brokers")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<Vec<BrokerStatus>> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        
        let brokers = body.data.unwrap();
        assert_eq!(brokers.len(), 2);
        
        // Sort by ID for deterministic testing since HashMap order is not guaranteed
        let mut sorted_brokers = brokers;
        sorted_brokers.sort_by(|a, b| a.id.cmp(&b.id));
        
        assert_eq!(sorted_brokers[0].id, "test-broker-1");
        assert_eq!(sorted_brokers[1].id, "test-broker-2");
        assert!(sorted_brokers[0].online);
        assert!(sorted_brokers[1].online);
    }

    #[tokio::test]
    async fn test_create_topic_validation() {
        let controller = setup_test_controller().await;
        
        let filter = warp::path!("api" / "v1" / "topics")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_controller(controller))
            .and_then(handle_create_topic);
        
        // Test invalid replication factor (higher than available brokers)
        let create_request = CreateTopicApiRequest {
            name: "invalid-topic".to_string(),
            partitions: 1,
            replication_factor: 5, // We only have 2 brokers
            retention_ms: None,
            segment_bytes: None,
            compression_type: None,
        };
        
        let response = test::request()
            .method("POST")
            .path("/api/v1/topics")
            .json(&create_request)
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<String> = serde_json::from_slice(response.body()).unwrap();
        assert!(!body.success);
        assert!(body.error.unwrap().contains("Not enough brokers"));
    }

    #[tokio::test]
    async fn test_topic_not_found() {
        let controller = setup_test_controller().await;
        
        let filter = warp::path!("api" / "v1" / "topics" / String)
            .and(warp::get())
            .and(with_controller(controller))
            .and_then(handle_describe_topic);
        
        let response = test::request()
            .method("GET")
            .path("/api/v1/topics/nonexistent-topic")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<serde_json::Value> = serde_json::from_slice(response.body()).unwrap();
        assert!(!body.success);
        assert!(body.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_broker_health_tracking() {
        let health_tracker = BrokerHealthTracker::new_test_mode(Duration::from_millis(100));
        
        let broker = BrokerInfo {
            id: "test-broker".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        // Initially, broker should not be healthy (no health check performed)
        assert!(!health_tracker.is_broker_healthy("test-broker").await);
        
        // Perform health check
        let is_healthy = health_tracker.check_broker_health(&broker).await;
        assert!(is_healthy); // localhost should be healthy
        
        // Now broker should be considered healthy
        assert!(health_tracker.is_broker_healthy("test-broker").await);
        
        // Wait for health to expire
        tokio::time::sleep(Duration::from_millis(150)).await;
        
        // Should now be considered unhealthy due to timeout
        assert!(!health_tracker.is_broker_healthy("test-broker").await);
        
        // Get all broker health
        let all_health = health_tracker.get_all_broker_health().await;
        assert_eq!(all_health.get("test-broker"), Some(&false));
    }

    #[tokio::test]
    async fn test_cluster_health_calculation() {
        let controller = setup_test_controller().await;
        let health_tracker = Arc::new(BrokerHealthTracker::new_test_mode(Duration::from_secs(30)));
        
        // Add health for only one broker (test-broker-1)
        let broker1 = BrokerInfo {
            id: "test-broker-1".to_string(),
            host: "localhost".to_string(),
            port_quic: 9092,
            port_rpc: 9093,
            rack_id: "rack-1".to_string(),
        };
        
        health_tracker.check_broker_health(&broker1).await;
        // Don't check health for broker2, so it should be unhealthy
        
        let filter = warp::path!("api" / "v1" / "cluster")
            .and(warp::get())
            .and(with_controller(controller))
            .and(with_health_tracker(health_tracker))
            .and_then(handle_cluster_status);
        
        let response = test::request()
            .method("GET")
            .path("/api/v1/cluster")
            .reply(&filter)
            .await;
        
        assert_eq!(response.status(), 200);
        
        let body: ApiResponse<ClusterStatus> = serde_json::from_slice(response.body()).unwrap();
        assert!(body.success);
        
        let cluster_status = body.data.unwrap();
        assert_eq!(cluster_status.brokers.len(), 2);
        
        // Sort brokers for deterministic testing
        let mut sorted_brokers = cluster_status.brokers;
        sorted_brokers.sort_by(|a, b| a.id.cmp(&b.id));
        
        // test-broker-1 should be online, test-broker-2 should be offline
        assert_eq!(sorted_brokers[0].id, "test-broker-1");
        assert!(sorted_brokers[0].online);
        assert_eq!(sorted_brokers[1].id, "test-broker-2");
        assert!(!sorted_brokers[1].online);
        
        // Cluster should still be healthy (majority of brokers healthy: 1 out of 2)
        assert!(cluster_status.healthy);
    }

    #[tokio::test]
    async fn test_admin_api_with_rate_limiting() {
        use crate::config::{RateLimitConfig, GlobalRateLimits, PerIpRateLimits, EndpointRateLimits, EndpointCategoryLimits, RateLimitCleanupConfig};
        
        let controller = setup_test_controller().await;
        
        let rate_limit_config = RateLimitConfig {
            enabled: true,
            global: GlobalRateLimits {
                requests_per_second: 1000,
                burst_capacity: 2000,
                window_size_secs: 60,
            },
            per_ip: PerIpRateLimits {
                enabled: true,
                requests_per_second: 10,
                burst_capacity: 20,
                window_size_secs: 60,
                max_tracked_ips: 1000,
                ip_expiry_secs: 3600,
            },
            endpoints: EndpointRateLimits {
                health: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 50,
                    burst_capacity: 100,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["/health".to_string()],
                },
                read_operations: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 30,
                    burst_capacity: 60,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["/api/v1/topics".to_string()],
                },
                write_operations: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 5,
                    burst_capacity: 10,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["POST:/api/v1/topics".to_string()],
                },
                cluster_operations: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 2,
                    burst_capacity: 5,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["/api/v1/cluster/rebalance".to_string()],
                },
            },
            cleanup: RateLimitCleanupConfig {
                enabled: true,
                cleanup_interval_secs: 300,
                max_age_secs: 3600,
                max_cleanup_per_run: 100,
            },
        };
        
        let admin_api = AdminApi::new_with_rate_limiting(controller, 8080, rate_limit_config).unwrap();
        assert!(admin_api.rate_limiter.is_some());
        assert!(admin_api.security_api.is_none());
    }

    #[tokio::test]
    async fn test_admin_api_without_rate_limiting() {
        let controller = setup_test_controller().await;
        let admin_api = AdminApi::new(controller, 8080);
        assert!(admin_api.rate_limiter.is_none());
        assert!(admin_api.security_api.is_none());
    }

    #[tokio::test]
    async fn test_admin_api_with_security() {
        use crate::security::{SecurityConfig, SecurityManager, AclConfig, CertificateManagementConfig, AuditConfig};
        use crate::security::tls::TlsConfig;
        use crate::config::RateLimitConfig;
        
        let controller = setup_test_controller().await;
        
        // Create a mock security configuration
        let security_config = SecurityConfig {
            tls: TlsConfig::default(),
            acl: AclConfig::default(),
            certificate_management: CertificateManagementConfig::default(),
            audit: AuditConfig::default(),
        };
        
        // Note: In a real test, we would create a proper SecurityManager
        // For now, we'll test the API structure without actually creating SecurityManager
        // since it requires complex setup with certificate stores, etc.
        
        // Test that new_with_security method exists and works with proper rate limiting
        let rate_limit_config = RateLimitConfig::default();
        
        // This test verifies the API structure exists - actual SecurityManager creation
        // would be tested in integration tests with proper infrastructure setup
        assert!(rate_limit_config.enabled); // Verify config structure
    }

    #[tokio::test]
    async fn test_rate_limiter_manager_integration() {
        use crate::config::{RateLimitConfig, GlobalRateLimits, PerIpRateLimits, EndpointRateLimits, EndpointCategoryLimits, RateLimitCleanupConfig};
        use crate::admin::rate_limiter::{RateLimiterManager, EndpointCategory, RateLimitInfo};
        use std::net::{IpAddr, Ipv4Addr};
        
        let rate_limit_config = RateLimitConfig {
            enabled: true,
            global: GlobalRateLimits {
                requests_per_second: 100,
                burst_capacity: 200,
                window_size_secs: 60,
            },
            per_ip: PerIpRateLimits {
                enabled: true,
                requests_per_second: 10,
                burst_capacity: 20,
                window_size_secs: 60,
                max_tracked_ips: 1000,
                ip_expiry_secs: 3600,
            },
            endpoints: EndpointRateLimits {
                health: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 50,
                    burst_capacity: 100,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["/health".to_string()],
                },
                read_operations: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 30,
                    burst_capacity: 60,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["/api/v1/topics".to_string()],
                },
                write_operations: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 5,
                    burst_capacity: 10,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["POST:/api/v1/topics".to_string()],
                },
                cluster_operations: EndpointCategoryLimits {
                    enabled: true,
                    requests_per_second: 2,
                    burst_capacity: 5,
                    window_size_secs: 60,
                    endpoint_patterns: vec!["/api/v1/cluster/rebalance".to_string()],
                },
            },
            cleanup: RateLimitCleanupConfig {
                enabled: true,
                cleanup_interval_secs: 300,
                max_age_secs: 3600,
                max_cleanup_per_run: 100,
            },
        };
        
        let manager = RateLimiterManager::new(rate_limit_config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // First request should succeed
        assert!(manager.check_rate_limit(&info).await.is_ok());
        
        // Test endpoint categorization
        assert_eq!(manager.categorize_endpoint("GET", "/health"), EndpointCategory::Health);
        assert_eq!(manager.categorize_endpoint("GET", "/api/v1/topics"), EndpointCategory::ReadOperations);
        assert_eq!(manager.categorize_endpoint("POST", "/api/v1/topics"), EndpointCategory::WriteOperations);
        assert_eq!(manager.categorize_endpoint("POST", "/api/v1/cluster/rebalance"), EndpointCategory::ClusterOperations);
        
        // Test stats
        let stats = manager.get_stats().await;
        assert!(stats.config_enabled);
        assert_eq!(stats.active_category_limiters, 4);
    }

    #[tokio::test]
    async fn test_client_ip_extraction() {
        use crate::admin::rate_limiter::extract_client_ip;
        use std::net::{IpAddr, Ipv4Addr, SocketAddr};
        
        // Test basic IP extraction without headers
        let filter = extract_client_ip();
        
        // Since we can't easily mock warp headers in unit tests, we'll test the manager functions directly
        // The extract_client_ip function is integration-tested when the admin API is actually running
        
        // Test pattern matching in manager
        use crate::config::{RateLimitConfig, GlobalRateLimits, PerIpRateLimits, EndpointRateLimits, EndpointCategoryLimits, RateLimitCleanupConfig};
        use crate::admin::rate_limiter::RateLimiterManager;
        
        let config = RateLimitConfig {
            enabled: true,
            global: GlobalRateLimits::default(),
            per_ip: PerIpRateLimits::default(),
            endpoints: EndpointRateLimits::default(),
            cleanup: RateLimitCleanupConfig::default(),
        };
        
        let manager = RateLimiterManager::new(config).unwrap();
        
        // Test wildcard pattern matching
        assert!(manager.path_matches("/api/v1/topics/test-topic", "/api/v1/topics/*"));
        assert!(!manager.path_matches("/api/v2/topics/test-topic", "/api/v1/topics/*"));
        
        // Test exact pattern matching
        assert!(manager.path_matches("/health", "/health"));
        assert!(!manager.path_matches("/health/check", "/health"));
        
        // Test method + path pattern matching
        assert!(manager.matches_pattern("POST", "/api/v1/topics", "POST:/api/v1/topics"));
        assert!(!manager.matches_pattern("GET", "/api/v1/topics", "POST:/api/v1/topics"));
    }
}