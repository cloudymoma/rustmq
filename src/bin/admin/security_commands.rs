use rustmq::Result;
use clap::{Subcommand, Args};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::{info, debug, error};
use chrono::{DateTime, Utc};

use super::api_client::{AdminApiClient, GenerateCaRequest, 
                        IssueCertificateRequest, CreateAclRuleRequest, UpdateAclRuleRequest,
                        AclEvaluationRequest, BulkAclEvaluationRequest, SecurityAuditLogRequest,
                        CertificateValidationRequest, RevokeCertificateRequest, MaintenanceRequest};
use super::formatters::{print_output, print_error, print_success, print_warning, 
                        print_info, confirm_operation, ProgressIndicator};
use crate::Cli;

// ==================== Certificate Authority Commands ====================

#[derive(Subcommand)]
pub enum CaCommands {
    /// Initialize a new root CA
    Init(CaInitCommand),
    /// List all CA certificates
    List(CaListCommand),
    /// Get information about a specific CA
    Info(CaInfoCommand),
}

#[derive(Args)]
pub struct CaInitCommand {
    /// Common Name for the CA
    #[arg(long)]
    pub cn: String,
    /// Organization name
    #[arg(long)]
    pub org: Option<String>,
    /// Country code (2 letters)
    #[arg(long)]
    pub country: Option<String>,
    /// Validity period in years
    #[arg(long, default_value = "10")]
    pub validity_years: u32,
    /// Key size in bits
    #[arg(long, default_value = "4096")]
    pub key_size: u32,
}

#[derive(Args)]
pub struct CaListCommand {
    /// Filter by status (active, inactive, expired)
    #[arg(long)]
    pub status: Option<String>,
}

#[derive(Args)]
pub struct CaInfoCommand {
    /// CA ID to get information about
    pub ca_id: String,
}

// Intermediate CA commands removed - only root CA supported for simplicity

pub async fn execute_ca_command(
    cmd: &CaCommands,
    api_client: &AdminApiClient,
    cli: &Cli,
) -> Result<()> {
    match cmd {
        CaCommands::Init(args) => {
            info!("Initializing new root CA: {}", args.cn);
            
            let request = GenerateCaRequest {
                common_name: args.cn.clone(),
                organization: args.org.clone(),
                country: args.country.clone(),
                validity_days: Some(args.validity_years * 365),
                key_size: Some(args.key_size),
            };
            
            let progress = ProgressIndicator::new("Generating root CA certificate", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/ca/generate", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_success("Root CA generated successfully", cli.no_color);
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Unknown error".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CaCommands::List(args) => {
            debug!("Listing CA certificates");
            
            let mut query = HashMap::new();
            if let Some(status) = &args.status {
                query.insert("status".to_string(), status.clone());
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = if query.is_empty() {
                api_client.get("/api/v1/security/ca/list").await?
            } else {
                api_client.get_with_query("/api/v1/security/ca/list", &query).await?
            };
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No CA certificates found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to list CAs".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CaCommands::Info(args) => {
            debug!("Getting CA information for: {}", args.ca_id);
            
            let path = format!("/api/v1/security/ca/{}", args.ca_id);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_error(&format!("CA '{}' not found", args.ca_id), cli.no_color);
                    std::process::exit(1);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get CA info".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        // Intermediate CA command removed - only root CA supported for simplicity
    }
    
    Ok(())
}

// ==================== Certificate Management Commands ====================

#[derive(Subcommand)]
pub enum CertCommands {
    /// Issue a new certificate
    Issue(CertIssueCommand),
    /// List certificates
    List(CertListCommand),
    /// Get certificate information
    Info(CertInfoCommand),
    /// Renew a certificate
    Renew(CertRenewCommand),
    /// Rotate a certificate
    Rotate(CertRotateCommand),
    /// Revoke a certificate
    Revoke(CertRevokeCommand),
    /// Get certificate status
    Status(CertStatusCommand),
    /// List expiring certificates
    Expiring(CertExpiringCommand),
    /// Validate a certificate
    Validate(CertValidateCommand),
    /// Export certificate
    Export(CertExportCommand),
}

#[derive(Args)]
pub struct CertIssueCommand {
    /// Principal name (subject)
    #[arg(long)]
    pub principal: String,
    /// Certificate role (broker, client, admin)
    #[arg(long)]
    pub role: Option<String>,
    /// CA ID to use for signing
    #[arg(long)]
    pub ca_id: Option<String>,
    /// Subject Alternative Names
    #[arg(long)]
    pub san: Vec<String>,
    /// Organization name
    #[arg(long)]
    pub org: Option<String>,
    /// Validity period in days
    #[arg(long, default_value = "365")]
    pub validity_days: u32,
}

#[derive(Args)]
pub struct CertListCommand {
    /// Filter by status (active, expired, revoked)
    #[arg(long)]
    pub filter: Option<String>,
    /// Filter by role
    #[arg(long)]
    pub role: Option<String>,
    /// Filter by principal
    #[arg(long)]
    pub principal: Option<String>,
}

#[derive(Args)]
pub struct CertInfoCommand {
    /// Certificate ID
    pub cert_id: String,
}

#[derive(Args)]
pub struct CertRenewCommand {
    /// Certificate ID to renew
    pub cert_id: String,
    /// New validity period in days
    #[arg(long)]
    pub validity_days: Option<u32>,
}

#[derive(Args)]
pub struct CertRotateCommand {
    /// Certificate ID to rotate
    pub cert_id: String,
}

#[derive(Args)]
pub struct CertRevokeCommand {
    /// Certificate ID to revoke
    pub cert_id: String,
    /// Revocation reason
    #[arg(long, default_value = "unspecified")]
    pub reason: String,
    /// Reason code
    #[arg(long)]
    pub reason_code: Option<u32>,
    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct CertStatusCommand {
    /// Certificate ID
    pub cert_id: String,
}

#[derive(Args)]
pub struct CertExpiringCommand {
    /// Days threshold for expiring certificates
    #[arg(long, default_value = "30")]
    pub days: u32,
}

#[derive(Args)]
pub struct CertValidateCommand {
    /// Path to certificate file
    #[arg(long)]
    pub cert_file: PathBuf,
    /// Check revocation status
    #[arg(long)]
    pub check_revocation: bool,
}

#[derive(Args)]
pub struct CertExportCommand {
    /// Certificate ID
    pub cert_id: String,
    /// Export format (pem, der, p12)
    #[arg(long, default_value = "pem")]
    pub format: String,
    /// Output file path
    #[arg(long)]
    pub output: Option<PathBuf>,
}

pub async fn execute_cert_command(
    cmd: &CertCommands,
    api_client: &AdminApiClient,
    cli: &Cli,
) -> Result<()> {
    match cmd {
        CertCommands::Issue(args) => {
            info!("Issuing certificate for principal: {}", args.principal);
            
            let request = IssueCertificateRequest {
                ca_id: args.ca_id.clone().unwrap_or_else(|| "default".to_string()),
                common_name: args.principal.clone(),
                subject_alt_names: if args.san.is_empty() { None } else { Some(args.san.clone()) },
                organization: args.org.clone(),
                role: args.role.clone(),
                validity_days: Some(args.validity_days),
                key_usage: None,
                extended_key_usage: None,
            };
            
            let progress = ProgressIndicator::new("Issuing certificate", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/certificates/issue", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_success("Certificate issued successfully", cli.no_color);
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Unknown error".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::List(args) => {
            debug!("Listing certificates");
            
            let mut query = HashMap::new();
            if let Some(filter) = &args.filter {
                query.insert("status".to_string(), filter.clone());
            }
            if let Some(role) = &args.role {
                query.insert("role".to_string(), role.clone());
            }
            if let Some(principal) = &args.principal {
                query.insert("principal".to_string(), principal.clone());
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = if query.is_empty() {
                api_client.get("/api/v1/security/certificates").await?
            } else {
                api_client.get_with_query("/api/v1/security/certificates", &query).await?
            };
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No certificates found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to list certificates".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Info(args) => {
            debug!("Getting certificate information for: {}", args.cert_id);
            
            let path = format!("/api/v1/security/certificates/{}", args.cert_id);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_error(&format!("Certificate '{}' not found", args.cert_id), cli.no_color);
                    std::process::exit(1);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get certificate info".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Renew(args) => {
            info!("Renewing certificate: {}", args.cert_id);
            
            let path = format!("/api/v1/security/certificates/renew/{}", args.cert_id);
            let response = api_client.post::<serde_json::Value, serde_json::Value>(&path, &serde_json::json!({})).await?;
            
            if response.success {
                print_success(&format!("Certificate '{}' renewed successfully", args.cert_id), cli.no_color);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to renew certificate".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Rotate(args) => {
            info!("Rotating certificate: {}", args.cert_id);
            
            let path = format!("/api/v1/security/certificates/rotate/{}", args.cert_id);
            let response = api_client.post::<serde_json::Value, serde_json::Value>(&path, &serde_json::json!({})).await?;
            
            if response.success {
                print_success(&format!("Certificate '{}' rotated successfully", args.cert_id), cli.no_color);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to rotate certificate".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Revoke(args) => {
            if !args.force && !confirm_operation("revoke certificate", &args.cert_id) {
                print_info("Operation cancelled", cli.no_color);
                return Ok(());
            }
            
            info!("Revoking certificate: {}", args.cert_id);
            
            let request = RevokeCertificateRequest {
                reason: args.reason.clone(),
                reason_code: args.reason_code,
            };
            
            let path = format!("/api/v1/security/certificates/revoke/{}", args.cert_id);
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post(&path, &request).await?;
            
            if response.success {
                print_success(&format!("Certificate '{}' revoked successfully", args.cert_id), cli.no_color);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to revoke certificate".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Status(args) => {
            debug!("Getting certificate status for: {}", args.cert_id);
            
            let path = format!("/api/v1/security/certificates/{}/status", args.cert_id);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_error(&format!("Certificate '{}' not found", args.cert_id), cli.no_color);
                    std::process::exit(1);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get certificate status".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Expiring(args) => {
            debug!("Getting certificates expiring within {} days", args.days);
            
            let mut query = HashMap::new();
            query.insert("days".to_string(), args.days.to_string());
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.get_with_query("/api/v1/security/certificates/expiring", &query).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No expiring certificates found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get expiring certificates".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Validate(args) => {
            debug!("Validating certificate file: {:?}", args.cert_file);
            
            let cert_pem = std::fs::read_to_string(&args.cert_file)
                .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read certificate file: {}", e)))?;
            
            let request = CertificateValidationRequest {
                certificate_pem: cert_pem,
                chain_pem: None,
                check_revocation: Some(args.check_revocation),
            };
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/certificates/validate", &request).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to validate certificate".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        CertCommands::Export(args) => {
            debug!("Exporting certificate: {}", args.cert_id);
            
            let path = format!("/api/v1/security/certificates/{}/chain", args.cert_id);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    if let Some(output_path) = &args.output {
                        // Write to file
                        let content = serde_json::to_string_pretty(data)?;
                        std::fs::write(output_path, content)
                            .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to write output file: {}", e)))?;
                        print_success(&format!("Certificate exported to: {:?}", output_path), cli.no_color);
                    } else {
                        // Print to stdout
                        print_output(data, cli.format, cli.no_color)?;
                    }
                } else {
                    print_error(&format!("Certificate '{}' not found", args.cert_id), cli.no_color);
                    std::process::exit(1);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to export certificate".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
    }
    
    Ok(())
}

// ==================== ACL Management Commands ====================

#[derive(Subcommand)]
pub enum AclCommands {
    /// Create a new ACL rule
    Create(AclCreateCommand),
    /// List ACL rules
    List(AclListCommand),
    /// Get ACL rule information
    Info(AclInfoCommand),
    /// Update an ACL rule
    Update(AclUpdateCommand),
    /// Delete an ACL rule
    Delete(AclDeleteCommand),
    /// Test ACL evaluation
    Test(AclTestCommand),
    /// Get principal permissions
    Permissions(AclPermissionsCommand),
    /// Get rules for a resource
    Rules(AclRulesCommand),
    /// Bulk test ACL evaluations
    BulkTest(AclBulkTestCommand),
    /// Sync ACL rules to brokers
    Sync(AclSyncCommand),
    /// Get ACL version
    Version,
    /// Invalidate ACL cache
    CacheInvalidate(AclCacheInvalidateCommand),
    /// Warm ACL cache
    CacheWarm(AclCacheWarmCommand),
}

#[derive(Args)]
pub struct AclCreateCommand {
    /// Principal (user/service identity)
    #[arg(long)]
    pub principal: String,
    /// Resource pattern
    #[arg(long)]
    pub resource: String,
    /// Resource type (topic, group, cluster)
    #[arg(long, default_value = "topic")]
    pub resource_type: String,
    /// Operation permissions (comma-separated)
    #[arg(long)]
    pub permissions: String,
    /// Effect (allow, deny)
    #[arg(long, default_value = "allow")]
    pub effect: String,
    /// Additional conditions
    #[arg(long)]
    pub conditions: Vec<String>,
}

#[derive(Args)]
pub struct AclListCommand {
    /// Filter by principal
    #[arg(long)]
    pub principal: Option<String>,
    /// Filter by resource pattern
    #[arg(long)]
    pub resource: Option<String>,
    /// Filter by effect
    #[arg(long)]
    pub effect: Option<String>,
}

#[derive(Args)]
pub struct AclInfoCommand {
    /// Rule ID
    pub rule_id: String,
}

#[derive(Args)]
pub struct AclUpdateCommand {
    /// Rule ID to update
    pub rule_id: String,
    /// New principal
    #[arg(long)]
    pub principal: Option<String>,
    /// New resource pattern
    #[arg(long)]
    pub resource: Option<String>,
    /// New permissions (comma-separated)
    #[arg(long)]
    pub permissions: Option<String>,
    /// New effect
    #[arg(long)]
    pub effect: Option<String>,
}

#[derive(Args)]
pub struct AclDeleteCommand {
    /// Rule ID to delete
    pub rule_id: String,
    /// Skip confirmation prompt
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct AclTestCommand {
    /// Principal to test
    #[arg(long)]
    pub principal: String,
    /// Resource to test
    #[arg(long)]
    pub resource: String,
    /// Operation to test
    #[arg(long)]
    pub operation: String,
}

#[derive(Args)]
pub struct AclPermissionsCommand {
    /// Principal to get permissions for
    pub principal: String,
}

#[derive(Args)]
pub struct AclRulesCommand {
    /// Resource pattern to get rules for
    pub resource: String,
}

#[derive(Args)]
pub struct AclBulkTestCommand {
    /// Input file with test cases (JSON)
    #[arg(long)]
    pub input_file: PathBuf,
}

#[derive(Args)]
pub struct AclSyncCommand {
    /// Force synchronization
    #[arg(long)]
    pub force: bool,
}

#[derive(Args)]
pub struct AclCacheInvalidateCommand {
    /// Principals to invalidate (comma-separated)
    #[arg(long)]
    pub principals: Option<String>,
}

#[derive(Args)]
pub struct AclCacheWarmCommand {
    /// Principals to warm cache for (comma-separated)
    #[arg(long)]
    pub principals: String,
}

pub async fn execute_acl_command(
    cmd: &AclCommands,
    api_client: &AdminApiClient,
    cli: &Cli,
) -> Result<()> {
    match cmd {
        AclCommands::Create(args) => {
            info!("Creating ACL rule for principal: {}", args.principal);
            
            let permissions: Vec<&str> = args.permissions.split(',').map(|s| s.trim()).collect();
            if permissions.is_empty() {
                print_error("At least one permission must be specified", cli.no_color);
                std::process::exit(1);
            }
            
            // Parse conditions from key=value format
            let mut conditions = HashMap::new();
            for condition in &args.conditions {
                if let Some((key, value)) = condition.split_once('=') {
                    conditions.insert(key.trim().to_string(), value.trim().to_string());
                } else {
                    print_error(&format!("Invalid condition format: '{}'. Use key=value format", condition), cli.no_color);
                    std::process::exit(1);
                }
            }
            
            // Create a rule for each permission
            for permission in permissions {
                let request = CreateAclRuleRequest {
                    principal: args.principal.clone(),
                    resource_pattern: args.resource.clone(),
                    resource_type: args.resource_type.clone(),
                    operation: permission.to_string(),
                    effect: args.effect.clone(),
                    conditions: Some(conditions.clone()),
                };
                
                let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/acl/rules", &request).await?;
                
                if !response.success {
                    let error_msg = response.error.unwrap_or_else(|| "Failed to create ACL rule".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            }
            
            print_success("ACL rule(s) created successfully", cli.no_color);
        }
        AclCommands::List(args) => {
            debug!("Listing ACL rules");
            
            let mut query = HashMap::new();
            if let Some(principal) = &args.principal {
                query.insert("principal".to_string(), principal.clone());
            }
            if let Some(resource) = &args.resource {
                query.insert("resource".to_string(), resource.clone());
            }
            if let Some(effect) = &args.effect {
                query.insert("effect".to_string(), effect.clone());
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = if query.is_empty() {
                api_client.get("/api/v1/security/acl/rules").await?
            } else {
                api_client.get_with_query("/api/v1/security/acl/rules", &query).await?
            };
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No ACL rules found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to list ACL rules".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Info(args) => {
            debug!("Getting ACL rule information for: {}", args.rule_id);
            
            let path = format!("/api/v1/security/acl/rules/{}", args.rule_id);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_error(&format!("ACL rule '{}' not found", args.rule_id), cli.no_color);
                    std::process::exit(1);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get ACL rule info".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Update(args) => {
            info!("Updating ACL rule: {}", args.rule_id);
            
            let request = UpdateAclRuleRequest {
                principal: args.principal.clone(),
                resource_pattern: args.resource.clone(),
                resource_type: None,
                operation: args.permissions.clone(),
                effect: args.effect.clone(),
                conditions: None,
            };
            
            let path = format!("/api/v1/security/acl/rules/{}", args.rule_id);
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.put(&path, &request).await?;
            
            if response.success {
                print_success(&format!("ACL rule '{}' updated successfully", args.rule_id), cli.no_color);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to update ACL rule".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Delete(args) => {
            if !args.force && !confirm_operation("delete ACL rule", &args.rule_id) {
                print_info("Operation cancelled", cli.no_color);
                return Ok(());
            }
            
            info!("Deleting ACL rule: {}", args.rule_id);
            
            let path = format!("/api/v1/security/acl/rules/{}", args.rule_id);
            let response = api_client.delete::<serde_json::Value>(&path).await?;
            
            if response.success {
                print_success(&format!("ACL rule '{}' deleted successfully", args.rule_id), cli.no_color);
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to delete ACL rule".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Test(args) => {
            debug!("Testing ACL evaluation for: {} on {}", args.principal, args.resource);
            
            let request = AclEvaluationRequest {
                principal: args.principal.clone(),
                resource: args.resource.clone(),
                operation: args.operation.clone(),
                context: None,
            };
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/acl/evaluate", &request).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to evaluate ACL".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Permissions(args) => {
            debug!("Getting permissions for principal: {}", args.principal);
            
            let path = format!("/api/v1/security/acl/principals/{}/permissions", args.principal);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info(&format!("No permissions found for principal '{}'", args.principal), cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get principal permissions".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Rules(args) => {
            debug!("Getting rules for resource: {}", args.resource);
            
            let path = format!("/api/v1/security/acl/resources/{}/rules", args.resource);
            let response = api_client.get::<serde_json::Value>(&path).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info(&format!("No rules found for resource '{}'", args.resource), cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get resource rules".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::BulkTest(args) => {
            debug!("Running bulk ACL evaluation from file: {:?}", args.input_file);
            
            let input_content = std::fs::read_to_string(&args.input_file)
                .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read input file: {}", e)))?;
            
            let request: BulkAclEvaluationRequest = serde_json::from_str(&input_content)?;
            
            let progress = ProgressIndicator::new(&format!("Evaluating {} ACL requests", request.evaluations.len()), cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/acl/bulk-evaluate", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Failed to perform bulk ACL evaluation".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::Sync(args) => {
            if args.force || confirm_operation("sync ACL rules to all brokers", "cluster") {
                info!("Synchronizing ACL rules to brokers");
                
                let progress = ProgressIndicator::new("Synchronizing ACL rules", cli.no_color);
                progress.start();
                
                let response = api_client.post::<serde_json::Value, serde_json::Value>("/api/v1/security/acl/sync", &serde_json::json!({})).await?;
                
                if response.success {
                    progress.finish(true);
                    print_success("ACL rules synchronized successfully", cli.no_color);
                } else {
                    progress.finish(false);
                    let error_msg = response.error.unwrap_or_else(|| "Failed to sync ACL rules".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            } else {
                print_info("Operation cancelled", cli.no_color);
            }
        }
        AclCommands::Version => {
            debug!("Getting ACL version information");
            
            let response = api_client.get::<serde_json::Value>("/api/v1/security/acl/version").await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get ACL version".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::CacheInvalidate(args) => {
            info!("Invalidating ACL caches");
            
            let progress = ProgressIndicator::new("Invalidating ACL caches", cli.no_color);
            progress.start();
            
            let response = api_client.post::<serde_json::Value, serde_json::Value>("/api/v1/security/acl/cache/invalidate", &serde_json::json!({})).await?;
            
            if response.success {
                progress.finish(true);
                print_success("ACL caches invalidated successfully", cli.no_color);
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Failed to invalidate ACL caches".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AclCommands::CacheWarm(args) => {
            info!("Warming ACL caches for principals: {}", args.principals);
            
            let principals: Vec<String> = args.principals.split(',').map(|s| s.trim().to_string()).collect();
            let request = serde_json::json!({ "principals": principals });
            
            let progress = ProgressIndicator::new("Warming ACL caches", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/acl/cache/warm", &request).await?;
            
            if response.success {
                progress.finish(true);
                print_success("ACL caches warmed successfully", cli.no_color);
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Failed to warm ACL caches".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
    }
    
    Ok(())
}

// ==================== Security Audit Commands ====================

#[derive(Subcommand)]
pub enum AuditCommands {
    /// View security audit logs
    Logs(AuditLogsCommand),
    /// View real-time security events
    Events(AuditEventsCommand),
    /// View certificate operation audit trail
    Certificates(AuditCertificatesCommand),
    /// View ACL change audit trail
    Acl(AuditAclCommand),
}

#[derive(Args)]
pub struct AuditLogsCommand {
    /// Start time filter (RFC3339 format)
    #[arg(long)]
    pub since: Option<String>,
    /// End time filter (RFC3339 format)
    #[arg(long)]
    pub until: Option<String>,
    /// Event type filter
    #[arg(long)]
    pub r#type: Option<String>,
    /// Principal filter
    #[arg(long)]
    pub principal: Option<String>,
    /// Limit number of results
    #[arg(long, default_value = "100")]
    pub limit: u32,
    /// Follow mode (real-time)
    #[arg(long)]
    pub follow: bool,
}

#[derive(Args)]
pub struct AuditEventsCommand {
    /// Follow mode (real-time)
    #[arg(long)]
    pub follow: bool,
    /// Event type filter
    #[arg(long)]
    pub filter: Option<String>,
}

#[derive(Args)]
pub struct AuditCertificatesCommand {
    /// Operation filter (issue, renew, revoke)
    #[arg(long)]
    pub operation: Option<String>,
    /// Certificate ID filter
    #[arg(long)]
    pub cert_id: Option<String>,
}

#[derive(Args)]
pub struct AuditAclCommand {
    /// Principal filter
    #[arg(long)]
    pub principal: Option<String>,
    /// Operation filter (create, update, delete)
    #[arg(long)]
    pub operation: Option<String>,
}

pub async fn execute_audit_command(
    cmd: &AuditCommands,
    api_client: &AdminApiClient,
    cli: &Cli,
) -> Result<()> {
    match cmd {
        AuditCommands::Logs(args) => {
            debug!("Getting security audit logs");
            
            let start_time = if let Some(since) = &args.since {
                Some(DateTime::parse_from_rfc3339(since)
                    .map_err(|e| rustmq::error::RustMqError::Config(format!("Invalid since time format: {}", e)))?
                    .with_timezone(&Utc))
            } else {
                None
            };
            
            let end_time = if let Some(until) = &args.until {
                Some(DateTime::parse_from_rfc3339(until)
                    .map_err(|e| rustmq::error::RustMqError::Config(format!("Invalid until time format: {}", e)))?
                    .with_timezone(&Utc))
            } else {
                None
            };
            
            let mut query = HashMap::new();
            if let Some(event_type) = &args.r#type {
                query.insert("event_type".to_string(), event_type.clone());
            }
            if let Some(principal) = &args.principal {
                query.insert("principal".to_string(), principal.clone());
            }
            query.insert("limit".to_string(), args.limit.to_string());
            
            if start_time.is_some() || end_time.is_some() {
                // TODO: Add time range to query when SecurityAuditLogRequest is used in query params
                if let Some(start) = start_time {
                    query.insert("start_time".to_string(), start.to_rfc3339());
                }
                if let Some(end) = end_time {
                    query.insert("end_time".to_string(), end.to_rfc3339());
                }
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.get_with_query("/api/v1/security/audit/logs", &query).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No audit logs found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get audit logs".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AuditCommands::Events(args) => {
            debug!("Getting real-time security events");
            
            let mut query = HashMap::new();
            if let Some(filter) = &args.filter {
                query.insert("filter".to_string(), filter.clone());
            }
            if args.follow {
                query.insert("follow".to_string(), "true".to_string());
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.get_with_query("/api/v1/security/audit/events", &query).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No security events found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get security events".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AuditCommands::Certificates(args) => {
            debug!("Getting certificate operation audit trail");
            
            let mut query = HashMap::new();
            if let Some(operation) = &args.operation {
                query.insert("operation".to_string(), operation.clone());
            }
            if let Some(cert_id) = &args.cert_id {
                query.insert("certificate_id".to_string(), cert_id.clone());
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.get_with_query("/api/v1/security/audit/certificates", &query).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No certificate audit trail found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get certificate audit trail".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        AuditCommands::Acl(args) => {
            debug!("Getting ACL change audit trail");
            
            let mut query = HashMap::new();
            if let Some(principal) = &args.principal {
                query.insert("principal".to_string(), principal.clone());
            }
            if let Some(operation) = &args.operation {
                query.insert("operation".to_string(), operation.clone());
            }
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.get_with_query("/api/v1/security/audit/acl", &query).await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_info("No ACL audit trail found", cli.no_color);
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get ACL audit trail".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
    }
    
    Ok(())
}

// ==================== General Security Commands ====================

#[derive(Subcommand)]
pub enum SecurityCommands {
    /// Get overall security status
    Status,
    /// Get security performance metrics
    Metrics,
    /// Get security health status
    Health,
    /// Get security configuration
    Config,
    /// Perform security maintenance operations
    Cleanup(SecurityCleanupCommand),
    /// Backup security configuration
    Backup(SecurityBackupCommand),
    /// Restore security configuration
    Restore(SecurityRestoreCommand),
    /// WebPKI certificate validation and management
    #[command(subcommand)]
    Webpki(WebpkiCommands),
}

// ==================== WebPKI Certificate Validation Commands ====================

#[derive(Subcommand)]
pub enum WebpkiCommands {
    /// Validate certificate using WebPKI
    Validate(WebpkiValidateCommand),
    /// Validate certificate chain
    ValidateChain(WebpkiValidateChainCommand),
    /// Manage trust anchors
    TrustAnchors(WebpkiTrustAnchorsCommand),
    /// Test WebPKI performance
    Benchmark(WebpkiBenchmarkCommand),
    /// Get WebPKI status and configuration
    Status,
    /// Test certificate against multiple validation methods
    Compare(WebpkiCompareCommand),
}

#[derive(Args)]
pub struct WebpkiValidateCommand {
    /// Path to certificate file (PEM format)
    #[arg(long)]
    pub cert_file: PathBuf,
    /// Path to CA certificate file (optional)
    #[arg(long)]
    pub ca_file: Option<PathBuf>,
    /// Enable strict validation (no fallback)
    #[arg(long)]
    pub strict: bool,
    /// Check certificate expiry
    #[arg(long)]
    pub check_expiry: bool,
    /// Expected hostname for validation
    #[arg(long)]
    pub hostname: Option<String>,
}

#[derive(Args)]
pub struct WebpkiValidateChainCommand {
    /// Path to certificate chain file (PEM format)
    #[arg(long)]
    pub chain_file: PathBuf,
    /// Path to trust anchor file
    #[arg(long)]
    pub trust_anchors_file: Option<PathBuf>,
    /// Verification time (RFC3339 format, defaults to now)
    #[arg(long)]
    pub verification_time: Option<String>,
    /// Check entire chain expiry
    #[arg(long)]
    pub check_expiry: bool,
}

#[derive(Args)]
pub struct WebpkiTrustAnchorsCommand {
    /// List current trust anchors
    #[arg(long)]
    pub list: bool,
    /// Add trust anchor from file
    #[arg(long)]
    pub add_file: Option<PathBuf>,
    /// Remove trust anchor by thumbprint
    #[arg(long)]
    pub remove: Option<String>,
    /// Validate trust anchor configuration
    #[arg(long)]
    pub validate: bool,
}

#[derive(Args)]
pub struct WebpkiBenchmarkCommand {
    /// Number of validation iterations
    #[arg(long, default_value = "1000")]
    pub iterations: u32,
    /// Path to test certificate file
    #[arg(long)]
    pub cert_file: PathBuf,
    /// Compare with legacy validation
    #[arg(long)]
    pub compare_legacy: bool,
    /// Output detailed timing information
    #[arg(long)]
    pub detailed: bool,
}

#[derive(Args)]
pub struct WebpkiCompareCommand {
    /// Path to certificate file
    #[arg(long)]
    pub cert_file: PathBuf,
    /// Show detailed validation results
    #[arg(long)]
    pub verbose: bool,
    /// Test specific hostname
    #[arg(long)]
    pub hostname: Option<String>,
}

#[derive(Args)]
pub struct SecurityCleanupCommand {
    /// Clean up expired certificates
    #[arg(long)]
    pub expired_certs: bool,
    /// Clean up cache entries
    #[arg(long)]
    pub cache_entries: bool,
    /// Dry run (show what would be cleaned)
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct SecurityBackupCommand {
    /// Output file path
    #[arg(long)]
    pub output: PathBuf,
    /// Include certificate data
    #[arg(long)]
    pub include_certs: bool,
    /// Include ACL rules
    #[arg(long)]
    pub include_acl: bool,
}

#[derive(Args)]
pub struct SecurityRestoreCommand {
    /// Input backup file path
    #[arg(long)]
    pub input: PathBuf,
    /// Force restore without confirmation
    #[arg(long)]
    pub force: bool,
}

pub async fn execute_security_command(
    cmd: &SecurityCommands,
    api_client: &AdminApiClient,
    cli: &Cli,
) -> Result<()> {
    match cmd {
        SecurityCommands::Status => {
            debug!("Getting overall security status");
            
            let response = api_client.get::<serde_json::Value>("/api/v1/security/status").await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get security status".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        SecurityCommands::Metrics => {
            debug!("Getting security performance metrics");
            
            let response = api_client.get::<serde_json::Value>("/api/v1/security/metrics").await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get security metrics".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        SecurityCommands::Health => {
            debug!("Getting security health status");
            
            let response = api_client.get::<serde_json::Value>("/api/v1/security/health").await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get security health".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        SecurityCommands::Config => {
            debug!("Getting security configuration");
            
            let response = api_client.get::<serde_json::Value>("/api/v1/security/configuration").await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get security configuration".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        SecurityCommands::Cleanup(args) => {
            info!("Performing security maintenance cleanup");
            
            let mut operations = Vec::new();
            if args.expired_certs {
                operations.push("cleanup_expired_certificates".to_string());
            }
            if args.cache_entries {
                operations.push("cleanup_cache_entries".to_string());
            }
            
            if operations.is_empty() {
                print_error("At least one cleanup operation must be specified", cli.no_color);
                std::process::exit(1);
            }
            
            for operation in operations {
                let mut parameters = HashMap::new();
                if args.dry_run {
                    parameters.insert("dry_run".to_string(), "true".to_string());
                }
                
                let request = MaintenanceRequest {
                    operation: operation.clone(),
                    parameters: Some(parameters),
                };
                
                let progress = ProgressIndicator::new(&format!("Running {}", operation), cli.no_color);
                progress.start();
                
                let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/maintenance/cleanup", &request).await?;
                
                if response.success {
                    progress.finish(true);
                    if let Some(data) = &response.data {
                        print_output(data, cli.format, cli.no_color)?;
                    }
                } else {
                    progress.finish(false);
                    let error_msg = response.error.unwrap_or_else(|| "Failed to perform cleanup".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            }
        }
        SecurityCommands::Backup(args) => {
            info!("Backing up security configuration to: {:?}", args.output);
            
            let mut parameters = HashMap::new();
            parameters.insert("output_path".to_string(), args.output.to_string_lossy().to_string());
            if args.include_certs {
                parameters.insert("include_certificates".to_string(), "true".to_string());
            }
            if args.include_acl {
                parameters.insert("include_acl".to_string(), "true".to_string());
            }
            
            let request = MaintenanceRequest {
                operation: "backup".to_string(),
                parameters: Some(parameters),
            };
            
            let progress = ProgressIndicator::new("Creating security backup", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/maintenance/backup", &request).await?;
            
            if response.success {
                progress.finish(true);
                print_success(&format!("Security configuration backed up to: {:?}", args.output), cli.no_color);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Failed to create backup".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        SecurityCommands::Restore(args) => {
            if !args.force && !confirm_operation("restore security configuration from backup", &args.input.to_string_lossy()) {
                print_info("Operation cancelled", cli.no_color);
                return Ok(());
            }
            
            info!("Restoring security configuration from: {:?}", args.input);
            
            let mut parameters = HashMap::new();
            parameters.insert("input_path".to_string(), args.input.to_string_lossy().to_string());
            
            let request = MaintenanceRequest {
                operation: "restore".to_string(),
                parameters: Some(parameters),
            };
            
            let progress = ProgressIndicator::new("Restoring security configuration", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/maintenance/restore", &request).await?;
            
            if response.success {
                progress.finish(true);
                print_success("Security configuration restored successfully", cli.no_color);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Failed to restore configuration".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        SecurityCommands::Webpki(webpki_cmd) => {
            execute_webpki_command(webpki_cmd, api_client, cli).await?
        }
    }
    
    Ok(())
}

// ==================== WebPKI Command Implementation ====================

pub async fn execute_webpki_command(
    cmd: &WebpkiCommands,
    api_client: &AdminApiClient,
    cli: &Cli,
) -> Result<()> {
    match cmd {
        WebpkiCommands::Validate(args) => {
            info!("Validating certificate using WebPKI: {:?}", args.cert_file);
            
            // Read certificate file
            let cert_pem = std::fs::read_to_string(&args.cert_file)
                .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read certificate file: {}", e)))?;
            
            // Read CA file if provided
            let ca_pem = if let Some(ca_file) = &args.ca_file {
                Some(std::fs::read_to_string(ca_file)
                    .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read CA file: {}", e)))?)
            } else {
                None
            };
            
            // Build request
            let mut request = serde_json::json!({
                "certificate_pem": cert_pem,
                "validation_method": "webpki",
                "strict_mode": args.strict,
                "check_expiry": args.check_expiry
            });
            
            if let Some(ca) = ca_pem {
                request["ca_certificate_pem"] = serde_json::Value::String(ca);
            }
            
            if let Some(hostname) = &args.hostname {
                request["expected_hostname"] = serde_json::Value::String(hostname.clone());
            }
            
            let progress = ProgressIndicator::new("Validating certificate with WebPKI", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/webpki/validate", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_success("Certificate validation completed successfully", cli.no_color);
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "WebPKI validation failed".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        WebpkiCommands::ValidateChain(args) => {
            info!("Validating certificate chain using WebPKI: {:?}", args.chain_file);
            
            // Read certificate chain file
            let chain_pem = std::fs::read_to_string(&args.chain_file)
                .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read certificate chain file: {}", e)))?;
            
            // Read trust anchors file if provided
            let trust_anchors_pem = if let Some(trust_file) = &args.trust_anchors_file {
                Some(std::fs::read_to_string(trust_file)
                    .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read trust anchors file: {}", e)))?)
            } else {
                None
            };
            
            // Parse verification time if provided
            let verification_time = if let Some(time_str) = &args.verification_time {
                Some(DateTime::parse_from_rfc3339(time_str)
                    .map_err(|e| rustmq::error::RustMqError::Config(format!("Invalid verification time format: {}", e)))?
                    .with_timezone(&Utc))
            } else {
                None
            };
            
            // Build request
            let mut request = serde_json::json!({
                "certificate_chain_pem": chain_pem,
                "validation_method": "webpki",
                "check_expiry": args.check_expiry
            });
            
            if let Some(trust_anchors) = trust_anchors_pem {
                request["trust_anchors_pem"] = serde_json::Value::String(trust_anchors);
            }
            
            if let Some(verification_time) = verification_time {
                request["verification_time"] = serde_json::Value::String(verification_time.to_rfc3339());
            }
            
            let progress = ProgressIndicator::new("Validating certificate chain with WebPKI", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/webpki/validate-chain", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_success("Certificate chain validation completed successfully", cli.no_color);
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "WebPKI chain validation failed".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        WebpkiCommands::TrustAnchors(args) => {
            if args.list {
                debug!("Listing WebPKI trust anchors");
                
                let response = api_client.get::<serde_json::Value>("/api/v1/security/webpki/trust-anchors").await?;
                
                if response.success {
                    if let Some(data) = &response.data {
                        print_output(data, cli.format, cli.no_color)?;
                    } else {
                        print_info("No trust anchors configured", cli.no_color);
                    }
                } else {
                    let error_msg = response.error.unwrap_or_else(|| "Failed to list trust anchors".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            } else if let Some(add_file) = &args.add_file {
                info!("Adding trust anchor from file: {:?}", add_file);
                
                let trust_anchor_pem = std::fs::read_to_string(add_file)
                    .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read trust anchor file: {}", e)))?;
                
                let request = serde_json::json!({
                    "trust_anchor_pem": trust_anchor_pem
                });
                
                let progress = ProgressIndicator::new("Adding trust anchor", cli.no_color);
                progress.start();
                
                let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/webpki/trust-anchors", &request).await?;
                
                if response.success {
                    progress.finish(true);
                    print_success("Trust anchor added successfully", cli.no_color);
                    if let Some(data) = &response.data {
                        print_output(data, cli.format, cli.no_color)?;
                    }
                } else {
                    progress.finish(false);
                    let error_msg = response.error.unwrap_or_else(|| "Failed to add trust anchor".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            } else if let Some(thumbprint) = &args.remove {
                info!("Removing trust anchor with thumbprint: {}", thumbprint);
                
                let path = format!("/api/v1/security/webpki/trust-anchors/{}", thumbprint);
                let response = api_client.delete::<serde_json::Value>(&path).await?;
                
                if response.success {
                    print_success(&format!("Trust anchor '{}' removed successfully", thumbprint), cli.no_color);
                } else {
                    let error_msg = response.error.unwrap_or_else(|| "Failed to remove trust anchor".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            } else if args.validate {
                info!("Validating trust anchor configuration");
                
                let response = api_client.post::<serde_json::Value, serde_json::Value>("/api/v1/security/webpki/trust-anchors/validate", &serde_json::json!({})).await?;
                
                if response.success {
                    print_success("Trust anchor configuration is valid", cli.no_color);
                    if let Some(data) = &response.data {
                        print_output(data, cli.format, cli.no_color)?;
                    }
                } else {
                    let error_msg = response.error.unwrap_or_else(|| "Trust anchor validation failed".to_string());
                    print_error(&error_msg, cli.no_color);
                    std::process::exit(1);
                }
            } else {
                print_error("No trust anchor operation specified. Use --list, --add-file, --remove, or --validate", cli.no_color);
                std::process::exit(1);
            }
        }
        WebpkiCommands::Benchmark(args) => {
            info!("Running WebPKI validation benchmark with {} iterations", args.iterations);
            
            // Read certificate file
            let cert_pem = std::fs::read_to_string(&args.cert_file)
                .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read certificate file: {}", e)))?;
            
            let request = serde_json::json!({
                "certificate_pem": cert_pem,
                "iterations": args.iterations,
                "compare_legacy": args.compare_legacy,
                "detailed": args.detailed
            });
            
            let progress = ProgressIndicator::new(&format!("Running {} validation iterations", args.iterations), cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/webpki/benchmark", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_success("WebPKI benchmark completed successfully", cli.no_color);
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "WebPKI benchmark failed".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        WebpkiCommands::Status => {
            debug!("Getting WebPKI status and configuration");
            
            let response = api_client.get::<serde_json::Value>("/api/v1/security/webpki/status").await?;
            
            if response.success {
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                }
            } else {
                let error_msg = response.error.unwrap_or_else(|| "Failed to get WebPKI status".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
        WebpkiCommands::Compare(args) => {
            info!("Comparing validation methods for certificate: {:?}", args.cert_file);
            
            // Read certificate file
            let cert_pem = std::fs::read_to_string(&args.cert_file)
                .map_err(|e| rustmq::error::RustMqError::Config(format!("Failed to read certificate file: {}", e)))?;
            
            let mut request = serde_json::json!({
                "certificate_pem": cert_pem,
                "verbose": args.verbose
            });
            
            if let Some(hostname) = &args.hostname {
                request["hostname"] = serde_json::Value::String(hostname.clone());
            }
            
            let progress = ProgressIndicator::new("Comparing validation methods", cli.no_color);
            progress.start();
            
            let response: super::api_client::ApiResponse<serde_json::Value> = api_client.post("/api/v1/security/webpki/compare", &request).await?;
            
            if response.success {
                progress.finish(true);
                if let Some(data) = &response.data {
                    print_output(data, cli.format, cli.no_color)?;
                } else {
                    print_success("Validation comparison completed successfully", cli.no_color);
                }
            } else {
                progress.finish(false);
                let error_msg = response.error.unwrap_or_else(|| "Validation comparison failed".to_string());
                print_error(&error_msg, cli.no_color);
                std::process::exit(1);
            }
        }
    }
    
    Ok(())
}