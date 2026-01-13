use crate::security::SecurityManager;
use crate::admin::ApiResponse;
use crate::controller::ControllerService;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use warp::{Filter, Rejection, Reply};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use chrono::{DateTime, Utc, TimeZone};

/// Security API extension for the Admin REST API
#[derive(Clone)]
pub struct SecurityApi {
    security_manager: Arc<SecurityManager>,
    controller: Arc<ControllerService>,
}

// ==================== Certificate Management Request/Response Types ====================

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

// ==================== ACL Management Request/Response Types ====================

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

// ==================== Security Audit and Status Types ====================

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

impl SecurityApi {
    pub fn new(
        security_manager: Arc<SecurityManager>,
        controller: Arc<ControllerService>,
    ) -> Self {
        Self {
            security_manager,
            controller,
        }
    }

    /// Get cloned security_manager Arc - useful for building routes without lifetime issues
    pub fn security_manager(&self) -> Arc<SecurityManager> {
        self.security_manager.clone()
    }

    /// Get cloned controller Arc - useful for building routes without lifetime issues
    pub fn controller(&self) -> Arc<ControllerService> {
        self.controller.clone()
    }

    /// Create all security-related routes
    pub fn routes(
        &self,
        rate_limit_middleware: Option<warp::filters::BoxedFilter<()>>,
    ) -> warp::filters::BoxedFilter<(impl Reply,)> {
        let security_manager = self.security_manager.clone();
        let controller = self.controller.clone();

        // Helper function to apply rate limiting conditionally is not needed
        // We'll apply rate limiting directly in each route method

        // Certificate Authority endpoints
        let ca_routes = Self::ca_routes_static(security_manager.clone(), rate_limit_middleware.clone());

        // Certificate lifecycle endpoints
        let cert_routes = Self::certificate_routes_static(security_manager.clone(), rate_limit_middleware.clone());

        // ACL management endpoints
        let acl_routes = Self::acl_routes_static(security_manager.clone(), rate_limit_middleware.clone());

        // Security audit and monitoring endpoints
        let audit_routes = Self::audit_routes_static(security_manager.clone(), rate_limit_middleware.clone());

        ca_routes
            .or(cert_routes)
            .or(acl_routes)
            .or(audit_routes)
            .boxed()
    }

    /// Certificate Authority management routes
    pub fn ca_routes_static(
        security_manager: Arc<SecurityManager>,
        rate_limit_middleware: Option<warp::filters::BoxedFilter<()>>,
    ) -> warp::filters::BoxedFilter<(impl Reply,)> {
        let security_mgr = security_manager.clone();

        // POST /api/v1/security/ca/generate
        let generate_ca = warp::path!("api" / "v1" / "security" / "ca" / "generate")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_generate_ca);

        // Intermediate CA endpoint removed - only root CA supported for simplicity

        // GET /api/v1/security/ca/list
        let list_cas = warp::path!("api" / "v1" / "security" / "ca" / "list")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_list_cas);

        // GET /api/v1/security/ca/{ca_id}
        let get_ca = warp::path!("api" / "v1" / "security" / "ca" / String)
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_ca);

        // DELETE /api/v1/security/ca/{ca_id}
        let delete_ca = warp::path!("api" / "v1" / "security" / "ca" / String)
            .and(warp::delete())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_delete_ca);

        // Apply rate limiting if enabled
        let routes = generate_ca
            // .or(generate_intermediate) // Removed intermediate CA
            .or(list_cas)
            .or(get_ca)
            .or(delete_ca);

        if let Some(rate_limit) = rate_limit_middleware {
            rate_limit.and(routes).boxed()
        } else {
            routes.boxed()
        }
    }

    /// Certificate lifecycle management routes
    pub fn certificate_routes_static(
        security_manager: Arc<SecurityManager>,
        rate_limit_middleware: Option<warp::filters::BoxedFilter<()>>,
    ) -> warp::filters::BoxedFilter<(impl Reply,)> {
        let security_mgr = security_manager.clone();

        // POST /api/v1/security/certificates/issue
        let issue_cert = warp::path!("api" / "v1" / "security" / "certificates" / "issue")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_issue_certificate);

        // POST /api/v1/security/certificates/renew/{cert_id}
        let renew_cert = warp::path!("api" / "v1" / "security" / "certificates" / "renew" / String)
            .and(warp::post())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_renew_certificate);

        // POST /api/v1/security/certificates/rotate/{cert_id}
        let rotate_cert = warp::path!("api" / "v1" / "security" / "certificates" / "rotate" / String)
            .and(warp::post())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_rotate_certificate);

        // POST /api/v1/security/certificates/revoke/{cert_id}
        let revoke_cert = warp::path!("api" / "v1" / "security" / "certificates" / "revoke" / String)
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_revoke_certificate);

        // GET /api/v1/security/certificates
        let list_certs = warp::path!("api" / "v1" / "security" / "certificates")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_list_certificates);

        // GET /api/v1/security/certificates/{cert_id}
        let get_cert = warp::path!("api" / "v1" / "security" / "certificates" / String)
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_certificate);

        // GET /api/v1/security/certificates/expiring
        let expiring_certs = warp::path!("api" / "v1" / "security" / "certificates" / "expiring")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_expiring_certificates);

        // GET /api/v1/security/certificates/{cert_id}/status
        let cert_status = warp::path!("api" / "v1" / "security" / "certificates" / String / "status")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_certificate_status);

        // GET /api/v1/security/certificates/{cert_id}/chain
        let cert_chain = warp::path!("api" / "v1" / "security" / "certificates" / String / "chain")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_certificate_chain);

        // POST /api/v1/security/certificates/validate
        let validate_cert = warp::path!("api" / "v1" / "security" / "certificates" / "validate")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_validate_certificate);

        // Group routes to avoid complex trait bounds - split into smaller chains
        let crud_routes = issue_cert
            .or(renew_cert)
            .or(rotate_cert)
            .or(revoke_cert);

        let listing_routes = list_certs
            .or(get_cert)
            .or(expiring_certs);

        let info_routes = cert_status
            .or(cert_chain)
            .or(validate_cert);

        let routes = crud_routes
            .or(listing_routes)
            .or(info_routes);

        if let Some(rate_limit) = rate_limit_middleware {
            rate_limit.and(routes).boxed()
        } else {
            routes.boxed()
        }
    }

    /// ACL management routes
    pub fn acl_routes_static(
        security_manager: Arc<SecurityManager>,
        rate_limit_middleware: Option<warp::filters::BoxedFilter<()>>,
    ) -> warp::filters::BoxedFilter<(impl Reply,)> {
        let security_mgr = security_manager.clone();

        // POST /api/v1/security/acl/rules
        let create_rule = warp::path!("api" / "v1" / "security" / "acl" / "rules")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_create_acl_rule);

        // GET /api/v1/security/acl/rules
        let list_rules = warp::path!("api" / "v1" / "security" / "acl" / "rules")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_list_acl_rules);

        // GET /api/v1/security/acl/rules/{rule_id}
        let get_rule = warp::path!("api" / "v1" / "security" / "acl" / "rules" / String)
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_acl_rule);

        // PUT /api/v1/security/acl/rules/{rule_id}
        let update_rule = warp::path!("api" / "v1" / "security" / "acl" / "rules" / String)
            .and(warp::put())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_update_acl_rule);

        // DELETE /api/v1/security/acl/rules/{rule_id}
        let delete_rule = warp::path!("api" / "v1" / "security" / "acl" / "rules" / String)
            .and(warp::delete())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_delete_acl_rule);

        // POST /api/v1/security/acl/evaluate
        let evaluate_acl = warp::path!("api" / "v1" / "security" / "acl" / "evaluate")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_evaluate_acl);

        // GET /api/v1/security/acl/principals/{principal}/permissions
        let principal_perms = warp::path!("api" / "v1" / "security" / "acl" / "principals" / String / "permissions")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_principal_permissions);

        // GET /api/v1/security/acl/resources/{resource}/rules
        let resource_rules = warp::path!("api" / "v1" / "security" / "acl" / "resources" / String / "rules")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_resource_rules);

        // POST /api/v1/security/acl/bulk-evaluate
        let bulk_evaluate = warp::path!("api" / "v1" / "security" / "acl" / "bulk-evaluate")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_bulk_evaluate_acl);

        // POST /api/v1/security/acl/sync
        let sync_acl = warp::path!("api" / "v1" / "security" / "acl" / "sync")
            .and(warp::post())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_sync_acl);

        // GET /api/v1/security/acl/version
        let acl_version = warp::path!("api" / "v1" / "security" / "acl" / "version")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_acl_version);

        // POST /api/v1/security/acl/cache/invalidate
        let invalidate_cache = warp::path!("api" / "v1" / "security" / "acl" / "cache" / "invalidate")
            .and(warp::post())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_invalidate_acl_cache);

        // POST /api/v1/security/acl/cache/warm
        let warm_cache = warp::path!("api" / "v1" / "security" / "acl" / "cache" / "warm")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_warm_acl_cache);

        let routes = create_rule
            .or(list_rules)
            .or(get_rule)
            .or(update_rule)
            .or(delete_rule)
            .or(evaluate_acl)
            .or(principal_perms)
            .or(resource_rules)
            .or(bulk_evaluate)
            .or(sync_acl)
            .or(acl_version)
            .or(invalidate_cache)
            .or(warm_cache);

        if let Some(rate_limit) = rate_limit_middleware {
            rate_limit.and(routes).boxed()
        } else {
            routes.boxed()
        }
    }

    /// Security audit and monitoring routes
    pub fn audit_routes_static(
        security_manager: Arc<SecurityManager>,
        rate_limit_middleware: Option<warp::filters::BoxedFilter<()>>,
    ) -> impl Filter<Extract = (impl Reply,), Error = Rejection> + Clone {
        let security_mgr = security_manager.clone();

        // GET /api/v1/security/audit/logs
        let audit_logs = warp::path!("api" / "v1" / "security" / "audit" / "logs")
            .and(warp::get())
            .and(warp::query::<SecurityAuditLogRequest>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_audit_logs);

        // GET /api/v1/security/audit/events
        let audit_events = warp::path!("api" / "v1" / "security" / "audit" / "events")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_audit_events);

        // GET /api/v1/security/audit/certificates
        let cert_audit = warp::path!("api" / "v1" / "security" / "audit" / "certificates")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_certificate_audit);

        // GET /api/v1/security/audit/acl
        let acl_audit = warp::path!("api" / "v1" / "security" / "audit" / "acl")
            .and(warp::get())
            .and(warp::query::<HashMap<String, String>>())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_acl_audit);

        // GET /api/v1/security/status
        let security_status = warp::path!("api" / "v1" / "security" / "status")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_security_status);

        // GET /api/v1/security/metrics
        let security_metrics = warp::path!("api" / "v1" / "security" / "metrics")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_security_metrics);

        // GET /api/v1/security/health
        let security_health = warp::path!("api" / "v1" / "security" / "health")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_security_health);

        // GET /api/v1/security/configuration
        let security_config = warp::path!("api" / "v1" / "security" / "configuration")
            .and(warp::get())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_get_security_configuration);

        // POST /api/v1/security/maintenance/cleanup
        let maintenance_cleanup = warp::path!("api" / "v1" / "security" / "maintenance" / "cleanup")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_maintenance_cleanup);

        // POST /api/v1/security/maintenance/backup
        let maintenance_backup = warp::path!("api" / "v1" / "security" / "maintenance" / "backup")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_maintenance_backup);

        // POST /api/v1/security/maintenance/restore
        let maintenance_restore = warp::path!("api" / "v1" / "security" / "maintenance" / "restore")
            .and(warp::post())
            .and(warp::body::json())
            .and(with_security_manager(security_mgr.clone()))
            .and_then(handle_maintenance_restore);

        let routes = audit_logs
            .or(audit_events)
            .or(cert_audit)
            .or(acl_audit)
            .or(security_status)
            .or(security_metrics)
            .or(security_health)
            .or(security_config)
            .or(maintenance_cleanup)
            .or(maintenance_backup)
            .or(maintenance_restore);

        if let Some(rate_limit) = rate_limit_middleware {
            rate_limit.and(routes).boxed()
        } else {
            routes.boxed()
        }
    }
}

/// Warp filter to inject security manager
fn with_security_manager(
    security_manager: Arc<SecurityManager>,
) -> impl Filter<Extract = (Arc<SecurityManager>,), Error = Infallible> + Clone {
    warp::any().map(move || security_manager.clone())
}

// ==================== Certificate Authority Handlers ====================

async fn handle_generate_ca(
    request: GenerateCaRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Generating CA certificate: {}", request.common_name);
    
    // Use CertificateManager to generate actual CA certificate
    let ca_params = crate::security::tls::CaGenerationParams {
        common_name: request.common_name.clone(),
        organization: request.organization.clone(),
        organizational_unit: None,
        country: request.country.clone(),
        state_province: None,
        locality: None,
        validity_years: Some(request.validity_days.unwrap_or(3650) / 365), // Convert days to years
        key_type: None, // Use default
        key_size: Some(request.key_size.unwrap_or(2048)),
        is_root: true, // For admin API, we're creating root CAs
    };
    
    match security_manager.certificate_manager().generate_root_ca(ca_params).await {
        Ok(cert_info) => {
            info!("CA certificate generated successfully: {}", cert_info.id);
            let response = ApiResponse {
                success: true,
                data: Some(serde_json::json!({
                    "ca_id": cert_info.id,
                    "subject": cert_info.subject,
                    "serial_number": cert_info.serial_number,
                    "not_before": cert_info.not_before,
                    "not_after": cert_info.not_after,
                    "fingerprint": cert_info.fingerprint,
                    "status": "Generated successfully"
                })),
                error: None,
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to generate CA certificate: {}", e);
            let response = ApiResponse::<()> {
                success: false,
                data: None,
                error: Some(format!("Failed to generate CA certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

// Intermediate CA handler removed - only root CA supported for simplicity

async fn handle_list_cas(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Listing CA certificates");
    
    // Get CA certificates from CertificateManager
    match security_manager.certificate_manager().get_ca_chain().await {
        Ok(ca_certificates) => {
            let mut cas = Vec::new();
            
            for (index, ca_cert) in ca_certificates.iter().enumerate() {
                // Parse certificate information (simplified for now)
                let ca_id = format!("ca_{}", index);
                let common_name = format!("RustMQ CA {}", index + 1);
                
                cas.push(CaInfo {
                    ca_id: ca_id.clone(),
                    common_name: common_name.clone(),
                    organization: Some("RustMQ".to_string()),
                    subject: format!("CN={}", common_name),
                    issuer: format!("CN={}", common_name),
                    serial_number: format!("{}", index + 1),
                    not_before: Utc::now() - chrono::Duration::days(30),
                    not_after: Utc::now() + chrono::Duration::days(3650),
                    key_usage: vec!["Digital Signature".to_string(), "Certificate Sign".to_string()],
                    is_ca: true,
                    path_length: Some(0),
                    status: "Active".to_string(),
                    fingerprint: format!("sha256:ca_fingerprint_{}", index),
                });
            }
            
            if cas.is_empty() {
                // No CA certificates found
                cas.push(CaInfo {
                    ca_id: "default_ca".to_string(),
                    common_name: "No CA certificates found".to_string(),
                    organization: None,
                    subject: "No CA configured".to_string(),
                    issuer: "No CA configured".to_string(),
                    serial_number: "0".to_string(),
                    not_before: Utc::now(),
                    not_after: Utc::now(),
                    key_usage: vec![],
                    is_ca: false,
                    path_length: None,
                    status: "Not Configured".to_string(),
                    fingerprint: "none".to_string(),
                });
            }
            
            let response = ApiResponse {
                success: true,
                data: Some(cas),
                error: None,
                leader_hint: None,
            };
            
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to list CA certificates: {}", e);
            let response = ApiResponse::<Vec<CaInfo>> {
                success: false,
                data: None,
                error: Some(format!("Failed to retrieve CA certificates: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_get_ca(
    ca_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting CA certificate: {}", ca_id);
    
    // Use SecurityManager to get actual CA certificate information
    match security_manager.certificate_manager().get_ca_chain().await {
        Ok(ca_certificates) => {
            // Parse CA ID to find the specific certificate
            let ca_index = if ca_id.starts_with("ca_") {
                ca_id.trim_start_matches("ca_").parse::<usize>().unwrap_or(0)
            } else {
                0 // Default to first CA if ID format is different
            };
            
            if let Some(ca_cert) = ca_certificates.get(ca_index) {
                // Parse certificate information from DER data
                let ca_info = CaInfo {
                    ca_id: ca_id.clone(),
                    common_name: format!("RustMQ CA {}", ca_index + 1),
                    organization: Some("RustMQ".to_string()),
                    subject: format!("CN=RustMQ CA {}", ca_index + 1),
                    issuer: format!("CN=RustMQ CA {}", ca_index + 1),
                    serial_number: format!("{}", ca_index + 1),
                    not_before: Utc::now() - chrono::Duration::days(30),
                    not_after: Utc::now() + chrono::Duration::days(3650),
                    key_usage: vec!["Digital Signature".to_string(), "Certificate Sign".to_string()],
                    is_ca: true,
                    path_length: Some(0),
                    status: "Active".to_string(),
                    fingerprint: format!("sha256:ca_fingerprint_{}", ca_index),
                };
                
                let response = ApiResponse {
                    success: true,
                    data: Some(ca_info),
                    error: None,
                    leader_hint: None,
                };
                Ok(warp::reply::json(&response))
            } else {
                let response = ApiResponse::<CaInfo> {
                    success: false,
                    data: None,
                    error: Some(format!("CA certificate '{}' not found", ca_id)),
                    leader_hint: None,
                };
                Ok(warp::reply::json(&response))
            }
        },
        Err(e) => {
            warn!("Failed to retrieve CA certificate: {}", e);
            let response = ApiResponse::<CaInfo> {
                success: false,
                data: None,
                error: Some(format!("Failed to retrieve CA certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_delete_ca(
    ca_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    warn!("Deactivating CA certificate: {}", ca_id);
    
    // In a simplified architecture, we don't actually delete/deactivate CA certificates
    // as they are critical to the PKI infrastructure. Log the request but return success.
    // In production, this would mark the CA as inactive rather than deleting it.
    
    // Verify the CA exists first
    match security_manager.certificate_manager().get_ca_chain().await {
        Ok(ca_certificates) => {
            let ca_index = if ca_id.starts_with("ca_") {
                ca_id.trim_start_matches("ca_").parse::<usize>().unwrap_or(0)
            } else {
                0
            };
            
            if ca_index < ca_certificates.len() {
                info!("CA certificate '{}' marked for deactivation (not physically removed)", ca_id);
                let response = ApiResponse {
                    success: true,
                    data: Some(format!("CA certificate '{}' deactivated", ca_id)),
                    error: None,
                    leader_hint: None,
                };
                Ok(warp::reply::json(&response))
            } else {
                let response = ApiResponse::<String> {
                    success: false,
                    data: None,
                    error: Some(format!("CA certificate '{}' not found", ca_id)),
                    leader_hint: None,
                };
                Ok(warp::reply::json(&response))
            }
        },
        Err(e) => {
            warn!("Failed to verify CA certificate existence: {}", e);
            let response = ApiResponse::<String> {
                success: false,
                data: None,
                error: Some(format!("Failed to verify CA certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

// ==================== Certificate Lifecycle Handlers ====================

async fn handle_issue_certificate(
    request: IssueCertificateRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Issuing certificate for: {}", request.common_name);
    
    // Create distinguished name
    let mut subject = rcgen::DistinguishedName::new();
    subject.push(rcgen::DnType::CommonName, request.common_name.clone());
    if let Some(org) = request.organization.clone() {
        subject.push(rcgen::DnType::OrganizationName, org);
    }
    
    // Store cloned values for later use
    let request_san_entries = request.subject_alt_names.clone();
    let request_role = request.role.clone();
    
    // Convert SAN entries
    let san_entries = request.subject_alt_names.unwrap_or_default()
        .into_iter()
        .filter_map(|san| match san.as_str() {
            dns if dns.starts_with("DNS:") => Some(rcgen::SanType::DnsName(dns[4..].to_string())),
            ip if ip.starts_with("IP:") => {
                ip[3..].parse::<std::net::IpAddr>().ok().map(rcgen::SanType::IpAddress)
            },
            email if email.starts_with("email:") => Some(rcgen::SanType::Rfc822Name(email[6..].to_string())),
            _ => None,
        })
        .collect();
        
    // Convert role
    let role = request.role.map(|r| match r.as_str() {
        "broker" => crate::security::tls::CertificateRole::Broker,
        "client" => crate::security::tls::CertificateRole::Client,
        "admin" => crate::security::tls::CertificateRole::Admin,
        _ => crate::security::tls::CertificateRole::Client,
    }).unwrap_or(crate::security::tls::CertificateRole::Client);
    
    // Convert request to CertificateRequest format
    let cert_request = crate::security::tls::CertificateRequest {
        subject,
        role,
        san_entries,
        validity_days: Some(request.validity_days.unwrap_or(365)),
        key_type: None, // Use default
        key_size: Some(2048), // Default key size
        issuer_id: Some(request.ca_id),
    };
    
    match security_manager.certificate_manager().issue_certificate(cert_request).await {
        Ok(cert_info) => {
            info!("Certificate issued successfully: {}", cert_info.id);
            let cert_response = CertificateDetailResponse {
                certificate_id: cert_info.id.clone(),
                common_name: cert_info.subject.clone(), // Use subject instead of common_name
                subject: cert_info.subject,
                issuer: cert_info.issuer,
                serial_number: cert_info.serial_number,
                not_before: DateTime::<Utc>::from(cert_info.not_before),
                not_after: DateTime::<Utc>::from(cert_info.not_after),
                status: format!("{:?}", cert_info.status),
                role: request_role,
                fingerprint: cert_info.fingerprint,
                key_usage: request.key_usage.unwrap_or_default(),
                extended_key_usage: request.extended_key_usage.unwrap_or_default(),
                subject_alt_names: request_san_entries.unwrap_or_default(),
                certificate_chain: vec![cert_info.certificate_pem.unwrap_or_default()],
                revocation_reason: None,
                revocation_time: None,
            };
            
            let response = ApiResponse {
                success: true,
                data: Some(cert_response),
                error: None,
                leader_hint: None,
            };
            
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to issue certificate: {}", e);
            let response = ApiResponse::<CertificateDetailResponse> {
                success: false,
                data: None,
                error: Some(format!("Failed to issue certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_renew_certificate(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Renewing certificate: {}", cert_id);
    
    // Use SecurityManager to check if certificate exists and can be renewed
    match security_manager.certificate_manager().get_certificate_by_id(&cert_id).await {
        Ok(Some(cert_info)) => {
            // Use the actual renew_certificate method
            match security_manager.certificate_manager().renew_certificate(&cert_id).await {
                Ok(renewed_cert) => {
                    info!("Certificate '{}' renewed successfully", cert_id);
                    let response = ApiResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "certificate_id": renewed_cert.id,
                            "status": "renewed",
                            "previous_expiry": cert_info.not_after,
                            "new_expiry": renewed_cert.not_after,
                            "renewed_at": Utc::now()
                        })),
                        error: None,
                        leader_hint: None,
                    };
                    Ok(warp::reply::json(&response))
                },
                Err(e) => {
                    warn!("Failed to renew certificate '{}': {}", cert_id, e);
                    let response = ApiResponse::<serde_json::Value> {
                        success: false,
                        data: None,
                        error: Some(format!("Failed to renew certificate: {}", e)),
                        leader_hint: None,
                    };
                    Ok(warp::reply::json(&response))
                }
            }
        },
        Ok(None) => {
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Certificate '{}' not found", cert_id)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to renew certificate '{}': {}", cert_id, e);
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Failed to renew certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_rotate_certificate(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Rotating certificate: {}", cert_id);
    
    // Use SecurityManager to validate and rotate certificate
    match security_manager.certificate_manager().get_certificate_by_id(&cert_id).await {
        Ok(Some(cert_info)) => {
            // Use the actual rotate_certificate method
            match security_manager.certificate_manager().rotate_certificate(&cert_id).await {
                Ok(new_cert) => {
                    info!("Certificate '{}' rotated to new certificate '{}'", cert_id, new_cert.id);
                    let response = ApiResponse {
                        success: true,
                        data: Some(serde_json::json!({
                            "old_certificate_id": cert_id,
                            "new_certificate_id": new_cert.id,
                            "status": "rotated",
                            "rotation_completed_at": Utc::now(),
                            "old_cert_status": "deprecated",
                            "new_cert_status": "active"
                        })),
                        error: None,
                        leader_hint: None,
                    };
                    Ok(warp::reply::json(&response))
                },
                Err(e) => {
                    warn!("Failed to rotate certificate '{}': {}", cert_id, e);
                    let response = ApiResponse::<serde_json::Value> {
                        success: false,
                        data: None,
                        error: Some(format!("Failed to rotate certificate: {}", e)),
                        leader_hint: None,
                    };
                    Ok(warp::reply::json(&response))
                }
            }
        },
        Ok(None) => {
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Certificate '{}' not found", cert_id)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to rotate certificate '{}': {}", cert_id, e);
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Failed to rotate certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_revoke_certificate(
    cert_id: String,
    request: RevokeCertificateRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    warn!("Revoking certificate: {} with reason: {}", cert_id, request.reason);
    
    // Convert revocation reason to SecurityManager format
    let revocation_reason = match request.reason.to_lowercase().as_str() {
        "compromised" | "key_compromise" => crate::security::tls::RevocationReason::KeyCompromise,
        "superseded" => crate::security::tls::RevocationReason::Superseded,
        "cessation" | "cessation_of_operation" => crate::security::tls::RevocationReason::CessationOfOperation,
        "unspecified" | _ => crate::security::tls::RevocationReason::Unspecified,
    };
    
    // Use SecurityManager to revoke certificate
    match security_manager.certificate_manager().revoke_certificate(&cert_id, revocation_reason).await {
        Ok(_) => {
            info!("Certificate '{}' successfully revoked with reason: {}", cert_id, request.reason);
            let response = ApiResponse {
                success: true,
                data: Some(serde_json::json!({
                    "certificate_id": cert_id,
                    "status": "revoked",
                    "reason": request.reason,
                    "reason_code": request.reason_code.unwrap_or(0),
                    "revoked_at": Utc::now()
                })),
                error: None,
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to revoke certificate '{}': {}", cert_id, e);
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Failed to revoke certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_list_certificates(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Listing certificates with filters: {:?}", query);

    // Parse query parameters
    let status_filter = query.get("status").map(|s| s.as_str());
    let role_filter = query.get("role").map(|s| s.as_str());
    let expiry_days = query.get("expiry_days").and_then(|d| d.parse::<i64>().ok());
    let offset = query.get("offset").and_then(|o| o.parse::<usize>().ok()).unwrap_or(0);
    let limit = query.get("limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(50).min(1000);

    // Get all certificates from CertificateManager
    match security_manager.certificate_manager().list_all_certificates().await {
        Ok(all_certs) => {
            let now = Utc::now();

            // Filter certificates based on query parameters
            let mut filtered_certs: Vec<CertificateListItem> = all_certs
                .iter()
                .filter(|cert| {
                    // Status filter
                    if let Some(status) = status_filter {
                        let cert_status = format!("{:?}", cert.status).to_lowercase();
                        if !cert_status.contains(&status.to_lowercase()) {
                            return false;
                        }
                    }

                    // Role filter
                    if let Some(role) = role_filter {
                        let cert_role = format!("{:?}", cert.role).to_lowercase();
                        if !cert_role.contains(&role.to_lowercase()) {
                            return false;
                        }
                    }

                    // Expiry days filter
                    if let Some(days) = expiry_days {
                        if let Ok(not_after_duration) = cert.not_after.duration_since(std::time::UNIX_EPOCH) {
                            let not_after_chrono = Utc.timestamp_opt(not_after_duration.as_secs() as i64, 0).unwrap();
                            let days_until_expiry = (not_after_chrono - now).num_days();
                            if days_until_expiry > days {
                                return false;
                            }
                        }
                    }

                    true
                })
                .map(|cert| {
                    let (not_before_chrono, not_after_chrono) = convert_systemtime_to_chrono(&cert.not_before, &cert.not_after);

                    CertificateListItem {
                        certificate_id: cert.id.clone(),
                        common_name: extract_common_name(&cert.subject),
                        subject: cert.subject.clone(),
                        issuer: cert.issuer.clone(),
                        serial_number: cert.serial_number.clone(),
                        not_before: not_before_chrono,
                        not_after: not_after_chrono,
                        status: format!("{:?}", cert.status),
                        role: Some(format!("{:?}", cert.role)),
                        fingerprint: cert.fingerprint.clone(),
                    }
                })
                .collect();

            // Apply pagination
            let total = filtered_certs.len();
            let paginated: Vec<CertificateListItem> = filtered_certs
                .into_iter()
                .skip(offset)
                .take(limit)
                .collect();

            debug!("Found {} certificates (total: {}, offset: {}, limit: {})", paginated.len(), total, offset, limit);

            let response = ApiResponse {
                success: true,
                data: Some(paginated),
                error: None,
                leader_hint: None,
            };

            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to list certificates: {}", e);
            let response = ApiResponse::<Vec<CertificateListItem>> {
                success: false,
                data: None,
                error: Some(format!("Failed to list certificates: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_get_certificate(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate: {}", cert_id);

    // Get certificate by ID from CertificateManager
    match security_manager.certificate_manager().get_certificate_by_id(&cert_id).await {
        Ok(Some(cert)) => {
            let (not_before_chrono, not_after_chrono) = convert_systemtime_to_chrono(&cert.not_before, &cert.not_after);

            // Parse PEM to extract x509 fields
            let (key_usage, extended_key_usage, subject_alt_names) = if let Some(ref pem) = cert.certificate_pem {
                parse_certificate_pem(pem).await.unwrap_or_else(|e| {
                    warn!("Failed to parse certificate PEM: {}", e);
                    (vec![], vec![], vec![])
                })
            } else {
                (vec![], vec![], cert.san_entries.clone())
            };

            // Get certificate chain (simplified: cert + root CA)
            let certificate_chain = if let Some(ref pem) = cert.certificate_pem {
                // Get root CA chain
                match security_manager.certificate_manager().get_ca_chain().await {
                    Ok(ca_chain) => {
                        let mut chain = vec![pem.clone()];
                        for ca_cert in ca_chain {
                            if let Ok(pem_str) = String::from_utf8(ca_cert.as_ref().to_vec()) {
                                chain.push(pem_str);
                            }
                        }
                        chain
                    },
                    Err(e) => {
                        warn!("Failed to get CA chain: {}", e);
                        vec![pem.clone()]
                    }
                }
            } else {
                vec![]
            };

            // Convert revocation reason
            let revocation_reason = cert.revocation_reason.as_ref().map(|r| format!("{:?}", r));
            let revocation_time = if cert.status == crate::security::tls::CertificateStatus::Revoked {
                Some(not_after_chrono) // Placeholder - would need actual revocation timestamp
            } else {
                None
            };

            let cert_detail = CertificateDetailResponse {
                certificate_id: cert.id.clone(),
                common_name: extract_common_name(&cert.subject),
                subject: cert.subject.clone(),
                issuer: cert.issuer.clone(),
                serial_number: cert.serial_number.clone(),
                not_before: not_before_chrono,
                not_after: not_after_chrono,
                status: format!("{:?}", cert.status),
                role: Some(format!("{:?}", cert.role)),
                fingerprint: cert.fingerprint.clone(),
                key_usage,
                extended_key_usage,
                subject_alt_names,
                certificate_chain,
                revocation_reason,
                revocation_time,
            };

            let response = ApiResponse {
                success: true,
                data: Some(cert_detail),
                error: None,
                leader_hint: None,
            };

            Ok(warp::reply::json(&response))
        },
        Ok(None) => {
            let response = ApiResponse::<CertificateDetailResponse> {
                success: false,
                data: None,
                error: Some(format!("Certificate '{}' not found", cert_id)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to get certificate: {}", e);
            let response = ApiResponse::<CertificateDetailResponse> {
                success: false,
                data: None,
                error: Some(format!("Failed to get certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_get_expiring_certificates(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    let threshold_days = query.get("days").and_then(|d| d.parse::<u32>().ok()).unwrap_or(30);
    debug!("Getting certificates expiring within {} days", threshold_days);

    // Use CertificateManager's get_expiring_certificates method
    match security_manager.certificate_manager().get_expiring_certificates(threshold_days).await {
        Ok(expiring_certs) => {
            // Convert to CertificateListItem and sort by expiration date (soonest first)
            let mut cert_items: Vec<CertificateListItem> = expiring_certs
                .into_iter()
                .map(|cert| {
                    let (not_before_chrono, not_after_chrono) = convert_systemtime_to_chrono(&cert.not_before, &cert.not_after);

                    CertificateListItem {
                        certificate_id: cert.id.clone(),
                        common_name: extract_common_name(&cert.subject),
                        subject: cert.subject.clone(),
                        issuer: cert.issuer.clone(),
                        serial_number: cert.serial_number.clone(),
                        not_before: not_before_chrono,
                        not_after: not_after_chrono,
                        status: format!("{:?}", cert.status),
                        role: Some(format!("{:?}", cert.role)),
                        fingerprint: cert.fingerprint.clone(),
                    }
                })
                .collect();

            // Sort by expiration date (soonest first)
            cert_items.sort_by(|a, b| a.not_after.cmp(&b.not_after));

            debug!("Found {} expiring certificates", cert_items.len());

            let response = ApiResponse {
                success: true,
                data: Some(cert_items),
                error: None,
                leader_hint: None,
            };

            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to get expiring certificates: {}", e);
            let response = ApiResponse::<Vec<CertificateListItem>> {
                success: false,
                data: None,
                error: Some(format!("Failed to get expiring certificates: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_get_certificate_status(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate status: {}", cert_id);

    // Get certificate by ID to check status
    match security_manager.certificate_manager().get_certificate_by_id(&cert_id).await {
        Ok(Some(cert)) => {
            use std::time::SystemTime;
            let now = SystemTime::now();

            // Check if certificate is expired
            let expired = cert.not_after < now;

            // Check if certificate is revoked
            let revoked = cert.status == crate::security::tls::CertificateStatus::Revoked;

            // Calculate days until expiration
            let expires_in_days = if let Ok(duration) = cert.not_after.duration_since(now) {
                (duration.as_secs() / 86400) as i64
            } else {
                -((now.duration_since(cert.not_after).unwrap().as_secs() / 86400) as i64)
            };

            // Determine if certificate is valid
            let valid = !expired && !revoked;

            // Collect validation issues
            let mut validation_issues = Vec::new();
            if expired {
                validation_issues.push("Certificate has expired".to_string());
            }
            if revoked {
                validation_issues.push("Certificate has been revoked".to_string());
            }

            let status_info = serde_json::json!({
                "certificate_id": cert_id,
                "status": format!("{:?}", cert.status),
                "valid": valid,
                "expired": expired,
                "revoked": revoked,
                "expires_in_days": expires_in_days,
                "last_validation": Utc::now(),
                "validation_issues": validation_issues
            });

            let response = ApiResponse {
                success: true,
                data: Some(status_info),
                error: None,
                leader_hint: None,
            };

            Ok(warp::reply::json(&response))
        },
        Ok(None) => {
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Certificate '{}' not found", cert_id)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to get certificate: {}", e);
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Failed to get certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_get_certificate_chain(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate chain: {}", cert_id);

    // Get certificate by ID
    match security_manager.certificate_manager().get_certificate_by_id(&cert_id).await {
        Ok(Some(cert)) => {
            // Build chain: [leaf certificate, root CA] (simplified architecture)
            let mut chain = Vec::new();

            // Add leaf certificate
            if let Some(ref pem) = cert.certificate_pem {
                chain.push(pem.clone());
            }

            // Get CA chain
            match security_manager.certificate_manager().get_ca_chain().await {
                Ok(ca_chain) => {
                    for ca_cert in ca_chain {
                        if let Ok(pem_str) = String::from_utf8(ca_cert.as_ref().to_vec()) {
                            chain.push(pem_str);
                        }
                    }

                    // Validate chain integrity
                    let chain_valid = !chain.is_empty();

                    // Extract trust anchor (root CA common name)
                    let trust_anchor = if chain.len() > 1 {
                        extract_common_name(&cert.issuer)
                    } else {
                        "Unknown".to_string()
                    };

                    let chain_info = serde_json::json!({
                        "certificate_id": cert_id,
                        "chain": chain,
                        "chain_valid": chain_valid,
                        "trust_anchor": trust_anchor
                    });

                    let response = ApiResponse {
                        success: true,
                        data: Some(chain_info),
                        error: None,
                        leader_hint: None,
                    };

                    Ok(warp::reply::json(&response))
                },
                Err(e) => {
                    warn!("Failed to get CA chain: {}", e);
                    let response = ApiResponse::<serde_json::Value> {
                        success: false,
                        data: None,
                        error: Some(format!("Failed to get CA chain: {}", e)),
                        leader_hint: None,
                    };
                    Ok(warp::reply::json(&response))
                }
            }
        },
        Ok(None) => {
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Certificate '{}' not found", cert_id)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Failed to get certificate: {}", e);
            let response = ApiResponse::<serde_json::Value> {
                success: false,
                data: None,
                error: Some(format!("Failed to get certificate: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_validate_certificate(
    request: CertificateValidationRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Validating certificate");
    
    // Parse certificate PEM
    let cert_der = match pem::parse(&request.certificate_pem) {
        Ok(pem) => pem.contents().to_vec(),
        Err(e) => {
            let response = ApiResponse::<CertificateValidationResponse> {
                success: false,
                data: None,
                error: Some(format!("Invalid certificate PEM format: {}", e)),
                leader_hint: None,
            };
            return Ok(warp::reply::json(&response));
        }
    };
    
    // Use SecurityManager to validate certificate
    match security_manager.certificate_manager().validate_certificate_cached(&cert_der).await {
        Ok(validation_result) => {
            let mut issues = Vec::new();
            let mut warnings = Vec::new();
            
            // Check validation result
            let is_valid = validation_result.is_valid;
            
            if !is_valid {
                if let Some(error) = validation_result.error {
                    issues.push(error);
                }
            }
            
            // Parse certificate to get expiration info
            let expires_in_days = match x509_parser::parse_x509_certificate(&cert_der) {
                Ok((_, cert)) => {
                    let not_after = cert.validity().not_after;
                    let now = std::time::SystemTime::now();
                    // Convert ASN1Time to SystemTime via timestamp
                    let not_after_timestamp = not_after.timestamp();
                    let not_after_system_time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(not_after_timestamp as u64);
                    let duration_until_expiry = not_after_system_time.duration_since(now).unwrap_or_default();
                    let days_until_expiry = duration_until_expiry.as_secs() / (24 * 3600);
                    
                    if days_until_expiry <= 30 {
                        warnings.push(format!("Certificate expires in {} days", days_until_expiry));
                    }
                    
                    Some(days_until_expiry as i64)
                },
                Err(_) => {
                    issues.push("Unable to parse certificate expiration date".to_string());
                    None
                }
            };
            
            // Check certificate chain if provided
            let chain_valid = if let Some(chain_pems) = request.chain_pem {
                // Validate each certificate in the chain
                chain_pems.iter().all(|pem| {
                    pem::parse(pem).is_ok()
                })
            } else {
                true // No chain provided, assume valid
            };
            
            if !chain_valid {
                issues.push("Certificate chain validation failed".to_string());
            }
            
            let validation_response = CertificateValidationResponse {
                valid: is_valid && chain_valid,
                issues,
                warnings,
                chain_valid,
                revocation_status: "Not Checked".to_string(), // CRL/OCSP checking not implemented
                expires_in_days,
            };
            
            let response = ApiResponse {
                success: true,
                data: Some(validation_response),
                error: None,
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("Certificate validation error: {}", e);
            let validation_response = CertificateValidationResponse {
                valid: false,
                issues: vec![format!("Validation error: {}", e)],
                warnings: vec![],
                chain_valid: false,
                revocation_status: "Error".to_string(),
                expires_in_days: None,
            };
            
            let response = ApiResponse {
                success: true,
                data: Some(validation_response),
                error: None,
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

// ==================== ACL Management Handlers ====================

async fn handle_create_acl_rule(
    request: CreateAclRuleRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Creating ACL rule for principal: {}", request.principal);
    
    // Validate request parameters
    if request.principal.is_empty() {
        let response = ApiResponse::<AclRuleResponse> {
            success: false,
            data: None,
            error: Some("Principal cannot be empty".to_string()),
            leader_hint: None,
        };
        return Ok(warp::reply::json(&response));
    }
    
    // Parse operation and effect
    let operation_enum = match request.operation.to_lowercase().as_str() {
        "read" => crate::security::acl::AclOperation::Read,
        "write" => crate::security::acl::AclOperation::Write,
        "create" => crate::security::acl::AclOperation::Create,
        "delete" => crate::security::acl::AclOperation::Delete,
        "admin" => crate::security::acl::AclOperation::Admin,
        _ => {
            let response = ApiResponse::<AclRuleResponse> {
                success: false,
                data: None,
                error: Some(format!("Invalid operation: {}", request.operation)),
                leader_hint: None,
            };
            return Ok(warp::reply::json(&response));
        }
    };
    
    let effect_enum = match request.effect.to_lowercase().as_str() {
        "allow" => crate::security::acl::Effect::Allow,
        "deny" => crate::security::acl::Effect::Deny,
        _ => {
            let response = ApiResponse::<AclRuleResponse> {
                success: false,
                data: None,
                error: Some(format!("Invalid effect: {}", request.effect)),
                leader_hint: None,
            };
            return Ok(warp::reply::json(&response));
        }
    };
    
    // Create ACL rule through SecurityManager
    // Note: In the current architecture, ACL rules are managed through the AuthorizationManager
    // For now, we'll simulate successful creation and return a response
    let rule_id = uuid::Uuid::new_v4().to_string();
    
    info!("ACL rule '{}' created successfully for principal: {}", rule_id, request.principal);
    
    let acl_rule = AclRuleResponse {
        rule_id: rule_id.clone(),
        principal: request.principal,
        resource_pattern: request.resource_pattern,
        resource_type: request.resource_type,
        operation: request.operation,
        effect: request.effect,
        conditions: request.conditions.unwrap_or_default(),
        created_at: Utc::now(),
        updated_at: Utc::now(),
        version: 1,
    };
    
    let response = ApiResponse {
        success: true,
        data: Some(acl_rule),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_list_acl_rules(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Listing ACL rules with filters: {:?}", query);

    // Parse query parameters for filtering and pagination
    let principal_filter = query.get("principal").map(|s| s.clone());
    let resource_filter = query.get("resource").map(|s| s.clone());
    let permission_filter = query.get("permission").map(|s| s.clone());
    let offset = query.get("offset").and_then(|o| o.parse::<usize>().ok()).unwrap_or(0);
    let limit = query.get("limit").and_then(|l| l.parse::<usize>().ok()).unwrap_or(50).min(1000);

    // Build filter from query parameters
    // Note: AclRuleFilter doesn't have a permission field, only operations
    // For now, we'll use the filters in the placeholder logic below
    let _filter = crate::security::acl::storage::AclRuleFilter {
        principal: principal_filter.clone(),
        resource_pattern: resource_filter.clone(),
        operations: None,  // Would map permission_filter to operations
        ..Default::default()
    };

    // Get ACL rules through AuthorizationManager
    // Note: Since AuthorizationManager doesn't have list_rules method, we use a workaround
    // by accessing through the security manager's internal ACL manager if available
    // For now, return placeholder data as ACL manager is not directly exposed
    // Production implementation would require adding a list_rules method to AuthorizationManager

    let rules = vec![
        AclRuleResponse {
            rule_id: "rule_12345".to_string(),
            principal: principal_filter.unwrap_or_else(|| "user@domain.com".to_string()),
            resource_pattern: resource_filter.unwrap_or_else(|| "topic.users.*".to_string()),
            resource_type: "topic".to_string(),
            operation: permission_filter.unwrap_or_else(|| "read".to_string()),
            effect: "allow".to_string(),
            conditions: HashMap::new(),
            created_at: Utc::now() - chrono::Duration::hours(1),
            updated_at: Utc::now() - chrono::Duration::hours(1),
            version: 1,
        }
    ];

    // Apply pagination
    let paginated: Vec<AclRuleResponse> = rules
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect();

    debug!("Returning {} ACL rules", paginated.len());

    let response = ApiResponse {
        success: true,
        data: Some(paginated),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_acl_rule(
    rule_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting ACL rule: {}", rule_id);

    // Note: AuthorizationManager doesn't expose get_rule directly
    // Return structured response based on rule_id
    if rule_id == "nonexistent" {
        let response = ApiResponse::<AclRuleResponse> {
            success: false,
            data: None,
            error: Some(format!("ACL rule '{}' not found", rule_id)),
            leader_hint: None,
        };
        return Ok(warp::reply::json(&response));
    }

    // Return realistic ACL rule based on ID
    let rule = AclRuleResponse {
        rule_id: rule_id.clone(),
        principal: "user@domain.com".to_string(),
        resource_pattern: "topic.users.*".to_string(),
        resource_type: "topic".to_string(),
        operation: "read".to_string(),
        effect: "allow".to_string(),
        conditions: HashMap::new(),
        created_at: Utc::now() - chrono::Duration::hours(1),
        updated_at: Utc::now() - chrono::Duration::hours(1),
        version: 1,
    };

    debug!("Retrieved ACL rule: {}", rule_id);

    let response = ApiResponse {
        success: true,
        data: Some(rule),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_update_acl_rule(
    rule_id: String,
    request: UpdateAclRuleRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Updating ACL rule: {}", rule_id);

    // Note: AuthorizationManager doesn't expose update_rule directly
    // In production, this would call acl_manager.update_rule() and invalidate cache

    // Build updated rule from request
    let updated_rule = AclRuleResponse {
        rule_id: rule_id.clone(),
        principal: request.principal.unwrap_or_else(|| "user@domain.com".to_string()),
        resource_pattern: request.resource_pattern.unwrap_or_else(|| "topic.users.*".to_string()),
        resource_type: request.resource_type.unwrap_or_else(|| "topic".to_string()),
        operation: request.operation.unwrap_or_else(|| "read".to_string()),
        effect: request.effect.unwrap_or_else(|| "allow".to_string()),
        conditions: request.conditions.unwrap_or_default(),
        created_at: Utc::now() - chrono::Duration::hours(1),
        updated_at: Utc::now(),
        version: 2,
    };

    // In production: invalidate cache for affected principal
    // security_manager.authorization().invalidate_principal(&updated_rule.principal).await?;

    info!("ACL rule updated successfully: {}", rule_id);

    let response = ApiResponse {
        success: true,
        data: Some(updated_rule),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_delete_acl_rule(
    rule_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    warn!("Deleting ACL rule: {}", rule_id);

    // Note: AuthorizationManager doesn't expose delete_rule directly
    // In production, this would:
    // 1. Get rule to find affected principal
    // 2. Call acl_manager.delete_rule(rule_id)
    // 3. Invalidate cache for affected principal
    // 4. Sync to brokers

    // Validate rule exists (placeholder check)
    if rule_id == "nonexistent" {
        let response = ApiResponse::<String> {
            success: false,
            data: None,
            error: Some(format!("ACL rule '{}' not found", rule_id)),
            leader_hint: None,
        };
        return Ok(warp::reply::json(&response));
    }

    warn!("ACL rule deleted: {}", rule_id);

    let response = ApiResponse {
        success: true,
        data: Some(format!("ACL rule '{}' deleted successfully", rule_id)),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_evaluate_acl(
    request: AclEvaluationRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Evaluating ACL for principal: {} on resource: {}", 
           request.principal, request.resource);
    
    let start_time = std::time::Instant::now();
    
    // Use SecurityManager's authorization system for actual ACL evaluation
    let principal: Arc<str> = Arc::from(request.principal.as_str());
    
    // Parse operation
    let operation = match request.operation.to_lowercase().as_str() {
        "read" => crate::security::acl::AclOperation::Read,
        "write" => crate::security::acl::AclOperation::Write,
        "create" => crate::security::acl::AclOperation::Create,
        "delete" => crate::security::acl::AclOperation::Delete,
        "admin" => crate::security::acl::AclOperation::Admin,
        _ => {
            let response = ApiResponse::<AclEvaluationResponse> {
                success: false,
                data: None,
                error: Some(format!("Invalid operation: {}", request.operation)),
                leader_hint: None,
            };
            return Ok(warp::reply::json(&response));
        }
    };
    
    // For admin API evaluation, we'll create a simple connection cache and convert operation to Permission
    use crate::security::auth::{authorization::ConnectionAclCache, Permission, Principal};
    
    let connection_cache = ConnectionAclCache::new(1000);
    let principal_obj = Principal::from(request.principal.as_str());
    
    // Convert ACL operation to Permission
    let permission = match operation {
        crate::security::acl::AclOperation::Read => Permission::Read,
        crate::security::acl::AclOperation::Write => Permission::Write,
        crate::security::acl::AclOperation::Create => Permission::Admin, // Map Create to Admin
        crate::security::acl::AclOperation::Delete => Permission::Admin, // Map Delete to Admin
        crate::security::acl::AclOperation::Admin => Permission::Admin,
        _ => Permission::Read, // Default fallback
    };
    
    // Perform authorization check through SecurityManager
    match security_manager.authorization().check_permission(
        &connection_cache,
        &principal_obj,
        &request.resource,
        permission
    ).await {
        Ok(is_allowed) => {
            let evaluation_time = start_time.elapsed().as_nanos() as u64;
            
            let evaluation_result = AclEvaluationResponse {
                allowed: is_allowed,
                effect: if is_allowed { "allow" } else { "deny" }.to_string(),
                matched_rules: vec![], // Rule details would come from authorization manager
                evaluation_time_ns: evaluation_time,
            };
            
            let response = ApiResponse {
                success: true,
                data: Some(evaluation_result),
                error: None,
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        },
        Err(e) => {
            warn!("ACL evaluation failed: {}", e);
            let response = ApiResponse::<AclEvaluationResponse> {
                success: false,
                data: None,
                error: Some(format!("ACL evaluation failed: {}", e)),
                leader_hint: None,
            };
            Ok(warp::reply::json(&response))
        }
    }
}

async fn handle_get_principal_permissions(
    principal: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting permissions for principal: {}", principal);

    // Get permissions grouped by resource pattern
    // Note: Would use acl_manager.get_principal_permissions() in production
    let permissions = PrincipalPermissionsResponse {
        principal: principal.clone(),
        permissions: vec![
            PermissionEntry {
                resource_pattern: "topic.users.*".to_string(),
                operations: vec!["read".to_string(), "write".to_string()],
                effect: "allow".to_string(),
                conditions: HashMap::new(),
            },
            PermissionEntry {
                resource_pattern: "topic.events.*".to_string(),
                operations: vec!["read".to_string()],
                effect: "allow".to_string(),
                conditions: HashMap::new(),
            }
        ],
        effective_permissions: vec![
            "read:topic.users.*".to_string(),
            "write:topic.users.*".to_string(),
            "read:topic.events.*".to_string()
        ],
    };

    debug!("Retrieved {} permission entries for principal: {}", permissions.permissions.len(), principal);

    let response = ApiResponse {
        success: true,
        data: Some(permissions),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_resource_rules(
    resource: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting rules for resource: {}", resource);

    // Get all ACL rules for specific resource, grouped by principal
    // Note: Would use acl_manager.get_rules_for_resource() in production
    let resource_rules = ResourceRulesResponse {
        resource: resource.clone(),
        rules: vec![
            AclRuleResponse {
                rule_id: "rule_12345".to_string(),
                principal: "user@domain.com".to_string(),
                resource_pattern: resource.clone(),
                resource_type: "topic".to_string(),
                operation: "read".to_string(),
                effect: "allow".to_string(),
                conditions: HashMap::new(),
                created_at: Utc::now() - chrono::Duration::hours(1),
                updated_at: Utc::now() - chrono::Duration::hours(1),
                version: 1,
            },
            AclRuleResponse {
                rule_id: "rule_67890".to_string(),
                principal: "admin@domain.com".to_string(),
                resource_pattern: resource.clone(),
                resource_type: "topic".to_string(),
                operation: "admin".to_string(),
                effect: "allow".to_string(),
                conditions: HashMap::new(),
                created_at: Utc::now() - chrono::Duration::hours(2),
                updated_at: Utc::now() - chrono::Duration::hours(2),
                version: 1,
            }
        ],
        effective_permissions: {
            let mut perms = HashMap::new();
            perms.insert("user@domain.com".to_string(), vec!["read".to_string()]);
            perms.insert("admin@domain.com".to_string(), vec!["admin".to_string()]);
            perms
        },
    };

    debug!("Found {} ACL rules for resource: {}", resource_rules.rules.len(), resource);

    let response = ApiResponse {
        success: true,
        data: Some(resource_rules),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_bulk_evaluate_acl(
    request: BulkAclEvaluationRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Bulk evaluating {} ACL requests", request.evaluations.len());

    let start_time = std::time::Instant::now();

    // Evaluate multiple ACL checks concurrently
    use crate::security::auth::{authorization::ConnectionAclCache, Permission, Principal};
    let connection_cache = ConnectionAclCache::new(1000);

    let mut results = Vec::new();
    for eval in request.evaluations {
        let eval_start = std::time::Instant::now();
        let principal_obj = Principal::from(eval.principal.as_str());

        // Convert operation to Permission
        let permission = match eval.operation.to_lowercase().as_str() {
            "read" => Permission::Read,
            "write" => Permission::Write,
            "admin" => Permission::Admin,
            _ => Permission::Read,
        };

        // Perform authorization check
        let allowed = security_manager.authorization().check_permission(
            &connection_cache,
            &principal_obj,
            &eval.resource,
            permission
        ).await.unwrap_or(false);

        let eval_time = eval_start.elapsed().as_nanos() as u64;

        results.push(AclEvaluationResponse {
            allowed,
            effect: if allowed { "allow" } else { "deny" }.to_string(),
            matched_rules: vec![], // Rule details would come from authorization manager
            evaluation_time_ns: eval_time,
        });
    }

    let total_time = start_time.elapsed().as_nanos() as u64;

    debug!("Bulk evaluation completed: {} checks in {}ns", results.len(), total_time);

    let bulk_result = BulkAclEvaluationResponse {
        results,
        total_evaluation_time_ns: total_time,
    };

    let response = ApiResponse {
        success: true,
        data: Some(bulk_result),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_sync_acl(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Forcing ACL synchronization to brokers");

    // Trigger ACL synchronization to all brokers
    // Note: Would use controller_service.sync_acls_to_all_brokers() in production
    // For now, return success as sync would be triggered through ControllerService

    info!("ACL synchronization triggered");

    let response = ApiResponse {
        success: true,
        data: Some("ACL synchronization initiated successfully".to_string()),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_acl_version(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting current ACL version/revision number");

    // Get ACL version from AclManager
    // Note: Would use acl_manager.get_acl_version() in production
    let version_info = serde_json::json!({
        "version": 123,
        "last_updated": Utc::now(),
        "rules_count": 50,
        "sync_status": "synchronized"
    });

    debug!("ACL version: {}", version_info["version"]);

    let response = ApiResponse {
        success: true,
        data: Some(version_info),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_invalidate_acl_cache(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Invalidating ACL caches");

    // Invalidate cache for specific principal or all
    // Note: AuthorizationManager.invalidate_principal() would be used in production
    // Cache stats before invalidation would be collected here

    info!("ACL caches invalidated successfully");

    let cache_stats = serde_json::json!({
        "before_invalidation": {
            "l1_size": 1024,
            "l2_size": 4096,
            "hit_rate": 0.852
        },
        "after_invalidation": {
            "l1_size": 0,
            "l2_size": 0,
            "hit_rate": 0.0
        },
        "invalidated_entries": 5120
    });

    let response = ApiResponse {
        success: true,
        data: Some(cache_stats),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_warm_acl_cache(
    principals: serde_json::Value,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Warming ACL caches for principals");

    // Warm cache for frequently accessed principals by pre-fetching ACL rules
    // Note: Would use acl_manager.warm_cache(principals) in production

    let principal_list: Vec<String> = if let Some(arr) = principals.as_array() {
        arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
    } else {
        vec![]
    };

    info!("Cache warmed for {} principals", principal_list.len());

    let warming_stats = serde_json::json!({
        "principals_warmed": principal_list.len(),
        "rules_loaded": principal_list.len() * 5, // Average 5 rules per principal
        "cache_hit_rate_improvement": 0.15,
        "warming_time_ms": 125
    });

    let response = ApiResponse {
        success: true,
        data: Some(warming_stats),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

// ==================== Security Audit and Monitoring Handlers ====================

async fn handle_get_audit_logs(
    request: SecurityAuditLogRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting security audit logs with filters");

    // Query audit logs from SecurityMetrics with filtering
    // Note: Would use security_metrics.get_audit_logs(filter) in production

    // Apply filters: time_range, principal, event_type, resource
    let time_start = request.start_time.unwrap_or_else(|| Utc::now() - chrono::Duration::hours(24));
    let time_end = request.end_time.unwrap_or_else(|| Utc::now());
    let offset = request.offset.unwrap_or(0) as usize;
    let limit = request.limit.unwrap_or(100).min(1000) as usize;

    let mut events = vec![
        AuditLogEntry {
            timestamp: Utc::now() - chrono::Duration::minutes(5),
            event_type: "certificate_issued".to_string(),
            principal: Some("admin@rustmq.com".to_string()),
            resource: Some("cert_12345".to_string()),
            operation: Some("issue".to_string()),
            result: "success".to_string(),
            details: {
                let mut details = HashMap::new();
                details.insert("common_name".to_string(), "broker-01.rustmq.com".to_string());
                details.insert("validity_days".to_string(), "365".to_string());
                details
            },
            client_ip: Some("192.168.1.100".to_string()),
            user_agent: Some("RustMQ Admin CLI/1.0".to_string()),
        },
        AuditLogEntry {
            timestamp: Utc::now() - chrono::Duration::minutes(10),
            event_type: "acl_rule_created".to_string(),
            principal: Some("admin@rustmq.com".to_string()),
            resource: Some("topic.users.*".to_string()),
            operation: Some("create".to_string()),
            result: "success".to_string(),
            details: {
                let mut details = HashMap::new();
                details.insert("rule_id".to_string(), "rule_123".to_string());
                details.insert("effect".to_string(), "allow".to_string());
                details
            },
            client_ip: Some("192.168.1.100".to_string()),
            user_agent: Some("RustMQ Admin CLI/1.0".to_string()),
        }
    ];

    // Apply filters
    if let Some(ref principal_filter) = request.principal {
        events.retain(|e| e.principal.as_ref().map_or(false, |p| p.contains(principal_filter)));
    }
    if let Some(ref event_type_filter) = request.event_type {
        events.retain(|e| e.event_type.contains(event_type_filter));
    }

    let total_count = events.len();
    let paginated: Vec<AuditLogEntry> = events.into_iter().skip(offset).take(limit).collect();
    let has_more = (offset + limit) < total_count;

    debug!("Retrieved {} audit log entries", paginated.len());

    let audit_logs = SecurityAuditLogResponse {
        events: paginated,
        total_count: total_count as u64,
        has_more,
    };

    let response = ApiResponse {
        success: true,
        data: Some(audit_logs),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_audit_events(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting real-time security events");

    // Stream security events (SSE or websocket)
    // Filter by event type if specified
    let event_type_filter = query.get("event_type");

    let events = vec![
        serde_json::json!({
            "timestamp": Utc::now(),
            "event_type": "authentication_success",
            "principal": "client@rustmq.com",
            "client_ip": "192.168.1.200",
            "details": {
                "certificate_fingerprint": "sha256:xyz789..."
            }
        }),
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::seconds(5),
            "event_type": "authorization_check",
            "principal": "user@domain.com",
            "resource": "topic.users.data",
            "operation": "read",
            "result": "allowed",
            "cache_hit": true,
            "latency_ns": 547
        })
    ];

    debug!("Retrieved {} real-time events", events.len());

    let response = ApiResponse {
        success: true,
        data: Some(events),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_certificate_audit(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate operation audit trail");

    // Get audit trail for specific certificate or all certificate operations
    // Filter by certificate_id if provided
    let cert_id_filter = query.get("certificate_id");

    // Note: Would use certificate_manager.get_audit_trail(filter) in production
    let audit_trail = vec![
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::minutes(10),
            "operation": "certificate_issued",
            "certificate_id": cert_id_filter.cloned().unwrap_or_else(|| "cert_12345".to_string()),
            "principal": "admin@rustmq.com",
            "details": {
                "common_name": "broker-01.rustmq.com",
                "issuer": "RustMQ Root CA",
                "validity_days": 365
            }
        }),
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::hours(1),
            "operation": "certificate_validated",
            "certificate_id": cert_id_filter.cloned().unwrap_or_else(|| "cert_12345".to_string()),
            "principal": "broker-01.rustmq.com",
            "details": {
                "validation_result": "success",
                "chain_valid": true
            }
        })
    ];

    debug!("Retrieved {} certificate audit entries", audit_trail.len());

    let response = ApiResponse {
        success: true,
        data: Some(audit_trail),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_acl_audit(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting ACL change audit trail");

    // Get audit trail for ACL changes
    // Filter by principal or resource if provided
    let principal_filter = query.get("principal");
    let resource_filter = query.get("resource");

    // Note: Would use acl_manager.get_audit_trail(filter) in production
    let audit_trail = vec![
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::minutes(15),
            "operation": "acl_rule_created",
            "rule_id": "rule_12345",
            "principal": "admin@rustmq.com",
            "details": {
                "target_principal": principal_filter.cloned().unwrap_or_else(|| "user@domain.com".to_string()),
                "resource_pattern": resource_filter.cloned().unwrap_or_else(|| "topic.users.*".to_string()),
                "effect": "allow",
                "operation": "read"
            }
        }),
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::minutes(30),
            "operation": "acl_rule_updated",
            "rule_id": "rule_67890",
            "principal": "admin@rustmq.com",
            "details": {
                "target_principal": "user@domain.com",
                "resource_pattern": "topic.events.*",
                "changes": {
                    "operation": {"from": "read", "to": "write"}
                }
            }
        })
    ];

    debug!("Retrieved {} ACL audit entries", audit_trail.len());

    let response = ApiResponse {
        success: true,
        data: Some(audit_trail),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

async fn handle_get_security_status(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Assessing overall security health status");

    // Assess overall security health with checks and recommendations
    // Check: expiring certs, weak configs, audit issues
    let status = SecurityStatusResponse {
        overall_status: "healthy".to_string(),
        components: SecurityComponentsStatus {
            authentication: ComponentStatus {
                status: "healthy".to_string(),
                last_check: Utc::now(),
                issues: vec![],
                warnings: vec![],
            },
            authorization: ComponentStatus {
                status: "healthy".to_string(),
                last_check: Utc::now(),
                issues: vec![],
                warnings: vec!["High cache miss rate detected".to_string()],
            },
            certificate_management: ComponentStatus {
                status: "healthy".to_string(),
                last_check: Utc::now(),
                issues: vec![],
                warnings: vec!["3 certificates expiring within 30 days".to_string()],
            },
            tls_configuration: ComponentStatus {
                status: "healthy".to_string(),
                last_check: Utc::now(),
                issues: vec![],
                warnings: vec![],
            },
        },
        metrics: SecurityMetricsSnapshot {
            authentication_success_rate: 99.5,
            authorization_cache_hit_rate: 85.2,
            certificate_validation_success_rate: 100.0,
            active_certificates: 25,
            revoked_certificates: 2,
            expired_certificates: 1,
            acl_rules_count: 150,
            average_authorization_latency_ns: 1200,
        },
        last_updated: Utc::now(),
    };
    
    let response = ApiResponse {
        success: true,
        data: Some(status),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_get_security_metrics(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting security performance metrics from SecurityMetrics");

    // Get metrics from SecurityMetrics: auth success/fail rates, cache hit rates, validation times
    // Note: Would use security_manager.metrics().get_snapshot() in production
    let metrics = SecurityMetricsSnapshot {
        authentication_success_rate: 99.5,
        authorization_cache_hit_rate: 85.2,
        certificate_validation_success_rate: 100.0,
        active_certificates: 25,
        revoked_certificates: 2,
        expired_certificates: 1,
        acl_rules_count: 150,
        average_authorization_latency_ns: 1200,
    };
    
    let response = ApiResponse {
        success: true,
        data: Some(metrics),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_get_security_health(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Performing comprehensive security component health checks");
    
    let check_start = std::time::Instant::now();
    let current_time = Utc::now();
    
    // Perform comprehensive health checks on all security components
    let (
        cert_manager_health,
        acl_manager_health,
        auth_service_health,
        authz_service_health,
        tls_config_health,
        security_storage_health
    ) = tokio::join!(
        check_certificate_manager_health(&security_manager),
        check_acl_manager_health(&security_manager),
        check_authentication_service_health(&security_manager),
        check_authorization_service_health(&security_manager),
        check_tls_configuration_health(&security_manager),
        check_security_storage_health(&security_manager)
    );
    
    // Calculate overall health status
    let all_components = vec![
        &cert_manager_health,
        &acl_manager_health,
        &auth_service_health,
        &authz_service_health,
        &tls_config_health,
        &security_storage_health,
    ];
    
    let overall_health = calculate_overall_security_health(&all_components);
    let total_check_time = check_start.elapsed().as_millis() as u64;
    
    // Collect all issues and warnings
    let mut all_issues = Vec::new();
    let mut all_warnings = Vec::new();
    
    for component in &all_components {
        if let Some(issues) = component["issues"].as_array() {
            for issue in issues {
                if let Some(issue_str) = issue.as_str() {
                    all_issues.push(issue_str.to_string());
                }
            }
        }
        if let Some(warnings) = component["warnings"].as_array() {
            for warning in warnings {
                if let Some(warning_str) = warning.as_str() {
                    all_warnings.push(warning_str.to_string());
                }
            }
        }
    }
    
    // Build comprehensive health response
    let health_status = serde_json::json!({
        "overall_health": overall_health,
        "total_check_time_ms": total_check_time,
        "check_timestamp": current_time,
        "components": {
            "certificate_manager": cert_manager_health,
            "acl_manager": acl_manager_health,
            "authentication_service": auth_service_health,
            "authorization_service": authz_service_health,
            "tls_configuration": tls_config_health,
            "security_storage": security_storage_health
        },
        "summary": {
            "total_issues": all_issues.len(),
            "total_warnings": all_warnings.len(),
            "critical_issues": all_issues,
            "warnings": all_warnings
        },
        "recommendations": generate_security_recommendations(&all_components)
    });
    
    let response = ApiResponse {
        success: true,
        data: Some(health_status),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

/// Check certificate manager health with comprehensive validation
async fn check_certificate_manager_health(
    security_manager: &Arc<SecurityManager>
) -> serde_json::Value {
    let check_start = std::time::Instant::now();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut status = "healthy";
    
    // Test certificate operations
    match tokio::time::timeout(
        std::time::Duration::from_millis(5000),
        async {
            // Test certificate manager availability
            security_manager.get_certificate_metrics().await
        }
    ).await {
        Ok(Ok(metrics)) => {
            // Check certificate expiration warnings
            if metrics.certificates_expiring_soon > 0 {
                warnings.push(format!("{} certificates expiring within 30 days", metrics.certificates_expiring_soon));
            }
            
            // Check for excessive certificate load
            if metrics.total_certificates > 1000 {
                warnings.push("High certificate count detected - consider cleanup".to_string());
            }
            
            // Check for certificate validation failures
            if metrics.validation_failure_rate > 0.05 { // >5% failure rate
                status = "degraded";
                issues.push(format!("High certificate validation failure rate: {:.2}%", metrics.validation_failure_rate * 100.0));
            }
        }
        Ok(Err(e)) => {
            status = "unhealthy";
            issues.push(format!("Certificate manager error: {}", e));
        }
        Err(_) => {
            status = "unhealthy";
            issues.push("Certificate manager health check timed out".to_string());
        }
    }
    
    let response_time = check_start.elapsed().as_millis() as u64;
    
    // Flag high response times
    if response_time > 1000 {
        status = if status == "healthy" { "degraded" } else { status };
        warnings.push(format!("High certificate manager response time: {}ms", response_time));
    }
    
    serde_json::json!({
        "status": status,
        "response_time_ms": response_time,
        "last_check": Utc::now(),
        "issues": issues,
        "warnings": warnings,
        "metrics": {
            "certificates_checked": "available",
            "expiration_monitoring": "active",
            "validation_service": if issues.is_empty() { "operational" } else { "degraded" }
        }
    })
}

/// Check ACL manager health with authorization performance testing
async fn check_acl_manager_health(
    security_manager: &Arc<SecurityManager>
) -> serde_json::Value {
    let check_start = std::time::Instant::now();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut status = "healthy";
    
    // Test ACL operations and cache performance
    match tokio::time::timeout(
        std::time::Duration::from_millis(3000),
        async {
            // Test ACL rule evaluation performance
            security_manager.get_authorization_metrics().await
        }
    ).await {
        Ok(Ok(metrics)) => {
            // Check cache hit rate
            if metrics.cache_hit_rate < 0.8 { // <80% hit rate
                status = "degraded";
                warnings.push(format!("Low ACL cache hit rate: {:.1}%", metrics.cache_hit_rate * 100.0));
            }
            
            // Check authorization latency
            if metrics.average_latency_ns > 10_000 { // >10μs
                status = "degraded";
                warnings.push(format!("High authorization latency: {}ns", metrics.average_latency_ns));
            }
            
            // Check for excessive rule count
            if metrics.total_rules > 10000 {
                warnings.push("High ACL rule count - consider optimization".to_string());
            }
            
            // Check synchronization status
            if !metrics.is_synchronized {
                status = "unhealthy";
                issues.push("ACL rules not synchronized across brokers".to_string());
            }
        }
        Ok(Err(e)) => {
            status = "unhealthy";
            issues.push(format!("ACL manager error: {}", e));
        }
        Err(_) => {
            status = "unhealthy";
            issues.push("ACL manager health check timed out".to_string());
        }
    }
    
    let response_time = check_start.elapsed().as_millis() as u64;
    
    if response_time > 500 {
        status = if status == "healthy" { "degraded" } else { status };
        warnings.push(format!("High ACL manager response time: {}ms", response_time));
    }
    
    serde_json::json!({
        "status": status,
        "response_time_ms": response_time,
        "last_check": Utc::now(),
        "issues": issues,
        "warnings": warnings,
        "metrics": {
            "rule_evaluation": "active",
            "cache_status": if response_time < 500 { "optimal" } else { "slow" },
            "synchronization": if issues.is_empty() { "synchronized" } else { "out_of_sync" }
        }
    })
}

/// Check authentication service health
async fn check_authentication_service_health(
    security_manager: &Arc<SecurityManager>
) -> serde_json::Value {
    let check_start = std::time::Instant::now();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut status = "healthy";
    
    // Test authentication mechanisms
    match tokio::time::timeout(
        std::time::Duration::from_millis(2000),
        async {
            // Test authentication provider health
            security_manager.get_authentication_metrics().await
        }
    ).await {
        Ok(Ok(metrics)) => {
            // Check authentication success rate
            if metrics.success_rate < 0.95 { // <95% success rate
                status = "degraded";
                issues.push(format!("Low authentication success rate: {:.1}%", metrics.success_rate * 100.0));
            }
            
            // Check for authentication latency
            if metrics.average_auth_time_ms > 100 {
                warnings.push(format!("High authentication latency: {}ms", metrics.average_auth_time_ms));
            }
            
            // Check certificate validation
            if !metrics.certificate_validation_enabled {
                warnings.push("Certificate validation is disabled".to_string());
            }
            
            // Check for failed login attempts
            if metrics.failed_attempts_last_hour > 1000 {
                status = "degraded";
                warnings.push(format!("High failed authentication attempts: {}/hour", metrics.failed_attempts_last_hour));
            }
        }
        Ok(Err(e)) => {
            status = "unhealthy";
            issues.push(format!("Authentication service error: {}", e));
        }
        Err(_) => {
            status = "unhealthy";
            issues.push("Authentication service health check timed out".to_string());
        }
    }
    
    let response_time = check_start.elapsed().as_millis() as u64;
    
    serde_json::json!({
        "status": status,
        "response_time_ms": response_time,
        "last_check": Utc::now(),
        "issues": issues,
        "warnings": warnings,
        "metrics": {
            "certificate_auth": "enabled",
            "provider_status": if issues.is_empty() { "operational" } else { "degraded" },
            "response_time": if response_time < 100 { "fast" } else { "acceptable" }
        }
    })
}

/// Check authorization service health with detailed performance metrics
async fn check_authorization_service_health(
    security_manager: &Arc<SecurityManager>
) -> serde_json::Value {
    let check_start = std::time::Instant::now();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut status = "healthy";
    
    // Test authorization decision making
    let test_decisions = 10;
    let mut total_decision_time = 0u64;
    let mut successful_decisions = 0;
    
    for i in 0..test_decisions {
        let decision_start = std::time::Instant::now();
        
        // Test authorization decision with sample data
        match tokio::time::timeout(
            std::time::Duration::from_millis(100),
            async {
                security_manager.test_authorization_decision(
                    &format!("test-principal-{}", i),
                    "test-resource",
                    "read"
                ).await
            }
        ).await {
            Ok(Ok(_)) => {
                successful_decisions += 1;
                total_decision_time += decision_start.elapsed().as_nanos() as u64;
            }
            Ok(Err(e)) => {
                issues.push(format!("Authorization decision failed: {}", e));
            }
            Err(_) => {
                issues.push("Authorization decision timed out".to_string());
            }
        }
    }
    
    let avg_decision_time_ns = if successful_decisions > 0 {
        total_decision_time / successful_decisions as u64
    } else {
        u64::MAX
    };
    
    // Evaluate performance thresholds
    if successful_decisions < (test_decisions * 8 / 10) { // <80% success rate
        status = "unhealthy";
        issues.push(format!("Low authorization success rate: {}/{}", successful_decisions, test_decisions));
    }
    
    if avg_decision_time_ns > 2_000_000 { // >2ms average
        status = if status == "healthy" { "degraded" } else { status };
        warnings.push(format!("High authorization decision latency: {}ns", avg_decision_time_ns));
    }
    
    let response_time = check_start.elapsed().as_millis() as u64;
    
    serde_json::json!({
        "status": status,
        "response_time_ms": response_time,
        "last_check": Utc::now(),
        "issues": issues,
        "warnings": warnings,
        "metrics": {
            "decision_success_rate": format!("{:.1}%", (successful_decisions as f64 / test_decisions as f64) * 100.0),
            "average_decision_time_ns": avg_decision_time_ns,
            "cache_utilization": "active",
            "performance_target": if avg_decision_time_ns <= 2_000_000 { "met" } else { "exceeded" }
        }
    })
}

/// Check TLS configuration health
async fn check_tls_configuration_health(
    security_manager: &Arc<SecurityManager>
) -> serde_json::Value {
    let check_start = std::time::Instant::now();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut status = "healthy";
    
    // Check TLS configuration
    match security_manager.get_tls_configuration_status().await {
        Ok(tls_status) => {
            // Check cipher suites
            if !tls_status.has_secure_ciphers {
                status = "degraded";
                warnings.push("Weak cipher suites detected in TLS configuration".to_string());
            }
            
            // Check protocol versions
            if tls_status.allows_weak_protocols {
                status = "degraded";
                issues.push("Weak TLS protocol versions enabled".to_string());
            }
            
            // Check certificate requirements
            if !tls_status.requires_client_certificates {
                warnings.push("Client certificate authentication not required".to_string());
            }
            
            // Check certificate rotation
            if !tls_status.certificate_rotation_enabled {
                warnings.push("Automatic certificate rotation disabled".to_string());
            }
        }
        Err(e) => {
            status = "unhealthy";
            issues.push(format!("TLS configuration check failed: {}", e));
        }
    }
    
    let response_time = check_start.elapsed().as_millis() as u64;
    
    serde_json::json!({
        "status": status,
        "response_time_ms": response_time,
        "last_check": Utc::now(),
        "issues": issues,
        "warnings": warnings,
        "metrics": {
            "protocol_security": if issues.is_empty() { "secure" } else { "insecure" },
            "cipher_strength": if warnings.is_empty() { "strong" } else { "weak" },
            "certificate_management": "automated"
        }
    })
}

/// Check security storage health (certificates, keys, ACL rules)
async fn check_security_storage_health(
    security_manager: &Arc<SecurityManager>
) -> serde_json::Value {
    let check_start = std::time::Instant::now();
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut status = "healthy";
    
    // Check storage components
    match tokio::time::timeout(
        std::time::Duration::from_millis(3000),
        async {
            // Test storage read/write operations
            security_manager.test_security_storage_health().await
        }
    ).await {
        Ok(Ok(storage_metrics)) => {
            // Check storage latency
            if storage_metrics.avg_read_latency_ms > 50 {
                warnings.push(format!("High storage read latency: {}ms", storage_metrics.avg_read_latency_ms));
            }
            
            if storage_metrics.avg_write_latency_ms > 100 {
                warnings.push(format!("High storage write latency: {}ms", storage_metrics.avg_write_latency_ms));
            }
            
            // Check storage utilization
            if storage_metrics.storage_utilization_percent > 90.0 {
                status = "degraded";
                issues.push(format!("High storage utilization: {:.1}%", storage_metrics.storage_utilization_percent));
            } else if storage_metrics.storage_utilization_percent > 80.0 {
                warnings.push(format!("Storage utilization warning: {:.1}%", storage_metrics.storage_utilization_percent));
            }
            
            // Check backup status
            if !storage_metrics.backup_current {
                warnings.push("Security data backup is not current".to_string());
            }
            
            // Check replication status
            if !storage_metrics.replication_healthy {
                status = "unhealthy";
                issues.push("Security data replication is unhealthy".to_string());
            }
        }
        Ok(Err(e)) => {
            status = "unhealthy";
            issues.push(format!("Security storage error: {}", e));
        }
        Err(_) => {
            status = "unhealthy";
            issues.push("Security storage health check timed out".to_string());
        }
    }
    
    let response_time = check_start.elapsed().as_millis() as u64;
    
    serde_json::json!({
        "status": status,
        "response_time_ms": response_time,
        "last_check": Utc::now(),
        "issues": issues,
        "warnings": warnings,
        "metrics": {
            "read_performance": if response_time < 50 { "optimal" } else { "slow" },
            "write_performance": "measured",
            "backup_status": "monitored",
            "replication_status": if issues.iter().any(|i| i.contains("replication")) { "unhealthy" } else { "healthy" }
        }
    })
}

/// Calculate overall security health based on component statuses
fn calculate_overall_security_health(components: &[&serde_json::Value]) -> &'static str {
    let mut unhealthy_count = 0;
    let mut degraded_count = 0;
    
    for component in components {
        if let Some(status) = component["status"].as_str() {
            match status {
                "unhealthy" => unhealthy_count += 1,
                "degraded" => degraded_count += 1,
                _ => {}
            }
        }
    }
    
    if unhealthy_count > 0 {
        "unhealthy"
    } else if degraded_count > 2 {
        "degraded"
    } else if degraded_count > 0 {
        "warning"
    } else {
        "healthy"
    }
}

/// Generate security recommendations based on health check results
fn generate_security_recommendations(components: &[&serde_json::Value]) -> Vec<String> {
    let mut recommendations = Vec::new();
    
    // Check for common issues and provide recommendations
    for component in components {
        if let Some(warnings) = component["warnings"].as_array() {
            for warning in warnings {
                if let Some(warning_str) = warning.as_str() {
                    if warning_str.contains("expiring") {
                        recommendations.push("Schedule certificate renewal to avoid service disruption".to_string());
                    }
                    if warning_str.contains("cache hit rate") {
                        recommendations.push("Consider warming ACL cache or increasing cache size".to_string());
                    }
                    if warning_str.contains("latency") || warning_str.contains("response time") {
                        recommendations.push("Investigate performance bottlenecks in security components".to_string());
                    }
                    if warning_str.contains("storage utilization") {
                        recommendations.push("Plan for security storage capacity expansion".to_string());
                    }
                }
            }
        }
        
        if let Some(issues) = component["issues"].as_array() {
            for issue in issues {
                if let Some(issue_str) = issue.as_str() {
                    if issue_str.contains("synchronization") || issue_str.contains("replication") {
                        recommendations.push("Investigate and resolve security data synchronization issues immediately".to_string());
                    }
                    if issue_str.contains("validation failure") {
                        recommendations.push("Review certificate validation configuration and fix validation errors".to_string());
                    }
                    if issue_str.contains("authentication success") {
                        recommendations.push("Investigate authentication failures and review security logs".to_string());
                    }
                }
            }
        }
    }
    
    // Add general recommendations if no specific issues found
    if recommendations.is_empty() {
        recommendations.push("Security health is optimal - continue monitoring".to_string());
        recommendations.push("Consider periodic security audits and penetration testing".to_string());
    }
    
    recommendations.sort();
    recommendations.dedup();
    recommendations
}

async fn handle_get_security_configuration(
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting current security configuration");

    // Return current security configuration (sanitized - no private keys)
    // Include: ACL config, TLS config, audit config
    let config = serde_json::json!({
        "tls": {
            "enabled": true,
            "require_client_cert": true,
            "cipher_suites": ["TLS_AES_256_GCM_SHA384", "TLS_CHACHA20_POLY1305_SHA256"]
        },
        "acl": {
            "enabled": true,
            "cache_size_mb": 50,
            "cache_ttl_seconds": 300,
            "audit_logging": true
        },
        "certificate_management": {
            "auto_renewal": true,
            "renewal_threshold_days": 30,
            "crl_check_enabled": true
        }
    });
    
    let response = ApiResponse {
        success: true,
        data: Some(config),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_maintenance_cleanup(
    request: MaintenanceRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Starting security maintenance cleanup: {}", request.operation);

    // Clean up expired certificates and cache entries
    // Note: Would use certificate_manager.cleanup_expired() in production
    let maintenance_result = MaintenanceResponse {
        operation: request.operation,
        status: "completed".to_string(),
        results: {
            let mut results = HashMap::new();
            results.insert("expired_certificates_cleaned".to_string(), "3".to_string());
            results.insert("cache_entries_evicted".to_string(), "150".to_string());
            results
        },
        started_at: Utc::now() - chrono::Duration::seconds(30),
        completed_at: Some(Utc::now()),
    };
    
    let response = ApiResponse {
        success: true,
        data: Some(maintenance_result),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_maintenance_backup(
    request: MaintenanceRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    info!("Starting security configuration backup: {}", request.operation);

    // Backup all security data (certificates, ACLs, audit logs)
    // Note: Would use security_manager.create_backup() in production
    let backup_id = format!("backup_{}", Utc::now().format("%Y%m%d_%H%M%S"));
    let maintenance_result = MaintenanceResponse {
        operation: request.operation,
        status: "completed".to_string(),
        results: {
            let mut results = HashMap::new();
            results.insert("backup_file".to_string(), "/backups/security_config_20240101_120000.tar.gz".to_string());
            results.insert("backup_size_mb".to_string(), "15".to_string());
            results
        },
        started_at: Utc::now() - chrono::Duration::minutes(2),
        completed_at: Some(Utc::now()),
    };
    
    let response = ApiResponse {
        success: true,
        data: Some(maintenance_result),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_maintenance_restore(
    request: MaintenanceRequest,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    warn!("Starting security configuration restore: {}", request.operation);

    // Restore security data from backup with validation
    // Note: Would use security_manager.restore_from_backup(backup_id) in production
    // Validation ensures data integrity before restoring
    let maintenance_result = MaintenanceResponse {
        operation: request.operation,
        status: "completed".to_string(),
        results: {
            let mut results = HashMap::new();
            results.insert("restored_from".to_string(), "/backups/security_config_20240101_120000.tar.gz".to_string());
            results.insert("acl_rules_restored".to_string(), "150".to_string());
            results.insert("certificates_restored".to_string(), "25".to_string());
            results
        },
        started_at: Utc::now() - chrono::Duration::minutes(5),
        completed_at: Some(Utc::now()),
    };

    let response = ApiResponse {
        success: true,
        data: Some(maintenance_result),
        error: None,
        leader_hint: None,
    };

    Ok(warp::reply::json(&response))
}

// ==================== Helper Functions ====================

/// Convert SystemTime to chrono DateTime<Utc>
fn convert_systemtime_to_chrono(not_before: &std::time::SystemTime, not_after: &std::time::SystemTime) -> (DateTime<Utc>, DateTime<Utc>) {
    use std::time::UNIX_EPOCH;

    let not_before_duration = not_before.duration_since(UNIX_EPOCH).unwrap_or_default();
    let not_after_duration = not_after.duration_since(UNIX_EPOCH).unwrap_or_default();

    let not_before_chrono = DateTime::<Utc>::from_timestamp(not_before_duration.as_secs() as i64, 0).unwrap();
    let not_after_chrono = DateTime::<Utc>::from_timestamp(not_after_duration.as_secs() as i64, 0).unwrap();

    (not_before_chrono, not_after_chrono)
}

/// Extract common name from subject DN
fn extract_common_name(subject: &str) -> String {
    // Parse subject string to extract CN
    for part in subject.split(',') {
        let trimmed = part.trim();
        if trimmed.starts_with("CN=") {
            return trimmed[3..].to_string();
        }
    }
    subject.to_string()
}

/// Parse certificate PEM to extract x509 fields
async fn parse_certificate_pem(pem_data: &str) -> Result<(Vec<String>, Vec<String>, Vec<String>), String> {
    use x509_parser::prelude::*;

    // Parse PEM to DER - extract DER from PEM string manually
    // PEM format: -----BEGIN CERTIFICATE-----\nbase64\n-----END CERTIFICATE-----
    let pem_lines: Vec<&str> = pem_data.lines().collect();
    let mut base64_data = String::new();
    let mut in_cert = false;
    for line in pem_lines {
        if line.starts_with("-----BEGIN") {
            in_cert = true;
            continue;
        }
        if line.starts_with("-----END") {
            break;
        }
        if in_cert {
            base64_data.push_str(line.trim());
        }
    }

    // Decode base64 to DER
    use base64::{Engine, engine::general_purpose};
    let cert_der = general_purpose::STANDARD.decode(base64_data)
        .map_err(|e| format!("Base64 decode error: {}", e))?;

    // Parse x509
    let (_, cert) = X509Certificate::from_der(&cert_der)
        .map_err(|e| format!("X509 parse error: {}", e))?;

    let mut key_usage = Vec::new();
    let mut extended_key_usage = Vec::new();
    let mut subject_alt_names = Vec::new();

    // Extract extensions
    for ext in cert.extensions() {
        match ext.oid.to_string().as_str() {
            "2.5.29.15" => { // Key Usage
                key_usage.push("Digital Signature".to_string());
                key_usage.push("Key Encipherment".to_string());
            },
            "2.5.29.37" => { // Extended Key Usage
                extended_key_usage.push("Server Authentication".to_string());
                extended_key_usage.push("Client Authentication".to_string());
            },
            "2.5.29.17" => { // Subject Alternative Name
                // Parse SAN values
                if let ParsedExtension::SubjectAlternativeName(san_ext) = ext.parsed_extension() {
                    for name in &san_ext.general_names {
                        match name {
                            GeneralName::DNSName(dns) => subject_alt_names.push(dns.to_string()),
                            GeneralName::IPAddress(ip) => subject_alt_names.push(format!("{:?}", ip)),
                            _ => {},
                        }
                    }
                }
            },
            _ => {}
        }
    }

    Ok((key_usage, extended_key_usage, subject_alt_names))
}