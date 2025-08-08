//! Secure Connection Management for mTLS-enabled QUIC
//!
//! This module provides authenticated connection wrappers and security metadata
//! for RustMQ's mTLS-enabled QUIC server, integrating certificate validation,
//! principal extraction, and request-level authorization.

use crate::{
    error::RustMqError,
    security::{
        AuthContext, AuthorizationManager, Permission,
        SecurityMetrics,
    },
    security::auth::authorization::ConnectionAclCache,
    Result,
};
use quinn::Connection;
use rustls::Certificate;
use std::{
    net::SocketAddr,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime},
};

/// Authenticated connection wrapper that provides security context and authorization
/// for all requests made over a QUIC connection with mTLS authentication
#[derive(Clone)]
pub struct AuthenticatedConnection {
    /// Underlying Quinn QUIC connection
    connection: Connection,
    /// Authentication context with principal and certificate info
    auth_context: AuthContext,
    /// Security metadata about the TLS connection
    security_metadata: Arc<RwLock<ConnectionSecurityMetadata>>,
    /// Authorization manager for request-level permission checks
    authz_manager: Arc<AuthorizationManager>,
    /// Per-connection ACL cache for performance
    acl_cache: Arc<ConnectionAclCache>,
    /// Security metrics collection
    metrics: Arc<SecurityMetrics>,
}

impl AuthenticatedConnection {
    /// Create a new authenticated connection after successful mTLS handshake
    pub async fn new(
        connection: Connection,
        auth_context: AuthContext,
        client_certificates: Vec<Certificate>,
        server_cert_fingerprint: String,
        authz_manager: Arc<AuthorizationManager>,
        metrics: Arc<SecurityMetrics>,
    ) -> Result<Self> {
        // TODO: Extract actual TLS connection details when API is available
        let security_metadata = Arc::new(RwLock::new(ConnectionSecurityMetadata {
            tls_version: "TLSv1.3".to_string(), // TODO: Get actual version
            cipher_suite: "TLS_AES_256_GCM_SHA384".to_string(), // TODO: Get actual cipher
            client_certificate_chain: client_certificates,
            server_certificate_fingerprint: server_cert_fingerprint,
            connection_established_at: SystemTime::now(),
            last_activity: SystemTime::now(),
        }));

        // Record authentication success with zero latency (placeholder)
        metrics.record_authentication_success(Duration::from_nanos(0));

        Ok(Self {
            connection,
            auth_context,
            security_metadata,
            acl_cache: authz_manager.create_connection_cache(),
            authz_manager,
            metrics,
        })
    }

    /// Get the principal (identity) of the authenticated client
    pub fn principal(&self) -> &Arc<str> {
        &self.auth_context.principal
    }

    /// Get the client certificate fingerprint if available
    pub fn certificate_fingerprint(&self) -> Option<&String> {
        self.auth_context.certificate_fingerprint.as_ref()
    }

    /// Get the full authentication context
    pub fn auth_context(&self) -> &AuthContext {
        &self.auth_context
    }

    /// Get the underlying QUIC connection
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Get connection security metadata
    pub fn security_metadata(&self) -> ConnectionSecurityMetadata {
        self.security_metadata.read().unwrap().clone()
    }

    /// Update last activity timestamp
    pub fn update_activity(&self) {
        if let Ok(mut metadata) = self.security_metadata.write() {
            metadata.last_activity = SystemTime::now();
        }
    }

    /// Check if the authenticated principal has permission for a specific resource and operation
    pub async fn authorize_request(
        &self,
        resource: &str,
        operation: Permission,
    ) -> Result<bool> {
        self.update_activity();
        
        let start = std::time::Instant::now();
        
        let result = self.authz_manager
            .check_permission(&self.acl_cache, &self.auth_context.principal, resource, operation)
            .await;

        let duration = start.elapsed();
        
        match &result {
            Ok(authorized) => {
                if *authorized {
                    self.metrics.record_authorization_success(duration);
                } else {
                    self.metrics.record_authorization_failure(duration);
                    tracing::warn!(
                        principal = %self.auth_context.principal,
                        resource = resource,
                        operation = ?operation,
                        "Authorization denied for authenticated request"
                    );
                }
            }
            Err(e) => {
                self.metrics.record_authorization_failure(duration);
                tracing::error!(
                    principal = %self.auth_context.principal,
                    resource = resource,
                    operation = ?operation,
                    error = %e,
                    "Authorization check failed"
                );
            }
        }

        result
    }

    /// Get connection statistics including network and security metrics
    pub fn connection_stats(&self) -> AuthenticatedConnectionStats {
        let security_metadata = self.security_metadata();
        
        AuthenticatedConnectionStats {
            remote_address: self.connection.remote_address(),
            connection_age: security_metadata.connection_established_at.elapsed()
                .unwrap_or(Duration::from_secs(0)),
            last_activity_age: security_metadata.last_activity.elapsed()
                .unwrap_or(Duration::from_secs(0)),
            security_metadata,
        }
    }

    /// Get the remote address of the client
    pub fn remote_address(&self) -> SocketAddr {
        self.connection.remote_address()
    }

    /// Close the connection gracefully
    pub fn close(&self, error_code: u32, reason: &[u8]) {
        self.connection.close(error_code.into(), reason);
        
        tracing::info!(
            principal = %self.auth_context.principal,
            remote_addr = %self.remote_address(),
            error_code = error_code,
            reason = %String::from_utf8_lossy(reason),
            "Closing authenticated connection"
        );
    }

    /// Check if the connection is still valid and not closed
    pub fn is_connected(&self) -> bool {
        !self.connection.close_reason().is_some()
    }

    /// Get connection uptime
    pub fn uptime(&self) -> Duration {
        self.security_metadata()
            .connection_established_at
            .elapsed()
            .unwrap_or(Duration::from_secs(0))
    }
}

/// Security metadata tracked for each authenticated connection
#[derive(Debug, Clone)]
pub struct ConnectionSecurityMetadata {
    /// TLS protocol version used (e.g., "TLSv1.3")
    pub tls_version: String,
    /// Cipher suite negotiated for the connection
    pub cipher_suite: String,
    /// Client certificate chain presented during handshake
    pub client_certificate_chain: Vec<Certificate>,
    /// Fingerprint of the server certificate used
    pub server_certificate_fingerprint: String,
    /// Timestamp when the connection was established
    pub connection_established_at: SystemTime,
    /// Timestamp of the last activity on this connection
    pub last_activity: SystemTime,
}

impl ConnectionSecurityMetadata {
    /// Get the client certificate subject DN if available
    pub fn client_certificate_subject(&self) -> Option<String> {
        self.client_certificate_chain.first().and_then(|cert| {
            // Parse certificate to extract subject DN
            // This would require additional certificate parsing logic
            // For now, return a placeholder
            Some("CN=client".to_string())
        })
    }

    /// Check if the connection is using a secure TLS version
    pub fn is_secure_tls_version(&self) -> bool {
        self.tls_version.contains("TLSv1.3") || self.tls_version.contains("TLSv1.2")
    }

    /// Get connection age
    pub fn connection_age(&self) -> Duration {
        self.connection_established_at.elapsed().unwrap_or(Duration::from_secs(0))
    }

    /// Get time since last activity
    pub fn idle_time(&self) -> Duration {
        self.last_activity.elapsed().unwrap_or(Duration::from_secs(0))
    }
}

/// Combined statistics for authenticated connections
#[derive(Debug)]
pub struct AuthenticatedConnectionStats {
    /// Remote address of the client
    pub remote_address: SocketAddr,
    /// Security metadata and authentication information
    pub security_metadata: ConnectionSecurityMetadata,
    /// Total connection uptime
    pub connection_age: Duration,
    /// Time since last activity
    pub last_activity_age: Duration,
}

impl AuthenticatedConnectionStats {
    /// Check if the connection appears healthy
    pub fn is_healthy(&self) -> bool {
        // Connection is healthy if:
        // 1. It's using a secure TLS version
        // 2. It's not idle for too long (< 5 minutes)
        self.security_metadata.is_secure_tls_version()
            && self.last_activity_age < Duration::from_secs(300)
    }

    /// Get a summary string of the connection status
    pub fn summary(&self) -> String {
        format!(
            "Connection(principal={}, age={:?}, idle={:?}, tls={}, healthy={})",
            "redacted", // Don't log principals in summaries
            self.connection_age,
            self.last_activity_age,
            self.security_metadata.tls_version,
            self.is_healthy()
        )
    }
}

/// Pool for managing authenticated connections with security context
pub struct AuthenticatedConnectionPool {
    connections: Arc<RwLock<std::collections::HashMap<String, AuthenticatedConnection>>>,
    max_connections: usize,
    max_idle_time: Duration,
    metrics: Arc<SecurityMetrics>,
}

impl AuthenticatedConnectionPool {
    /// Create a new authenticated connection pool
    pub fn new(max_connections: usize, max_idle_time: Duration, metrics: Arc<SecurityMetrics>) -> Self {
        Self {
            connections: Arc::new(RwLock::new(std::collections::HashMap::new())),
            max_connections,
            max_idle_time,
            metrics,
        }
    }

    /// Add an authenticated connection to the pool
    pub fn add_connection(&self, connection_id: String, connection: AuthenticatedConnection) -> Result<()> {
        let mut connections = self.connections.write()
            .map_err(|e| RustMqError::Network(format!("Connection pool lock error: {}", e)))?;
        
        // Enforce connection limit
        if connections.len() >= self.max_connections {
            // Remove oldest idle connection
            if let Some((oldest_id, _)) = connections
                .iter()
                .filter(|(_, conn)| {
                    conn.security_metadata()
                        .idle_time() > self.max_idle_time
                })
                .min_by_key(|(_, conn)| conn.security_metadata().last_activity)
                .map(|(id, conn)| (id.clone(), conn.principal().clone()))
            {
                connections.remove(&oldest_id);
                tracing::debug!(
                    connection_id = oldest_id,
                    "Evicted idle connection from authenticated pool"
                );
            } else if connections.len() >= self.max_connections {
                return Err(RustMqError::Network(
                    "Connection pool full and no idle connections to evict".to_string()
                ));
            }
        }

        connections.insert(connection_id.clone(), connection);
        
        tracing::debug!(
            connection_id = connection_id,
            pool_size = connections.len(),
            "Added authenticated connection to pool"
        );

        Ok(())
    }

    /// Get an authenticated connection from the pool
    pub fn get_connection(&self, connection_id: &str) -> Option<AuthenticatedConnection> {
        let connections = self.connections.read().ok()?;
        connections.get(connection_id).cloned()
    }

    /// Remove a connection from the pool
    pub fn remove_connection(&self, connection_id: &str) -> Option<AuthenticatedConnection> {
        let mut connections = self.connections.write().ok()?;
        let removed = connections.remove(connection_id);
        
        if removed.is_some() {
            tracing::debug!(
                connection_id = connection_id,
                pool_size = connections.len(),
                "Removed authenticated connection from pool"
            );
        }
        
        removed
    }

    /// Clean up idle connections that exceed the maximum idle time
    pub fn cleanup_idle_connections(&self) -> usize {
        let mut connections = match self.connections.write() {
            Ok(conns) => conns,
            Err(_) => return 0,
        };

        let now = SystemTime::now();
        let initial_count = connections.len();

        connections.retain(|connection_id, connection| {
            let metadata = connection.security_metadata();
            let idle_time = now.duration_since(metadata.last_activity)
                .unwrap_or(Duration::from_secs(0));
            
            if idle_time > self.max_idle_time {
                tracing::debug!(
                    connection_id = connection_id,
                    principal = %connection.principal(),
                    idle_time = ?idle_time,
                    "Cleaning up idle authenticated connection"
                );
                false
            } else {
                true
            }
        });

        let cleaned_count = initial_count - connections.len();
        if cleaned_count > 0 {
            tracing::info!(
                cleaned_connections = cleaned_count,
                remaining_connections = connections.len(),
                "Cleaned up idle authenticated connections"
            );
        }

        cleaned_count
    }

    /// Get connection pool statistics
    pub fn stats(&self) -> ConnectionPoolStats {
        let connections = match self.connections.read() {
            Ok(conns) => conns,
            Err(_) => return ConnectionPoolStats::default(),
        };

        let total_connections = connections.len();
        let healthy_connections = connections.values()
            .filter(|conn| conn.connection_stats().is_healthy())
            .count();

        ConnectionPoolStats {
            total_connections,
            healthy_connections,
            max_connections: self.max_connections,
            max_idle_time: self.max_idle_time,
        }
    }
}

/// Statistics for the authenticated connection pool
#[derive(Debug, Default)]
pub struct ConnectionPoolStats {
    pub total_connections: usize,
    pub healthy_connections: usize,
    pub max_connections: usize,
    pub max_idle_time: Duration,
}

impl ConnectionPoolStats {
    /// Check if the pool is operating within healthy parameters
    pub fn is_healthy(&self) -> bool {
        // Pool is healthy if:
        // 1. Not at capacity
        // 2. Most connections are healthy
        self.total_connections < self.max_connections
            && (self.total_connections == 0 || 
                (self.healthy_connections as f32 / self.total_connections as f32) > 0.8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{SecurityConfig, SecurityManager};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_connection_security_metadata() {
        let metadata = ConnectionSecurityMetadata {
            tls_version: "TLSv1.3".to_string(),
            cipher_suite: "TLS_AES_256_GCM_SHA384".to_string(),
            client_certificate_chain: vec![],
            server_certificate_fingerprint: "test-fingerprint".to_string(),
            connection_established_at: SystemTime::now(),
            last_activity: SystemTime::now(),
        };

        assert!(metadata.is_secure_tls_version());
        assert!(metadata.connection_age() < Duration::from_secs(1));
        assert!(metadata.idle_time() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn test_connection_pool_basic_operations() {
        let config = SecurityConfig {
            tls: Default::default(),
            acl: Default::default(),
            certificate_management: Default::default(),
            audit: Default::default(),
        };
        
        let security_manager = SecurityManager::new(config).await.unwrap();
        let metrics = security_manager.metrics().clone();
        
        let pool = AuthenticatedConnectionPool::new(
            2,
            Duration::from_secs(60),
            metrics,
        );

        let stats = pool.stats();
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.max_connections, 2);
        assert!(stats.is_healthy());
    }

    #[tokio::test]
    async fn test_connection_pool_cleanup() {
        let config = SecurityConfig {
            tls: Default::default(),
            acl: Default::default(),
            certificate_management: Default::default(),
            audit: Default::default(),
        };
        
        let security_manager = SecurityManager::new(config).await.unwrap();
        let metrics = security_manager.metrics().clone();
        
        let pool = AuthenticatedConnectionPool::new(
            10,
            Duration::from_millis(1), // Very short idle time for testing
            metrics,
        );

        // Test cleanup with no connections
        let cleaned = pool.cleanup_idle_connections();
        assert_eq!(cleaned, 0);

        // Pool should remain healthy
        assert!(pool.stats().is_healthy());
    }
}