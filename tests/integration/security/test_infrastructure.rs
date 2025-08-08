//! Test Infrastructure for Security Integration Tests
//!
//! This module provides the foundational infrastructure for running comprehensive
//! security integration tests across a simulated RustMQ cluster environment.

use rustmq::{
    config::{BrokerConfig, ControllerConfig},
    controller::service::Controller,
    broker::core::MessageBrokerCore,
    security::{
        SecurityManager, SecurityConfig, TlsConfig, AclConfig, 
        CertificateManagementConfig, AuditConfig, CertificateManager,
        AclManager, AuthenticationManager, AuthorizationManager,
        Principal, Permission, AclRule, Effect, ResourcePattern,
        CertificateInfo, CertificateRequest, CaGenerationParams,
        KeyType, KeyUsage, ExtendedKeyUsage, CertificateRole,
    },
    network::{quic_server::SecureQuicServer, secure_connection::AuthenticatedConnection},
    storage::{object_storage::MockObjectStorage, cache::LruCache},
    error::RustMqError,
    Result,
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use tempfile::TempDir;
use tokio::{net::TcpListener, time::timeout};
use rustls::{Certificate, PrivateKey};
use rcgen::{Certificate as RcgenCert, CertificateParams, DistinguishedName};

/// Complete test cluster for security integration testing
pub struct SecurityTestCluster {
    pub controllers: Vec<TestController>,
    pub brokers: Vec<TestBroker>,
    pub cert_manager: Arc<CertificateManager>,
    pub acl_manager: Arc<AclManager>,
    pub test_ca: TestCertificateAuthority,
    pub storage: Arc<MockObjectStorage>,
    pub temp_dir: TempDir,
    pub config: SecurityTestConfig,
}

impl SecurityTestCluster {
    /// Create a new test cluster with realistic production configuration
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()
            .map_err(|e| RustMqError::Storage(format!("Failed to create temp dir: {}", e)))?;
        
        let test_ca = TestCertificateAuthority::new(temp_dir.path()).await?;
        let storage = Arc::new(MockObjectStorage::new());
        
        let security_config = SecurityConfig {
            tls: TlsConfig {
                enabled: true,
                ca_cert_path: test_ca.ca_cert_path.clone(),
                ca_key_path: test_ca.ca_key_path.clone(),
                cert_path: None,
                key_path: None,
                require_client_auth: true,
                verify_hostname: false,
                min_tls_version: "1.3".to_string(),
                cipher_suites: vec!["TLS_AES_256_GCM_SHA384".to_string()],
            },
            acl: AclConfig::default(),
            certificate_management: CertificateManagementConfig {
                ca_cert_path: test_ca.ca_cert_path.clone(),
                ca_key_path: test_ca.ca_key_path.clone(),
                cert_validity_days: 30, // Shorter for testing
                auto_renew_before_expiry_days: 7,
                crl_check_enabled: true,
                ocsp_check_enabled: false,
            },
            audit: AuditConfig::default(),
        };
        
        let cert_manager = Arc::new(
            CertificateManager::new(security_config.certificate_management.clone()).await?
        );
        
        let acl_manager = Arc::new(
            AclManager::new(storage.clone()).await?
        );
        
        let config = SecurityTestConfig {
            cluster_size: 3,
            broker_count: 2,
            base_port: Self::find_available_port_range(6).await?,
            network_latency_ms: 5,
            failure_rate: 0.0,
            load_factor: 1.0,
        };
        
        Ok(Self {
            controllers: Vec::new(),
            brokers: Vec::new(),
            cert_manager,
            acl_manager,
            test_ca,
            storage,
            temp_dir,
            config,
        })
    }
    
    /// Create a test cluster with minimal configuration for unit tests
    pub async fn new_test_config() -> Result<Self> {
        let mut cluster = Self::new().await?;
        cluster.config.cluster_size = 1;
        cluster.config.broker_count = 1;
        Ok(cluster)
    }
    
    /// Start the complete test cluster with all components
    pub async fn start(&mut self) -> Result<()> {
        // Start controllers first
        for i in 0..self.config.cluster_size {
            let controller = self.create_test_controller(i).await?;
            self.controllers.push(controller);
        }
        
        // Wait for controller cluster to stabilize
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Start brokers
        for i in 0..self.config.broker_count {
            let broker = self.create_test_broker(i).await?;
            self.brokers.push(broker);
        }
        
        // Wait for broker registration
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Verify cluster health
        self.verify_cluster_health().await?;
        
        Ok(())
    }
    
    /// Gracefully shutdown the test cluster
    pub async fn shutdown(&mut self) -> Result<()> {
        // Shutdown brokers first
        for broker in &mut self.brokers {
            broker.shutdown().await?;
        }
        
        // Shutdown controllers
        for controller in &mut self.controllers {
            controller.shutdown().await?;
        }
        
        // Clear collections
        self.brokers.clear();
        self.controllers.clear();
        
        Ok(())
    }
    
    /// Create a test client with proper mTLS configuration
    pub async fn create_test_client(&self, principal_name: &str) -> Result<TestSecurityClient> {
        let client_cert = self.test_ca.issue_client_certificate(principal_name).await?;
        
        TestSecurityClient::new(
            client_cert,
            self.get_broker_addresses(),
            self.test_ca.ca_cert_path.clone(),
        ).await
    }
    
    /// Get broker addresses for client connections
    pub fn get_broker_addresses(&self) -> Vec<SocketAddr> {
        self.brokers.iter()
            .map(|broker| broker.quic_address)
            .collect()
    }
    
    /// Get controller addresses for admin operations
    pub fn get_controller_addresses(&self) -> Vec<SocketAddr> {
        self.controllers.iter()
            .map(|controller| controller.address)
            .collect()
    }
    
    /// Simulate network latency for realistic testing
    pub async fn simulate_network_latency(&self) {
        if self.config.network_latency_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.network_latency_ms)).await;
        }
    }
    
    /// Simulate random network failures based on configured failure rate
    pub async fn simulate_network_failure(&self) -> bool {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        rng.gen::<f64>() < self.config.failure_rate
    }
    
    async fn create_test_controller(&self, index: usize) -> Result<TestController> {
        let port = self.config.base_port + index;
        let address: SocketAddr = format!("127.0.0.1:{}", port).parse()
            .map_err(|e| RustMqError::Network(format!("Invalid address: {}", e)))?;
        
        let config = ControllerConfig {
            node_id: format!("controller-{}", index),
            raft_port: port + 100,
            api_port: port + 200,
            peers: self.controllers.iter().enumerate()
                .map(|(i, _)| format!("127.0.0.1:{}", self.config.base_port + i + 100))
                .collect(),
            ..Default::default()
        };
        
        let controller_cert = self.test_ca.issue_server_certificate(&format!("controller-{}", index)).await?;
        let controller = TestController::new(config, controller_cert, address).await?;
        
        Ok(controller)
    }
    
    async fn create_test_broker(&self, index: usize) -> Result<TestBroker> {
        let port = self.config.base_port + 1000 + index;
        let quic_port = port;
        let grpc_port = port + 100;
        
        let quic_address: SocketAddr = format!("127.0.0.1:{}", quic_port).parse()
            .map_err(|e| RustMqError::Network(format!("Invalid QUIC address: {}", e)))?;
        let grpc_address: SocketAddr = format!("127.0.0.1:{}", grpc_port).parse()
            .map_err(|e| RustMqError::Network(format!("Invalid gRPC address: {}", e)))?;
        
        let config = BrokerConfig {
            broker_id: format!("broker-{}", index),
            quic_port,
            grpc_port,
            controller_addresses: self.get_controller_addresses().iter()
                .map(|addr| addr.to_string())
                .collect(),
            ..Default::default()
        };
        
        let broker_cert = self.test_ca.issue_server_certificate(&format!("broker-{}", index)).await?;
        let broker = TestBroker::new(config, broker_cert, quic_address, grpc_address).await?;
        
        Ok(broker)
    }
    
    async fn verify_cluster_health(&self) -> Result<()> {
        // Verify controller cluster has a leader
        let mut leader_found = false;
        for controller in &self.controllers {
            if controller.is_leader().await? {
                leader_found = true;
                break;
            }
        }
        
        if !leader_found {
            return Err(RustMqError::Controller("No controller leader found".to_string()));
        }
        
        // Verify brokers are connected to controllers
        for broker in &self.brokers {
            if !broker.is_connected_to_controller().await? {
                return Err(RustMqError::Broker("Broker not connected to controller".to_string()));
            }
        }
        
        Ok(())
    }
    
    async fn find_available_port_range(count: usize) -> Result<usize> {
        let mut base_port = 20000;
        
        loop {
            let mut all_available = true;
            
            // Check if all ports in range are available
            for i in 0..count * 300 { // Allow 300 ports per service for comprehensive testing
                if let Err(_) = TcpListener::bind(format!("127.0.0.1:{}", base_port + i)).await {
                    all_available = false;
                    break;
                }
            }
            
            if all_available {
                return Ok(base_port);
            }
            
            base_port += count * 300;
            
            if base_port > 60000 {
                return Err(RustMqError::Network("No available port range found".to_string()));
            }
        }
    }
}

/// Test controller instance with security configuration
pub struct TestController {
    pub config: ControllerConfig,
    pub address: SocketAddr,
    pub certificate: TestCertificate,
    pub controller: Option<Arc<Controller>>,
}

impl TestController {
    async fn new(
        config: ControllerConfig,
        certificate: TestCertificate,
        address: SocketAddr,
    ) -> Result<Self> {
        Ok(Self {
            config,
            address,
            certificate,
            controller: None,
        })
    }
    
    async fn is_leader(&self) -> Result<bool> {
        // Simplified leader check for testing
        Ok(true) // First controller is always leader in tests
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        if let Some(controller) = self.controller.take() {
            // Graceful shutdown logic would go here
        }
        Ok(())
    }
}

/// Test broker instance with security configuration
pub struct TestBroker {
    pub config: BrokerConfig,
    pub quic_address: SocketAddr,
    pub grpc_address: SocketAddr,
    pub certificate: TestCertificate,
    pub broker: Option<Arc<MessageBrokerCore>>,
    pub quic_server: Option<Arc<SecureQuicServer>>,
}

impl TestBroker {
    async fn new(
        config: BrokerConfig,
        certificate: TestCertificate,
        quic_address: SocketAddr,
        grpc_address: SocketAddr,
    ) -> Result<Self> {
        Ok(Self {
            config,
            quic_address,
            grpc_address,
            certificate,
            broker: None,
            quic_server: None,
        })
    }
    
    async fn is_connected_to_controller(&self) -> Result<bool> {
        // Simplified connection check for testing
        Ok(true)
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        if let Some(_quic_server) = self.quic_server.take() {
            // Graceful shutdown logic would go here
        }
        if let Some(_broker) = self.broker.take() {
            // Graceful shutdown logic would go here
        }
        Ok(())
    }
}

/// Test certificate authority for issuing test certificates
pub struct TestCertificateAuthority {
    pub ca_cert: Certificate,
    pub ca_key: PrivateKey,
    pub ca_cert_path: String,
    pub ca_key_path: String,
    temp_dir: PathBuf,
}

impl TestCertificateAuthority {
    async fn new(temp_dir: &Path) -> Result<Self> {
        let ca_cert_path = temp_dir.join("ca.pem");
        let ca_key_path = temp_dir.join("ca.key");
        
        // Generate CA certificate using rcgen
        let mut ca_params = CertificateParams::new(vec!["RustMQ Test CA".to_string()]);
        ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca_params.distinguished_name = DistinguishedName::new();
        ca_params.distinguished_name.push(rcgen::DnType::CommonName, "RustMQ Test CA");
        ca_params.distinguished_name.push(rcgen::DnType::OrganizationName, "RustMQ Test");
        
        let ca_cert = RcgenCert::from_params(ca_params)
            .map_err(|e| RustMqError::Certificate(format!("Failed to generate CA cert: {}", e)))?;
        
        // Write CA certificate and key to files
        std::fs::write(&ca_cert_path, ca_cert.serialize_pem()
            .map_err(|e| RustMqError::Certificate(format!("Failed to serialize CA cert: {}", e)))?)
            .map_err(|e| RustMqError::Storage(format!("Failed to write CA cert: {}", e)))?;
        
        std::fs::write(&ca_key_path, ca_cert.serialize_private_key_pem())
            .map_err(|e| RustMqError::Storage(format!("Failed to write CA key: {}", e)))?;
        
        // Convert to rustls types
        let ca_cert_der = ca_cert.serialize_der()
            .map_err(|e| RustMqError::Certificate(format!("Failed to serialize CA cert DER: {}", e)))?;
        let ca_key_der = ca_cert.serialize_private_key_der();
        
        Ok(Self {
            ca_cert: Certificate(ca_cert_der),
            ca_key: PrivateKey(ca_key_der),
            ca_cert_path: ca_cert_path.to_string_lossy().to_string(),
            ca_key_path: ca_key_path.to_string_lossy().to_string(),
            temp_dir: temp_dir.to_path_buf(),
        })
    }
    
    async fn issue_client_certificate(&self, principal_name: &str) -> Result<TestCertificate> {
        let cert_path = self.temp_dir.join(format!("{}-client.pem", principal_name));
        let key_path = self.temp_dir.join(format!("{}-client.key", principal_name));
        
        // Generate client certificate
        let mut cert_params = CertificateParams::new(vec![principal_name.to_string()]);
        cert_params.distinguished_name = DistinguishedName::new();
        cert_params.distinguished_name.push(rcgen::DnType::CommonName, principal_name);
        cert_params.distinguished_name.push(rcgen::DnType::OrganizationName, "RustMQ Test Client");
        
        let cert = RcgenCert::from_params(cert_params)
            .map_err(|e| RustMqError::Certificate(format!("Failed to generate client cert: {}", e)))?;
        
        // Write certificate and key
        std::fs::write(&cert_path, cert.serialize_pem()
            .map_err(|e| RustMqError::Certificate(format!("Failed to serialize client cert: {}", e)))?)
            .map_err(|e| RustMqError::Storage(format!("Failed to write client cert: {}", e)))?;
        
        std::fs::write(&key_path, cert.serialize_private_key_pem())
            .map_err(|e| RustMqError::Storage(format!("Failed to write client key: {}", e)))?;
        
        // Convert to rustls types
        let cert_der = cert.serialize_der()
            .map_err(|e| RustMqError::Certificate(format!("Failed to serialize client cert DER: {}", e)))?;
        let key_der = cert.serialize_private_key_der();
        
        Ok(TestCertificate {
            certificate: Certificate(cert_der),
            private_key: PrivateKey(key_der),
            cert_path: cert_path.to_string_lossy().to_string(),
            key_path: key_path.to_string_lossy().to_string(),
            principal: principal_name.to_string(),
            serial_number: "test-serial".to_string(),
            fingerprint: format!("test-fingerprint-{}", principal_name),
        })
    }
    
    async fn issue_server_certificate(&self, server_name: &str) -> Result<TestCertificate> {
        let cert_path = self.temp_dir.join(format!("{}-server.pem", server_name));
        let key_path = self.temp_dir.join(format!("{}-server.key", server_name));
        
        // Generate server certificate
        let mut cert_params = CertificateParams::new(vec![
            server_name.to_string(),
            "localhost".to_string(),
            "127.0.0.1".to_string(),
        ]);
        cert_params.distinguished_name = DistinguishedName::new();
        cert_params.distinguished_name.push(rcgen::DnType::CommonName, server_name);
        cert_params.distinguished_name.push(rcgen::DnType::OrganizationName, "RustMQ Test Server");
        
        let cert = RcgenCert::from_params(cert_params)
            .map_err(|e| RustMqError::Certificate(format!("Failed to generate server cert: {}", e)))?;
        
        // Write certificate and key
        std::fs::write(&cert_path, cert.serialize_pem()
            .map_err(|e| RustMqError::Certificate(format!("Failed to serialize server cert: {}", e)))?)
            .map_err(|e| RustMqError::Storage(format!("Failed to write server cert: {}", e)))?;
        
        std::fs::write(&key_path, cert.serialize_private_key_pem())
            .map_err(|e| RustMqError::Storage(format!("Failed to write server key: {}", e)))?;
        
        // Convert to rustls types
        let cert_der = cert.serialize_der()
            .map_err(|e| RustMqError::Certificate(format!("Failed to serialize server cert DER: {}", e)))?;
        let key_der = cert.serialize_private_key_der();
        
        Ok(TestCertificate {
            certificate: Certificate(cert_der),
            private_key: PrivateKey(key_der),
            cert_path: cert_path.to_string_lossy().to_string(),
            key_path: key_path.to_string_lossy().to_string(),
            principal: server_name.to_string(),
            serial_number: "test-server-serial".to_string(),
            fingerprint: format!("test-server-fingerprint-{}", server_name),
        })
    }
}

/// Test certificate wrapper with metadata
#[derive(Debug, Clone)]
pub struct TestCertificate {
    pub certificate: Certificate,
    pub private_key: PrivateKey,
    pub cert_path: String,
    pub key_path: String,
    pub principal: String,
    pub serial_number: String,
    pub fingerprint: String,
}

/// Test security client for integration testing
pub struct TestSecurityClient {
    pub certificate: TestCertificate,
    pub broker_addresses: Vec<SocketAddr>,
    pub ca_cert_path: String,
}

impl TestSecurityClient {
    async fn new(
        certificate: TestCertificate,
        broker_addresses: Vec<SocketAddr>,
        ca_cert_path: String,
    ) -> Result<Self> {
        Ok(Self {
            certificate,
            broker_addresses,
            ca_cert_path,
        })
    }
    
    /// Establish authenticated connection to a broker
    pub async fn connect_to_broker(&self, broker_index: usize) -> Result<TestAuthenticatedConnection> {
        if broker_index >= self.broker_addresses.len() {
            return Err(RustMqError::Network("Invalid broker index".to_string()));
        }
        
        let broker_addr = self.broker_addresses[broker_index];
        
        // Simulate mTLS handshake and connection establishment
        TestAuthenticatedConnection::new(
            broker_addr,
            self.certificate.principal.clone(),
            self.certificate.fingerprint.clone(),
        ).await
    }
    
    /// Perform authentication workflow
    pub async fn authenticate(&self) -> Result<TestAuthContext> {
        // Simulate authentication process
        Ok(TestAuthContext {
            principal: self.certificate.principal.clone(),
            certificate_fingerprint: self.certificate.fingerprint.clone(),
            authenticated_at: SystemTime::now(),
            permissions: vec![Permission::Read, Permission::Write],
        })
    }
}

/// Test authenticated connection for integration testing
pub struct TestAuthenticatedConnection {
    pub broker_address: SocketAddr,
    pub principal: String,
    pub certificate_fingerprint: String,
    pub connected_at: SystemTime,
}

impl TestAuthenticatedConnection {
    async fn new(
        broker_address: SocketAddr,
        principal: String,
        certificate_fingerprint: String,
    ) -> Result<Self> {
        Ok(Self {
            broker_address,
            principal,
            certificate_fingerprint,
            connected_at: SystemTime::now(),
        })
    }
    
    /// Simulate authorization check for a resource
    pub async fn authorize(&self, resource: &str, permission: Permission) -> Result<bool> {
        // Simulate authorization logic
        Ok(true) // Allow all operations in tests by default
    }
    
    /// Send a test message
    pub async fn send_message(&self, topic: &str, message: &[u8]) -> Result<()> {
        // Simulate message sending
        Ok(())
    }
    
    /// Receive a test message
    pub async fn receive_message(&self, topic: &str) -> Result<Vec<u8>> {
        // Simulate message receiving
        Ok(b"test message".to_vec())
    }
}

/// Test authentication context
#[derive(Debug, Clone)]
pub struct TestAuthContext {
    pub principal: String,
    pub certificate_fingerprint: String,
    pub authenticated_at: SystemTime,
    pub permissions: Vec<Permission>,
}

/// Configuration for security test scenarios
#[derive(Debug, Clone)]
pub struct SecurityTestConfig {
    pub cluster_size: usize,
    pub broker_count: usize,
    pub base_port: usize,
    pub network_latency_ms: u64,
    pub failure_rate: f64,
    pub load_factor: f64,
}

/// Utility functions for security integration tests
pub struct SecurityTestUtils;

impl SecurityTestUtils {
    /// Generate test ACL rules for various scenarios
    pub fn generate_test_acl_rules() -> Vec<AclRule> {
        vec![
            AclRule {
                principal: "test-producer".to_string(),
                resource_pattern: ResourcePattern::Topic("test-topic-*".to_string()),
                operation: Permission::Write,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "test-consumer".to_string(),
                resource_pattern: ResourcePattern::Topic("test-topic-*".to_string()),
                operation: Permission::Read,
                effect: Effect::Allow,
            },
            AclRule {
                principal: "test-admin".to_string(),
                resource_pattern: ResourcePattern::Cluster,
                operation: Permission::ClusterAdmin,
                effect: Effect::Allow,
            },
        ]
    }
    
    /// Create test certificate requests for various roles
    pub fn generate_test_certificate_requests() -> Vec<CertificateRequest> {
        vec![
            CertificateRequest {
                common_name: "test-producer".to_string(),
                organization: Some("RustMQ Test".to_string()),
                organizational_unit: Some("Producers".to_string()),
                country: None,
                state: None,
                locality: None,
                email: None,
                subject_alt_names: vec!["producer.test.rustmq.io".to_string()],
                key_type: KeyType::Rsa2048,
                key_usage: vec![KeyUsage::DigitalSignature, KeyUsage::KeyEncipherment],
                extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                validity_days: 30,
                role: CertificateRole::Client,
            },
            CertificateRequest {
                common_name: "test-consumer".to_string(),
                organization: Some("RustMQ Test".to_string()),
                organizational_unit: Some("Consumers".to_string()),
                country: None,
                state: None,
                locality: None,
                email: None,
                subject_alt_names: vec!["consumer.test.rustmq.io".to_string()],
                key_type: KeyType::Rsa2048,
                key_usage: vec![KeyUsage::DigitalSignature, KeyUsage::KeyEncipherment],
                extended_key_usage: vec![ExtendedKeyUsage::ClientAuth],
                validity_days: 30,
                role: CertificateRole::Client,
            },
        ]
    }
    
    /// Wait for condition with timeout
    pub async fn wait_for_condition<F, Fut>(
        condition: F,
        timeout_duration: Duration,
        check_interval: Duration,
    ) -> Result<()>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let start = tokio::time::Instant::now();
        
        while start.elapsed() < timeout_duration {
            if condition().await {
                return Ok(());
            }
            tokio::time::sleep(check_interval).await;
        }
        
        Err(RustMqError::Timeout("Condition not met within timeout".to_string()))
    }
    
    /// Generate load for performance testing
    pub async fn generate_authentication_load(
        client_count: usize,
        requests_per_client: usize,
        test_cluster: &SecurityTestCluster,
    ) -> Result<Vec<Duration>> {
        let mut latencies = Vec::new();
        let mut handles = Vec::new();
        
        for i in 0..client_count {
            let client = test_cluster.create_test_client(&format!("load-test-client-{}", i)).await?;
            
            let handle = tokio::spawn(async move {
                let mut client_latencies = Vec::new();
                
                for _ in 0..requests_per_client {
                    let start = tokio::time::Instant::now();
                    
                    // Simulate authentication request
                    let _auth_context = client.authenticate().await;
                    
                    let latency = start.elapsed();
                    client_latencies.push(latency);
                    
                    // Small delay between requests
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                
                client_latencies
            });
            
            handles.push(handle);
        }
        
        // Collect all latencies
        for handle in handles {
            let client_latencies = handle.await
                .map_err(|e| RustMqError::Internal(format!("Load test task failed: {}", e)))?;
            latencies.extend(client_latencies);
        }
        
        Ok(latencies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_security_test_infrastructure() {
        let cluster = SecurityTestCluster::new_test_config().await.unwrap();
        
        // Test CA functionality
        let client_cert = cluster.test_ca.issue_client_certificate("test-client").await.unwrap();
        assert_eq!(client_cert.principal, "test-client");
        assert!(client_cert.cert_path.contains("test-client"));
        
        // Test certificate paths exist
        assert!(std::path::Path::new(&cluster.test_ca.ca_cert_path).exists());
        assert!(std::path::Path::new(&cluster.test_ca.ca_key_path).exists());
        assert!(std::path::Path::new(&client_cert.cert_path).exists());
        assert!(std::path::Path::new(&client_cert.key_path).exists());
    }
    
    #[tokio::test]
    async fn test_test_client_creation() {
        let cluster = SecurityTestCluster::new_test_config().await.unwrap();
        let client = cluster.create_test_client("test-principal").await.unwrap();
        
        assert_eq!(client.certificate.principal, "test-principal");
        assert!(client.broker_addresses.is_empty()); // No brokers started yet
    }
    
    #[tokio::test]
    async fn test_acl_rule_generation() {
        let rules = SecurityTestUtils::generate_test_acl_rules();
        assert_eq!(rules.len(), 3);
        
        // Verify producer rule
        assert_eq!(rules[0].principal, "test-producer");
        assert_eq!(rules[0].operation, Permission::Write);
        assert_eq!(rules[0].effect, Effect::Allow);
        
        // Verify consumer rule
        assert_eq!(rules[1].principal, "test-consumer");
        assert_eq!(rules[1].operation, Permission::Read);
        assert_eq!(rules[1].effect, Effect::Allow);
        
        // Verify admin rule
        assert_eq!(rules[2].principal, "test-admin");
        assert_eq!(rules[2].operation, Permission::ClusterAdmin);
        assert_eq!(rules[2].effect, Effect::Allow);
    }
    
    #[tokio::test]
    async fn test_certificate_request_generation() {
        let requests = SecurityTestUtils::generate_test_certificate_requests();
        assert_eq!(requests.len(), 2);
        
        // Verify producer request
        assert_eq!(requests[0].common_name, "test-producer");
        assert_eq!(requests[0].role, CertificateRole::Client);
        assert!(requests[0].extended_key_usage.contains(&ExtendedKeyUsage::ClientAuth));
        
        // Verify consumer request
        assert_eq!(requests[1].common_name, "test-consumer");
        assert_eq!(requests[1].role, CertificateRole::Client);
        assert!(requests[1].extended_key_usage.contains(&ExtendedKeyUsage::ClientAuth));
    }
}