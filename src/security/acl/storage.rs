//! ACL Persistent Storage Layer
//!
//! Provides persistent storage for ACL rules with versioning, indexing, and backup capabilities.
//! Uses RustMQ's storage traits for consistency with the overall architecture.

use super::{AclOperation, AclRule, Effect};
use crate::error::{Result, RustMqError};
use crate::storage::traits::{Cache, ObjectStorage};

use async_trait::async_trait;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock as AsyncRwLock;
use uuid::Uuid;

/// Storage configuration for ACL persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclStorageConfig {
    /// Object storage key prefix for ACL data
    pub key_prefix: String,
    /// Batch size for rule operations
    pub batch_size: usize,
    /// Backup interval in seconds
    pub backup_interval_secs: u64,
    /// Number of backup versions to retain
    pub backup_retention_count: usize,
    /// Enable compression for stored data
    pub enable_compression: bool,
}

impl Default for AclStorageConfig {
    fn default() -> Self {
        Self {
            key_prefix: "acl/".to_string(),
            batch_size: 100,
            backup_interval_secs: 3600, // 1 hour
            backup_retention_count: 24, // 24 hours of backups
            enable_compression: true,
        }
    }
}

/// Versioned ACL rule for change tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedAclRule {
    /// The ACL rule
    pub rule: AclRule,
    /// Version number for this rule
    pub version: u64,
    /// Timestamp when rule was created/updated
    pub timestamp: u64,
    /// Principal who created/updated this rule
    pub created_by: String,
    /// Change description
    pub change_description: String,
    /// Whether this rule is deleted (soft delete)
    pub deleted: bool,
}

/// ACL rule filter for querying
#[derive(Debug, Clone, Default)]
pub struct AclRuleFilter {
    /// Filter by principal pattern
    pub principal: Option<String>,
    /// Filter by resource pattern
    pub resource_pattern: Option<String>,
    /// Filter by operations
    pub operations: Option<Vec<AclOperation>>,
    /// Filter by effect
    pub effect: Option<Effect>,
    /// Filter by version range
    pub version_range: Option<(u64, u64)>,
    /// Include deleted rules
    pub include_deleted: bool,
    /// Maximum number of results
    pub limit: Option<usize>,
}

/// ACL storage snapshot for backup and recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclSnapshot {
    /// All ACL rules at snapshot time
    pub rules: Vec<VersionedAclRule>,
    /// Current version counter
    pub version_counter: u64,
    /// Snapshot timestamp
    pub timestamp: u64,
    /// Snapshot ID
    pub snapshot_id: String,
    /// Metadata about the snapshot
    pub metadata: HashMap<String, String>,
}

/// ACL permission for authorization evaluation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclPermission {
    /// Resource pattern this permission applies to
    pub resource_pattern: String,
    /// Operations allowed
    pub operations: Vec<AclOperation>,
    /// Effect of the permission
    pub effect: Effect,
    /// Rule ID that granted this permission
    pub rule_id: String,
}

/// Persistent storage interface for ACL rules
#[async_trait]
pub trait AclStorage: Send + Sync {
    /// Create a new ACL rule
    async fn create_rule(
        &self,
        rule: AclRule,
        created_by: &str,
        description: &str,
    ) -> Result<String>;

    /// Update an existing ACL rule
    async fn update_rule(
        &self,
        rule_id: &str,
        rule: AclRule,
        updated_by: &str,
        description: &str,
    ) -> Result<()>;

    /// Delete an ACL rule (soft delete)
    async fn delete_rule(&self, rule_id: &str, deleted_by: &str, description: &str) -> Result<()>;

    /// Get an ACL rule by ID
    async fn get_rule(&self, rule_id: &str) -> Result<Option<VersionedAclRule>>;

    /// List ACL rules with filtering
    async fn list_rules(&self, filter: AclRuleFilter) -> Result<Vec<VersionedAclRule>>;

    /// Get all ACL rules for a specific principal
    async fn get_rules_for_principal(&self, principal: &str) -> Result<Vec<VersionedAclRule>>;

    /// Get current version counter
    async fn get_version(&self) -> Result<u64>;

    /// Get all rules since a specific version
    async fn get_rules_since_version(&self, version: u64) -> Result<Vec<VersionedAclRule>>;

    /// Create a snapshot of all ACL data
    async fn create_snapshot(&self) -> Result<AclSnapshot>;

    /// Restore from a snapshot
    async fn restore_snapshot(&self, snapshot: AclSnapshot) -> Result<()>;

    /// Get available snapshots
    async fn list_snapshots(&self) -> Result<Vec<String>>;

    /// Validate storage integrity
    async fn validate_integrity(&self) -> Result<bool>;
}

/// Object storage based ACL storage implementation
pub struct ObjectStorageAclStorage {
    /// Object storage backend
    object_storage: Arc<dyn ObjectStorage>,
    /// Cache for frequently accessed rules
    cache: Arc<dyn Cache>,
    /// Storage configuration
    config: AclStorageConfig,
    /// Version counter
    version_counter: Arc<AsyncRwLock<u64>>,
    /// Rule index for fast lookups
    rule_index: Arc<AsyncRwLock<HashMap<String, String>>>, // rule_id -> object_key
    /// Principal index for fast principal-based queries
    principal_index: Arc<AsyncRwLock<HashMap<String, HashSet<String>>>>, // principal_pattern -> rule_ids
}

impl ObjectStorageAclStorage {
    /// Create a new object storage ACL storage
    pub async fn new(
        object_storage: Arc<dyn ObjectStorage>,
        cache: Arc<dyn Cache>,
        config: AclStorageConfig,
    ) -> Result<Self> {
        let storage = Self {
            object_storage,
            cache,
            config,
            version_counter: Arc::new(AsyncRwLock::new(0)),
            rule_index: Arc::new(AsyncRwLock::new(HashMap::new())),
            principal_index: Arc::new(AsyncRwLock::new(HashMap::new())),
        };

        // Initialize from existing data
        storage.initialize_indices().await?;

        Ok(storage)
    }

    /// Initialize indices from existing storage
    async fn initialize_indices(&self) -> Result<()> {
        let rules = self.load_all_rules().await?;
        let mut rule_index = self.rule_index.write().await;
        let mut principal_index = self.principal_index.write().await;
        let mut max_version = 0u64;

        for rule in rules {
            let object_key = self.get_rule_object_key(&rule.rule.id);
            rule_index.insert(rule.rule.id.clone(), object_key);

            // Update principal index
            principal_index
                .entry(rule.rule.principal.clone())
                .or_insert_with(HashSet::new)
                .insert(rule.rule.id);

            // Track max version
            if rule.version > max_version {
                max_version = rule.version;
            }
        }

        *self.version_counter.write().await = max_version;
        Ok(())
    }

    /// Load all rules from storage
    async fn load_all_rules(&self) -> Result<Vec<VersionedAclRule>> {
        let keys = self
            .object_storage
            .list(&format!("{}rules/", self.config.key_prefix))
            .await?;

        let mut rules = Vec::new();
        for key in keys {
            if let Ok(data) = self.object_storage.get(&key).await {
                if let Ok(rule) = self.deserialize_rule(&data) {
                    rules.push(rule);
                }
            }
        }

        Ok(rules)
    }

    /// Get object key for a rule
    fn get_rule_object_key(&self, rule_id: &str) -> String {
        format!("{}rules/{}.json", self.config.key_prefix, rule_id)
    }

    /// Get cache key for a rule
    fn get_rule_cache_key(&self, rule_id: &str) -> String {
        format!("acl:rule:{}", rule_id)
    }

    /// Serialize rule to bytes
    fn serialize_rule(&self, rule: &VersionedAclRule) -> Result<Bytes> {
        let json = serde_json::to_string(rule).map_err(|e| RustMqError::Config(e.to_string()))?;

        if self.config.enable_compression {
            // Simple compression using gzip would go here
            // For now, just return JSON bytes
            Ok(Bytes::from(json))
        } else {
            Ok(Bytes::from(json))
        }
    }

    /// Deserialize rule from bytes
    fn deserialize_rule(&self, data: &Bytes) -> Result<VersionedAclRule> {
        let json = if self.config.enable_compression {
            // Decompression would go here
            std::str::from_utf8(data).map_err(|e| RustMqError::Config(e.to_string()))?
        } else {
            std::str::from_utf8(data).map_err(|e| RustMqError::Config(e.to_string()))?
        };

        serde_json::from_str(json).map_err(|e| RustMqError::Config(e.to_string()))
    }

    /// Get next version number
    async fn next_version(&self) -> u64 {
        let mut version = self.version_counter.write().await;
        *version += 1;
        *version
    }

    /// Current timestamp in seconds since epoch
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
}

#[async_trait]
impl AclStorage for ObjectStorageAclStorage {
    async fn create_rule(
        &self,
        mut rule: AclRule,
        created_by: &str,
        description: &str,
    ) -> Result<String> {
        // Generate rule ID if not provided
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }

        // Check if rule already exists
        if self.rule_index.read().await.contains_key(&rule.id) {
            return Err(RustMqError::AclEvaluation(format!(
                "Rule with ID {} already exists",
                rule.id
            )));
        }

        let versioned_rule = VersionedAclRule {
            rule: rule.clone(),
            version: self.next_version().await,
            timestamp: Self::current_timestamp(),
            created_by: created_by.to_string(),
            change_description: description.to_string(),
            deleted: false,
        };

        // Store in object storage
        let object_key = self.get_rule_object_key(&rule.id);
        let data = self.serialize_rule(&versioned_rule)?;
        self.object_storage.put(&object_key, data.clone()).await?;

        // Update cache
        let cache_key = self.get_rule_cache_key(&rule.id);
        self.cache.put(&cache_key, data).await?;

        // Update indices
        {
            let mut rule_index = self.rule_index.write().await;
            rule_index.insert(rule.id.clone(), object_key);
        }

        {
            let mut principal_index = self.principal_index.write().await;
            principal_index
                .entry(rule.principal.clone())
                .or_insert_with(HashSet::new)
                .insert(rule.id.clone());
        }

        Ok(rule.id)
    }

    async fn update_rule(
        &self,
        rule_id: &str,
        mut rule: AclRule,
        updated_by: &str,
        description: &str,
    ) -> Result<()> {
        // Ensure rule ID matches
        rule.id = rule_id.to_string();

        // Check if rule exists
        let existing_rule = self
            .get_rule(rule_id)
            .await?
            .ok_or_else(|| RustMqError::AclEvaluation(format!("Rule {} not found", rule_id)))?;

        let versioned_rule = VersionedAclRule {
            rule: rule.clone(),
            version: self.next_version().await,
            timestamp: Self::current_timestamp(),
            created_by: updated_by.to_string(),
            change_description: description.to_string(),
            deleted: false,
        };

        // Store in object storage
        let object_key = self.get_rule_object_key(rule_id);
        let data = self.serialize_rule(&versioned_rule)?;
        self.object_storage.put(&object_key, data.clone()).await?;

        // Update cache
        let cache_key = self.get_rule_cache_key(rule_id);
        self.cache.put(&cache_key, data).await?;

        // Update principal index if principal changed
        if existing_rule.rule.principal != rule.principal {
            let mut principal_index = self.principal_index.write().await;

            // Remove from old principal
            if let Some(old_rules) = principal_index.get_mut(&existing_rule.rule.principal) {
                old_rules.remove(rule_id);
            }

            // Add to new principal
            principal_index
                .entry(rule.principal.clone())
                .or_insert_with(HashSet::new)
                .insert(rule_id.to_string());
        }

        Ok(())
    }

    async fn delete_rule(&self, rule_id: &str, deleted_by: &str, description: &str) -> Result<()> {
        // Get existing rule
        let mut existing_rule = self
            .get_rule(rule_id)
            .await?
            .ok_or_else(|| RustMqError::AclEvaluation(format!("Rule {} not found", rule_id)))?;

        // Mark as deleted
        existing_rule.deleted = true;
        existing_rule.version = self.next_version().await;
        existing_rule.timestamp = Self::current_timestamp();
        existing_rule.created_by = deleted_by.to_string();
        existing_rule.change_description = description.to_string();

        // Store in object storage
        let object_key = self.get_rule_object_key(rule_id);
        let data = self.serialize_rule(&existing_rule)?;
        self.object_storage.put(&object_key, data).await?;

        // Remove from cache and indices
        let cache_key = self.get_rule_cache_key(rule_id);
        self.cache.remove(&cache_key).await?;

        {
            let mut principal_index = self.principal_index.write().await;
            if let Some(rules) = principal_index.get_mut(&existing_rule.rule.principal) {
                rules.remove(rule_id);
            }
        }

        Ok(())
    }

    async fn get_rule(&self, rule_id: &str) -> Result<Option<VersionedAclRule>> {
        // Try cache first
        let cache_key = self.get_rule_cache_key(rule_id);
        if let Ok(Some(data)) = self.cache.get(&cache_key).await {
            if let Ok(rule) = self.deserialize_rule(&data) {
                return Ok(Some(rule));
            }
        }

        // Try object storage
        let object_key = self.get_rule_object_key(rule_id);
        match self.object_storage.get(&object_key).await {
            Ok(data) => {
                let rule = self.deserialize_rule(&data)?;
                // Update cache
                let _ = self.cache.put(&cache_key, data).await;
                Ok(Some(rule))
            }
            Err(RustMqError::ObjectNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn list_rules(&self, filter: AclRuleFilter) -> Result<Vec<VersionedAclRule>> {
        let all_rules = self.load_all_rules().await?;
        let mut filtered_rules = Vec::new();

        for rule in all_rules {
            // Skip deleted rules unless explicitly requested
            if rule.deleted && !filter.include_deleted {
                continue;
            }

            // Apply filters
            if let Some(ref principal) = filter.principal {
                if rule.rule.principal != *principal {
                    continue;
                }
            }

            if let Some(ref resource_pattern) = filter.resource_pattern {
                if rule.rule.resource.pattern != *resource_pattern {
                    continue;
                }
            }

            if let Some(ref operations) = filter.operations {
                if !operations
                    .iter()
                    .any(|op| rule.rule.operations.contains(op))
                {
                    continue;
                }
            }

            if let Some(effect) = filter.effect {
                if rule.rule.effect != effect {
                    continue;
                }
            }

            if let Some((min_version, max_version)) = filter.version_range {
                if rule.version < min_version || rule.version > max_version {
                    continue;
                }
            }

            filtered_rules.push(rule);

            // Apply limit
            if let Some(limit) = filter.limit {
                if filtered_rules.len() >= limit {
                    break;
                }
            }
        }

        Ok(filtered_rules)
    }

    async fn get_rules_for_principal(&self, principal: &str) -> Result<Vec<VersionedAclRule>> {
        let principal_index = self.principal_index.read().await;
        let mut rules = Vec::new();

        // Get direct matches
        if let Some(rule_ids) = principal_index.get(principal) {
            for rule_id in rule_ids {
                if let Ok(Some(rule)) = self.get_rule(rule_id).await {
                    if !rule.deleted {
                        rules.push(rule);
                    }
                }
            }
        }

        // Check wildcard patterns
        for (pattern, rule_ids) in principal_index.iter() {
            if pattern.contains('*') {
                // Simple wildcard matching
                if pattern == "*"
                    || (pattern.ends_with('*')
                        && principal.starts_with(&pattern[..pattern.len() - 1]))
                    || (pattern.starts_with('*') && principal.ends_with(&pattern[1..]))
                {
                    for rule_id in rule_ids {
                        if let Ok(Some(rule)) = self.get_rule(rule_id).await {
                            if !rule.deleted {
                                rules.push(rule);
                            }
                        }
                    }
                }
            }
        }

        Ok(rules)
    }

    async fn get_version(&self) -> Result<u64> {
        Ok(*self.version_counter.read().await)
    }

    async fn get_rules_since_version(&self, version: u64) -> Result<Vec<VersionedAclRule>> {
        let all_rules = self.load_all_rules().await?;
        let filtered_rules = all_rules
            .into_iter()
            .filter(|rule| rule.version > version)
            .collect();

        Ok(filtered_rules)
    }

    async fn create_snapshot(&self) -> Result<AclSnapshot> {
        let all_rules = self.load_all_rules().await?;
        let version = self.get_version().await?;

        let snapshot = AclSnapshot {
            rules: all_rules,
            version_counter: version,
            timestamp: Self::current_timestamp(),
            snapshot_id: Uuid::new_v4().to_string(),
            metadata: HashMap::new(),
        };

        // Store snapshot
        let snapshot_key = format!(
            "{}snapshots/{}.json",
            self.config.key_prefix, snapshot.snapshot_id
        );
        let data =
            serde_json::to_string(&snapshot).map_err(|e| RustMqError::Config(e.to_string()))?;
        self.object_storage
            .put(&snapshot_key, Bytes::from(data))
            .await?;

        Ok(snapshot)
    }

    async fn restore_snapshot(&self, snapshot: AclSnapshot) -> Result<()> {
        // Clear existing indices
        self.rule_index.write().await.clear();
        self.principal_index.write().await.clear();

        // Restore rules
        for rule in snapshot.rules {
            if !rule.deleted {
                let object_key = self.get_rule_object_key(&rule.rule.id);
                let data = self.serialize_rule(&rule)?;
                self.object_storage.put(&object_key, data).await?;

                // Update indices
                self.rule_index
                    .write()
                    .await
                    .insert(rule.rule.id.clone(), object_key);
                self.principal_index
                    .write()
                    .await
                    .entry(rule.rule.principal.clone())
                    .or_insert_with(HashSet::new)
                    .insert(rule.rule.id);
            }
        }

        // Restore version counter
        *self.version_counter.write().await = snapshot.version_counter;

        Ok(())
    }

    async fn list_snapshots(&self) -> Result<Vec<String>> {
        let keys = self
            .object_storage
            .list(&format!("{}snapshots/", self.config.key_prefix))
            .await?;

        let snapshot_ids = keys
            .into_iter()
            .filter_map(|key| {
                key.strip_prefix(&format!("{}snapshots/", self.config.key_prefix))
                    .and_then(|s| s.strip_suffix(".json"))
                    .map(|s| s.to_string())
            })
            .collect();

        Ok(snapshot_ids)
    }

    async fn validate_integrity(&self) -> Result<bool> {
        // Basic integrity check - ensure all indexed rules exist in storage
        let rule_index = self.rule_index.read().await;

        for (rule_id, object_key) in rule_index.iter() {
            if !self.object_storage.exists(object_key).await? {
                return Ok(false);
            }

            // Verify rule can be loaded
            if let Err(_) = self.get_rule(rule_id).await {
                return Ok(false);
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::acl::{ResourcePattern, ResourceType};
    use crate::storage::cache::LruCache;
    use crate::storage::object_storage::LocalObjectStorage;
    use tempfile::TempDir;

    async fn create_test_storage() -> (ObjectStorageAclStorage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let object_storage =
            Arc::new(LocalObjectStorage::new(temp_dir.path().to_path_buf()).unwrap());
        let cache = Arc::new(LruCache::new(1024 * 1024)); // 1MB cache
        let config = AclStorageConfig::default();

        let storage = ObjectStorageAclStorage::new(object_storage, cache, config)
            .await
            .unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_create_and_get_rule() {
        let (storage, _temp_dir) = create_test_storage().await;

        let rule = AclRule::new(
            "".to_string(), // Will be generated
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        let rule_id = storage
            .create_rule(rule.clone(), "admin", "Initial rule")
            .await
            .unwrap();
        assert!(!rule_id.is_empty());

        let retrieved_rule = storage.get_rule(&rule_id).await.unwrap().unwrap();
        assert_eq!(retrieved_rule.rule.principal, "user-123");
        assert_eq!(retrieved_rule.rule.resource.pattern, "test-topic");
        assert_eq!(retrieved_rule.created_by, "admin");
    }

    #[tokio::test]
    async fn test_update_rule() {
        let (storage, _temp_dir) = create_test_storage().await;

        let rule = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        let rule_id = storage
            .create_rule(rule, "admin", "Initial rule")
            .await
            .unwrap();

        let mut updated_rule = AclRule::new(
            rule_id.clone(),
            "user-456".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "updated-topic".to_string(),
            },
            vec![AclOperation::Write],
            Effect::Deny,
        );

        storage
            .update_rule(&rule_id, updated_rule, "admin", "Updated rule")
            .await
            .unwrap();

        let retrieved_rule = storage.get_rule(&rule_id).await.unwrap().unwrap();
        assert_eq!(retrieved_rule.rule.principal, "user-456");
        assert_eq!(retrieved_rule.rule.resource.pattern, "updated-topic");
    }

    #[tokio::test]
    async fn test_delete_rule() {
        let (storage, _temp_dir) = create_test_storage().await;

        let rule = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        let rule_id = storage
            .create_rule(rule, "admin", "Initial rule")
            .await
            .unwrap();

        storage
            .delete_rule(&rule_id, "admin", "Rule no longer needed")
            .await
            .unwrap();

        let retrieved_rule = storage.get_rule(&rule_id).await.unwrap().unwrap();
        assert!(retrieved_rule.deleted);
    }

    #[tokio::test]
    async fn test_list_rules_with_filter() {
        let (storage, _temp_dir) = create_test_storage().await;

        // Create multiple rules
        let rule1 = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "topic-1".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        let rule2 = AclRule::new(
            "".to_string(),
            "user-456".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "topic-2".to_string(),
            },
            vec![AclOperation::Write],
            Effect::Deny,
        );

        storage.create_rule(rule1, "admin", "Rule 1").await.unwrap();
        storage.create_rule(rule2, "admin", "Rule 2").await.unwrap();

        // Filter by effect
        let filter = AclRuleFilter {
            effect: Some(Effect::Allow),
            ..Default::default()
        };

        let rules = storage.list_rules(filter).await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule.principal, "user-123");
    }

    #[tokio::test]
    async fn test_get_rules_for_principal() {
        let (storage, _temp_dir) = create_test_storage().await;

        let rule = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        storage
            .create_rule(rule, "admin", "Test rule")
            .await
            .unwrap();

        let rules = storage.get_rules_for_principal("user-123").await.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule.principal, "user-123");
    }

    #[tokio::test]
    async fn test_version_tracking() {
        let (storage, _temp_dir) = create_test_storage().await;

        let initial_version = storage.get_version().await.unwrap();

        let rule = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        storage
            .create_rule(rule, "admin", "Test rule")
            .await
            .unwrap();

        let new_version = storage.get_version().await.unwrap();
        assert!(new_version > initial_version);
    }

    #[tokio::test]
    async fn test_snapshot_and_restore() {
        let (storage, _temp_dir) = create_test_storage().await;

        // Create a rule
        let rule = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        storage
            .create_rule(rule, "admin", "Test rule")
            .await
            .unwrap();

        // Create snapshot
        let snapshot = storage.create_snapshot().await.unwrap();
        assert_eq!(snapshot.rules.len(), 1);

        // Clear and restore
        storage.rule_index.write().await.clear();
        storage.restore_snapshot(snapshot).await.unwrap();

        // Verify restoration
        let rules = storage.list_rules(AclRuleFilter::default()).await.unwrap();
        assert_eq!(rules.len(), 1);
    }

    #[tokio::test]
    async fn test_integrity_validation() {
        let (storage, _temp_dir) = create_test_storage().await;

        let rule = AclRule::new(
            "".to_string(),
            "user-123".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        storage
            .create_rule(rule, "admin", "Test rule")
            .await
            .unwrap();

        let is_valid = storage.validate_integrity().await.unwrap();
        assert!(is_valid);
    }
}

/// In-memory ACL storage for testing
#[derive(Debug, Clone)]
pub struct InMemoryAclStorage {
    rules: Arc<AsyncRwLock<HashMap<String, VersionedAclRule>>>,
    principal_index: Arc<AsyncRwLock<HashMap<String, HashSet<String>>>>,
}

impl InMemoryAclStorage {
    /// Create a new in-memory storage
    pub fn new() -> Self {
        Self {
            rules: Arc::new(AsyncRwLock::new(HashMap::new())),
            principal_index: Arc::new(AsyncRwLock::new(HashMap::new())),
        }
    }

    /// Create a rule
    pub async fn create_rule(&self, mut rule: AclRule) -> Result<()> {
        if rule.id.is_empty() {
            rule.id = Uuid::new_v4().to_string();
        }

        let versioned = VersionedAclRule {
            rule: rule.clone(),
            version: 1,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            created_by: "test".to_string(),
            change_description: "test rule".to_string(),
            deleted: false,
        };

        self.rules.write().await.insert(rule.id.clone(), versioned);

        let mut index = self.principal_index.write().await;
        index
            .entry(rule.principal.clone())
            .or_insert_with(HashSet::new)
            .insert(rule.id.clone());

        Ok(())
    }

    /// Get a rule by ID
    pub async fn get_rule(&self, id: &str) -> Result<AclRule> {
        self.rules
            .read()
            .await
            .get(id)
            .map(|v| v.rule.clone())
            .ok_or_else(|| RustMqError::AclEvaluation(format!("Rule {} not found", id)))
    }

    /// Delete a rule
    pub async fn delete_rule(&self, id: &str) -> Result<()> {
        let mut rules = self.rules.write().await;
        if let Some(versioned) = rules.remove(id) {
            let mut index = self.principal_index.write().await;
            if let Some(rule_ids) = index.get_mut(&versioned.rule.principal) {
                rule_ids.remove(id);
            }
        }
        Ok(())
    }

    /// Get rules for a principal
    pub async fn get_rules_for_principal(&self, principal: &str) -> Result<Vec<AclRule>> {
        let index = self.principal_index.read().await;
        let rules = self.rules.read().await;

        if let Some(rule_ids) = index.get(principal) {
            Ok(rule_ids
                .iter()
                .filter_map(|id| rules.get(id).map(|v| v.rule.clone()))
                .collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// List all rules
    pub async fn list_all_rules(&self) -> Result<Vec<AclRule>> {
        Ok(self
            .rules
            .read()
            .await
            .values()
            .map(|v| v.rule.clone())
            .collect())
    }

    /// Take a snapshot
    pub async fn snapshot(&self) -> Result<AclSnapshot> {
        let rules = self.rules.read().await;
        Ok(AclSnapshot {
            rules: rules.values().cloned().collect(),
            version_counter: 1,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            snapshot_id: Uuid::new_v4().to_string(),
            metadata: HashMap::new(),
        })
    }

    /// Restore from snapshot
    pub async fn restore(&self, snapshot: AclSnapshot) -> Result<()> {
        let mut rules = self.rules.write().await;
        let mut index = self.principal_index.write().await;

        rules.clear();
        index.clear();

        for versioned in snapshot.rules {
            let rule_id = versioned.rule.id.clone();
            let principal = versioned.rule.principal.clone();

            rules.insert(rule_id.clone(), versioned);
            index
                .entry(principal)
                .or_insert_with(HashSet::new)
                .insert(rule_id);
        }

        Ok(())
    }
}
