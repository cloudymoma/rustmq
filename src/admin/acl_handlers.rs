use crate::Result;
use crate::security::acl::{ResourcePattern, ResourceType, manager::AclManagerTrait};
use crate::security::{
    AclManager, AclOperation, AclRule, AuthorizationManager, Effect, PermissionSet,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info};

/// ACL management handlers for the Security API
pub struct AclHandlers {
    acl_manager: Arc<AclManager>,
    authorization_manager: Arc<AuthorizationManager>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AclRuleCreationResponse {
    pub rule_id: String,
    pub principal: String,
    pub resource_pattern: String,
    pub resource_type: String,
    pub operation: String,
    pub effect: String,
    pub conditions: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AclEvaluationMetrics {
    pub total_rules_evaluated: u32,
    pub cache_hits: u32,
    pub cache_misses: u32,
    pub evaluation_time_breakdown: HashMap<String, u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PrincipalAnalysis {
    pub principal: String,
    pub total_rules: u32,
    pub allow_rules: u32,
    pub deny_rules: u32,
    pub resource_access: HashMap<String, Vec<String>>,
    pub effective_permissions: PermissionSet,
    pub risk_assessment: RiskAssessment,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RiskAssessment {
    pub risk_level: String,
    pub factors: Vec<String>,
    pub recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AclSyncStatus {
    pub version: u64,
    pub last_sync: DateTime<Utc>,
    pub broker_sync_status: HashMap<String, BrokerSyncInfo>,
    pub pending_operations: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BrokerSyncInfo {
    pub broker_id: String,
    pub last_successful_sync: DateTime<Utc>,
    pub current_version: u64,
    pub sync_lag: i64,
    pub status: String,
}

impl AclHandlers {
    pub fn new(
        acl_manager: Arc<AclManager>,
        authorization_manager: Arc<AuthorizationManager>,
    ) -> Self {
        Self {
            acl_manager,
            authorization_manager,
        }
    }

    /// Create a new ACL rule with validation
    pub async fn create_acl_rule(
        &self,
        principal: String,
        resource_pattern: String,
        resource_type: String,
        operation: String,
        effect: String,
        conditions: Option<HashMap<String, String>>,
    ) -> Result<AclRuleCreationResponse> {
        info!("Creating ACL rule for principal: {}", principal);

        // Validate inputs
        self.validate_principal(&principal)?;
        self.validate_resource_pattern(&resource_pattern)?;

        // Parse and validate resource type
        let resource_type_enum = self.parse_resource_type(&resource_type)?;

        // Parse and validate operation
        let operation_enum = self.parse_operation(&operation)?;

        // Parse and validate effect
        let effect_enum = self.parse_effect(&effect)?;

        // Create ACL rule
        let acl_rule = AclRule {
            id: uuid::Uuid::new_v4().to_string(), // Generate a unique ID
            principal: principal.clone(),
            resource: ResourcePattern::new(resource_type_enum, resource_pattern.clone()),
            operations: vec![operation_enum],
            effect: effect_enum,
            conditions: conditions.clone().unwrap_or_default(),
        };

        // Create the rule in AclManager
        let created_rule = self.acl_manager.create_rule(acl_rule).await?;

        // Invalidate cache for the principal
        self.authorization_manager
            .invalidate_principal_cache(&principal)
            .await?;

        let response = AclRuleCreationResponse {
            rule_id: created_rule.id.clone(),
            principal,
            resource_pattern,
            resource_type,
            operation,
            effect,
            conditions: conditions.unwrap_or_default(),
            created_at: Utc::now(),
            version: 1,
        };

        info!("Successfully created ACL rule: {}", response.rule_id);
        Ok(response)
    }

    /// Update an existing ACL rule
    pub async fn update_acl_rule(
        &self,
        rule_id: String,
        principal: Option<String>,
        resource_pattern: Option<String>,
        resource_type: Option<String>,
        operation: Option<String>,
        effect: Option<String>,
        conditions: Option<HashMap<String, String>>,
    ) -> Result<AclRuleCreationResponse> {
        info!("Updating ACL rule: {}", rule_id);

        // Get the existing rule
        let mut existing_rule = self.acl_manager.get_rule(rule_id.clone()).await?;
        let original_principal = existing_rule.principal.clone();

        // Update fields if provided
        if let Some(new_principal) = principal {
            self.validate_principal(&new_principal)?;
            existing_rule.principal = new_principal;
        }

        if let Some(new_pattern) = resource_pattern {
            self.validate_resource_pattern(&new_pattern)?;
            existing_rule.resource =
                ResourcePattern::new(existing_rule.resource.resource_type, new_pattern);
        }

        if let Some(new_resource_type) = resource_type {
            let resource_type_enum = self.parse_resource_type(&new_resource_type)?;
            existing_rule.resource =
                ResourcePattern::new(resource_type_enum, existing_rule.resource.pattern.clone());
        }

        if let Some(new_operation) = operation {
            let operation_enum = self.parse_operation(&new_operation)?;
            existing_rule.operations = vec![operation_enum];
        }

        if let Some(new_effect) = effect {
            let effect_enum = self.parse_effect(&new_effect)?;
            existing_rule.effect = effect_enum;
        }

        if let Some(new_conditions) = conditions {
            existing_rule.conditions = new_conditions;
        }

        // Note: AclRule doesn't have updated_at or version fields

        // Update the rule
        let updated_rule = self
            .acl_manager
            .update_rule(rule_id.clone(), existing_rule.clone())
            .await?;

        // Invalidate caches for both old and new principals
        self.authorization_manager
            .invalidate_principal_cache(&original_principal)
            .await?;
        if original_principal != updated_rule.principal {
            self.authorization_manager
                .invalidate_principal_cache(&updated_rule.principal)
                .await?;
        }

        let response = AclRuleCreationResponse {
            rule_id: updated_rule.id.clone(),
            principal: updated_rule.principal,
            resource_pattern: updated_rule.resource.pattern,
            resource_type: format!("{:?}", updated_rule.resource.resource_type),
            operation: format!(
                "{:?}",
                updated_rule
                    .operations
                    .first()
                    .unwrap_or(&AclOperation::Read)
            ),
            effect: format!("{:?}", updated_rule.effect),
            conditions: updated_rule.conditions,
            created_at: Utc::now(), // AclRule doesn't have created_at field
            version: 1,             // AclRule doesn't have version field
        };

        info!("Successfully updated ACL rule: {}", rule_id);
        Ok(response)
    }

    /// Delete an ACL rule
    pub async fn delete_acl_rule(&self, rule_id: String) -> Result<String> {
        info!("Deleting ACL rule: {}", rule_id);

        // Get the rule to find the principal for cache invalidation
        let rule = self.acl_manager.get_rule(rule_id.clone()).await?;
        let principal = rule.principal.clone();

        // Delete the rule
        self.acl_manager.delete_rule(rule_id.clone()).await?;

        // Invalidate cache for the principal
        self.authorization_manager
            .invalidate_principal_cache(&principal)
            .await?;

        info!("Successfully deleted ACL rule: {}", rule_id);
        Ok(format!("ACL rule '{}' deleted successfully", rule_id))
    }

    /// List ACL rules with filtering and pagination
    pub async fn list_acl_rules(&self, filters: HashMap<String, String>) -> Result<Vec<AclRule>> {
        debug!("Listing ACL rules with filters: {:?}", filters);

        let principal_filter = filters.get("principal").cloned();
        let resource_filter = filters.get("resource").cloned();
        let effect_filter = filters
            .get("effect")
            .and_then(|e| self.parse_effect(e).ok());

        let limit = filters
            .get("limit")
            .and_then(|l| l.parse::<usize>().ok())
            .unwrap_or(100);

        let offset = filters
            .get("offset")
            .and_then(|o| o.parse::<usize>().ok())
            .unwrap_or(0);

        // Build filter for AclManager
        let acl_filter = crate::security::acl::storage::AclRuleFilter {
            principal: principal_filter.clone(),
            resource_pattern: resource_filter.clone(),
            operations: None, // We'll filter operations manually if needed
            effect: effect_filter.clone(),
            version_range: None,
            include_deleted: false,
            limit: Some(limit + offset), // Get enough for pagination
        };

        // Get rules from AclManager
        let mut rules = self.acl_manager.list_rules(Some(acl_filter)).await?;

        // Apply filters
        if let Some(principal) = principal_filter {
            rules.retain(|rule| rule.principal.contains(&principal));
        }

        if let Some(resource) = resource_filter {
            rules.retain(|rule| rule.resource.pattern.contains(&resource));
        }

        if let Some(effect) = effect_filter {
            rules.retain(|rule| rule.effect == effect);
        }

        // Apply pagination
        let total = rules.len();
        let start = offset.min(total);
        let end = (offset + limit).min(total);
        rules = rules[start..end].to_vec();

        debug!(
            "Found {} ACL rules (showing {} from offset {})",
            total,
            rules.len(),
            offset
        );
        Ok(rules)
    }

    /// Evaluate ACL permissions for a principal
    pub async fn evaluate_acl(
        &self,
        principal: String,
        resource: String,
        operation: String,
        context: Option<HashMap<String, String>>,
    ) -> Result<(bool, String, Vec<String>, AclEvaluationMetrics)> {
        debug!(
            "Evaluating ACL for principal: {} on resource: {}",
            principal, resource
        );

        let start_time = std::time::Instant::now();

        // Parse operation
        let operation_enum = self.parse_operation(&operation)?;

        // Create evaluation context
        let eval_context = context.unwrap_or_default();

        // Perform authorization check using AuthorizationManager
        let l1_cache = self.authorization_manager.create_connection_cache();
        let principal_obj: crate::security::auth::Principal = Arc::from(principal.as_str());
        let permission = match operation_enum {
            AclOperation::Read | AclOperation::Describe | AclOperation::List => {
                crate::security::auth::Permission::Read
            }
            AclOperation::Write | AclOperation::Create => crate::security::auth::Permission::Write,
            AclOperation::Admin
            | AclOperation::Alter
            | AclOperation::Delete
            | AclOperation::Cluster
            | AclOperation::All => crate::security::auth::Permission::Admin,
            AclOperation::Connect => crate::security::auth::Permission::Read, // Map connect to read for simplicity
        };

        let allowed = self
            .authorization_manager
            .check_permission(&*l1_cache, &principal_obj, &resource, permission)
            .await?;

        let authorization_result = if allowed {
            crate::security::acl::policy::PolicyDecision::Allow
        } else {
            crate::security::acl::policy::PolicyDecision::Deny {
                reason: format!("Permission denied for {} on {}", principal, resource),
            }
        };

        let evaluation_time = start_time.elapsed().as_nanos() as u64;

        // Get metrics (simplified for now - PolicyDecision doesn't have metrics fields)
        let metrics = AclEvaluationMetrics {
            total_rules_evaluated: 0, // TODO: Add metrics to PolicyDecision
            cache_hits: 0,
            cache_misses: 0,
            evaluation_time_breakdown: {
                let mut breakdown = HashMap::new();
                breakdown.insert("total_time_ns".to_string(), evaluation_time);
                breakdown.insert("cache_lookup_ns".to_string(), 0u64);
                breakdown.insert("rule_evaluation_ns".to_string(), evaluation_time);
                breakdown
            },
        };

        let effect_str = match authorization_result {
            crate::security::acl::policy::PolicyDecision::Allow => "allow".to_string(),
            crate::security::acl::policy::PolicyDecision::Deny { .. } => "deny".to_string(),
            crate::security::acl::policy::PolicyDecision::NoMatch => "deny".to_string(),
        };

        let allowed = matches!(
            authorization_result,
            crate::security::acl::policy::PolicyDecision::Allow
        );

        Ok((
            allowed,
            effect_str,
            Vec::new(), // TODO: PolicyDecision doesn't have matched_rules field
            metrics,
        ))
    }

    /// Get comprehensive permissions for a principal
    pub async fn get_principal_permissions(&self, principal: String) -> Result<PrincipalAnalysis> {
        debug!("Getting permissions analysis for principal: {}", principal);

        // Get all rules for this principal
        let versioned_rules = self.acl_manager.get_rules_for_principal(&principal).await?;
        let rules: Vec<AclRule> = versioned_rules
            .iter()
            .filter(|vr| !vr.deleted)
            .map(|vr| vr.rule.clone())
            .collect();

        // Analyze rules
        let total_rules = rules.len() as u32;
        let allow_rules = rules
            .iter()
            .filter(|rule| rule.effect == Effect::Allow)
            .count() as u32;
        let deny_rules = total_rules - allow_rules;

        // Build resource access map
        let mut resource_access: HashMap<String, Vec<String>> = HashMap::new();
        for rule in &rules {
            let resource_key = format!(
                "{}:{}",
                format!("{:?}", rule.resource.resource_type).to_lowercase(),
                rule.resource.pattern
            );
            let operation_str = format!(
                "{:?}",
                rule.operations.first().unwrap_or(&AclOperation::Read)
            )
            .to_lowercase();

            resource_access
                .entry(resource_key)
                .or_insert_with(Vec::new)
                .push(operation_str);
        }

        // Get effective permissions from ACL manager
        let permissions = self
            .acl_manager
            .get_principal_permissions(&principal)
            .await?;
        let mut effective_permissions = PermissionSet::new();

        // Convert permissions to PermissionSet
        for perm in permissions {
            for op in perm.operations {
                // Convert AclOperation to Permission
                let permission = match op {
                    AclOperation::Read | AclOperation::Describe | AclOperation::List => {
                        crate::security::auth::Permission::Read
                    }
                    AclOperation::Write | AclOperation::Create => {
                        crate::security::auth::Permission::Write
                    }
                    AclOperation::Admin
                    | AclOperation::Alter
                    | AclOperation::Delete
                    | AclOperation::Cluster
                    | AclOperation::All => crate::security::auth::Permission::Admin,
                    AclOperation::Connect => crate::security::auth::Permission::Read,
                };
                effective_permissions.add_permission(permission);
            }
        }

        // Perform risk assessment
        let risk_assessment = self.assess_principal_risk(&principal, &rules).await?;

        let analysis = PrincipalAnalysis {
            principal,
            total_rules,
            allow_rules,
            deny_rules,
            resource_access,
            effective_permissions,
            risk_assessment,
        };

        Ok(analysis)
    }

    /// Get rules affecting a specific resource
    pub async fn get_resource_rules(
        &self,
        resource: String,
    ) -> Result<(Vec<AclRule>, HashMap<String, Vec<String>>)> {
        debug!("Getting rules for resource: {}", resource);

        // Get all rules that might affect this resource
        let filter = crate::security::acl::storage::AclRuleFilter {
            principal: None,
            resource_pattern: Some(resource.clone()),
            operations: None,
            effect: None,
            version_range: None,
            include_deleted: false,
            limit: None,
        };
        let all_rules = self.acl_manager.list_rules(Some(filter)).await?;
        let matching_rules: Vec<AclRule> = all_rules
            .into_iter()
            .filter(|rule| self.resource_matches_pattern(&resource, &rule.resource.pattern))
            .collect();

        // Build effective permissions map
        let mut effective_permissions: HashMap<String, Vec<String>> = HashMap::new();
        for rule in &matching_rules {
            if rule.effect == Effect::Allow {
                let operation_str = format!(
                    "{:?}",
                    rule.operations.first().unwrap_or(&AclOperation::Read)
                )
                .to_lowercase();
                effective_permissions
                    .entry(rule.principal.clone())
                    .or_insert_with(Vec::new)
                    .push(operation_str);
            }
        }

        Ok((matching_rules, effective_permissions))
    }

    /// Perform bulk ACL evaluation
    pub async fn bulk_evaluate_acl(
        &self,
        evaluations: Vec<(String, String, String, Option<HashMap<String, String>>)>,
    ) -> Result<(Vec<(bool, String, Vec<String>)>, AclEvaluationMetrics)> {
        debug!("Bulk evaluating {} ACL requests", evaluations.len());

        let start_time = std::time::Instant::now();
        let mut results = Vec::new();
        let mut total_cache_hits = 0;
        let mut total_cache_misses = 0;
        let mut total_rules_evaluated = 0;

        for (principal, resource, operation, context) in evaluations {
            let (allowed, effect, matched_rules, metrics) = self
                .evaluate_acl(principal, resource, operation, context)
                .await?;

            results.push((allowed, effect, matched_rules));
            total_cache_hits += metrics.cache_hits;
            total_cache_misses += metrics.cache_misses;
            total_rules_evaluated += metrics.total_rules_evaluated;
        }

        let total_time = start_time.elapsed().as_nanos() as u64;

        let metrics = AclEvaluationMetrics {
            total_rules_evaluated,
            cache_hits: total_cache_hits,
            cache_misses: total_cache_misses,
            evaluation_time_breakdown: {
                let mut breakdown = HashMap::new();
                breakdown.insert("total_time_ns".to_string(), total_time);
                breakdown.insert(
                    "average_per_evaluation_ns".to_string(),
                    total_time / results.len() as u64,
                );
                breakdown
            },
        };

        Ok((results, metrics))
    }

    /// Force ACL synchronization to brokers
    pub async fn sync_acl(&self) -> Result<AclSyncStatus> {
        info!("Forcing ACL synchronization to brokers");

        // Trigger synchronization
        self.acl_manager.force_sync().await?;

        // Get sync status
        let sync_status = self.get_acl_sync_status().await?;

        Ok(sync_status)
    }

    /// Get current ACL version and sync status
    pub async fn get_acl_version(&self) -> Result<(u64, DateTime<Utc>, u32)> {
        debug!("Getting current ACL version");

        let version = self.acl_manager.get_current_version().await;
        let last_updated = self
            .acl_manager
            .get_last_update_time()
            .await
            .unwrap_or_else(|| chrono::Utc::now());
        let rules_count = self.acl_manager.get_rules_count().await as u32;

        Ok((version, last_updated, rules_count))
    }

    /// Invalidate ACL caches
    pub async fn invalidate_acl_cache(&self) -> Result<String> {
        info!("Invalidating ACL caches");

        // Invalidate all authorization caches
        self.authorization_manager.invalidate_all_caches().await?;

        Ok("ACL caches invalidated successfully".to_string())
    }

    /// Warm ACL caches for specific principals
    pub async fn warm_acl_cache(&self, principals: Vec<String>) -> Result<String> {
        info!("Warming ACL caches for {} principals", principals.len());

        // Get all resources to warm the cache with
        // In production, you might want to limit this to frequently accessed resources
        let resources: Vec<String> = vec![
            "topic.*".to_string(),
            "group.*".to_string(),
            "cluster.*".to_string(),
        ];

        // Warm caches
        self.authorization_manager
            .warm_cache(principals, resources)
            .await?;

        Ok("ACL caches warmed successfully".to_string())
    }

    // Private helper methods

    async fn get_acl_sync_status(&self) -> Result<AclSyncStatus> {
        // This would normally query the actual broker sync status
        // For now, return mock data with actual version
        let sync_status = AclSyncStatus {
            version: self.acl_manager.get_current_version().await,
            last_sync: Utc::now() - chrono::Duration::minutes(5),
            broker_sync_status: {
                let mut status = HashMap::new();
                status.insert(
                    "broker-1".to_string(),
                    BrokerSyncInfo {
                        broker_id: "broker-1".to_string(),
                        last_successful_sync: Utc::now() - chrono::Duration::minutes(5),
                        current_version: 123,
                        sync_lag: 0,
                        status: "synchronized".to_string(),
                    },
                );
                status.insert(
                    "broker-2".to_string(),
                    BrokerSyncInfo {
                        broker_id: "broker-2".to_string(),
                        last_successful_sync: Utc::now() - chrono::Duration::minutes(7),
                        current_version: 122,
                        sync_lag: 1,
                        status: "lagging".to_string(),
                    },
                );
                status
            },
            pending_operations: 0,
        };

        Ok(sync_status)
    }

    async fn assess_principal_risk(
        &self,
        principal: &str,
        rules: &[AclRule],
    ) -> Result<RiskAssessment> {
        let mut risk_factors = Vec::new();
        let mut recommendations = Vec::new();

        // Check for overly permissive rules
        let wildcard_rules = rules
            .iter()
            .filter(|rule| rule.resource.pattern.contains('*'))
            .count();

        if wildcard_rules > 5 {
            risk_factors.push("High number of wildcard resource patterns".to_string());
            recommendations
                .push("Review and restrict wildcard patterns where possible".to_string());
        }

        // Check for administrative privileges
        let admin_operations = rules
            .iter()
            .filter(|rule| {
                rule.operations
                    .iter()
                    .any(|op| matches!(op, AclOperation::Admin | AclOperation::Cluster))
            })
            .count();

        if admin_operations > 0 {
            risk_factors.push("Has administrative operations".to_string());
            recommendations
                .push("Ensure administrative access is necessary and well-monitored".to_string());
        }

        // Determine risk level
        let risk_level = match risk_factors.len() {
            0 => "low",
            1..=2 => "medium",
            _ => "high",
        };

        Ok(RiskAssessment {
            risk_level: risk_level.to_string(),
            factors: risk_factors,
            recommendations,
        })
    }

    fn validate_principal(&self, principal: &str) -> Result<()> {
        Self::validate_principal_static(principal)
    }

    fn validate_resource_pattern(&self, pattern: &str) -> Result<()> {
        Self::validate_resource_pattern_static(pattern)
    }

    // Static versions for testing
    fn validate_principal_static(principal: &str) -> Result<()> {
        if principal.is_empty() {
            return Err(crate::error::RustMqError::ValidationError(
                "Principal cannot be empty".to_string(),
            ));
        }

        if principal.len() > 255 {
            return Err(crate::error::RustMqError::ValidationError(
                "Principal length cannot exceed 255 characters".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_resource_pattern_static(pattern: &str) -> Result<()> {
        if pattern.is_empty() {
            return Err(crate::error::RustMqError::ValidationError(
                "Resource pattern cannot be empty".to_string(),
            ));
        }

        if pattern.len() > 512 {
            return Err(crate::error::RustMqError::ValidationError(
                "Resource pattern length cannot exceed 512 characters".to_string(),
            ));
        }

        Ok(())
    }

    fn parse_resource_type(&self, resource_type: &str) -> Result<ResourceType> {
        Self::parse_resource_type_static(resource_type)
    }

    fn parse_operation(&self, operation: &str) -> Result<AclOperation> {
        Self::parse_operation_static(operation)
    }

    fn parse_effect(&self, effect: &str) -> Result<Effect> {
        Self::parse_effect_static(effect)
    }

    // Static versions of parsing functions for testing
    fn parse_resource_type_static(resource_type: &str) -> Result<ResourceType> {
        match resource_type.to_lowercase().as_str() {
            "topic" => Ok(ResourceType::Topic),
            "consumer_group" | "group" => Ok(ResourceType::ConsumerGroup),
            "broker" => Ok(ResourceType::Broker),
            "cluster" => Ok(ResourceType::Cluster),
            _ => Err(crate::error::RustMqError::ValidationError(format!(
                "Invalid resource type: {}",
                resource_type
            ))),
        }
    }

    fn parse_operation_static(operation: &str) -> Result<AclOperation> {
        match operation.to_lowercase().as_str() {
            "read" => Ok(AclOperation::Read),
            "write" => Ok(AclOperation::Write),
            "create" => Ok(AclOperation::Create),
            "delete" => Ok(AclOperation::Delete),
            "alter" => Ok(AclOperation::Alter),
            "describe" => Ok(AclOperation::Describe),
            "cluster" => Ok(AclOperation::Cluster),
            "admin" => Ok(AclOperation::Admin),
            "all" => Ok(AclOperation::All),
            _ => Err(crate::error::RustMqError::ValidationError(format!(
                "Invalid operation: {}",
                operation
            ))),
        }
    }

    fn parse_effect_static(effect: &str) -> Result<Effect> {
        match effect.to_lowercase().as_str() {
            "allow" => Ok(Effect::Allow),
            "deny" => Ok(Effect::Deny),
            _ => Err(crate::error::RustMqError::ValidationError(format!(
                "Invalid effect: {}",
                effect
            ))),
        }
    }

    fn resource_matches_pattern(&self, resource: &str, pattern: &str) -> bool {
        Self::resource_matches_pattern_static(resource, pattern)
    }

    // Static version for testing
    fn resource_matches_pattern_static(resource: &str, pattern: &str) -> bool {
        // Simple pattern matching implementation
        if pattern.contains('*') {
            let prefix = pattern.trim_end_matches('*');
            resource.starts_with(prefix)
        } else {
            resource == pattern
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test the utility functions that don't require complex setup

    #[test]
    fn test_parse_resource_type_standalone() {
        // Test the static parsing functions without requiring struct instances
        assert_eq!(
            AclHandlers::parse_resource_type_static("topic").unwrap(),
            ResourceType::Topic
        );
        assert_eq!(
            AclHandlers::parse_resource_type_static("TOPIC").unwrap(),
            ResourceType::Topic
        );
        assert_eq!(
            AclHandlers::parse_resource_type_static("group").unwrap(),
            ResourceType::ConsumerGroup
        );
        assert!(AclHandlers::parse_resource_type_static("invalid").is_err());
    }

    #[test]
    fn test_parse_operation_standalone() {
        // Test the static parsing functions without requiring struct instances
        assert_eq!(
            AclHandlers::parse_operation_static("read").unwrap(),
            AclOperation::Read
        );
        assert_eq!(
            AclHandlers::parse_operation_static("READ").unwrap(),
            AclOperation::Read
        );
        assert_eq!(
            AclHandlers::parse_operation_static("write").unwrap(),
            AclOperation::Write
        );
        assert!(AclHandlers::parse_operation_static("invalid").is_err());
    }

    #[test]
    fn test_parse_effect_standalone() {
        // Test the static parsing functions without requiring struct instances
        assert_eq!(
            AclHandlers::parse_effect_static("allow").unwrap(),
            Effect::Allow
        );
        assert_eq!(
            AclHandlers::parse_effect_static("ALLOW").unwrap(),
            Effect::Allow
        );
        assert_eq!(
            AclHandlers::parse_effect_static("deny").unwrap(),
            Effect::Deny
        );
        assert!(AclHandlers::parse_effect_static("invalid").is_err());
    }

    #[test]
    fn test_resource_matches_pattern_standalone() {
        // Test the static parsing functions without requiring struct instances

        // Exact match
        assert!(AclHandlers::resource_matches_pattern_static(
            "topic.users.events",
            "topic.users.events"
        ));

        // Wildcard match
        assert!(AclHandlers::resource_matches_pattern_static(
            "topic.users.events",
            "topic.users.*"
        ));
        assert!(AclHandlers::resource_matches_pattern_static(
            "topic.users.logs",
            "topic.users.*"
        ));

        // No match
        assert!(!AclHandlers::resource_matches_pattern_static(
            "topic.admin.events",
            "topic.users.*"
        ));
        assert!(!AclHandlers::resource_matches_pattern_static(
            "different.topic",
            "topic.users.events"
        ));
    }

    #[test]
    fn test_validate_principal_standalone() {
        // Test the static validation functions without requiring struct instances

        // Valid principal
        assert!(AclHandlers::validate_principal_static("user@domain.com").is_ok());

        // Empty principal
        assert!(AclHandlers::validate_principal_static("").is_err());

        // Too long principal
        let long_principal = "a".repeat(256);
        assert!(AclHandlers::validate_principal_static(&long_principal).is_err());
    }

    #[test]
    fn test_validate_resource_pattern_standalone() {
        // Test the static validation functions without requiring struct instances

        // Valid pattern
        assert!(AclHandlers::validate_resource_pattern_static("topic.users.*").is_ok());

        // Empty pattern
        assert!(AclHandlers::validate_resource_pattern_static("").is_err());

        // Too long pattern
        let long_pattern = "a".repeat(513);
        assert!(AclHandlers::validate_resource_pattern_static(&long_pattern).is_err());
    }

    // Note: Full integration tests for ACL operations should be done at the security module level
    // These tests focus on the standalone utility functions that can be tested without complex setup
}
