//! ACL Rules and Operations
//!
//! Defines the structure and operations for Access Control List rules.

use super::{Effect, ResourcePattern};
use crate::security::auth::Permission;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// ACL rule defining access permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRule {
    /// Rule identifier
    pub id: String,

    /// Principal this rule applies to (supports patterns)
    pub principal: String,

    /// Resource pattern this rule applies to
    pub resource: ResourcePattern,

    /// Operations allowed or denied
    pub operations: Vec<AclOperation>,

    /// Effect of this rule (Allow or Deny)
    pub effect: Effect,

    /// Additional conditions
    pub conditions: HashMap<String, String>,
}

/// ACL entry for caching and evaluation
#[derive(Debug, Clone)]
pub struct AclEntry {
    /// The ACL rule
    pub rule: AclRule,

    /// Compiled principal pattern for fast matching
    pub principal_pattern: CompiledPattern,

    /// Compiled resource pattern for fast matching
    pub resource_pattern: CompiledPattern,
}

/// ACL operation enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AclOperation {
    /// Read operation (consume messages, describe topics)
    Read,

    /// Write operation (produce messages)
    Write,

    /// Admin operation (create/delete topics, manage ACLs)
    Admin,

    /// Describe operation (get topic metadata)
    Describe,

    /// Cluster operation (cluster management)
    Cluster,

    /// All operations (wildcard)
    All,

    /// Connect operation (establish connections)
    Connect,

    /// Delete operation (delete resources)
    Delete,

    /// Alter operation (modify resources)
    Alter,

    /// Create operation (create resources)
    Create,

    /// List operation (list topics, brokers)
    List,
}

impl From<Permission> for AclOperation {
    fn from(permission: Permission) -> Self {
        match permission {
            Permission::Read => AclOperation::Read,
            Permission::Write => AclOperation::Write,
            Permission::Admin => AclOperation::Admin,
        }
    }
}

impl From<AclOperation> for Permission {
    fn from(operation: AclOperation) -> Self {
        match operation {
            AclOperation::Read
            | AclOperation::List
            | AclOperation::Connect
            | AclOperation::Describe => Permission::Read,
            AclOperation::Write => Permission::Write,
            AclOperation::Admin
            | AclOperation::Cluster
            | AclOperation::All
            | AclOperation::Delete
            | AclOperation::Alter
            | AclOperation::Create => Permission::Admin,
        }
    }
}

/// Compiled pattern for efficient matching
#[derive(Debug, Clone)]
pub enum CompiledPattern {
    /// Exact string match
    Exact(String),

    /// Wildcard pattern (e.g., "user-*")
    Wildcard { prefix: String, suffix: String },

    /// Regex pattern
    Regex(regex::Regex),

    /// Match all pattern
    All,
}

impl CompiledPattern {
    /// Compile a pattern string into an efficient matcher
    pub fn compile(pattern: &str) -> Result<Self, regex::Error> {
        if pattern == "*" {
            return Ok(CompiledPattern::All);
        }

        if pattern.contains('*') {
            // Simple wildcard pattern
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 {
                return Ok(CompiledPattern::Wildcard {
                    prefix: parts[0].to_string(),
                    suffix: parts[1].to_string(),
                });
            }
        }

        // Check if it contains regex special characters
        if pattern.chars().any(|c| "[]{}()^$+?|\\".contains(c)) {
            let regex = regex::Regex::new(pattern)?;
            return Ok(CompiledPattern::Regex(regex));
        }

        // Exact match
        Ok(CompiledPattern::Exact(pattern.to_string()))
    }

    /// Test if this pattern matches a given string
    pub fn matches(&self, text: &str) -> bool {
        match self {
            CompiledPattern::Exact(exact) => text == exact,
            CompiledPattern::Wildcard { prefix, suffix } => {
                text.starts_with(prefix) && text.ends_with(suffix)
            }
            CompiledPattern::Regex(regex) => regex.is_match(text),
            CompiledPattern::All => true,
        }
    }
}

impl AclRule {
    /// Create a new ACL rule
    pub fn new(
        id: String,
        principal: String,
        resource: ResourcePattern,
        operations: Vec<AclOperation>,
        effect: Effect,
    ) -> Self {
        Self {
            id,
            principal,
            resource,
            operations,
            effect,
            conditions: HashMap::new(),
        }
    }

    /// Add a condition to this rule
    pub fn with_condition(mut self, key: String, value: String) -> Self {
        self.conditions.insert(key, value);
        self
    }

    /// Check if this rule applies to a given principal
    pub fn applies_to_principal(&self, principal: &str) -> bool {
        // Simple pattern matching - could be enhanced
        if self.principal == "*" {
            return true;
        }

        if self.principal.contains('*') {
            let parts: Vec<&str> = self.principal.split('*').collect();
            if parts.len() == 2 {
                return principal.starts_with(parts[0]) && principal.ends_with(parts[1]);
            }
        }

        self.principal == principal
    }

    /// Check if this rule applies to a given operation
    pub fn applies_to_operation(&self, operation: &AclOperation) -> bool {
        self.operations.contains(operation)
    }
}

impl AclEntry {
    /// Create a new ACL entry from a rule
    pub fn from_rule(rule: AclRule) -> Result<Self, regex::Error> {
        let principal_pattern = CompiledPattern::compile(&rule.principal)?;
        let resource_pattern = CompiledPattern::compile(&rule.resource.pattern)?;

        Ok(Self {
            rule,
            principal_pattern,
            resource_pattern,
        })
    }

    /// Check if this entry matches the given parameters
    pub fn matches(&self, principal: &str, resource: &str, operation: &AclOperation) -> bool {
        // Check principal match
        if !self.principal_pattern.matches(principal) {
            return false;
        }

        // Check resource match
        if !self.resource_pattern.matches(resource) {
            return false;
        }

        // Check operation match
        if !self.rule.applies_to_operation(operation) {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::acl::{Effect, ResourceType};

    #[test]
    fn test_compiled_pattern_exact() {
        let pattern = CompiledPattern::compile("test-user").unwrap();
        assert!(pattern.matches("test-user"));
        assert!(!pattern.matches("test-admin"));
    }

    #[test]
    fn test_compiled_pattern_wildcard() {
        let pattern = CompiledPattern::compile("user-*").unwrap();
        assert!(pattern.matches("user-123"));
        assert!(pattern.matches("user-admin"));
        assert!(!pattern.matches("admin-123"));
    }

    #[test]
    fn test_compiled_pattern_all() {
        let pattern = CompiledPattern::compile("*").unwrap();
        assert!(pattern.matches("anything"));
        assert!(pattern.matches(""));
    }

    #[test]
    fn test_acl_rule_principal_matching() {
        let rule = AclRule::new(
            "test-rule".to_string(),
            "user-*".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-topic".to_string(),
            },
            vec![AclOperation::Read],
            Effect::Allow,
        );

        assert!(rule.applies_to_principal("user-123"));
        assert!(!rule.applies_to_principal("admin-123"));
    }

    #[test]
    fn test_acl_entry_matching() {
        let rule = AclRule::new(
            "test-rule".to_string(),
            "user-*".to_string(),
            ResourcePattern {
                resource_type: ResourceType::Topic,
                pattern: "test-*".to_string(),
            },
            vec![AclOperation::Read, AclOperation::Write],
            Effect::Allow,
        );

        let entry = AclEntry::from_rule(rule).unwrap();

        assert!(entry.matches("user-123", "test-topic", &AclOperation::Read));
        assert!(entry.matches("user-456", "test-data", &AclOperation::Write));
        assert!(!entry.matches("admin-123", "test-topic", &AclOperation::Read));
        assert!(!entry.matches("user-123", "prod-topic", &AclOperation::Read));
        assert!(!entry.matches("user-123", "test-topic", &AclOperation::Admin));
    }
}
