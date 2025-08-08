pub mod api;
pub mod rate_limiter;
pub mod security_api;
pub mod certificate_handlers;
pub mod acl_handlers;

pub use api::{AdminApi, ApiResponse, BrokerHealthTracker};
pub use rate_limiter::{
    RateLimiterManager, 
    EndpointCategory, 
    RateLimitInfo, 
    RateLimitError,
    RateLimitStats,
    extract_client_ip,
    with_rate_limiter,
    rate_limit_filter,
    handle_rate_limit_rejection,
    RateLimitRejection,
};
pub use security_api::{SecurityApi};
pub use certificate_handlers::{CertificateHandlers, CertificateCreationResponse, CertificateOperationResult};
pub use acl_handlers::{
    AclHandlers, AclRuleCreationResponse, AclEvaluationMetrics, 
    PrincipalAnalysis, RiskAssessment, AclSyncStatus, BrokerSyncInfo
};