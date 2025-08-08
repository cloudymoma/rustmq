//! Policy Decision and Effect Types

use serde::{Deserialize, Serialize};

/// Effect of an ACL rule
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Effect {
    /// Allow the operation
    Allow,
    
    /// Deny the operation
    Deny,
}

/// Policy decision result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Operation is allowed
    Allow,
    
    /// Operation is denied with reason
    Deny { reason: String },
    
    /// No applicable policy (default deny)
    NoMatch,
}

impl PolicyDecision {
    /// Check if the decision allows the operation
    pub fn is_allowed(&self) -> bool {
        matches!(self, PolicyDecision::Allow)
    }
    
    /// Check if the decision denies the operation
    pub fn is_denied(&self) -> bool {
        !self.is_allowed()
    }
    
    /// Get the denial reason if this is a deny decision
    pub fn denial_reason(&self) -> Option<&str> {
        match self {
            PolicyDecision::Deny { reason } => Some(reason),
            PolicyDecision::NoMatch => Some("No matching ACL policy"),
            PolicyDecision::Allow => None,
        }
    }
}