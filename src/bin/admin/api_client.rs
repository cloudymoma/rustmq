use rustmq::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use hyper::{Body, Client, Method, Request, Response, StatusCode};
use hyper::client::HttpConnector;
use std::time::Duration;
use tracing::{debug, error};
use chrono::{DateTime, Utc};

/// HTTP client for communicating with the Admin REST API
pub struct AdminApiClient {
    client: Client<HttpConnector>,
    base_url: String,
    timeout: Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub leader_hint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RateLimitError {
    pub message: String,
    pub retry_after: Option<u64>,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
}

impl AdminApiClient {
    pub fn new(base_url: &str) -> Result<Self> {
        let client = Client::builder()
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            .build_http();
            
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            timeout: Duration::from_secs(60),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Make a GET request to the API
    pub async fn get<T>(&self, path: &str) -> Result<ApiResponse<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.make_request::<(), T>(Method::GET, path, None).await
    }

    /// Make a GET request with query parameters
    pub async fn get_with_query<T>(&self, path: &str, query: &HashMap<String, String>) -> Result<ApiResponse<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        let query_string = query
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
            
        let full_path = if query_string.is_empty() {
            path.to_string()
        } else {
            format!("{}?{}", path, query_string)
        };
        
        self.make_request::<(), T>(Method::GET, &full_path, None).await
    }

    /// Make a POST request to the API
    pub async fn post<T, R>(&self, path: &str, body: &T) -> Result<ApiResponse<R>>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        self.make_request(Method::POST, path, Some(body)).await
    }

    /// Make a PUT request to the API
    pub async fn put<T, R>(&self, path: &str, body: &T) -> Result<ApiResponse<R>>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        self.make_request(Method::PUT, path, Some(body)).await
    }

    /// Make a DELETE request to the API
    pub async fn delete<T>(&self, path: &str) -> Result<ApiResponse<T>>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.make_request::<(), T>(Method::DELETE, path, None).await
    }

    async fn make_request<T, R>(&self, method: Method, path: &str, body: Option<&T>) -> Result<ApiResponse<R>>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        debug!("Making {} request to: {}", method, url);

        let mut request = Request::builder()
            .method(method)
            .uri(&url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "RustMQ Admin CLI/0.9.1");

        let request_body = if let Some(body) = body {
            let json = serde_json::to_string(body)?;
            debug!("Request body: {}", json);
            Body::from(json)
        } else {
            Body::empty()
        };

        let request = request.body(request_body)?;

        let response = tokio::time::timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| rustmq::error::RustMqError::Timeout("Request timeout".to_string()))??;

        self.handle_response(response).await
    }

    async fn handle_response<R>(&self, response: Response<Body>) -> Result<ApiResponse<R>>
    where
        R: for<'de> Deserialize<'de>,
    {
        let status = response.status();
        let headers = response.headers().clone();
        
        debug!("Response status: {}", status);

        // Collect response body
        let body_bytes = hyper::body::to_bytes(response.into_body()).await?;
        let body_str = String::from_utf8_lossy(&body_bytes);
        
        debug!("Response body: {}", body_str);

        match status {
            StatusCode::OK => {
                let api_response: ApiResponse<R> = serde_json::from_str(&body_str)?;
                Ok(api_response)
            }
            StatusCode::TOO_MANY_REQUESTS => {
                // Handle rate limiting
                let rate_limit_info = RateLimitError {
                    message: "Rate limit exceeded".to_string(),
                    retry_after: headers
                        .get("retry-after")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok()),
                    limit: headers
                        .get("x-ratelimit-limit")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok()),
                    remaining: headers
                        .get("x-ratelimit-remaining")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.parse().ok()),
                };
                
                Err(rustmq::error::RustMqError::RateLimitExceeded { 
                    principal: format!(
                        "Rate limit exceeded. Retry after: {}s, Limit: {}, Remaining: {}",
                        rate_limit_info.retry_after.unwrap_or(60),
                        rate_limit_info.limit.unwrap_or(0),
                        rate_limit_info.remaining.unwrap_or(0)
                    )
                })
            }
            StatusCode::NOT_FOUND => {
                let error_response: ApiResponse<R> = serde_json::from_str(&body_str)
                    .unwrap_or_else(|_| ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Resource not found".to_string()),
                        leader_hint: None,
                    });
                Ok(error_response)
            }
            StatusCode::BAD_REQUEST => {
                let error_response: ApiResponse<R> = serde_json::from_str(&body_str)
                    .unwrap_or_else(|_| ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Bad request".to_string()),
                        leader_hint: None,
                    });
                Ok(error_response)
            }
            StatusCode::UNAUTHORIZED => {
                Err(rustmq::error::RustMqError::AuthenticationFailed("Unauthorized".to_string()))
            }
            StatusCode::FORBIDDEN => {
                Err(rustmq::error::RustMqError::AuthorizationFailed("Forbidden".to_string()))
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                let error_response: ApiResponse<R> = serde_json::from_str(&body_str)
                    .unwrap_or_else(|_| ApiResponse {
                        success: false,
                        data: None,
                        error: Some("Internal server error".to_string()),
                        leader_hint: None,
                    });
                Ok(error_response)
            }
            _ => {
                error!("Unexpected HTTP status: {}", status);
                Err(rustmq::error::RustMqError::Network(format!(
                    "HTTP {}: {}",
                    status.as_u16(),
                    body_str
                )))
            }
        }
    }
}

// Add URL encoding dependency
pub mod urlencoding {
    pub fn encode(input: &str) -> String {
        input
            .chars()
            .map(|c| match c {
                'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
                _ => format!("%{:02X}", c as u8),
            })
            .collect()
    }
}

// Certificate Management Types (matching security_api.rs)
#[derive(Debug, Serialize, Deserialize)]
pub struct GenerateCaRequest {
    pub common_name: String,
    pub organization: Option<String>,
    pub country: Option<String>,
    pub validity_days: Option<u32>,
    pub key_size: Option<u32>,
}

// Intermediate CA request removed - only root CA supported for simplicity

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct CertificateDetailResponse {
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

#[derive(Debug, Serialize, Deserialize)]
pub struct RevokeCertificateRequest {
    pub reason: String,
    pub reason_code: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertificateValidationRequest {
    pub certificate_pem: String,
    pub chain_pem: Option<Vec<String>>,
    pub check_revocation: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CertificateValidationResponse {
    pub valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub chain_valid: bool,
    pub revocation_status: String,
    pub expires_in_days: Option<i64>,
}

// ACL Management Types
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAclRuleRequest {
    pub principal: String,
    pub resource_pattern: String,
    pub resource_type: String,
    pub operation: String,
    pub effect: String,
    pub conditions: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAclRuleRequest {
    pub principal: Option<String>,
    pub resource_pattern: Option<String>,
    pub resource_type: Option<String>,
    pub operation: Option<String>,
    pub effect: Option<String>,
    pub conditions: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct AclEvaluationRequest {
    pub principal: String,
    pub resource: String,
    pub operation: String,
    pub context: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AclEvaluationResponse {
    pub allowed: bool,
    pub effect: String,
    pub matched_rules: Vec<String>,
    pub evaluation_time_ns: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BulkAclEvaluationRequest {
    pub evaluations: Vec<AclEvaluationRequest>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BulkAclEvaluationResponse {
    pub results: Vec<AclEvaluationResponse>,
    pub total_evaluation_time_ns: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrincipalPermissionsResponse {
    pub principal: String,
    pub permissions: Vec<PermissionEntry>,
    pub effective_permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub resource_pattern: String,
    pub operations: Vec<String>,
    pub effect: String,
    pub conditions: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceRulesResponse {
    pub resource: String,
    pub rules: Vec<AclRuleResponse>,
    pub effective_permissions: HashMap<String, Vec<String>>,
}

// Security Audit and Status Types
#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityAuditLogRequest {
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub event_type: Option<String>,
    pub principal: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityAuditLogResponse {
    pub events: Vec<AuditLogEntry>,
    pub total_count: u64,
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityStatusResponse {
    pub overall_status: String,
    pub components: SecurityComponentsStatus,
    pub metrics: SecurityMetricsSnapshot,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityComponentsStatus {
    pub authentication: ComponentStatus,
    pub authorization: ComponentStatus,
    pub certificate_management: ComponentStatus,
    pub tls_configuration: ComponentStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub status: String,
    pub last_check: DateTime<Utc>,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SecurityMetricsSnapshot {
    pub authentication_success_rate: f64,
    pub authorization_cache_hit_rate: f64,
    pub certificate_validation_success_rate: f64,
    pub active_certificates: u64,
    pub revoked_certificates: u64,
    pub expired_certificates: u64,
    pub acl_rules_count: u64,
    pub average_authorization_latency_ns: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MaintenanceRequest {
    pub operation: String,
    pub parameters: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MaintenanceResponse {
    pub operation: String,
    pub status: String,
    pub results: HashMap<String, String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}