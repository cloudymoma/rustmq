//! Controller ACL Service Integration
//!
//! Provides gRPC endpoints for ACL management operations through the controller.

use crate::error::{Result, RustMqError};
use crate::security::acl::{
    AclManager, AclManagerTrait, AclRule, VersionedAclRule, AclRuleFilter, 
    Permission
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;
use tonic::{Request, Response, Status};

/// gRPC ACL management service
pub struct AclControllerService {
    /// ACL manager instance
    acl_manager: Arc<AclManager>,
    /// Service configuration
    config: AclServiceConfig,
    /// Request rate limiter
    rate_limiter: Arc<AsyncRwLock<HashMap<String, RateLimiter>>>,
}

/// Configuration for ACL controller service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclServiceConfig {
    /// Enable request rate limiting
    pub enable_rate_limiting: bool,
    /// Maximum requests per second per client
    pub max_requests_per_second: u32,
    /// Maximum number of ACL rules per principal
    pub max_rules_per_principal: usize,
    /// Maximum batch operation size
    pub max_batch_size: usize,
    /// Enable detailed audit logging
    pub enable_detailed_audit: bool,
}

impl Default for AclServiceConfig {
    fn default() -> Self {
        Self {
            enable_rate_limiting: true,
            max_requests_per_second: 100,
            max_rules_per_principal: 1000,
            max_batch_size: 100,
            enable_detailed_audit: true,
        }
    }
}

/// Simple rate limiter implementation
#[derive(Debug)]
struct RateLimiter {
    requests: Vec<std::time::Instant>,
    max_requests: u32,
    window_duration: std::time::Duration,
}

impl RateLimiter {
    fn new(max_requests: u32, window_duration: std::time::Duration) -> Self {
        Self {
            requests: Vec::new(),
            max_requests,
            window_duration,
        }
    }

    fn check_rate_limit(&mut self) -> bool {
        let now = std::time::Instant::now();
        
        // Remove old requests outside the time window
        self.requests.retain(|&request_time| {
            now.duration_since(request_time) < self.window_duration
        });

        // Check if we're under the limit
        if self.requests.len() < self.max_requests as usize {
            self.requests.push(now);
            true
        } else {
            false
        }
    }
}

/// gRPC request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAclRuleRequest {
    pub rule: AclRuleProto,
    pub created_by: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAclRuleResponse {
    pub rule_id: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAclRuleRequest {
    pub rule_id: String,
    pub rule: AclRuleProto,
    pub updated_by: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateAclRuleResponse {
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAclRuleRequest {
    pub rule_id: String,
    pub deleted_by: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteAclRuleResponse {
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAclRuleRequest {
    pub rule_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAclRuleResponse {
    pub rule: Option<VersionedAclRuleProto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAclRulesRequest {
    pub filter: AclRuleFilterProto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAclRulesResponse {
    pub rules: Vec<VersionedAclRuleProto>,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateAccessRequest {
    pub principal: String,
    pub resource: String,
    pub operation: String,
    pub context: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluateAccessResponse {
    pub allowed: bool,
    pub reason: Option<String>,
    pub matching_rule_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPrincipalPermissionsRequest {
    pub principal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetPrincipalPermissionsResponse {
    pub permissions: Vec<AclPermissionProto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAclsRequest {
    pub broker_id: Option<String>, // If None, sync to all brokers
    pub force_full_sync: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAclsResponse {
    pub synced_brokers: Vec<String>,
    pub failed_brokers: Vec<String>,
    pub current_version: u64,
}

/// Protobuf-compatible ACL rule representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRuleProto {
    pub id: String,
    pub principal: String,
    pub resource_type: String,
    pub resource_pattern: String,
    pub operations: Vec<String>,
    pub effect: String,
    pub conditions: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedAclRuleProto {
    pub rule: AclRuleProto,
    pub version: u64,
    pub timestamp: u64,
    pub created_by: String,
    pub change_description: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRuleFilterProto {
    pub principal: Option<String>,
    pub resource_pattern: Option<String>,
    pub operations: Vec<String>,
    pub effect: Option<String>,
    pub version_range: Option<(u64, u64)>,
    pub include_deleted: bool,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclPermissionProto {
    pub resource_pattern: String,
    pub operations: Vec<String>,
    pub effect: String,
    pub rule_id: String,
}

impl AclControllerService {
    /// Create a new ACL controller service
    pub fn new(acl_manager: Arc<AclManager>, config: AclServiceConfig) -> Self {
        Self {
            acl_manager,
            config,
            rate_limiter: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }

    /// Check rate limits for a client
    async fn check_rate_limit(&self, client_id: &str) -> Result<()> {
        if !self.config.enable_rate_limiting {
            return Ok(());
        }

        let mut limiters = self.rate_limiter.write().await;
        let limiter = limiters.entry(client_id.to_string()).or_insert_with(|| {
            RateLimiter::new(
                self.config.max_requests_per_second,
                std::time::Duration::from_secs(1),
            )
        });

        if limiter.check_rate_limit() {
            Ok(())
        } else {
            Err(RustMqError::RateLimited(format!(
                "Rate limit exceeded for client: {}",
                client_id
            )))
        }
    }

    /// Extract client ID from request metadata
    fn extract_client_id<T>(&self, request: &Request<T>) -> String {
        // In a real implementation, this would extract client ID from authentication metadata
        request
            .remote_addr()
            .map(|addr| addr.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Convert internal ACL rule to protobuf format
    fn to_acl_rule_proto(&self, rule: &AclRule) -> AclRuleProto {
        AclRuleProto {
            id: rule.id.clone(),
            principal: rule.principal.clone(),
            resource_type: format!("{:?}", rule.resource.resource_type),
            resource_pattern: rule.resource.pattern.clone(),
            operations: rule.operations.iter().map(|op| format!("{:?}", op)).collect(),
            effect: format!("{:?}", rule.effect),
            conditions: rule.conditions.clone(),
        }
    }

    /// Convert protobuf ACL rule to internal format
    fn from_acl_rule_proto(&self, proto: &AclRuleProto) -> Result<AclRule> {
        use crate::security::acl::{ResourcePattern, ResourceType};

        let resource_type = match proto.resource_type.as_str() {
            "Topic" => ResourceType::Topic,
            "ConsumerGroup" => ResourceType::ConsumerGroup,
            "Broker" => ResourceType::Broker,
            "Cluster" => ResourceType::Cluster,
            _ => return Err(RustMqError::InvalidParameter(format!("Invalid resource type: {}", proto.resource_type))),
        };

        let operations = proto.operations
            .iter()
            .map(|op| match op.as_str() {
                "Read" => Ok(crate::security::acl::AclOperation::Read),
                "Write" => Ok(crate::security::acl::AclOperation::Write),
                "Admin" => Ok(crate::security::acl::AclOperation::Admin),
                "Connect" => Ok(crate::security::acl::AclOperation::Connect),
                "List" => Ok(crate::security::acl::AclOperation::List),
                _ => Err(RustMqError::InvalidParameter(format!("Invalid operation: {}", op))),
            })
            .collect::<Result<Vec<_>>>()?;

        let effect = match proto.effect.as_str() {
            "Allow" => crate::security::acl::Effect::Allow,
            "Deny" => crate::security::acl::Effect::Deny,
            _ => return Err(RustMqError::InvalidParameter(format!("Invalid effect: {}", proto.effect))),
        };

        Ok(AclRule {
            id: proto.id.clone(),
            principal: proto.principal.clone(),
            resource: ResourcePattern {
                resource_type,
                pattern: proto.resource_pattern.clone(),
            },
            operations,
            effect,
            conditions: proto.conditions.clone(),
        })
    }

    /// Convert versioned ACL rule to protobuf format
    fn to_versioned_acl_rule_proto(&self, rule: &VersionedAclRule) -> VersionedAclRuleProto {
        VersionedAclRuleProto {
            rule: self.to_acl_rule_proto(&rule.rule),
            version: rule.version,
            timestamp: rule.timestamp,
            created_by: rule.created_by.clone(),
            change_description: rule.change_description.clone(),
            deleted: rule.deleted,
        }
    }

    /// Convert protobuf filter to internal format
    fn from_acl_rule_filter_proto(&self, proto: &AclRuleFilterProto) -> AclRuleFilter {
        let operations = if proto.operations.is_empty() {
            None
        } else {
            Some(
                proto.operations
                    .iter()
                    .filter_map(|op| match op.as_str() {
                        "Read" => Some(crate::security::acl::AclOperation::Read),
                        "Write" => Some(crate::security::acl::AclOperation::Write),
                        "Admin" => Some(crate::security::acl::AclOperation::Admin),
                        "Connect" => Some(crate::security::acl::AclOperation::Connect),
                        "List" => Some(crate::security::acl::AclOperation::List),
                        _ => None,
                    })
                    .collect(),
            )
        };

        let effect = proto.effect.as_ref().and_then(|e| match e.as_str() {
            "Allow" => Some(crate::security::acl::Effect::Allow),
            "Deny" => Some(crate::security::acl::Effect::Deny),
            _ => None,
        });

        AclRuleFilter {
            principal: proto.principal.clone(),
            resource_pattern: proto.resource_pattern.clone(),
            operations,
            effect,
            version_range: proto.version_range,
            include_deleted: proto.include_deleted,
            limit: proto.limit.map(|l| l as usize),
        }
    }
}

/// ACL service trait for gRPC implementation
#[async_trait]
pub trait AclService {
    /// Create a new ACL rule
    async fn create_acl_rule(
        &self,
        request: Request<CreateAclRuleRequest>,
    ) -> std::result::Result<Response<CreateAclRuleResponse>, Status>;

    /// Update an existing ACL rule
    async fn update_acl_rule(
        &self,
        request: Request<UpdateAclRuleRequest>,
    ) -> std::result::Result<Response<UpdateAclRuleResponse>, Status>;

    /// Delete an ACL rule
    async fn delete_acl_rule(
        &self,
        request: Request<DeleteAclRuleRequest>,
    ) -> std::result::Result<Response<DeleteAclRuleResponse>, Status>;

    /// Get an ACL rule by ID
    async fn get_acl_rule(
        &self,
        request: Request<GetAclRuleRequest>,
    ) -> std::result::Result<Response<GetAclRuleResponse>, Status>;

    /// List ACL rules with filtering
    async fn list_acl_rules(
        &self,
        request: Request<ListAclRulesRequest>,
    ) -> std::result::Result<Response<ListAclRulesResponse>, Status>;

    /// Evaluate access for a principal/resource/operation combination
    async fn evaluate_access(
        &self,
        request: Request<EvaluateAccessRequest>,
    ) -> std::result::Result<Response<EvaluateAccessResponse>, Status>;

    /// Get all permissions for a principal
    async fn get_principal_permissions(
        &self,
        request: Request<GetPrincipalPermissionsRequest>,
    ) -> std::result::Result<Response<GetPrincipalPermissionsResponse>, Status>;

    /// Synchronize ACLs to brokers
    async fn sync_acls(
        &self,
        request: Request<SyncAclsRequest>,
    ) -> std::result::Result<Response<SyncAclsResponse>, Status>;
}

#[async_trait]
impl AclService for AclControllerService {
    async fn create_acl_rule(
        &self,
        request: Request<CreateAclRuleRequest>,
    ) -> Result<Response<CreateAclRuleResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();
        
        // Convert protobuf to internal format
        let rule = self.from_acl_rule_proto(&req.rule)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // Create the rule
        let rule_id = self.acl_manager
            .create_acl_rule(rule, &req.created_by, &req.description)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let version = self.acl_manager
            .get_acl_version()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(CreateAclRuleResponse { rule_id, version }))
    }

    async fn update_acl_rule(
        &self,
        request: Request<UpdateAclRuleRequest>,
    ) -> Result<Response<UpdateAclRuleResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();
        
        // Convert protobuf to internal format
        let rule = self.from_acl_rule_proto(&req.rule)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // Update the rule
        self.acl_manager
            .update_acl_rule(&req.rule_id, rule, &req.updated_by, &req.description)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let version = self.acl_manager
            .get_acl_version()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(UpdateAclRuleResponse { version }))
    }

    async fn delete_acl_rule(
        &self,
        request: Request<DeleteAclRuleRequest>,
    ) -> Result<Response<DeleteAclRuleResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();

        // Delete the rule
        self.acl_manager
            .delete_acl_rule(&req.rule_id, &req.deleted_by, &req.description)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let version = self.acl_manager
            .get_acl_version()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(DeleteAclRuleResponse { version }))
    }

    async fn get_acl_rule(
        &self,
        request: Request<GetAclRuleRequest>,
    ) -> Result<Response<GetAclRuleResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();

        // Get the rule
        let rule = self.acl_manager
            .get_acl_rule(&req.rule_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = GetAclRuleResponse {
            rule: rule.map(|r| self.to_versioned_acl_rule_proto(&r)),
        };

        Ok(Response::new(response))
    }

    async fn list_acl_rules(
        &self,
        request: Request<ListAclRulesRequest>,
    ) -> Result<Response<ListAclRulesResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();
        let filter = self.from_acl_rule_filter_proto(&req.filter);

        // List rules
        let rules = self.acl_manager
            .list_acl_rules(filter)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = ListAclRulesResponse {
            total_count: rules.len() as u64,
            rules: rules.into_iter().map(|r| self.to_versioned_acl_rule_proto(&r)).collect(),
        };

        Ok(Response::new(response))
    }

    async fn evaluate_access(
        &self,
        request: Request<EvaluateAccessRequest>,
    ) -> Result<Response<EvaluateAccessResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits (higher limit for evaluation requests)
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();

        // Parse operation
        let operation = match req.operation.as_str() {
            "Read" => Permission::Read,
            "Write" => Permission::Write,
            "Admin" => Permission::Admin,
            "Connect" => Permission::Connect,
            "List" => Permission::List,
            _ => return Err(Status::invalid_argument(format!("Invalid operation: {}", req.operation))),
        };

        // Evaluate access
        let allowed = self.acl_manager
            .evaluate_access(&req.principal, &req.resource, operation, Some(req.context))
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = EvaluateAccessResponse {
            allowed,
            reason: None, // Could be enhanced to provide detailed reasons
            matching_rule_id: None, // Could be enhanced to return the matching rule ID
        };

        Ok(Response::new(response))
    }

    async fn get_principal_permissions(
        &self,
        request: Request<GetPrincipalPermissionsRequest>,
    ) -> Result<Response<GetPrincipalPermissionsResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();

        // Get permissions
        let permissions = self.acl_manager
            .get_principal_permissions(&req.principal)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let proto_permissions = permissions
            .into_iter()
            .map(|p| AclPermissionProto {
                resource_pattern: p.resource_pattern,
                operations: p.operations.into_iter().map(|op| format!("{:?}", op)).collect(),
                effect: format!("{:?}", p.effect),
                rule_id: p.rule_id,
            })
            .collect();

        let response = GetPrincipalPermissionsResponse {
            permissions: proto_permissions,
        };

        Ok(Response::new(response))
    }

    async fn sync_acls(
        &self,
        request: Request<SyncAclsRequest>,
    ) -> Result<Response<SyncAclsResponse>, Status> {
        let client_id = self.extract_client_id(&request);
        
        // Check rate limits
        self.check_rate_limit(&client_id).await
            .map_err(|e| Status::resource_exhausted(e.to_string()))?;

        let req = request.into_inner();

        let (synced_brokers, failed_brokers) = if let Some(broker_id) = req.broker_id {
            // Sync to specific broker
            match self.acl_manager.sync_acls_to_broker(&broker_id).await {
                Ok(_) => (vec![broker_id], vec![]),
                Err(_) => (vec![], vec![broker_id]),
            }
        } else {
            // Sync to all brokers
            match self.acl_manager.sync_acls_to_all_brokers().await {
                Ok(_) => {
                    // In a real implementation, we'd track which brokers succeeded/failed
                    (vec!["all".to_string()], vec![])
                }
                Err(_) => (vec![], vec!["all".to_string()]),
            }
        };

        let version = self.acl_manager
            .get_acl_version()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let response = SyncAclsResponse {
            synced_brokers,
            failed_brokers,
            current_version: version,
        };

        Ok(Response::new(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::acl::{ResourcePattern, ResourceType};

    #[test]
    fn test_acl_rule_proto_conversion() {
        let original_rule = AclRule {
            id: "test-rule".to_string(),
            principal: "user-123".to_string(),
            resource: ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            operations: vec![crate::security::acl::AclOperation::Read],
            effect: crate::security::acl::Effect::Allow,
            conditions: std::collections::HashMap::new(),
        };

        let service = AclControllerService::new(
            Arc::new(AclManager::new_mock()), // Would need to implement mock
            AclServiceConfig::default(),
        );

        let proto = service.to_acl_rule_proto(&original_rule);
        let converted_back = service.from_acl_rule_proto(&proto).unwrap();

        assert_eq!(original_rule.id, converted_back.id);
        assert_eq!(original_rule.principal, converted_back.principal);
        assert_eq!(original_rule.resource.pattern, converted_back.resource.pattern);
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(2, std::time::Duration::from_secs(1));
        
        // First two requests should succeed
        assert!(limiter.check_rate_limit());
        assert!(limiter.check_rate_limit());
        
        // Third request should fail
        assert!(!limiter.check_rate_limit());
    }

    #[test]
    fn test_acl_service_config_default() {
        let config = AclServiceConfig::default();
        assert!(config.enable_rate_limiting);
        assert_eq!(config.max_requests_per_second, 100);
        assert_eq!(config.max_rules_per_principal, 1000);
    }
}