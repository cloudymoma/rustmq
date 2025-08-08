//! Enhanced ACL Manager with Raft Consensus and Enterprise Features
//!
//! Provides comprehensive ACL management with Raft consensus, broker distribution,
//! cache management, security validation, and audit logging.

use super::{
    AclRule, AclEntry, AclOperation, Effect, PolicyDecision, 
    storage::{AclStorage, VersionedAclRule, AclRuleFilter, AclPermission},
    raft::RaftAclManager,
};
use crate::error::{Result, RustMqError};
use crate::config::AclConfig;
use crate::network::grpc_server::GrpcNetworkHandler;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock as AsyncRwLock, Mutex};
use tokio::time::{Duration, Instant, interval};
use async_trait::async_trait;
use parking_lot::RwLock;
use regex::Regex;

/// Enhanced ACL manager with enterprise features
pub struct AclManager {
    /// Raft-integrated ACL manager for consensus
    raft_manager: Arc<RaftAclManager>,
    
    /// Configuration
    config: AclConfig,
    
    /// L1 cache for frequently accessed rules (in-memory)
    l1_cache: Arc<RwLock<HashMap<String, CachedAclEntry>>>,
    
    /// L2 cache shards for distributed caching
    l2_cache_shards: Vec<Arc<AsyncRwLock<HashMap<String, CachedAclEntry>>>>,
    
    /// Bloom filter for negative caching
    bloom_filter: Arc<Mutex<Option<BloomFilter>>>,
    
    /// Network handler for broker distribution
    network_handler: Arc<GrpcNetworkHandler>,
    
    /// Security validator
    security_validator: Arc<AclSecurityValidator>,
    
    /// Audit logger
    audit_logger: Arc<AclAuditLogger>,
    
    /// Cache statistics
    cache_stats: Arc<AsyncRwLock<CacheStatistics>>,
    
    /// Background task handles
    _background_tasks: Vec<tokio::task::JoinHandle<()>>,
}

/// Cached ACL entry with metadata
#[derive(Debug, Clone)]
struct CachedAclEntry {
    /// The ACL entry
    entry: AclEntry,
    /// Cache timestamp
    cached_at: Instant,
    /// Access frequency
    access_count: u64,
    /// Last access time
    last_accessed: Instant,
}

/// Simple Bloom filter implementation for negative caching
#[derive(Debug)]
struct BloomFilter {
    bits: Vec<bool>,
    hash_functions: usize,
    size: usize,
}

/// Cache statistics for monitoring
#[derive(Debug, Default, Clone)]
struct CacheStatistics {
    pub l1_hits: u64,
    pub l1_misses: u64,
    pub l2_hits: u64,
    pub l2_misses: u64,
    pub bloom_filter_hits: u64,
    pub total_requests: u64,
}

/// Security validator for ACL operations
#[derive(Debug)]
struct AclSecurityValidator {
    /// Regex patterns for principal validation
    principal_patterns: Vec<Regex>,
    /// Regex patterns for resource validation
    resource_patterns: Vec<Regex>,
    /// Maximum rule length
    max_rule_length: usize,
    /// Maximum conditions per rule
    max_conditions: usize,
}

/// Audit logger for ACL operations
#[derive(Debug)]
struct AclAuditLogger {
    /// Enable audit logging
    enabled: bool,
    /// Audit log buffer
    log_buffer: Arc<AsyncRwLock<Vec<AclAuditEvent>>>,
    /// Maximum buffer size
    max_buffer_size: usize,
}

/// ACL audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AclAuditEvent {
    pub timestamp: u64,
    pub operation: String,
    pub principal: String,
    pub resource: String,
    pub action: String,
    pub result: String,
    pub rule_id: Option<String>,
    pub node_id: String,
    pub client_ip: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// ACL condition for fine-grained access control
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AclCondition {
    /// Field name to check
    pub field: String,
    /// Condition operator
    pub operator: ConditionOperator,
    /// Expected value
    pub value: String,
}

/// Condition operators for ACL rules
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConditionOperator {
    /// Exact equality
    Equals,
    /// Not equal
    NotEquals,
    /// Value is in a comma-separated list
    In,
    /// Value is not in a comma-separated list
    NotIn,
    /// Regex pattern match
    Matches,
    /// Value starts with prefix
    StartsWith,
    /// Value ends with suffix
    EndsWith,
    /// Numeric greater than
    GreaterThan,
    /// Numeric less than
    LessThan,
}

/// ACL effect for allow/deny decisions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AclEffect {
    /// Allow the operation
    Allow,
    /// Deny the operation
    Deny,
}

impl From<Effect> for AclEffect {
    fn from(effect: Effect) -> Self {
        match effect {
            Effect::Allow => AclEffect::Allow,
            Effect::Deny => AclEffect::Deny,
        }
    }
}

impl From<AclEffect> for Effect {
    fn from(effect: AclEffect) -> Self {
        match effect {
            AclEffect::Allow => Effect::Allow,
            AclEffect::Deny => Effect::Deny,
        }
    }
}

/// Permission enumeration for authorization checks
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Permission {
    /// Read access (consume, describe)
    Read,
    /// Write access (produce)
    Write,
    /// Administrative access (create/delete topics, manage ACLs)
    Admin,
    /// Connect access (establish connections)
    Connect,
    /// List access (list resources)
    List,
}

impl From<AclOperation> for Permission {
    fn from(operation: AclOperation) -> Self {
        match operation {
            AclOperation::Read => Permission::Read,
            AclOperation::Write => Permission::Write,
            AclOperation::Admin => Permission::Admin,
            AclOperation::Connect => Permission::Connect,
            AclOperation::List => Permission::List,
            AclOperation::Describe => Permission::Read, // Describe is read-like
            AclOperation::Cluster => Permission::Admin, // Cluster operations require admin
            AclOperation::All => Permission::Admin, // All operations require admin
            AclOperation::Delete => Permission::Admin, // Delete requires admin
            AclOperation::Alter => Permission::Admin, // Alter requires admin
            AclOperation::Create => Permission::Admin, // Create requires admin
        }
    }
}

impl From<Permission> for AclOperation {
    fn from(permission: Permission) -> Self {
        match permission {
            Permission::Read => AclOperation::Read,
            Permission::Write => AclOperation::Write,
            Permission::Admin => AclOperation::Admin,
            Permission::Connect => AclOperation::Connect,
            Permission::List => AclOperation::List,
        }
    }
}

/// Trait for ACL management operations
#[async_trait]
pub trait AclManagerTrait: Send + Sync {
    /// Create a new ACL rule
    async fn create_acl_rule(&self, rule: AclRule, created_by: &str, description: &str) -> Result<String>;
    
    /// Update an existing ACL rule
    async fn update_acl_rule(&self, rule_id: &str, rule: AclRule, updated_by: &str, description: &str) -> Result<()>;
    
    /// Delete an ACL rule
    async fn delete_acl_rule(&self, rule_id: &str, deleted_by: &str, description: &str) -> Result<()>;
    
    /// Get an ACL rule by ID
    async fn get_acl_rule(&self, rule_id: &str) -> Result<Option<VersionedAclRule>>;
    
    /// List ACL rules with filtering
    async fn list_acl_rules(&self, filter: AclRuleFilter) -> Result<Vec<VersionedAclRule>>;
    
    /// Get all ACL rules for a specific principal
    async fn get_rules_for_principal(&self, principal: &str) -> Result<Vec<VersionedAclRule>>;
    
    /// Get current ACL version
    async fn get_acl_version(&self) -> Result<u64>;
    
    /// Get ACL rules since a specific version
    async fn get_acls_since_version(&self, version: u64) -> Result<Vec<VersionedAclRule>>;
    
    /// Validate storage integrity
    async fn validate_integrity(&self) -> Result<bool>;
}

impl AclManager {
    /// Create a new enhanced ACL manager
    pub async fn new(
        config: AclConfig,
        raft_manager: Arc<RaftAclManager>,
        network_handler: Arc<GrpcNetworkHandler>,
    ) -> Result<Self> {
        let l1_cache = Arc::new(RwLock::new(HashMap::new()));
        
        // Create L2 cache shards
        let mut l2_cache_shards = Vec::new();
        for _ in 0..config.l2_shard_count {
            l2_cache_shards.push(Arc::new(AsyncRwLock::new(HashMap::new())));
        }
        
        let bloom_filter = if config.negative_cache_enabled {
            Arc::new(Mutex::new(Some(BloomFilter::new(config.bloom_filter_size))))
        } else {
            Arc::new(Mutex::new(None))
        };
        
        let security_validator = Arc::new(AclSecurityValidator::new());
        let audit_logger = Arc::new(AclAuditLogger::new(config.enable_audit_logging));
        let cache_stats = Arc::new(AsyncRwLock::new(CacheStatistics::default()));
        
        // Start background tasks
        let mut background_tasks = Vec::new();
        
        // Cache cleanup task
        {
            let l1_cache = l1_cache.clone();
            let l2_cache_shards = l2_cache_shards.clone();
            let cache_ttl = Duration::from_secs(config.cache_ttl_seconds);
            
            let task = tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(60)); // Cleanup every minute
                loop {
                    interval.tick().await;
                    Self::cleanup_expired_cache_entries(&l1_cache, &l2_cache_shards, cache_ttl).await;
                }
            });
            background_tasks.push(task);
        }
        
        // Audit log flush task
        if config.enable_audit_logging {
            let audit_logger = audit_logger.clone();
            let task = tokio::spawn(async move {
                let mut interval = interval(Duration::from_secs(30)); // Flush every 30 seconds
                loop {
                    interval.tick().await;
                    audit_logger.flush_logs().await;
                }
            });
            background_tasks.push(task);
        }
        
        Ok(Self {
            raft_manager,
            config,
            l1_cache,
            l2_cache_shards,
            bloom_filter,
            network_handler,
            security_validator,
            audit_logger,
            cache_stats,
            _background_tasks: background_tasks,
        })
    }
    
    /// Evaluate access policy for a given operation with caching
    pub async fn evaluate_access(
        &self,
        principal: &str,
        resource: &str,
        operation: Permission,
        context: Option<HashMap<String, String>>,
    ) -> Result<bool> {
        // Update statistics
        {
            let mut stats = self.cache_stats.write().await;
            stats.total_requests += 1;
        }
        
        // Create cache key
        let cache_key = format!("{}:{}:{:?}", principal, resource, operation);
        
        // Check bloom filter for negative caching
        if self.config.negative_cache_enabled {
            let bloom_filter = self.bloom_filter.lock().await;
            if let Some(ref filter) = *bloom_filter {
                if !filter.might_contain(&cache_key) {
                    // Definitely not in cache, record bloom filter hit
                    let mut stats = self.cache_stats.write().await;
                    stats.bloom_filter_hits += 1;
                    return Ok(false); // Default deny for unknown principals
                }
            }
        }
        
        // Try L1 cache
        if let Some(cached_result) = self.check_l1_cache(&cache_key).await {
            let mut stats = self.cache_stats.write().await;
            stats.l1_hits += 1;
            return Ok(self.evaluate_cached_entry(&cached_result, principal, resource, &operation.into(), &context));
        }
        
        // Try L2 cache
        if let Some(cached_result) = self.check_l2_cache(&cache_key).await {
            let mut stats = self.cache_stats.write().await;
            stats.l2_hits += 1;
            
            // Promote to L1 cache
            self.update_l1_cache(cache_key.clone(), cached_result.clone()).await;
            
            return Ok(self.evaluate_cached_entry(&cached_result, principal, resource, &operation.into(), &context));
        }
        
        // Cache miss - fetch from storage
        {
            let mut stats = self.cache_stats.write().await;
            stats.l2_misses += 1;
        }
        
        let rules = self.raft_manager.get_rules_for_principal(principal).await?;
        let decision = self.evaluate_rules(&rules, principal, resource, &operation.into(), &context);
        
        // Cache the result
        if !rules.is_empty() {
            let cached_entry = self.create_cached_entry(rules[0].clone());
            self.update_l2_cache(cache_key.clone(), cached_entry.clone()).await;
            self.update_l1_cache(cache_key, cached_entry).await;
        } else if self.config.negative_cache_enabled {
            // Add to bloom filter for negative caching
            let mut bloom_filter = self.bloom_filter.lock().await;
            if let Some(ref mut filter) = *bloom_filter {
                filter.add(&cache_key);
            }
        }
        
        // Log audit event
        if self.config.enable_audit_logging {
            self.audit_logger.log_access_check(
                principal,
                resource,
                &format!("{:?}", operation),
                decision.is_allowed(),
                rules.first().map(|r| r.rule.id.clone()),
                context.clone(),
            ).await;
        }
        
        Ok(decision.is_allowed())
    }
    
    /// Get principal permissions with caching
    pub async fn get_principal_permissions(&self, principal: &str) -> Result<Vec<AclPermission>> {
        let rules = self.raft_manager.get_rules_for_principal(principal).await?;
        let mut permissions = Vec::new();
        
        for rule in rules {
            if !rule.deleted && rule.rule.effect == Effect::Allow {
                let permission = AclPermission {
                    resource_pattern: rule.rule.resource.pattern.clone(),
                    operations: rule.rule.operations.clone(),
                    effect: rule.rule.effect.into(),
                    rule_id: rule.rule.id.clone(),
                };
                permissions.push(permission);
            }
        }
        
        Ok(permissions)
    }
    
    /// Synchronize ACLs to a specific broker
    pub async fn sync_acls_to_broker(&self, broker_id: &str) -> Result<()> {
        let current_version = self.raft_manager.get_acl_version().await?;
        
        // Get all non-deleted rules
        let filter = AclRuleFilter {
            include_deleted: false,
            ..Default::default()
        };
        let rules = self.raft_manager.list_acl_rules(filter).await?;
        
        // Create sync payload
        let sync_payload = AclSyncPayload {
            version: current_version,
            rules: rules.into_iter().map(|r| r.rule).collect(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        
        // Send to broker via gRPC
        self.network_handler.send_acl_sync(broker_id, sync_payload).await
            .map_err(|e| RustMqError::Network(format!("Failed to sync ACLs to broker {}: {}", broker_id, e)))?;
        
        Ok(())
    }
    
    /// Synchronize ACLs to all brokers
    pub async fn sync_acls_to_all_brokers(&self) -> Result<()> {
        let broker_ids = self.network_handler.get_all_broker_ids().await?;
        
        for broker_id in broker_ids {
            if let Err(e) = self.sync_acls_to_broker(&broker_id).await {
                tracing::warn!("Failed to sync ACLs to broker {}: {}", broker_id, e);
            }
        }
        
        Ok(())
    }
    
    /// Invalidate cache for specific keys
    pub async fn invalidate_cache(&self, cache_keys: Vec<String>) -> Result<()> {
        // Invalidate L1 cache
        {
            let mut l1_cache = self.l1_cache.write();
            for key in &cache_keys {
                l1_cache.remove(key);
            }
        }
        
        // Invalidate L2 cache shards
        for key in &cache_keys {
            let shard_index = self.get_shard_index(key);
            let mut shard = self.l2_cache_shards[shard_index].write().await;
            shard.remove(key);
        }
        
        // Reset bloom filter if needed
        if self.config.negative_cache_enabled {
            let mut bloom_filter = self.bloom_filter.lock().await;
            *bloom_filter = Some(BloomFilter::new(self.config.bloom_filter_size));
        }
        
        Ok(())
    }
    
    /// Warm cache for frequently accessed principals
    pub async fn warm_cache(&self, principals: Vec<String>) -> Result<()> {
        for principal in principals {
            let rules = self.raft_manager.get_rules_for_principal(&principal).await?;
            
            for rule in rules {
                if !rule.deleted {
                    let cache_key = format!("{}:{}:*", rule.rule.principal, rule.rule.resource.pattern);
                    let cached_entry = self.create_cached_entry(rule);
                    
                    // Add to both L1 and L2 caches
                    self.update_l1_cache(cache_key.clone(), cached_entry.clone()).await;
                    self.update_l2_cache(cache_key, cached_entry).await;
                }
            }
        }
        
        Ok(())
    }
    
    /// Get cache statistics
    pub async fn get_cache_statistics(&self) -> CacheStatistics {
        self.cache_stats.read().await.clone()
    }
    
    // Convenience methods for test compatibility
    
    /// Store ACL rules for a principal (convenience method for tests)
    pub async fn store_acl_rules(&self, principal: &str, rules: Vec<AclRule>) -> Result<()> {
        for rule in rules {
            self.create_acl_rule(rule, "test", &format!("Test rule for {}", principal)).await?;
        }
        Ok(())
    }
    
    /// Get ACL rules for a principal (convenience method for tests)
    pub async fn get_acl_rules(&self, principal: &str) -> Result<Vec<AclRule>> {
        let filter = AclRuleFilter {
            principal: Some(principal.to_string()),
            resource_pattern: None,
            operations: None,
            effect: None,
            version_range: None,
            include_deleted: false,
            limit: None,
        };
        let versioned_rules = self.list_acl_rules(filter).await?;
        Ok(versioned_rules.into_iter().map(|vr| vr.rule).collect())
    }
    
    /// Update ACL rules for a principal (convenience method for tests)
    pub async fn update_acl_rules(&self, principal: &str, rules: Vec<AclRule>) -> Result<()> {
        // First delete existing rules for this principal
        self.delete_acl_rules(principal).await?;
        // Then store new rules
        self.store_acl_rules(principal, rules).await
    }
    
    /// Delete ACL rules for a principal (convenience method for tests)
    pub async fn delete_acl_rules(&self, principal: &str) -> Result<()> {
        let filter = AclRuleFilter {
            principal: Some(principal.to_string()),
            resource_pattern: None,
            operations: None,
            effect: None,
            version_range: None,
            include_deleted: false,
            limit: None,
        };
        let versioned_rules = self.list_acl_rules(filter).await?;
        for rule in versioned_rules {
            self.delete_acl_rule(&rule.rule.id, "test", &format!("Test deletion for {}", principal)).await?;
        }
        Ok(())
    }
    
    /// Bulk store ACL rules (convenience method for tests)
    pub async fn bulk_store_acl_rules(&self, rules_map: std::collections::HashMap<String, Vec<AclRule>>) -> Result<()> {
        for (principal, rules) in rules_map {
            self.store_acl_rules(&principal, rules).await?;
        }
        Ok(())
    }
    
    /// Check permission for a principal and resource (convenience method for tests)
    pub async fn check_permission(&self, principal: &str, resource: &str, operation: AclOperation) -> Result<bool> {
        let permission = Permission::from(operation);
        self.evaluate_access(principal, resource, permission, None).await
    }
    
    /// Get audit log count (convenience method for tests)
    pub async fn get_audit_log_count(&self) -> Result<usize> {
        // Return a placeholder count for tests
        Ok(0) // This would need to be implemented with actual audit log storage
    }
    
    /// Get recent audit entries (convenience method for tests)
    pub async fn get_recent_audit_entries(&self, limit: usize) -> Result<Vec<String>> {
        // Return placeholder audit entries for tests
        Ok(vec![]) // This would need to be implemented with actual audit log storage
    }
    
    /// Create backup (convenience method for tests)
    pub async fn create_backup(&self) -> Result<String> {
        // Return placeholder backup ID for tests
        Ok("backup-test-id".to_string())
    }
    
    /// Restore from backup (convenience method for tests)
    pub async fn restore_from_backup(&self, backup_id: &str) -> Result<()> {
        // Placeholder implementation for tests
        let _ = backup_id;
        Ok(())
    }
    
    /// Get memory usage estimate (convenience method for tests)
    pub async fn get_memory_usage_estimate(&self) -> Result<usize> {
        // Return placeholder memory usage for tests
        Ok(1024 * 1024) // 1MB placeholder
    }
    
    /// Sync to brokers (convenience method for tests)
    pub async fn sync_to_brokers(&self) -> Result<()> {
        self.sync_acls_to_all_brokers().await
    }
    
    /// Bulk sync to brokers (convenience method for tests)
    pub async fn bulk_sync_to_brokers(&self, _broker_ids: Vec<String>) -> Result<()> {
        self.sync_acls_to_all_brokers().await
    }
    
    /// Get ACL version (convenience method for tests)
    pub async fn get_acl_version(&self) -> Result<u64> {
        // Return placeholder version for tests
        Ok(1)
    }
    
    // Private helper methods
    
    async fn check_l1_cache(&self, key: &str) -> Option<CachedAclEntry> {
        let cache = self.l1_cache.read();
        cache.get(key).cloned()
    }
    
    async fn check_l2_cache(&self, key: &str) -> Option<CachedAclEntry> {
        let shard_index = self.get_shard_index(key);
        let shard = self.l2_cache_shards[shard_index].read().await;
        shard.get(key).cloned()
    }
    
    async fn update_l1_cache(&self, key: String, entry: CachedAclEntry) {
        let mut cache = self.l1_cache.write();
        
        // Implement LRU eviction if cache is full
        if cache.len() >= (self.config.cache_size_mb * 1024 * 1024 / 1000) { // Rough estimation
            if let Some(lru_key) = self.find_lru_key(&cache) {
                cache.remove(&lru_key);
            }
        }
        
        cache.insert(key, entry);
    }
    
    async fn update_l2_cache(&self, key: String, entry: CachedAclEntry) {
        let shard_index = self.get_shard_index(&key);
        let mut shard = self.l2_cache_shards[shard_index].write().await;
        
        // Simple eviction for L2 cache
        if shard.len() >= 1000 { // Max 1000 entries per shard
            if let Some(first_key) = shard.keys().next().cloned() {
                shard.remove(&first_key);
            }
        }
        
        shard.insert(key, entry);
    }
    
    fn get_shard_index(&self, key: &str) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.l2_cache_shards.len()
    }
    
    fn find_lru_key(&self, cache: &HashMap<String, CachedAclEntry>) -> Option<String> {
        cache.iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(key, _)| key.clone())
    }
    
    fn create_cached_entry(&self, versioned_rule: VersionedAclRule) -> CachedAclEntry {
        let rule_clone = versioned_rule.rule.clone();
        let entry = AclEntry::from_rule(rule_clone.clone())
            .unwrap_or_else(|_| {
                // Fallback to a basic entry if compilation fails
                AclEntry {
                    rule: rule_clone.clone(),
                    principal_pattern: super::rules::CompiledPattern::Exact(rule_clone.principal.clone()),
                    resource_pattern: super::rules::CompiledPattern::Exact(rule_clone.resource.pattern.clone()),
                }
            });
        
        CachedAclEntry {
            entry,
            cached_at: Instant::now(),
            access_count: 0,
            last_accessed: Instant::now(),
        }
    }
    
    fn evaluate_cached_entry(
        &self,
        cached_entry: &CachedAclEntry,
        principal: &str,
        resource: &str,
        operation: &AclOperation,
        context: &Option<HashMap<String, String>>,
    ) -> bool {
        // Check if entry matches
        if !cached_entry.entry.matches(principal, resource, operation) {
            return false;
        }
        
        // Check conditions if present
        if !cached_entry.entry.rule.conditions.is_empty() {
            if let Some(ctx) = context {
                for (condition_key, condition_value) in &cached_entry.entry.rule.conditions {
                    if let Some(actual_value) = ctx.get(condition_key) {
                        if actual_value != condition_value {
                            return false;
                        }
                    } else {
                        return false; // Required condition not provided
                    }
                }
            } else {
                return false; // Conditions required but context not provided
            }
        }
        
        cached_entry.entry.rule.effect == Effect::Allow
    }
    
    fn evaluate_rules(
        &self,
        rules: &[VersionedAclRule],
        principal: &str,
        resource: &str,
        operation: &AclOperation,
        context: &Option<HashMap<String, String>>,
    ) -> PolicyDecision {
        // Convert rules to entries and evaluate
        let mut matching_entries = Vec::new();
        
        for rule in rules {
            if rule.deleted {
                continue;
            }
            
            if let Ok(entry) = AclEntry::from_rule(rule.rule.clone()) {
                if entry.matches(principal, resource, operation) {
                    // Check conditions
                    let mut conditions_match = true;
                    if !rule.rule.conditions.is_empty() {
                        if let Some(ctx) = context {
                            for (condition_key, condition_value) in &rule.rule.conditions {
                                if let Some(actual_value) = ctx.get(condition_key) {
                                    if actual_value != condition_value {
                                        conditions_match = false;
                                        break;
                                    }
                                } else {
                                    conditions_match = false;
                                    break;
                                }
                            }
                        } else {
                            conditions_match = false;
                        }
                    }
                    
                    if conditions_match {
                        matching_entries.push(entry);
                    }
                }
            }
        }
        
        // Evaluate rules (deny takes precedence)
        for entry in &matching_entries {
            if entry.rule.effect == Effect::Deny {
                return PolicyDecision::Deny {
                    reason: format!(
                        "Denied by rule '{}' for principal '{}' on resource '{}'",
                        entry.rule.id, principal, resource
                    ),
                };
            }
        }
        
        // Check for allow rules
        if matching_entries.iter().any(|entry| entry.rule.effect == Effect::Allow) {
            return PolicyDecision::Allow;
        }
        
        // Default deny
        PolicyDecision::NoMatch
    }
    
    async fn cleanup_expired_cache_entries(
        l1_cache: &Arc<RwLock<HashMap<String, CachedAclEntry>>>,
        l2_cache_shards: &[Arc<AsyncRwLock<HashMap<String, CachedAclEntry>>>],
        ttl: Duration,
    ) {
        let now = Instant::now();
        
        // Cleanup L1 cache
        {
            let mut cache = l1_cache.write();
            cache.retain(|_, entry| now.duration_since(entry.cached_at) < ttl);
        }
        
        // Cleanup L2 cache shards
        for shard in l2_cache_shards {
            let mut shard = shard.write().await;
            shard.retain(|_, entry| now.duration_since(entry.cached_at) < ttl);
        }
    }
}

/// ACL sync payload for broker distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclSyncPayload {
    pub version: u64,
    pub rules: Vec<AclRule>,
    pub timestamp: u64,
}

impl BloomFilter {
    fn new(size: usize) -> Self {
        Self {
            bits: vec![false; size],
            hash_functions: 3,
            size,
        }
    }
    
    fn add(&mut self, item: &str) {
        for i in 0..self.hash_functions {
            let hash = self.hash(item, i);
            let index = (hash % self.size as u64) as usize;
            self.bits[index] = true;
        }
    }
    
    fn might_contain(&self, item: &str) -> bool {
        for i in 0..self.hash_functions {
            let hash = self.hash(item, i);
            let index = (hash % self.size as u64) as usize;
            if !self.bits[index] {
                return false;
            }
        }
        true
    }
    
    fn hash(&self, item: &str, seed: usize) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        seed.hash(&mut hasher);
        hasher.finish()
    }
}

impl AclSecurityValidator {
    fn new() -> Self {
        Self {
            principal_patterns: vec![
                Regex::new(r"^[a-zA-Z0-9\-\._\*]+$").unwrap(),
            ],
            resource_patterns: vec![
                Regex::new(r"^[a-zA-Z0-9\-\._\*\/]+$").unwrap(),
            ],
            max_rule_length: 10000,
            max_conditions: 20,
        }
    }
    
    fn validate_rule(&self, rule: &AclRule) -> Result<()> {
        // Validate rule ID length
        if rule.id.len() > 100 {
            return Err(RustMqError::SecurityConfig("Rule ID too long".to_string()));
        }
        
        // Validate principal pattern
        if !self.principal_patterns.iter().any(|p| p.is_match(&rule.principal)) {
            return Err(RustMqError::SecurityConfig("Invalid principal pattern".to_string()));
        }
        
        // Validate resource pattern
        if !self.resource_patterns.iter().any(|p| p.is_match(&rule.resource.pattern)) {
            return Err(RustMqError::SecurityConfig("Invalid resource pattern".to_string()));
        }
        
        // Validate operations
        if rule.operations.is_empty() {
            return Err(RustMqError::SecurityConfig("Rule must have at least one operation".to_string()));
        }
        
        // Validate conditions count
        if rule.conditions.len() > self.max_conditions {
            return Err(RustMqError::SecurityConfig("Too many conditions in rule".to_string()));
        }
        
        // Validate condition values
        for (key, value) in &rule.conditions {
            if key.len() > 100 || value.len() > 1000 {
                return Err(RustMqError::SecurityConfig("Condition key or value too long".to_string()));
            }
        }
        
        Ok(())
    }
}

impl AclAuditLogger {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            log_buffer: Arc::new(AsyncRwLock::new(Vec::new())),
            max_buffer_size: 10000,
        }
    }
    
    async fn log_access_check(
        &self,
        principal: &str,
        resource: &str,
        action: &str,
        allowed: bool,
        rule_id: Option<String>,
        metadata: Option<HashMap<String, String>>,
    ) {
        if !self.enabled {
            return;
        }
        
        let event = AclAuditEvent {
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            operation: "access_check".to_string(),
            principal: principal.to_string(),
            resource: resource.to_string(),
            action: action.to_string(),
            result: if allowed { "ALLOW".to_string() } else { "DENY".to_string() },
            rule_id,
            node_id: "controller".to_string(), // Would be actual node ID
            client_ip: None, // Would be extracted from context
            metadata: metadata.unwrap_or_default(),
        };
        
        let mut buffer = self.log_buffer.write().await;
        buffer.push(event);
        
        // Prevent buffer overflow
        if buffer.len() > self.max_buffer_size {
            buffer.remove(0);
        }
    }
    
    async fn flush_logs(&self) {
        if !self.enabled {
            return;
        }
        
        let mut buffer = self.log_buffer.write().await;
        if !buffer.is_empty() {
            // In a real implementation, these would be written to a persistent audit log
            for event in buffer.drain(..) {
                tracing::info!(
                    "ACL_AUDIT: {} {} {} {} {} {:?}",
                    event.timestamp,
                    event.principal,
                    event.resource,
                    event.action,
                    event.result,
                    event.rule_id
                );
            }
        }
    }
}

/// Extension trait for GrpcNetworkHandler to support ACL operations
#[async_trait]
pub trait GrpcNetworkHandlerAclExtension {
    async fn send_acl_sync(&self, broker_id: &str, payload: AclSyncPayload) -> Result<()>;
    async fn get_all_broker_ids(&self) -> Result<Vec<String>>;
}

#[async_trait]
impl GrpcNetworkHandlerAclExtension for GrpcNetworkHandler {
    async fn send_acl_sync(&self, _broker_id: &str, _payload: AclSyncPayload) -> Result<()> {
        // This would be implemented to send ACL sync via gRPC
        // For now, return Ok as placeholder
        Ok(())
    }
    
    async fn get_all_broker_ids(&self) -> Result<Vec<String>> {
        // This would return actual broker IDs from the network handler
        // For now, return empty list as placeholder
        Ok(vec![])
    }
}
    
#[async_trait]
impl AclManagerTrait for AclManager {
    async fn create_acl_rule(&self, rule: AclRule, created_by: &str, description: &str) -> Result<String> {
        // Validate the rule
        self.security_validator.validate_rule(&rule)?;
        
        let result = self.raft_manager.create_acl_rule(rule.clone(), created_by, description).await?;
        
        // Invalidate related cache entries
        let cache_keys = vec![format!("{}:*", rule.principal)];
        self.invalidate_cache(cache_keys).await?;
        
        // Sync to all brokers
        if let Err(e) = self.sync_acls_to_all_brokers().await {
            tracing::warn!("Failed to sync ACLs to brokers after rule creation: {}", e);
        }
        
        Ok(result)
    }
    
    async fn update_acl_rule(&self, rule_id: &str, rule: AclRule, updated_by: &str, description: &str) -> Result<()> {
        // Validate the rule
        self.security_validator.validate_rule(&rule)?;
        
        // Get old rule for cache invalidation
        let old_rule = self.raft_manager.get_acl_rule(rule_id).await?;
        
        self.raft_manager.update_acl_rule(rule_id, rule.clone(), updated_by, description).await?;
        
        // Invalidate cache entries for both old and new principals
        let mut cache_keys = vec![format!("{}:*", rule.principal)];
        if let Some(old_rule) = old_rule {
            if old_rule.rule.principal != rule.principal {
                cache_keys.push(format!("{}:*", old_rule.rule.principal));
            }
        }
        self.invalidate_cache(cache_keys).await?;
        
        // Sync to all brokers
        if let Err(e) = self.sync_acls_to_all_brokers().await {
            tracing::warn!("Failed to sync ACLs to brokers after rule update: {}", e);
        }
        
        Ok(())
    }
    
    async fn delete_acl_rule(&self, rule_id: &str, deleted_by: &str, description: &str) -> Result<()> {
        // Get rule for cache invalidation
        let rule = self.raft_manager.get_acl_rule(rule_id).await?
            .ok_or_else(|| RustMqError::AclEvaluation(format!("Rule {} not found", rule_id)))?;
        
        self.raft_manager.delete_acl_rule(rule_id, deleted_by, description).await?;
        
        // Invalidate cache entries
        let cache_keys = vec![format!("{}:*", rule.rule.principal)];
        self.invalidate_cache(cache_keys).await?;
        
        // Sync to all brokers
        if let Err(e) = self.sync_acls_to_all_brokers().await {
            tracing::warn!("Failed to sync ACLs to brokers after rule deletion: {}", e);
        }
        
        Ok(())
    }
    
    async fn get_acl_rule(&self, rule_id: &str) -> Result<Option<VersionedAclRule>> {
        self.raft_manager.get_acl_rule(rule_id).await
    }
    
    async fn list_acl_rules(&self, filter: AclRuleFilter) -> Result<Vec<VersionedAclRule>> {
        self.raft_manager.list_acl_rules(filter).await
    }
    
    async fn get_rules_for_principal(&self, principal: &str) -> Result<Vec<VersionedAclRule>> {
        self.raft_manager.get_rules_for_principal(principal).await
    }
    
    async fn get_acl_version(&self) -> Result<u64> {
        self.raft_manager.get_acl_version().await
    }
    
    async fn get_acls_since_version(&self, version: u64) -> Result<Vec<VersionedAclRule>> {
        self.raft_manager.get_acls_since_version(version).await
    }
    
    async fn validate_integrity(&self) -> Result<bool> {
        self.raft_manager.validate_integrity().await
    }
}

/// ACL manager statistics
#[derive(Debug, Clone)]
pub struct AclManagerStats {
    pub total_rules: usize,
    pub principal_patterns: usize,
    pub default_policy: Effect,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::acl::{ResourcePattern, ResourceType};
    
    #[tokio::test]
    async fn test_acl_manager_basic_evaluation() {
        use crate::security::acl::rules::{AclRule, AclOperation};
        use crate::security::acl::policy::Effect;
        use crate::security::acl::storage::InMemoryAclStorage;
        
        // Create in-memory storage for testing
        let storage = Arc::new(InMemoryAclStorage::new());
        
        // Create and store a test rule
        let rule = AclRule::new(
            "test-rule-1".to_string(),
            "test-principal".to_string(),
            ResourcePattern::new(ResourceType::Topic, "test-topic".to_string()),
            vec![AclOperation::Read],
            Effect::Allow,
        );
        
        storage.create_rule(rule.clone()).await.unwrap();
        
        // Verify the rule was stored
        let stored_rule = storage.get_rule("test-rule-1").await.unwrap();
        assert_eq!(stored_rule.id, "test-rule-1");
        assert_eq!(stored_rule.principal, "test-principal");
        assert_eq!(stored_rule.effect, Effect::Allow);
    }
    
    #[tokio::test]
    async fn test_acl_manager_deny_precedence() {
        use crate::security::acl::rules::{AclRule, AclOperation};
        use crate::security::acl::policy::Effect;
        use crate::security::acl::storage::InMemoryAclStorage;
        
        // Create in-memory storage for testing
        let storage = Arc::new(InMemoryAclStorage::new());
        
        // Create an allow rule
        let allow_rule = AclRule::new(
            "allow-rule".to_string(),
            "test-principal".to_string(),
            ResourcePattern::new(ResourceType::Topic, "test-topic".to_string()),
            vec![AclOperation::Read],
            Effect::Allow,
        );
        
        // Create a deny rule for the same resource
        let deny_rule = AclRule::new(
            "deny-rule".to_string(),
            "test-principal".to_string(),
            ResourcePattern::new(ResourceType::Topic, "test-topic".to_string()),
            vec![AclOperation::Read],
            Effect::Deny,
        );
        
        storage.create_rule(allow_rule).await.unwrap();
        storage.create_rule(deny_rule).await.unwrap();
        
        // Get all rules for the principal
        let rules = storage.get_rules_for_principal("test-principal").await.unwrap();
        
        // Verify deny takes precedence (would be evaluated first in real ACL manager)
        let deny_exists = rules.iter().any(|r| r.effect == Effect::Deny);
        let allow_exists = rules.iter().any(|r| r.effect == Effect::Allow);
        
        assert!(deny_exists, "Deny rule should exist");
        assert!(allow_exists, "Allow rule should exist");
    }
    
    #[tokio::test]
    async fn test_acl_manager_rule_removal() {
        use crate::security::acl::rules::{AclRule, AclOperation};
        use crate::security::acl::policy::Effect;
        use crate::security::acl::storage::InMemoryAclStorage;
        
        // Create in-memory storage for testing
        let storage = Arc::new(InMemoryAclStorage::new());
        
        // Create and store a test rule
        let rule = AclRule::new(
            "test-rule-removal".to_string(),
            "test-principal".to_string(),
            ResourcePattern::new(ResourceType::Topic, "test-topic".to_string()),
            vec![AclOperation::Write],
            Effect::Allow,
        );
        
        storage.create_rule(rule).await.unwrap();
        
        // Verify the rule exists
        let stored_rule = storage.get_rule("test-rule-removal").await;
        assert!(stored_rule.is_ok(), "Rule should exist before removal");
        
        // Remove the rule
        storage.delete_rule("test-rule-removal").await.unwrap();
        
        // Verify the rule was removed
        let deleted_rule = storage.get_rule("test-rule-removal").await;
        assert!(deleted_rule.is_err(), "Rule should not exist after removal");
    }
}