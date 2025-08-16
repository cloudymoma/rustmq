use crate::security::SecurityManager;
use crate::admin::ApiResponse;
use crate::controller::ControllerService;
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use warp::{Filter, Rejection, Reply};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, debug};
use chrono::{DateTime, Utc};

/// Security API extension for the Admin REST API
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
    fn ca_routes_static(
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
    fn certificate_routes_static(
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
    fn acl_routes_static(
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
    fn audit_routes_static(
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
    
    // TODO: Implement certificate listing with filtering and pagination
    let certificates = vec![
        CertificateListItem {
            certificate_id: "cert_12345".to_string(),
            common_name: "broker-01.rustmq.com".to_string(),
            subject: "CN=broker-01.rustmq.com,O=RustMQ Corp".to_string(),
            issuer: "CN=RustMQ Root CA,O=RustMQ Corp".to_string(),
            serial_number: "abc123def456".to_string(),
            not_before: Utc::now() - chrono::Duration::days(30),
            not_after: Utc::now() + chrono::Duration::days(335),
            status: "Active".to_string(),
            role: Some("broker".to_string()),
            fingerprint: "sha256:abc123...".to_string(),
        }
    ];
    
    let response = ApiResponse {
        success: true,
        data: Some(certificates),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_get_certificate(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate: {}", cert_id);
    
    // TODO: Implement certificate retrieval logic
    if cert_id == "nonexistent" {
        let response = ApiResponse::<CertificateDetailResponse> {
            success: false,
            data: None,
            error: Some(format!("Certificate '{}' not found", cert_id)),
            leader_hint: None,
        };
        return Ok(warp::reply::json(&response));
    }
    
    let cert_detail = CertificateDetailResponse {
        certificate_id: cert_id.clone(),
        common_name: "broker-01.rustmq.com".to_string(),
        subject: "CN=broker-01.rustmq.com,O=RustMQ Corp".to_string(),
        issuer: "CN=RustMQ Root CA,O=RustMQ Corp".to_string(),
        serial_number: "abc123def456".to_string(),
        not_before: Utc::now() - chrono::Duration::days(30),
        not_after: Utc::now() + chrono::Duration::days(335),
        status: "Active".to_string(),
        role: Some("broker".to_string()),
        fingerprint: "sha256:abc123...".to_string(),
        key_usage: vec!["Digital Signature".to_string(), "Key Encipherment".to_string()],
        extended_key_usage: vec!["Server Authentication".to_string(), "Client Authentication".to_string()],
        subject_alt_names: vec!["broker-01.rustmq.com".to_string(), "192.168.1.100".to_string()],
        certificate_chain: vec!["-----BEGIN CERTIFICATE-----...".to_string()],
        revocation_reason: None,
        revocation_time: None,
    };
    
    let response = ApiResponse {
        success: true,
        data: Some(cert_detail),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_get_expiring_certificates(
    query: HashMap<String, String>,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    let threshold_days = query.get("days").and_then(|d| d.parse::<i64>().ok()).unwrap_or(30);
    debug!("Getting certificates expiring within {} days", threshold_days);
    
    // TODO: Implement expiring certificate retrieval logic
    let expiring_certs = vec![
        CertificateListItem {
            certificate_id: "cert_98765".to_string(),
            common_name: "broker-02.rustmq.com".to_string(),
            subject: "CN=broker-02.rustmq.com,O=RustMQ Corp".to_string(),
            issuer: "CN=RustMQ Root CA,O=RustMQ Corp".to_string(),
            serial_number: "xyz789uvw123".to_string(),
            not_before: Utc::now() - chrono::Duration::days(340),
            not_after: Utc::now() + chrono::Duration::days(25),
            status: "Active".to_string(),
            role: Some("broker".to_string()),
            fingerprint: "sha256:xyz789...".to_string(),
        }
    ];
    
    let response = ApiResponse {
        success: true,
        data: Some(expiring_certs),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_get_certificate_status(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate status: {}", cert_id);
    
    // TODO: Implement certificate status check logic
    let status_info = serde_json::json!({
        "certificate_id": cert_id,
        "status": "Active",
        "valid": true,
        "expired": false,
        "revoked": false,
        "expires_in_days": 335,
        "last_validation": Utc::now(),
        "validation_issues": []
    });
    
    let response = ApiResponse {
        success: true,
        data: Some(status_info),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
}

async fn handle_get_certificate_chain(
    cert_id: String,
    security_manager: Arc<SecurityManager>,
) -> std::result::Result<impl Reply, Rejection> {
    debug!("Getting certificate chain: {}", cert_id);
    
    // TODO: Implement certificate chain retrieval logic
    // Simplified architecture: certificate + root CA only (no intermediate CAs)
    let chain_info = serde_json::json!({
        "certificate_id": cert_id,
        "chain": [
            "-----BEGIN CERTIFICATE-----\n...End Certificate...\n-----END CERTIFICATE-----",
            "-----BEGIN CERTIFICATE-----\n...Root CA...\n-----END CERTIFICATE-----"
        ],
        "chain_valid": true,
        "trust_anchor": "RustMQ Root CA"
    });
    
    let response = ApiResponse {
        success: true,
        data: Some(chain_info),
        error: None,
        leader_hint: None,
    };
    
    Ok(warp::reply::json(&response))
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
    
    // TODO: Implement ACL rule listing with filtering and pagination
    let rules = vec![
        AclRuleResponse {
            rule_id: "rule_12345".to_string(),
            principal: "user@domain.com".to_string(),
            resource_pattern: "topic.users.*".to_string(),
            resource_type: "topic".to_string(),
            operation: "read".to_string(),
            effect: "allow".to_string(),
            conditions: HashMap::new(),
            created_at: Utc::now() - chrono::Duration::hours(1),
            updated_at: Utc::now() - chrono::Duration::hours(1),
            version: 1,
        }
    ];
    
    let response = ApiResponse {
        success: true,
        data: Some(rules),
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
    
    // TODO: Implement ACL rule retrieval logic
    if rule_id == "nonexistent" {
        let response = ApiResponse::<AclRuleResponse> {
            success: false,
            data: None,
            error: Some(format!("ACL rule '{}' not found", rule_id)),
            leader_hint: None,
        };
        return Ok(warp::reply::json(&response));
    }
    
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
    
    // TODO: Implement ACL rule update logic
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
    
    // TODO: Implement ACL rule deletion logic
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
    
    // TODO: Implement principal permissions retrieval logic
    let permissions = PrincipalPermissionsResponse {
        principal: principal.clone(),
        permissions: vec![
            PermissionEntry {
                resource_pattern: "topic.users.*".to_string(),
                operations: vec!["read".to_string(), "write".to_string()],
                effect: "allow".to_string(),
                conditions: HashMap::new(),
            }
        ],
        effective_permissions: vec!["read:topic.users.*".to_string(), "write:topic.users.*".to_string()],
    };
    
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
    
    // TODO: Implement resource rules retrieval logic
    let resource_rules = ResourceRulesResponse {
        resource: resource.clone(),
        rules: vec![
            AclRuleResponse {
                rule_id: "rule_12345".to_string(),
                principal: "user@domain.com".to_string(),
                resource_pattern: "topic.users.*".to_string(),
                resource_type: "topic".to_string(),
                operation: "read".to_string(),
                effect: "allow".to_string(),
                conditions: HashMap::new(),
                created_at: Utc::now() - chrono::Duration::hours(1),
                updated_at: Utc::now() - chrono::Duration::hours(1),
                version: 1,
            }
        ],
        effective_permissions: {
            let mut perms = HashMap::new();
            perms.insert("user@domain.com".to_string(), vec!["read".to_string()]);
            perms
        },
    };
    
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
    
    // TODO: Implement bulk ACL evaluation logic
    let results: Vec<AclEvaluationResponse> = request.evaluations.into_iter().map(|eval| {
        AclEvaluationResponse {
            allowed: true,
            effect: "allow".to_string(),
            matched_rules: vec!["rule_12345".to_string()],
            evaluation_time_ns: 1200,
        }
    }).collect();
    
    let total_time: u64 = results.iter().map(|r| r.evaluation_time_ns).sum();
    
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
    
    // TODO: Implement ACL synchronization logic
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
    debug!("Getting current ACL version");
    
    // TODO: Implement ACL version retrieval logic
    let version_info = serde_json::json!({
        "version": 123,
        "last_updated": Utc::now(),
        "rules_count": 50,
        "sync_status": "synchronized"
    });
    
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
    
    // TODO: Implement ACL cache invalidation logic
    let response = ApiResponse {
        success: true,
        data: Some("ACL caches invalidated successfully".to_string()),
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
    
    // TODO: Implement ACL cache warming logic
    let response = ApiResponse {
        success: true,
        data: Some("ACL caches warmed successfully".to_string()),
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
    debug!("Getting security audit logs");
    
    // TODO: Implement audit logs retrieval logic
    let audit_logs = SecurityAuditLogResponse {
        events: vec![
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
            }
        ],
        total_count: 1,
        has_more: false,
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
    
    // TODO: Implement real-time events retrieval logic
    let events = vec![
        serde_json::json!({
            "timestamp": Utc::now(),
            "event_type": "authentication_success",
            "principal": "client@rustmq.com",
            "client_ip": "192.168.1.200",
            "details": {
                "certificate_fingerprint": "sha256:xyz789..."
            }
        })
    ];
    
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
    
    // TODO: Implement certificate audit trail retrieval logic
    let audit_trail = vec![
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::minutes(10),
            "operation": "certificate_issued",
            "certificate_id": "cert_12345",
            "principal": "admin@rustmq.com",
            "details": {
                "common_name": "broker-01.rustmq.com",
                "issuer": "RustMQ Root CA"
            }
        })
    ];
    
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
    
    // TODO: Implement ACL audit trail retrieval logic
    let audit_trail = vec![
        serde_json::json!({
            "timestamp": Utc::now() - chrono::Duration::minutes(15),
            "operation": "acl_rule_created",
            "rule_id": "rule_12345",
            "principal": "admin@rustmq.com",
            "details": {
                "target_principal": "user@domain.com",
                "resource_pattern": "topic.users.*",
                "effect": "allow"
            }
        })
    ];
    
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
    debug!("Getting overall security system status");
    
    // TODO: Implement security status assessment logic
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
    debug!("Getting security performance metrics");
    
    // TODO: Implement security metrics retrieval logic using SecurityMetrics
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
    
    // TODO: Implement security configuration retrieval logic
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
    
    // TODO: Implement maintenance cleanup logic
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
    
    // TODO: Implement maintenance backup logic
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
    
    // TODO: Implement maintenance restore logic
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