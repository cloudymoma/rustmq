//! Miri memory safety tests for security components
//! 
//! These tests focus on detecting memory safety issues in:
//! - Certificate validation and parsing
//! - Cryptographic operations
//! - ACL authorization checks
//! - Authentication token handling

#[cfg(miri)]
mod miri_security_tests {
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::runtime::Runtime;
    
    use rustmq::security::auth::authentication::AuthenticationService;
    use rustmq::security::auth::authorization::AuthorizationService;
    use rustmq::security::acl::manager::ACLManager;
    use rustmq::security::acl::rules::{ACLRule, Permission, ResourcePattern};
    use rustmq::security::auth::principal::Principal;

    #[test]
    fn test_certificate_validation_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let auth_service = AuthenticationService::new(temp_dir.path()).await.unwrap();
            
            // Test basic certificate creation and validation
            let ca_cert = auth_service.create_ca_certificate("Test CA").await.unwrap();
            
            // Test certificate parsing with various inputs
            let cert_pem = ca_cert.serialize_pem().unwrap();
            let parsed_cert = auth_service.parse_certificate(&cert_pem).await.unwrap();
            
            assert_eq!(ca_cert.subject(), parsed_cert.subject());
            
            // Test certificate chain validation
            let client_cert = auth_service
                .create_client_certificate("test@example.com", &ca_cert)
                .await
                .unwrap();
            
            let chain = vec![client_cert.clone(), ca_cert.clone()];
            let is_valid = auth_service.validate_certificate_chain(&chain).await.unwrap();
            assert!(is_valid);
        });
    }

    #[test]
    fn test_certificate_parsing_edge_cases() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let auth_service = AuthenticationService::new(temp_dir.path()).await.unwrap();
            
            // Test empty certificate data
            let result = auth_service.parse_certificate("").await;
            assert!(result.is_err());
            
            // Test malformed certificate data
            let malformed = "-----BEGIN CERTIFICATE-----\nINVALID_DATA\n-----END CERTIFICATE-----";
            let result = auth_service.parse_certificate(malformed).await;
            assert!(result.is_err());
            
            // Test very long certificate data (potential buffer overruns)
            let very_long_cert = format!(
                "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----",
                "A".repeat(100000)
            );
            let result = auth_service.parse_certificate(&very_long_cert).await;
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_acl_authorization_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let acl_manager = ACLManager::new(temp_dir.path()).await.unwrap();
            
            // Test basic ACL operations
            let principal = Principal::new("user@example.com".to_string());
            let resource = ResourcePattern::new("topic.events.*".to_string()).unwrap();
            let rule = ACLRule::new(
                principal.clone(),
                resource.clone(),
                vec![Permission::Read, Permission::Write],
            );
            
            acl_manager.add_rule(rule).await.unwrap();
            
            // Test authorization check
            let is_authorized = acl_manager
                .check_authorization(&principal, "topic.events.user_activity", Permission::Read)
                .await
                .unwrap();
            assert!(is_authorized);
            
            // Test negative authorization
            let is_unauthorized = acl_manager
                .check_authorization(&principal, "topic.admin.system", Permission::Read)
                .await
                .unwrap();
            assert!(!is_unauthorized);
        });
    }

    #[test]
    fn test_acl_concurrent_access() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let acl_manager = Arc::new(ACLManager::new(temp_dir.path()).await.unwrap());
            
            // Test concurrent rule additions
            let mut add_handles = Vec::new();
            for i in 0..5 {
                let acl_clone = Arc::clone(&acl_manager);
                let handle = tokio::spawn(async move {
                    let principal = Principal::new(format!("user{}@example.com", i));
                    let resource = ResourcePattern::new(format!("topic.user{}.events.*", i)).unwrap();
                    let rule = ACLRule::new(
                        principal,
                        resource,
                        vec![Permission::Read, Permission::Write],
                    );
                    acl_clone.add_rule(rule).await.unwrap();
                });
                add_handles.push(handle);
            }
            
            futures::future::join_all(add_handles).await;
            
            // Test concurrent authorization checks
            let mut check_handles = Vec::new();
            for i in 0..5 {
                let acl_clone = Arc::clone(&acl_manager);
                let handle = tokio::spawn(async move {
                    let principal = Principal::new(format!("user{}@example.com", i));
                    let resource = format!("topic.user{}.events.login", i);
                    
                    let is_authorized = acl_clone
                        .check_authorization(&principal, &resource, Permission::Read)
                        .await
                        .unwrap();
                    assert!(is_authorized);
                    
                    // Test cross-user access (should be denied)
                    let other_resource = format!("topic.user{}.events.login", (i + 1) % 5);
                    let is_unauthorized = acl_clone
                        .check_authorization(&principal, &other_resource, Permission::Read)
                        .await
                        .unwrap();
                    assert!(!is_unauthorized);
                });
                check_handles.push(handle);
            }
            
            futures::future::join_all(check_handles).await;
        });
    }

    #[test]
    fn test_auth_token_memory_safety() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let auth_service = AuthenticationService::new(temp_dir.path()).await.unwrap();
            
            // Test token generation and validation
            let principal = Principal::new("test@example.com".to_string());
            let token = auth_service.generate_token(&principal).await.unwrap();
            
            // Test token validation
            let validated_principal = auth_service.validate_token(&token).await.unwrap();
            assert_eq!(principal.identity(), validated_principal.identity());
            
            // Test token with different principals
            for i in 0..10 {
                let p = Principal::new(format!("user{}@test.com", i));
                let t = auth_service.generate_token(&p).await.unwrap();
                let vp = auth_service.validate_token(&t).await.unwrap();
                assert_eq!(p.identity(), vp.identity());
            }
        });
    }

    #[test]
    fn test_auth_token_edge_cases() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let auth_service = AuthenticationService::new(temp_dir.path()).await.unwrap();
            
            // Test empty token
            let result = auth_service.validate_token("").await;
            assert!(result.is_err());
            
            // Test malformed token
            let malformed_token = "invalid.token.format";
            let result = auth_service.validate_token(malformed_token).await;
            assert!(result.is_err());
            
            // Test very long token (potential buffer issues)
            let long_token = "A".repeat(10000);
            let result = auth_service.validate_token(&long_token).await;
            assert!(result.is_err());
            
            // Test token with special characters in principal
            let special_principal = Principal::new("user+test@example.com".to_string());
            let token = auth_service.generate_token(&special_principal).await.unwrap();
            let validated = auth_service.validate_token(&token).await.unwrap();
            assert_eq!(special_principal.identity(), validated.identity());
        });
    }

    #[test]
    fn test_resource_pattern_memory_safety() {
        // Test resource pattern matching with various inputs
        let pattern = ResourcePattern::new("topic.events.*".to_string()).unwrap();
        
        // Test basic matching
        assert!(pattern.matches("topic.events.login"));
        assert!(pattern.matches("topic.events.logout"));
        assert!(!pattern.matches("topic.admin.system"));
        
        // Test edge cases
        assert!(!pattern.matches(""));
        assert!(!pattern.matches("topic.events"));  // No trailing part
        assert!(pattern.matches("topic.events."));  // Empty trailing part
        
        // Test very long resource names
        let long_resource = format!("topic.events.{}", "x".repeat(1000));
        assert!(pattern.matches(&long_resource));
        
        // Test special characters
        assert!(pattern.matches("topic.events.user-123"));
        assert!(pattern.matches("topic.events.user_activity"));
        assert!(pattern.matches("topic.events.user@domain.com"));
    }

    #[test]
    fn test_concurrent_security_operations() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let temp_dir = TempDir::new().unwrap();
            let auth_service = Arc::new(AuthenticationService::new(temp_dir.path()).await.unwrap());
            let acl_manager = Arc::new(ACLManager::new(temp_dir.path()).await.unwrap());
            
            // Test concurrent auth and authorization operations
            let mut handles = Vec::new();
            
            for i in 0..10 {
                let auth_clone = Arc::clone(&auth_service);
                let acl_clone = Arc::clone(&acl_manager);
                
                let handle = tokio::spawn(async move {
                    // Create principal and token
                    let principal = Principal::new(format!("concurrent_user{}@test.com", i));
                    let token = auth_clone.generate_token(&principal).await.unwrap();
                    
                    // Validate token
                    let validated = auth_clone.validate_token(&token).await.unwrap();
                    assert_eq!(principal.identity(), validated.identity());
                    
                    // Add ACL rule
                    let resource = ResourcePattern::new(format!("topic.user{}.data.*", i)).unwrap();
                    let rule = ACLRule::new(
                        principal.clone(),
                        resource,
                        vec![Permission::Read, Permission::Write],
                    );
                    acl_clone.add_rule(rule).await.unwrap();
                    
                    // Check authorization
                    let is_authorized = acl_clone
                        .check_authorization(&principal, &format!("topic.user{}.data.events", i), Permission::Read)
                        .await
                        .unwrap();
                    assert!(is_authorized);
                    
                    i
                });
                handles.push(handle);
            }
            
            let results: Vec<_> = futures::future::join_all(handles)
                .await
                .into_iter()
                .map(|r| r.unwrap())
                .collect();
            
            // Verify all operations completed
            assert_eq!(results.len(), 10);
            for (expected, actual) in (0..10).zip(results.iter()) {
                assert_eq!(expected, *actual);
            }
        });
    }
}