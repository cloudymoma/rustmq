use crate::config::{RateLimitConfig, EndpointCategoryLimits};
use crate::Result;
use dashmap::DashMap;
use governor::{
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::{debug, info, warn, error};
use warp::{Filter, Rejection, Reply};

/// Type alias for simple governor rate limiter (direct approach)
type AppRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Rate limiter manager for the admin API
#[derive(Debug)]
pub struct RateLimiterManager {
    config: RateLimitConfig,
    /// Global rate limiter
    global_limiter: Option<Arc<AppRateLimiter>>,
    /// Per-IP rate limiters
    ip_limiters: Arc<DashMap<IpAddr, IpRateLimiter>>,
    /// Endpoint category limiters
    category_limiters: Arc<RwLock<HashMap<EndpointCategory, Arc<AppRateLimiter>>>>,
    /// Cleanup tracking
    cleanup_state: Arc<RwLock<CleanupState>>,
}

#[derive(Debug, Clone)]
struct IpRateLimiter {
    limiter: Arc<AppRateLimiter>,
    last_access: Instant,
    request_count: u64,
}

#[derive(Debug)]
struct CleanupState {
    last_cleanup: Instant,
    cleanup_running: bool,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum EndpointCategory {
    Health,
    ReadOperations,
    WriteOperations,
    ClusterOperations,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimitError {
    pub error: String,
    pub retry_after: u64,
    pub limit: u32,
    pub remaining: u32,
    pub reset_time: u64,
}

#[derive(Debug)]
pub struct RateLimitInfo {
    pub category: EndpointCategory,
    pub client_ip: IpAddr,
    pub endpoint: String,
    pub method: String,
}

impl RateLimiterManager {
    /// Create a new rate limiter manager
    pub fn new(config: RateLimitConfig) -> Result<Self> {
        // Validate configuration
        config.validate()?;
        
        let global_limiter = if config.enabled && config.global.requests_per_second > 0 {
            let quota = Quota::per_second(NonZeroU32::new(config.global.requests_per_second)
                .ok_or_else(|| crate::error::RustMqError::InvalidConfig(
                    "global requests_per_second must be greater than 0".to_string()
                ))?)
                .allow_burst(NonZeroU32::new(config.global.burst_capacity)
                    .ok_or_else(|| crate::error::RustMqError::InvalidConfig(
                        "global burst_capacity must be greater than 0".to_string()
                    ))?);
            
            Some(Arc::new(RateLimiter::direct(quota)))
        } else {
            None
        };
        
        let mut category_limiters = HashMap::new();
        
        // Create endpoint category limiters
        if config.enabled {
            if config.endpoints.health.enabled {
                category_limiters.insert(
                    EndpointCategory::Health,
                    Self::create_category_limiter(&config.endpoints.health)?,
                );
            }
            
            if config.endpoints.read_operations.enabled {
                category_limiters.insert(
                    EndpointCategory::ReadOperations,
                    Self::create_category_limiter(&config.endpoints.read_operations)?,
                );
            }
            
            if config.endpoints.write_operations.enabled {
                category_limiters.insert(
                    EndpointCategory::WriteOperations,
                    Self::create_category_limiter(&config.endpoints.write_operations)?,
                );
            }
            
            if config.endpoints.cluster_operations.enabled {
                category_limiters.insert(
                    EndpointCategory::ClusterOperations,
                    Self::create_category_limiter(&config.endpoints.cluster_operations)?,
                );
            }
        }
        
        let num_category_limiters = category_limiters.len();
        
        let manager = Self {
            config,
            global_limiter,
            ip_limiters: Arc::new(DashMap::new()),
            category_limiters: Arc::new(RwLock::new(category_limiters)),
            cleanup_state: Arc::new(RwLock::new(CleanupState {
                last_cleanup: Instant::now(),
                cleanup_running: false,
            })),
        };
        
        // Start cleanup task if enabled
        if manager.config.cleanup.enabled {
            let manager_clone = manager.clone();
            tokio::spawn(async move {
                manager_clone.cleanup_task().await;
            });
        }
        
        info!("Rate limiter manager initialized with {} category limiters", 
              num_category_limiters);
        
        Ok(manager)
    }
    
    /// Create a rate limiter for an endpoint category
    fn create_category_limiter(
        category_config: &EndpointCategoryLimits,
    ) -> Result<Arc<AppRateLimiter>> {
        let quota = Quota::per_second(NonZeroU32::new(category_config.requests_per_second)
            .ok_or_else(|| crate::error::RustMqError::InvalidConfig(
                "category requests_per_second must be greater than 0".to_string()
            ))?)
            .allow_burst(NonZeroU32::new(category_config.burst_capacity)
                .ok_or_else(|| crate::error::RustMqError::InvalidConfig(
                    "category burst_capacity must be greater than 0".to_string()
                ))?);
        
        Ok(Arc::new(RateLimiter::direct(quota)))
    }
    
    /// Check if a request should be rate limited
    pub async fn check_rate_limit(&self, info: &RateLimitInfo) -> std::result::Result<(), RateLimitError> {
        if !self.config.enabled {
            return Ok(());
        }
        
        debug!("Checking rate limit for IP {} on {} {}", 
               info.client_ip, info.method, info.endpoint);
        
        // Check global rate limit first
        if let Some(global_limiter) = &self.global_limiter {
            if let Err(_) = global_limiter.check() {
                let retry_after = self.calculate_retry_after(&self.config.global.requests_per_second);
                warn!("Global rate limit exceeded for IP {}", info.client_ip);
                return Err(RateLimitError {
                    error: "Global rate limit exceeded".to_string(),
                    retry_after,
                    limit: self.config.global.requests_per_second,
                    remaining: 0,
                    reset_time: Self::calculate_reset_time(retry_after),
                });
            }
        }
        
        // Check per-IP rate limit
        if self.config.per_ip.enabled {
            if let Err(_) = self.check_ip_rate_limit(info.client_ip).await {
                let retry_after = self.calculate_retry_after(&self.config.per_ip.requests_per_second);
                warn!("Per-IP rate limit exceeded for IP {}", info.client_ip);
                return Err(RateLimitError {
                    error: "Per-IP rate limit exceeded".to_string(),
                    retry_after,
                    limit: self.config.per_ip.requests_per_second,
                    remaining: 0,
                    reset_time: Self::calculate_reset_time(retry_after),
                });
            }
        }
        
        // Check endpoint category rate limit
        if let Err(e) = self.check_category_rate_limit(&info.category, info.client_ip).await {
            warn!("Category rate limit exceeded for IP {} on {:?}", info.client_ip, info.category);
            return Err(e);
        }
        
        // Update IP access time
        self.update_ip_access(info.client_ip).await;
        
        // Trigger cleanup if needed
        self.maybe_trigger_cleanup().await;
        
        debug!("Rate limit check passed for IP {} on {} {}", 
               info.client_ip, info.method, info.endpoint);
        
        Ok(())
    }
    
    /// Check per-IP rate limit
    async fn check_ip_rate_limit(&self, client_ip: IpAddr) -> std::result::Result<(), ()> {
        // Get or create IP rate limiter
        let ip_limiter = self.ip_limiters.entry(client_ip).or_insert_with(|| {
            let quota = Quota::per_second(NonZeroU32::new(self.config.per_ip.requests_per_second).unwrap())
                .allow_burst(NonZeroU32::new(self.config.per_ip.burst_capacity).unwrap());
            
            IpRateLimiter {
                limiter: Arc::new(RateLimiter::direct(quota)),
                last_access: Instant::now(),
                request_count: 0,
            }
        });
        
        ip_limiter.limiter.check().map_err(|_| ())
    }
    
    /// Check endpoint category rate limit
    async fn check_category_rate_limit(
        &self, 
        category: &EndpointCategory, 
        _client_ip: IpAddr
    ) -> std::result::Result<(), RateLimitError> {
        let category_limiters = self.category_limiters.read().await;
        
        if let Some(limiter) = category_limiters.get(category) {
            if let Err(_) = limiter.check() {
                let (limit, retry_after) = match category {
                    EndpointCategory::Health => {
                        (self.config.endpoints.health.requests_per_second,
                         self.calculate_retry_after(&self.config.endpoints.health.requests_per_second))
                    }
                    EndpointCategory::ReadOperations => {
                        (self.config.endpoints.read_operations.requests_per_second,
                         self.calculate_retry_after(&self.config.endpoints.read_operations.requests_per_second))
                    }
                    EndpointCategory::WriteOperations => {
                        (self.config.endpoints.write_operations.requests_per_second,
                         self.calculate_retry_after(&self.config.endpoints.write_operations.requests_per_second))
                    }
                    EndpointCategory::ClusterOperations => {
                        (self.config.endpoints.cluster_operations.requests_per_second,
                         self.calculate_retry_after(&self.config.endpoints.cluster_operations.requests_per_second))
                    }
                };
                
                return Err(RateLimitError {
                    error: format!("{:?} rate limit exceeded", category),
                    retry_after,
                    limit,
                    remaining: 0,
                    reset_time: Self::calculate_reset_time(retry_after),
                });
            }
        }
        
        Ok(())
    }
    
    /// Update IP access time and request count
    async fn update_ip_access(&self, client_ip: IpAddr) {
        if let Some(mut ip_limiter) = self.ip_limiters.get_mut(&client_ip) {
            ip_limiter.last_access = Instant::now();
            ip_limiter.request_count += 1;
        }
    }
    
    /// Calculate retry after duration in seconds
    fn calculate_retry_after(&self, requests_per_second: &u32) -> u64 {
        // Simple calculation: 1 second + some buffer
        std::cmp::max(1, 60 / (*requests_per_second as u64))
    }
    
    /// Calculate reset time (Unix timestamp)
    fn calculate_reset_time(retry_after: u64) -> u64 {
        (Instant::now() + Duration::from_secs(retry_after))
            .duration_since(Instant::now())
            .as_secs() + 
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    /// Maybe trigger cleanup if enough time has passed
    async fn maybe_trigger_cleanup(&self) {
        let mut cleanup_state = self.cleanup_state.write().await;
        
        if !cleanup_state.cleanup_running && 
           cleanup_state.last_cleanup.elapsed().as_secs() >= self.config.cleanup.cleanup_interval_secs {
            cleanup_state.cleanup_running = true;
            cleanup_state.last_cleanup = Instant::now();
            drop(cleanup_state);
            
            let manager_clone = self.clone();
            tokio::spawn(async move {
                manager_clone.perform_cleanup().await;
                
                let mut cleanup_state = manager_clone.cleanup_state.write().await;
                cleanup_state.cleanup_running = false;
            });
        }
    }
    
    /// Background cleanup task
    async fn cleanup_task(&self) {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.cleanup.cleanup_interval_secs));
        
        loop {
            interval.tick().await;
            
            if !self.config.cleanup.enabled {
                break;
            }
            
            self.perform_cleanup().await;
        }
    }
    
    /// Perform cleanup of expired rate limiters
    async fn perform_cleanup(&self) {
        let max_age = Duration::from_secs(self.config.cleanup.max_age_secs);
        let mut cleaned_count = 0;
        let max_cleanup = self.config.cleanup.max_cleanup_per_run;
        
        // Clean up IP rate limiters
        self.ip_limiters.retain(|ip, limiter| {
            if cleaned_count >= max_cleanup {
                return true; // Stop cleaning if we've reached the limit
            }
            
            let should_retain = limiter.last_access.elapsed() < max_age;
            if !should_retain {
                debug!("Cleaning up rate limiter for IP {}", ip);
                cleaned_count += 1;
            }
            should_retain
        });
        
        if cleaned_count > 0 {
            info!("Cleaned up {} expired IP rate limiters", cleaned_count);
        }
        
        debug!("Rate limiter cleanup completed. Active IP limiters: {}", 
               self.ip_limiters.len());
    }
    
    /// Categorize an endpoint based on method and path
    pub fn categorize_endpoint(&self, method: &str, path: &str) -> EndpointCategory {
        // Check health endpoints
        for pattern in &self.config.endpoints.health.endpoint_patterns {
            if self.matches_pattern(method, path, pattern) {
                return EndpointCategory::Health;
            }
        }
        
        // Check cluster operations endpoints
        for pattern in &self.config.endpoints.cluster_operations.endpoint_patterns {
            if self.matches_pattern(method, path, pattern) {
                return EndpointCategory::ClusterOperations;
            }
        }
        
        // Check write operations (POST/DELETE/PUT/PATCH)
        if matches!(method, "POST" | "DELETE" | "PUT" | "PATCH") {
            for pattern in &self.config.endpoints.write_operations.endpoint_patterns {
                if self.matches_pattern(method, path, pattern) {
                    return EndpointCategory::WriteOperations;
                }
            }
        }
        
        // Default to read operations for GET requests
        EndpointCategory::ReadOperations
    }
    
    /// Check if method and path match a pattern
    pub fn matches_pattern(&self, method: &str, path: &str, pattern: &str) -> bool {
        // Support patterns like "POST:/api/v1/topics" or "/api/v1/topics"
        if pattern.contains(':') {
            let parts: Vec<&str> = pattern.splitn(2, ':').collect();
            if parts.len() == 2 {
                let pattern_method = parts[0];
                let pattern_path = parts[1];
                return method.eq_ignore_ascii_case(pattern_method) && 
                       self.path_matches(path, pattern_path);
            }
        }
        
        // Simple path matching
        self.path_matches(path, pattern)
    }
    
    /// Check if path matches pattern (with simple wildcard support)
    pub fn path_matches(&self, path: &str, pattern: &str) -> bool {
        if pattern.ends_with("/*") {
            let prefix = &pattern[..pattern.len() - 2];
            path.starts_with(prefix)
        } else if pattern.contains('*') {
            // More complex wildcard matching could be implemented here
            // For now, just do exact match if there's a wildcard in the middle
            false
        } else {
            path == pattern
        }
    }
    
    /// Get rate limiting statistics
    pub async fn get_stats(&self) -> RateLimitStats {
        let active_ip_limiters = self.ip_limiters.len();
        let category_limiters = self.category_limiters.read().await;
        let active_category_limiters = category_limiters.len();
        
        // Calculate total requests from IP limiters
        let total_requests: u64 = self.ip_limiters.iter()
            .map(|entry| entry.value().request_count)
            .sum();
        
        RateLimitStats {
            active_ip_limiters,
            active_category_limiters,
            total_requests,
            config_enabled: self.config.enabled,
        }
    }
}

impl Clone for RateLimiterManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            global_limiter: self.global_limiter.clone(),
            ip_limiters: self.ip_limiters.clone(),
            category_limiters: self.category_limiters.clone(),
            cleanup_state: self.cleanup_state.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RateLimitStats {
    pub active_ip_limiters: usize,
    pub active_category_limiters: usize,
    pub total_requests: u64,
    pub config_enabled: bool,
}

/// Extract client IP from warp request
pub fn extract_client_ip() -> impl Filter<Extract = (IpAddr,), Error = Rejection> + Clone {
    warp::header::optional::<String>("x-forwarded-for")
        .and(warp::header::optional::<String>("x-real-ip"))
        .and(warp::addr::remote())
        .map(|x_forwarded_for: Option<String>, x_real_ip: Option<String>, remote: Option<SocketAddr>| {
            // Try to extract IP from headers first (for proxy scenarios)
            if let Some(forwarded) = x_forwarded_for {
                if let Some(first_ip) = forwarded.split(',').next() {
                    if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                        return ip;
                    }
                }
            }
            
            if let Some(real_ip) = x_real_ip {
                if let Ok(ip) = real_ip.parse::<IpAddr>() {
                    return ip;
                }
            }
            
            // Fallback to remote address
            remote.map(|addr| addr.ip()).unwrap_or_else(|| "127.0.0.1".parse().unwrap())
        })
}

/// Create rate limiting filter
pub fn with_rate_limiter(
    manager: Arc<RateLimiterManager>,
) -> impl Filter<Extract = (Arc<RateLimiterManager>,), Error = Infallible> + Clone {
    warp::any().map(move || manager.clone())
}

/// Rate limiting middleware filter
pub fn rate_limit_filter(
    manager: Arc<RateLimiterManager>,
) -> warp::filters::BoxedFilter<()> {
    extract_client_ip()
        .and(warp::method())
        .and(warp::path::full())
        .and(with_rate_limiter(manager))
        .and_then(|client_ip: IpAddr, method: warp::http::Method, path: warp::path::FullPath, manager: Arc<RateLimiterManager>| async move {
            let category = manager.categorize_endpoint(method.as_str(), path.as_str());
            
            let info = RateLimitInfo {
                category,
                client_ip,
                endpoint: path.as_str().to_string(),
                method: method.as_str().to_string(),
            };
            
            match manager.check_rate_limit(&info).await {
                Ok(()) => Ok(()),
                Err(rate_limit_error) => {
                    error!("Rate limit exceeded for {} {} from IP {}: {}", 
                           method.as_str(), path.as_str(), client_ip, rate_limit_error.error);
                    
                    Err(warp::reject::custom(RateLimitRejection(rate_limit_error)))
                }
            }
        })
        .untuple_one()
        .boxed()
}

/// Custom rejection for rate limiting
#[derive(Debug)]
pub struct RateLimitRejection(pub RateLimitError);

impl warp::reject::Reject for RateLimitRejection {}

/// Handle rate limit rejections
pub async fn handle_rate_limit_rejection(rejection: Rejection) -> std::result::Result<warp::reply::Response, Infallible> {
    if let Some(rate_limit_rejection) = rejection.find::<RateLimitRejection>() {
        let error_response = crate::admin::api::ApiResponse::<()> {
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
        
        Ok(reply.into_response())
    } else {
        // Let other rejections pass through
        let reply = warp::reply::with_status(
            warp::reply::json(&crate::admin::api::ApiResponse::<()> {
                success: false,
                data: None,
                error: Some("Internal server error".to_string()),
                leader_hint: None,
            }),
            warp::http::StatusCode::INTERNAL_SERVER_ERROR
        );
        Ok(reply.into_response())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RateLimitConfig, GlobalRateLimits, PerIpRateLimits, EndpointRateLimits, EndpointCategoryLimits, RateLimitCleanupConfig};
    use std::net::Ipv4Addr;

    fn create_test_config() -> RateLimitConfig {
        RateLimitConfig {
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
                    endpoint_patterns: vec!["/api/v1/topics".to_string(), "/api/v1/brokers".to_string()],
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
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_manager_creation() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        assert!(manager.global_limiter.is_some());
        assert_eq!(manager.ip_limiters.len(), 0);
        
        let category_limiters = manager.category_limiters.read().await;
        assert_eq!(category_limiters.len(), 4);
        assert!(category_limiters.contains_key(&EndpointCategory::Health));
        assert!(category_limiters.contains_key(&EndpointCategory::ReadOperations));
        assert!(category_limiters.contains_key(&EndpointCategory::WriteOperations));
        assert!(category_limiters.contains_key(&EndpointCategory::ClusterOperations));
    }

    #[tokio::test]
    async fn test_endpoint_categorization() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        // Test health endpoint
        assert_eq!(
            manager.categorize_endpoint("GET", "/health"),
            EndpointCategory::Health
        );
        
        // Test read operations
        assert_eq!(
            manager.categorize_endpoint("GET", "/api/v1/topics"),
            EndpointCategory::ReadOperations
        );
        
        // Test write operations
        assert_eq!(
            manager.categorize_endpoint("POST", "/api/v1/topics"),
            EndpointCategory::WriteOperations
        );
        
        // Test cluster operations
        assert_eq!(
            manager.categorize_endpoint("POST", "/api/v1/cluster/rebalance"),
            EndpointCategory::ClusterOperations
        );
        
        // Test default to read operations for unknown GET endpoint
        assert_eq!(
            manager.categorize_endpoint("GET", "/unknown/endpoint"),
            EndpointCategory::ReadOperations
        );
    }

    #[tokio::test]
    async fn test_pattern_matching() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        // Test exact path matching
        assert!(manager.path_matches("/health", "/health"));
        assert!(!manager.path_matches("/health", "/api/v1/health"));
        
        // Test wildcard matching
        assert!(manager.path_matches("/api/v1/topics/test", "/api/v1/topics/*"));
        assert!(!manager.path_matches("/api/v2/topics/test", "/api/v1/topics/*"));
        
        // Test method + path matching
        assert!(manager.matches_pattern("POST", "/api/v1/topics", "POST:/api/v1/topics"));
        assert!(!manager.matches_pattern("GET", "/api/v1/topics", "POST:/api/v1/topics"));
    }

    #[tokio::test]
    async fn test_rate_limit_check_success() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // First request should succeed
        assert!(manager.check_rate_limit(&info).await.is_ok());
        
        // Check that IP limiter was created
        assert_eq!(manager.ip_limiters.len(), 1);
        assert!(manager.ip_limiters.contains_key(&client_ip));
    }

    #[tokio::test]
    async fn test_per_ip_rate_limiting() {
        let mut config = create_test_config();
        config.per_ip.requests_per_second = 1; // Very restrictive
        config.per_ip.burst_capacity = 1;
        
        let manager = RateLimiterManager::new(config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // First request should succeed
        assert!(manager.check_rate_limit(&info).await.is_ok());
        
        // Second request should be rate limited
        let result = manager.check_rate_limit(&info).await;
        assert!(result.is_err());
        
        if let Err(error) = result {
            assert!(error.error.contains("Per-IP rate limit exceeded"));
            assert_eq!(error.limit, 1);
            assert_eq!(error.remaining, 0);
            assert!(error.retry_after > 0);
        }
    }

    #[tokio::test]
    async fn test_category_rate_limiting() {
        let mut config = create_test_config();
        config.endpoints.write_operations.requests_per_second = 1; // Very restrictive
        config.endpoints.write_operations.burst_capacity = 1;
        
        let manager = RateLimiterManager::new(config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::WriteOperations,
            client_ip,
            endpoint: "/api/v1/topics".to_string(),
            method: "POST".to_string(),
        };
        
        // First request should succeed
        assert!(manager.check_rate_limit(&info).await.is_ok());
        
        // Second request should be rate limited
        let result = manager.check_rate_limit(&info).await;
        assert!(result.is_err());
        
        if let Err(error) = result {
            assert!(error.error.contains("WriteOperations rate limit exceeded"));
            assert_eq!(error.limit, 1);
        }
    }

    #[tokio::test]
    async fn test_disabled_rate_limiting() {
        let mut config = create_test_config();
        config.enabled = false;
        
        let manager = RateLimiterManager::new(config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // All requests should succeed when rate limiting is disabled
        for _ in 0..100 {
            assert!(manager.check_rate_limit(&info).await.is_ok());
        }
    }

    #[tokio::test]
    async fn test_cleanup_functionality() {
        let mut config = create_test_config();
        config.cleanup.max_age_secs = 1; // Very short expiry for testing
        
        let manager = RateLimiterManager::new(config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // Make a request to create an IP limiter
        assert!(manager.check_rate_limit(&info).await.is_ok());
        assert_eq!(manager.ip_limiters.len(), 1);
        
        // Wait for expiry
        tokio::time::sleep(tokio::time::Duration::from_millis(1100)).await;
        
        // Perform cleanup
        manager.perform_cleanup().await;
        
        // IP limiter should be cleaned up
        assert_eq!(manager.ip_limiters.len(), 0);
    }

    #[tokio::test]
    async fn test_get_stats() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        let client_ip = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let info = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // Make some requests
        for _ in 0..5 {
            let _ = manager.check_rate_limit(&info).await;
        }
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.active_ip_limiters, 1);
        assert_eq!(stats.active_category_limiters, 4);
        assert!(stats.total_requests >= 1);
        assert!(stats.config_enabled);
    }

    #[tokio::test]
    async fn test_multiple_ips() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        let ip1 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let ip2 = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2));
        
        let info1 = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip: ip1,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        let info2 = RateLimitInfo {
            category: EndpointCategory::Health,
            client_ip: ip2,
            endpoint: "/health".to_string(),
            method: "GET".to_string(),
        };
        
        // Both IPs should be able to make requests
        assert!(manager.check_rate_limit(&info1).await.is_ok());
        assert!(manager.check_rate_limit(&info2).await.is_ok());
        
        // Should have two separate IP limiters
        assert_eq!(manager.ip_limiters.len(), 2);
        assert!(manager.ip_limiters.contains_key(&ip1));
        assert!(manager.ip_limiters.contains_key(&ip2));
    }

    #[tokio::test]
    async fn test_retry_after_calculation() {
        let config = create_test_config();
        let manager = RateLimiterManager::new(config).unwrap();
        
        // Test with different rates
        assert_eq!(manager.calculate_retry_after(&60), 1); // 60 RPS -> 1 second
        assert_eq!(manager.calculate_retry_after(&30), 2); // 30 RPS -> 2 seconds
        assert_eq!(manager.calculate_retry_after(&10), 6); // 10 RPS -> 6 seconds
        assert_eq!(manager.calculate_retry_after(&1), 60); // 1 RPS -> 60 seconds
    }
}