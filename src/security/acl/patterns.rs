//! Resource Patterns for ACL Matching

use serde::{Deserialize, Serialize};

/// Resource pattern for ACL rules
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourcePattern {
    /// Type of resource
    pub resource_type: ResourceType,

    /// Pattern string (supports wildcards)
    pub pattern: String,
}

/// Resource type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// Topic resource
    Topic,

    /// Consumer group resource
    ConsumerGroup,

    /// Broker resource
    Broker,

    /// Cluster resource
    Cluster,
}

impl ResourcePattern {
    /// Create a new resource pattern
    pub fn new(resource_type: ResourceType, pattern: String) -> Self {
        Self {
            resource_type,
            pattern,
        }
    }

    /// Create a topic pattern
    pub fn topic(pattern: impl Into<String>) -> Self {
        Self::new(ResourceType::Topic, pattern.into())
    }

    /// Create a consumer group pattern
    pub fn consumer_group(pattern: impl Into<String>) -> Self {
        Self::new(ResourceType::ConsumerGroup, pattern.into())
    }

    /// Create a broker pattern
    pub fn broker(pattern: impl Into<String>) -> Self {
        Self::new(ResourceType::Broker, pattern.into())
    }

    /// Create a cluster pattern
    pub fn cluster(pattern: impl Into<String>) -> Self {
        Self::new(ResourceType::Cluster, pattern.into())
    }

    /// Get the pattern string
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Get the resource type
    pub fn resource_type(&self) -> &ResourceType {
        &self.resource_type
    }

    /// Check if this pattern matches a given resource name
    pub fn matches_name(&self, resource_name: &str) -> bool {
        if self.pattern == "*" {
            return true;
        }

        if self.pattern.contains("*") {
            // Handle prefix pattern (e.g., "data.*")
            if self.pattern.ends_with("*") {
                let pattern_without_wildcard = self.pattern.trim_end_matches("*");
                return resource_name.starts_with(pattern_without_wildcard);
            }

            // Handle suffix pattern (e.g., "*.logs")
            if self.pattern.starts_with("*") {
                let pattern_without_wildcard = self.pattern.trim_start_matches("*");
                return resource_name.ends_with(pattern_without_wildcard);
            }

            // Handle middle wildcard (e.g., "data.*.logs") - simplified version
            // For now, just check if both prefix and suffix match
            let parts: Vec<&str> = self.pattern.split('*').collect();
            if parts.len() == 2 {
                return resource_name.starts_with(parts[0]) && resource_name.ends_with(parts[1]);
            }
        }

        // Exact match
        self.pattern == resource_name
    }

    /// Check if this pattern matches a given resource type and name
    pub fn matches(&self, resource_type: &str, resource_name: &str) -> bool {
        // First check if resource type matches
        let expected_type = match self.resource_type {
            ResourceType::Topic => "topic",
            ResourceType::ConsumerGroup => "consumer_group",
            ResourceType::Broker => "broker",
            ResourceType::Cluster => "cluster",
        };

        if resource_type != expected_type {
            return false;
        }

        // Then check the resource name pattern
        self.matches_name(resource_name)
    }
}
