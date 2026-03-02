use serde::{Deserialize, Serialize};

/// Comprehensive rate limiting configuration using Token Bucket algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Global rate limiting toggle - enable/disable all rate limiting
    pub enabled: bool,
    /// Global rate limits that apply across all clients and endpoints
    pub global: GlobalRateLimits,
    /// Per-IP rate limits with burst allowance
    pub per_ip: PerIpRateLimits,
    /// Endpoint-specific rate limits categorized by operation type
    pub endpoints: EndpointRateLimits,
    /// Cleanup configuration for expired rate limiters
    pub cleanup: RateLimitCleanupConfig,
}

/// Global rate limits that apply across the entire admin API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRateLimits {
    /// Maximum requests per second across all clients
    pub requests_per_second: u32,
    /// Maximum burst capacity for global rate limiting
    pub burst_capacity: u32,
    /// Window size in seconds for rate limit calculations
    pub window_size_secs: u64,
}

/// Per-IP rate limiting configuration with burst allowance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerIpRateLimits {
    /// Enable per-IP rate limiting
    pub enabled: bool,
    /// Maximum requests per second per IP address
    pub requests_per_second: u32,
    /// Burst capacity - allows short bursts above the sustained rate
    pub burst_capacity: u32,
    /// Window size in seconds for per-IP rate calculations
    pub window_size_secs: u64,
    /// Maximum number of IP addresses to track simultaneously
    pub max_tracked_ips: usize,
    /// Time to keep IP rate limiters after last access (seconds)
    pub ip_expiry_secs: u64,
}

/// Endpoint-specific rate limits categorized by operation type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointRateLimits {
    /// Rate limits for health check endpoints (frequent monitoring)
    pub health: EndpointCategoryLimits,
    /// Rate limits for read operations (listing, describing resources)
    pub read_operations: EndpointCategoryLimits,
    /// Rate limits for write operations (creating, deleting resources)
    pub write_operations: EndpointCategoryLimits,
    /// Rate limits for cluster management operations (most expensive)
    pub cluster_operations: EndpointCategoryLimits,
}

/// Rate limiting configuration for a specific category of endpoints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointCategoryLimits {
    /// Enable rate limiting for this endpoint category
    pub enabled: bool,
    /// Maximum requests per second for this category
    pub requests_per_second: u32,
    /// Burst capacity for this category
    pub burst_capacity: u32,
    /// Window size in seconds for this category
    pub window_size_secs: u64,
    /// List of endpoint patterns that belong to this category
    pub endpoint_patterns: Vec<String>,
}

/// Configuration for cleaning up expired rate limiters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitCleanupConfig {
    /// Enable automatic cleanup of expired rate limiters
    pub enabled: bool,
    /// Interval between cleanup runs (seconds)
    pub cleanup_interval_secs: u64,
    /// Maximum age of unused rate limiters before cleanup (seconds)
    pub max_age_secs: u64,
    /// Maximum number of rate limiters to clean per run
    pub max_cleanup_per_run: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            global: GlobalRateLimits::default(),
            per_ip: PerIpRateLimits::default(),
            endpoints: EndpointRateLimits::default(),
            cleanup: RateLimitCleanupConfig::default(),
        }
    }
}

impl Default for GlobalRateLimits {
    fn default() -> Self {
        Self {
            requests_per_second: 1000, // 1000 RPS globally
            burst_capacity: 2000,      // Allow burst up to 2000 requests
            window_size_secs: 60,      // 1-minute window for rate calculations
        }
    }
}

impl Default for PerIpRateLimits {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_second: 50, // 50 RPS per IP
            burst_capacity: 100,     // Allow burst up to 100 requests per IP
            window_size_secs: 60,    // 1-minute window
            max_tracked_ips: 10000,  // Track up to 10,000 IP addresses
            ip_expiry_secs: 3600,    // Remove IP limiters after 1 hour of inactivity
        }
    }
}

impl Default for EndpointRateLimits {
    fn default() -> Self {
        Self {
            health: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 100, // Health checks are frequent
                burst_capacity: 200,
                window_size_secs: 60,
                endpoint_patterns: vec!["/health".to_string(), "/api/v1/health".to_string()],
            },
            read_operations: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 30, // Moderate limits for read operations
                burst_capacity: 60,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "/api/v1/cluster".to_string(),
                    "/api/v1/topics".to_string(),
                    "/api/v1/topics/*".to_string(), // GET operations
                    "/api/v1/brokers".to_string(),
                ],
            },
            write_operations: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 10, // Lower limits for write operations
                burst_capacity: 20,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "POST:/api/v1/topics".to_string(),
                    "DELETE:/api/v1/topics/*".to_string(),
                ],
            },
            cluster_operations: EndpointCategoryLimits {
                enabled: true,
                requests_per_second: 5, // Very restrictive for cluster operations
                burst_capacity: 10,
                window_size_secs: 60,
                endpoint_patterns: vec![
                    "/api/v1/cluster/rebalance".to_string(),
                    "/api/v1/cluster/scale".to_string(),
                    "/api/v1/brokers/*/decommission".to_string(),
                ],
            },
        }
    }
}

impl Default for RateLimitCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cleanup_interval_secs: 300, // Clean up every 5 minutes
            max_age_secs: 3600,         // Remove limiters after 1 hour of inactivity
            max_cleanup_per_run: 1000,  // Clean up to 1000 limiters per run
        }
    }
}

impl RateLimitConfig {
    /// Validate the rate limiting configuration
    pub fn validate(&self) -> crate::Result<()> {
        // Validate global limits
        self.global.validate()?;

        // Validate per-IP limits
        self.per_ip.validate()?;

        // Validate endpoint limits
        self.endpoints.validate()?;

        // Validate cleanup configuration
        self.cleanup.validate()?;

        Ok(())
    }
}

impl GlobalRateLimits {
    pub fn validate(&self) -> crate::Result<()> {
        if self.requests_per_second == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.requests_per_second must be greater than 0".to_string(),
            ));
        }

        if self.burst_capacity == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.burst_capacity must be greater than 0".to_string(),
            ));
        }

        if self.burst_capacity < self.requests_per_second {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.burst_capacity must be >= requests_per_second".to_string(),
            ));
        }

        if self.window_size_secs == 0 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.window_size_secs must be greater than 0".to_string(),
            ));
        }

        // Reasonable upper bounds
        if self.requests_per_second > 1_000_000 {
            return Err(crate::error::RustMqError::InvalidConfig(
                "rate_limiting.global.requests_per_second exceeds reasonable limit (1M RPS)"
                    .to_string(),
            ));
        }

        Ok(())
    }
}

impl PerIpRateLimits {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled {
            if self.requests_per_second == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.requests_per_second must be greater than 0".to_string(),
                ));
            }

            if self.burst_capacity == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.burst_capacity must be greater than 0".to_string(),
                ));
            }

            if self.burst_capacity < self.requests_per_second {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.burst_capacity must be >= requests_per_second"
                        .to_string(),
                ));
            }

            if self.window_size_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.window_size_secs must be greater than 0".to_string(),
                ));
            }

            if self.max_tracked_ips == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.max_tracked_ips must be greater than 0".to_string(),
                ));
            }

            if self.ip_expiry_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.ip_expiry_secs must be greater than 0".to_string(),
                ));
            }

            // Sanity check: don't track too many IPs (memory usage concern)
            if self.max_tracked_ips > 1_000_000 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.per_ip.max_tracked_ips exceeds reasonable limit (1M IPs)"
                        .to_string(),
                ));
            }
        }

        Ok(())
    }
}

impl EndpointRateLimits {
    pub fn validate(&self) -> crate::Result<()> {
        self.health.validate_category("health")?;
        self.read_operations.validate_category("read_operations")?;
        self.write_operations
            .validate_category("write_operations")?;
        self.cluster_operations
            .validate_category("cluster_operations")?;
        Ok(())
    }
}

impl EndpointCategoryLimits {
    pub fn validate_category(&self, category_name: &str) -> crate::Result<()> {
        if self.enabled {
            if self.requests_per_second == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "rate_limiting.endpoints.{}.requests_per_second must be greater than 0",
                    category_name
                )));
            }

            if self.burst_capacity == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "rate_limiting.endpoints.{}.burst_capacity must be greater than 0",
                    category_name
                )));
            }

            if self.burst_capacity < self.requests_per_second {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "rate_limiting.endpoints.{}.burst_capacity must be >= requests_per_second",
                    category_name
                )));
            }

            if self.window_size_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "rate_limiting.endpoints.{}.window_size_secs must be greater than 0",
                    category_name
                )));
            }

            if self.endpoint_patterns.is_empty() {
                return Err(crate::error::RustMqError::InvalidConfig(format!(
                    "rate_limiting.endpoints.{}.endpoint_patterns cannot be empty when enabled",
                    category_name
                )));
            }

            // Validate endpoint patterns are not empty strings
            for pattern in &self.endpoint_patterns {
                if pattern.trim().is_empty() {
                    return Err(crate::error::RustMqError::InvalidConfig(format!(
                        "rate_limiting.endpoints.{}.endpoint_patterns contains empty pattern",
                        category_name
                    )));
                }
            }
        }

        Ok(())
    }
}

impl RateLimitCleanupConfig {
    pub fn validate(&self) -> crate::Result<()> {
        if self.enabled {
            if self.cleanup_interval_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.cleanup_interval_secs must be greater than 0"
                        .to_string(),
                ));
            }

            if self.max_age_secs == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.max_age_secs must be greater than 0".to_string(),
                ));
            }

            if self.max_cleanup_per_run == 0 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.max_cleanup_per_run must be greater than 0".to_string(),
                ));
            }

            // Reasonable bounds
            if self.cleanup_interval_secs < 10 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.cleanup_interval_secs should be at least 10 seconds"
                        .to_string(),
                ));
            }

            if self.max_cleanup_per_run > 100_000 {
                return Err(crate::error::RustMqError::InvalidConfig(
                    "rate_limiting.cleanup.max_cleanup_per_run exceeds reasonable limit (100K)"
                        .to_string(),
                ));
            }
        }

        Ok(())
    }
}
