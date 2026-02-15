//! RustMQ Admin API Client
//!
//! Provides HTTP REST API client for RustMQ cluster management, security operations,
//! and administrative tasks.
//!
//! # Examples
//!
//! ```no_run
//! use rustmq_client::admin::{AdminClient, AdminConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create admin client
//!     let config = AdminConfig::new("http://localhost:8080");
//!     let client = AdminClient::new(config)?;
//!
//!     // Check cluster health
//!     let health = client.health().await?;
//!     println!("Cluster status: {}", health.status);
//!
//!     // List topics
//!     let topics = client.list_topics().await?;
//!     println!("Topics: {:?}", topics);
//!
//!     Ok(())
//! }
//! ```

use crate::error::{ClientError, Result};
use chrono::{DateTime, Utc};
use reqwest::{Client as HttpClient, ClientBuilder, RequestBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use url::Url;

// ==================== Configuration ====================

/// Configuration for the Admin API client
#[derive(Debug, Clone)]
pub struct AdminConfig {
    /// Base URL for the Admin API (e.g., "http://localhost:8080")
    pub base_url: String,

    /// Request timeout duration
    pub timeout: Duration,

    /// Enable TLS certificate validation
    pub tls_verify: bool,

    /// Optional TLS client certificate for mTLS authentication
    pub client_cert: Option<Vec<u8>>,

    /// Optional TLS client private key for mTLS authentication
    pub client_key: Option<Vec<u8>>,

    /// Optional CA certificate for custom trust chain
    pub ca_cert: Option<Vec<u8>>,

    /// Optional authentication token for API requests
    pub auth_token: Option<String>,

    /// Maximum number of retries for failed requests
    pub max_retries: u32,

    /// Enable connection pooling
    pub pool_enabled: bool,

    /// Maximum idle connections per host
    pub pool_max_idle_per_host: usize,
}

impl AdminConfig {
    /// Create a new admin configuration with the specified base URL
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            timeout: Duration::from_secs(30),
            tls_verify: true,
            client_cert: None,
            client_key: None,
            ca_cert: None,
            auth_token: None,
            max_retries: 3,
            pool_enabled: true,
            pool_max_idle_per_host: 10,
        }
    }

    /// Set request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Disable TLS certificate verification (for testing only)
    pub fn with_tls_verify(mut self, verify: bool) -> Self {
        self.tls_verify = verify;
        self
    }

    /// Set mTLS client certificate and key
    pub fn with_client_cert(mut self, cert: Vec<u8>, key: Vec<u8>) -> Self {
        self.client_cert = Some(cert);
        self.client_key = Some(key);
        self
    }

    /// Set CA certificate for custom trust chain
    pub fn with_ca_cert(mut self, ca_cert: Vec<u8>) -> Self {
        self.ca_cert = Some(ca_cert);
        self
    }

    /// Set authentication token
    pub fn with_auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Set maximum number of retries
    pub fn with_max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self::new("http://localhost:8080")
    }
}

// ==================== Request/Response Types ====================

/// Generic API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub leader_hint: Option<String>,
}

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub uptime_seconds: u64,
    pub is_leader: bool,
    pub raft_term: u64,
}

/// Broker status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerStatus {
    pub id: String,
    pub host: String,
    pub port_quic: u16,
    pub port_rpc: u16,
    pub rack_id: String,
    pub online: bool,
}

/// Cluster status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterStatus {
    pub brokers: Vec<BrokerStatus>,
    pub topics: Vec<TopicSummary>,
    pub leader: Option<String>,
    pub term: u64,
    pub healthy: bool,
}

/// Topic summary information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicSummary {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub created_at: DateTime<Utc>,
}

/// Create topic request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTopicRequest {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub retention_ms: Option<u64>,
    pub segment_bytes: Option<u64>,
    pub compression_type: Option<String>,
}

/// Topic detail information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicDetail {
    pub name: String,
    pub partitions: u32,
    pub replication_factor: u32,
    pub config: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub partition_assignments: Vec<serde_json::Value>,
}

// ==================== Security Types ====================

/// CA initialization request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateCaRequest {
    pub common_name: String,
    pub organization: Option<String>,
    pub country: Option<String>,
    pub validity_days: Option<u32>,
    pub key_size: Option<u32>,
}

/// CA information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaInfo {
    pub ca_id: String,
    pub common_name: String,
    pub organization: Option<String>,
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub key_usage: Vec<String>,
    pub is_ca: bool,
    pub path_length: Option<u32>,
    pub status: String,
    pub fingerprint: String,
}

/// Issue certificate request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueCertificateRequest {
    pub ca_id: String,
    pub common_name: String,
    pub subject_alt_names: Option<Vec<String>>,
    pub organization: Option<String>,
    pub role: Option<String>,
    pub validity_days: Option<u32>,
    pub key_usage: Option<Vec<String>>,
    pub extended_key_usage: Option<Vec<String>>,
}

/// Certificate list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateListItem {
    pub certificate_id: String,
    pub common_name: String,
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub status: String,
    pub role: Option<String>,
    pub fingerprint: String,
}

/// Certificate detail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateDetail {
    pub certificate_id: String,
    pub common_name: String,
    pub subject: String,
    pub issuer: String,
    pub serial_number: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub status: String,
    pub role: Option<String>,
    pub fingerprint: String,
    pub key_usage: Vec<String>,
    pub extended_key_usage: Vec<String>,
    pub subject_alt_names: Vec<String>,
    pub certificate_chain: Vec<String>,
    pub revocation_reason: Option<String>,
    pub revocation_time: Option<DateTime<Utc>>,
}

/// Revoke certificate request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevokeCertificateRequest {
    pub reason: String,
    pub reason_code: Option<u32>,
}

/// ACL rule creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAclRuleRequest {
    pub principal: String,
    pub resource_pattern: String,
    pub resource_type: String,
    pub operation: String,
    pub effect: String,
    pub conditions: Option<HashMap<String, String>>,
}

/// ACL rule response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRuleResponse {
    pub rule_id: String,
    pub principal: String,
    pub resource_pattern: String,
    pub resource_type: String,
    pub operation: String,
    pub effect: String,
    pub conditions: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub version: u64,
}

/// ACL evaluation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEvaluationRequest {
    pub principal: String,
    pub resource: String,
    pub operation: String,
    pub context: Option<HashMap<String, String>>,
}

/// ACL evaluation response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclEvaluationResponse {
    pub allowed: bool,
    pub effect: String,
    pub matched_rules: Vec<String>,
    pub evaluation_time_ns: u64,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub principal: Option<String>,
    pub resource: Option<String>,
    pub operation: Option<String>,
    pub result: String,
    pub details: HashMap<String, String>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
}

/// Audit log query request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogRequest {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub event_type: Option<String>,
    pub principal: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Audit log response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogResponse {
    pub events: Vec<AuditLogEntry>,
    pub total_count: u64,
    pub has_more: bool,
}

// ==================== Admin Client ====================

/// RustMQ Admin API client for cluster management and security operations
#[derive(Debug, Clone)]
pub struct AdminClient {
    config: AdminConfig,
    http_client: HttpClient,
    base_url: Url,
}

impl AdminClient {
    /// Create a new admin client with the specified configuration
    pub fn new(config: AdminConfig) -> Result<Self> {
        let base_url = Url::parse(&config.base_url)
            .map_err(|e| ClientError::InvalidConfig(format!("Invalid base URL: {}", e)))?;

        let mut builder = ClientBuilder::new()
            .timeout(config.timeout)
            .pool_max_idle_per_host(config.pool_max_idle_per_host)
            .danger_accept_invalid_certs(!config.tls_verify);

        // Add custom CA certificate if provided
        if let Some(ca_cert) = &config.ca_cert {
            let cert = reqwest::Certificate::from_pem(ca_cert)
                .map_err(|e| ClientError::Tls(format!("Invalid CA certificate: {}", e)))?;
            builder = builder.add_root_certificate(cert);
        }

        // Add client certificate for mTLS if provided
        if let (Some(cert), Some(key)) = (&config.client_cert, &config.client_key) {
            let mut pem = cert.clone();
            pem.extend_from_slice(key);

            let identity = reqwest::Identity::from_pem(&pem)
                .map_err(|e| ClientError::Tls(format!("Invalid client certificate: {}", e)))?;
            builder = builder.identity(identity);
        }

        let http_client = builder
            .build()
            .map_err(|e| ClientError::Connection(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            config,
            http_client,
            base_url,
        })
    }

    /// Helper to build request with authentication
    fn request(&self, method: reqwest::Method, path: &str) -> Result<RequestBuilder> {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| ClientError::InvalidConfig(format!("Invalid URL path: {}", e)))?;

        let mut req = self.http_client.request(method, url);

        // Add authentication token if configured
        if let Some(token) = &self.config.auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        Ok(req)
    }

    /// Execute a request and parse the response
    async fn execute<T>(&self, req: RequestBuilder) -> Result<ApiResponse<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = req
            .send()
            .await
            .map_err(|e| ClientError::Connection(format!("HTTP request failed: {}", e)))?;

        // Check status code
        let status = response.status();
        if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ClientError::Broker(format!(
                "HTTP {}: {}",
                status, error_text
            )));
        }

        // Parse JSON response
        let api_response: ApiResponse<T> = response.json().await.map_err(|e| {
            ClientError::Deserialization(format!("Failed to parse response: {}", e))
        })?;

        Ok(api_response)
    }

    // ==================== Health & Cluster Management ====================

    /// Check cluster health status
    pub async fn health(&self) -> Result<HealthResponse> {
        let req = self.request(reqwest::Method::GET, "/health")?;
        let response: HealthResponse = req
            .send()
            .await
            .map_err(|e| ClientError::Connection(format!("Health check failed: {}", e)))?
            .json()
            .await
            .map_err(|e| {
                ClientError::Deserialization(format!("Failed to parse health response: {}", e))
            })?;

        Ok(response)
    }

    /// Get cluster status including brokers and topics
    pub async fn cluster_status(&self) -> Result<ClusterStatus> {
        let req = self.request(reqwest::Method::GET, "/api/v1/cluster")?;
        let response = self.execute::<ClusterStatus>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    /// List all brokers in the cluster
    pub async fn list_brokers(&self) -> Result<Vec<BrokerStatus>> {
        let req = self.request(reqwest::Method::GET, "/api/v1/brokers")?;
        let response = self.execute::<Vec<BrokerStatus>>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    // ==================== Topic Management ====================

    /// List all topics
    pub async fn list_topics(&self) -> Result<Vec<TopicSummary>> {
        let req = self.request(reqwest::Method::GET, "/api/v1/topics")?;
        let response = self.execute::<Vec<TopicSummary>>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    /// Create a new topic
    pub async fn create_topic(&self, request: CreateTopicRequest) -> Result<String> {
        let req = self
            .request(reqwest::Method::POST, "/api/v1/topics")?
            .json(&request);

        let response = self.execute::<String>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    /// Delete a topic
    pub async fn delete_topic(&self, topic_name: &str) -> Result<String> {
        let path = format!("/api/v1/topics/{}", topic_name);
        let req = self.request(reqwest::Method::DELETE, &path)?;

        let response = self.execute::<String>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    /// Get topic details
    pub async fn describe_topic(&self, topic_name: &str) -> Result<TopicDetail> {
        let path = format!("/api/v1/topics/{}", topic_name);
        let req = self.request(reqwest::Method::GET, &path)?;

        let response = self.execute::<TopicDetail>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    // ==================== Security - CA Management ====================

    /// Generate a new Certificate Authority
    pub async fn generate_ca(&self, request: GenerateCaRequest) -> Result<CaInfo> {
        let req = self
            .request(reqwest::Method::POST, "/api/v1/security/ca/generate")?
            .json(&request);

        let response = self.execute::<CaInfo>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// List all Certificate Authorities
    pub async fn list_cas(&self) -> Result<Vec<CaInfo>> {
        let req = self.request(reqwest::Method::GET, "/api/v1/security/ca/list")?;

        let response = self.execute::<Vec<CaInfo>>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Get CA information by ID
    pub async fn get_ca(&self, ca_id: &str) -> Result<CaInfo> {
        let path = format!("/api/v1/security/ca/{}", ca_id);
        let req = self.request(reqwest::Method::GET, &path)?;

        let response = self.execute::<CaInfo>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Delete a Certificate Authority
    pub async fn delete_ca(&self, ca_id: &str) -> Result<String> {
        let path = format!("/api/v1/security/ca/{}", ca_id);
        let req = self.request(reqwest::Method::DELETE, &path)?;

        let response = self.execute::<String>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    // ==================== Security - Certificate Management ====================

    /// Issue a new certificate
    pub async fn issue_certificate(
        &self,
        request: IssueCertificateRequest,
    ) -> Result<CertificateDetail> {
        let req = self
            .request(reqwest::Method::POST, "/api/v1/security/certificates/issue")?
            .json(&request);

        let response = self.execute::<CertificateDetail>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// List certificates with optional filters
    pub async fn list_certificates(
        &self,
        filters: Option<HashMap<String, String>>,
    ) -> Result<Vec<CertificateListItem>> {
        let mut req = self.request(reqwest::Method::GET, "/api/v1/security/certificates")?;

        if let Some(filters) = filters {
            req = req.query(&filters);
        }

        let response = self.execute::<Vec<CertificateListItem>>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Get certificate details by ID
    pub async fn get_certificate(&self, cert_id: &str) -> Result<CertificateDetail> {
        let path = format!("/api/v1/security/certificates/{}", cert_id);
        let req = self.request(reqwest::Method::GET, &path)?;

        let response = self.execute::<CertificateDetail>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Revoke a certificate
    pub async fn revoke_certificate(
        &self,
        cert_id: &str,
        request: RevokeCertificateRequest,
    ) -> Result<String> {
        let path = format!("/api/v1/security/certificates/revoke/{}", cert_id);
        let req = self.request(reqwest::Method::POST, &path)?.json(&request);

        let response = self.execute::<String>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Renew a certificate
    pub async fn renew_certificate(&self, cert_id: &str) -> Result<CertificateDetail> {
        let path = format!("/api/v1/security/certificates/renew/{}", cert_id);
        let req = self.request(reqwest::Method::POST, &path)?;

        let response = self.execute::<CertificateDetail>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Get certificate status
    pub async fn get_certificate_status(&self, cert_id: &str) -> Result<String> {
        let path = format!("/api/v1/security/certificates/{}/status", cert_id);
        let req = self.request(reqwest::Method::GET, &path)?;

        let response = self.execute::<String>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    /// Get certificate chain
    pub async fn get_certificate_chain(&self, cert_id: &str) -> Result<Vec<String>> {
        let path = format!("/api/v1/security/certificates/{}/chain", cert_id);
        let req = self.request(reqwest::Method::GET, &path)?;

        let response = self.execute::<Vec<String>>(req).await?;

        if !response.success {
            return Err(ClientError::CertificateManagement(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::CertificateManagement("No data in response".to_string()))
    }

    // ==================== Security - ACL Management ====================

    /// Create a new ACL rule
    pub async fn create_acl_rule(&self, request: CreateAclRuleRequest) -> Result<AclRuleResponse> {
        let req = self
            .request(reqwest::Method::POST, "/api/v1/security/acl/rules")?
            .json(&request);

        let response = self.execute::<AclRuleResponse>(req).await?;

        if !response.success {
            return Err(ClientError::AclOperation(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::AclOperation("No data in response".to_string()))
    }

    /// List ACL rules with optional filters
    pub async fn list_acl_rules(
        &self,
        filters: Option<HashMap<String, String>>,
    ) -> Result<Vec<AclRuleResponse>> {
        let mut req = self.request(reqwest::Method::GET, "/api/v1/security/acl/rules")?;

        if let Some(filters) = filters {
            req = req.query(&filters);
        }

        let response = self.execute::<Vec<AclRuleResponse>>(req).await?;

        if !response.success {
            return Err(ClientError::AclOperation(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::AclOperation("No data in response".to_string()))
    }

    /// Delete an ACL rule
    pub async fn delete_acl_rule(&self, rule_id: &str) -> Result<String> {
        let path = format!("/api/v1/security/acl/rules/{}", rule_id);
        let req = self.request(reqwest::Method::DELETE, &path)?;

        let response = self.execute::<String>(req).await?;

        if !response.success {
            return Err(ClientError::AclOperation(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::AclOperation("No data in response".to_string()))
    }

    /// Evaluate ACL permissions
    pub async fn evaluate_acl(
        &self,
        request: AclEvaluationRequest,
    ) -> Result<AclEvaluationResponse> {
        let req = self
            .request(reqwest::Method::POST, "/api/v1/security/acl/evaluate")?
            .json(&request);

        let response = self.execute::<AclEvaluationResponse>(req).await?;

        if !response.success {
            return Err(ClientError::AclOperation(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::AclOperation("No data in response".to_string()))
    }

    // ==================== Security - Audit Logs ====================

    /// Get audit logs with optional filters
    pub async fn get_audit_logs(&self, request: AuditLogRequest) -> Result<AuditLogResponse> {
        let req = self
            .request(reqwest::Method::GET, "/api/v1/security/audit/logs")?
            .json(&request);

        let response = self.execute::<AuditLogResponse>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }

    /// Get audit trail for a specific principal
    pub async fn get_audit_trail(&self, principal: &str) -> Result<Vec<AuditLogEntry>> {
        let path = format!("/api/v1/security/audit/trail/{}", principal);
        let req = self.request(reqwest::Method::GET, &path)?;

        let response = self.execute::<Vec<AuditLogEntry>>(req).await?;

        if !response.success {
            return Err(ClientError::Broker(
                response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        response
            .data
            .ok_or_else(|| ClientError::Broker("No data in response".to_string()))
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_config_default() {
        let config = AdminConfig::default();
        assert_eq!(config.base_url, "http://localhost:8080");
        assert_eq!(config.timeout, Duration::from_secs(30));
        assert!(config.tls_verify);
        assert_eq!(config.max_retries, 3);
    }

    #[test]
    fn test_admin_config_builder() {
        let config = AdminConfig::new("https://rustmq.example.com:8443")
            .with_timeout(Duration::from_secs(60))
            .with_tls_verify(false)
            .with_auth_token("test-token")
            .with_max_retries(5);

        assert_eq!(config.base_url, "https://rustmq.example.com:8443");
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert!(!config.tls_verify);
        assert_eq!(config.auth_token, Some("test-token".to_string()));
        assert_eq!(config.max_retries, 5);
    }

    #[test]
    fn test_admin_client_creation() {
        let config = AdminConfig::new("http://localhost:8080");
        let client = AdminClient::new(config);
        assert!(client.is_ok());
    }

    #[test]
    fn test_admin_client_invalid_url() {
        let config = AdminConfig::new("not-a-url");
        let client = AdminClient::new(config);
        assert!(client.is_err());
    }

    #[tokio::test]
    async fn test_create_topic_request_serialization() {
        let request = CreateTopicRequest {
            name: "test-topic".to_string(),
            partitions: 3,
            replication_factor: 2,
            retention_ms: Some(86400000),
            segment_bytes: Some(1073741824),
            compression_type: Some("lz4".to_string()),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test-topic"));
        assert!(json.contains("\"partitions\":3"));
        assert!(json.contains("\"replication_factor\":2"));
    }

    #[tokio::test]
    async fn test_api_response_deserialization() {
        let json = r#"{
            "success": true,
            "data": "test-data",
            "error": null,
            "leader_hint": "broker-1"
        }"#;

        let response: ApiResponse<String> = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.data, Some("test-data".to_string()));
        assert_eq!(response.leader_hint, Some("broker-1".to_string()));
    }

    #[tokio::test]
    async fn test_health_response_deserialization() {
        let json = r#"{
            "status": "ok",
            "version": "1.0.0",
            "uptime_seconds": 3600,
            "is_leader": true,
            "raft_term": 5
        }"#;

        let health: HealthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(health.status, "ok");
        assert_eq!(health.version, "1.0.0");
        assert_eq!(health.uptime_seconds, 3600);
        assert!(health.is_leader);
        assert_eq!(health.raft_term, 5);
    }

    #[tokio::test]
    async fn test_broker_status_deserialization() {
        let json = r#"{
            "id": "broker-1",
            "host": "localhost",
            "port_quic": 9092,
            "port_rpc": 9093,
            "rack_id": "rack-1",
            "online": true
        }"#;

        let broker: BrokerStatus = serde_json::from_str(json).unwrap();
        assert_eq!(broker.id, "broker-1");
        assert_eq!(broker.host, "localhost");
        assert_eq!(broker.port_quic, 9092);
        assert_eq!(broker.port_rpc, 9093);
        assert_eq!(broker.rack_id, "rack-1");
        assert!(broker.online);
    }

    #[test]
    fn test_acl_evaluation_request_serialization() {
        let request = AclEvaluationRequest {
            principal: "user@example.com".to_string(),
            resource: "topic.events.*".to_string(),
            operation: "read".to_string(),
            context: Some(HashMap::from([(
                "source_ip".to_string(),
                "192.168.1.100".to_string(),
            )])),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("user@example.com"));
        assert!(json.contains("topic.events.*"));
        assert!(json.contains("read"));
    }
}
