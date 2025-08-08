//! Security API Integration Tests
//!
//! This module tests the integration between Admin REST API, Admin CLI,
//! and security backend components, ensuring end-to-end security management
//! workflows function correctly.
//!
//! Target: 5 comprehensive security API integration tests

use super::test_infrastructure::*;
use rustmq::{
    admin::{api::AdminApi, security_api::SecurityApiHandler},
    security::{
        AclRule, Effect, ResourcePattern, Permission, CertificateRequest,
        CertificateRole, KeyType, KeyUsage, ExtendedKeyUsage,
    },
    error::RustMqError,
    Result,
};
use std::{sync::Arc, time::{Duration, SystemTime}};
use serde_json::Value;
use reqwest::Client as HttpClient;

#[cfg(test)]
mod security_api_integration_tests {
    use super::*;

    /// Test 1: Admin REST API to security backend integration
    #[tokio::test]
    async fn test_admin_rest_api_security_backend_integration() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Start Admin API server
        let admin_api_port = cluster.config.base_port + 8000;
        let admin_api = AdminApi::new(
            cluster.acl_manager.clone(),
            cluster.cert_manager.clone(),
            format!("127.0.0.1:{}", admin_api_port),
        ).await.unwrap();
        
        let api_handle = tokio::spawn(async move {
            admin_api.start().await.unwrap();
        });
        
        // Wait for API server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let http_client = HttpClient::new();
        let base_url = format!("http://127.0.0.1:{}", admin_api_port);
        
        // Test ACL rule management through REST API
        
        // 1. Create ACL rule via POST /api/v1/acl/rules
        let create_rule_payload = serde_json::json!({
            "principal": "api-test-client",
            "resource_pattern": {
                "Topic": "api.test.*"
            },
            "operation": "Read",
            "effect": "Allow"
        });
        
        let create_response = http_client
            .post(&format!("{}/api/v1/acl/rules", base_url))
            .json(&create_rule_payload)
            .send()
            .await
            .unwrap();
        
        assert_eq!(create_response.status(), 201);
        let create_result: Value = create_response.json().await.unwrap();
        let rule_id = create_result["rule_id"].as_str().unwrap();
        assert!(!rule_id.is_empty());
        
        // 2. List ACL rules via GET /api/v1/acl/rules
        let list_response = http_client
            .get(&format!("{}/api/v1/acl/rules", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(list_response.status(), 200);
        let rules_list: Value = list_response.json().await.unwrap();
        let rules = rules_list["rules"].as_array().unwrap();
        assert!(rules.len() >= 1);
        
        let created_rule = rules.iter()
            .find(|rule| rule["id"].as_str().unwrap() == rule_id)
            .unwrap();
        assert_eq!(created_rule["principal"], "api-test-client");
        assert_eq!(created_rule["operation"], "Read");
        
        // 3. Get specific ACL rule via GET /api/v1/acl/rules/{id}
        let get_response = http_client
            .get(&format!("{}/api/v1/acl/rules/{}", base_url, rule_id))
            .send()
            .await
            .unwrap();
        
        assert_eq!(get_response.status(), 200);
        let rule_detail: Value = get_response.json().await.unwrap();
        assert_eq!(rule_detail["principal"], "api-test-client");
        assert_eq!(rule_detail["effect"], "Allow");
        
        // 4. Update ACL rule via PUT /api/v1/acl/rules/{id}
        let update_payload = serde_json::json!({
            "principal": "api-test-client",
            "resource_pattern": {
                "Topic": "api.test.updated.*"
            },
            "operation": "Write",
            "effect": "Allow"
        });
        
        let update_response = http_client
            .put(&format!("{}/api/v1/acl/rules/{}", base_url, rule_id))
            .json(&update_payload)
            .send()
            .await
            .unwrap();
        
        assert_eq!(update_response.status(), 200);
        let update_result: Value = update_response.json().await.unwrap();
        assert!(update_result["success"].as_bool().unwrap());
        
        // Verify update was applied
        let verify_response = http_client
            .get(&format!("{}/api/v1/acl/rules/{}", base_url, rule_id))
            .send()
            .await
            .unwrap();
        let updated_rule: Value = verify_response.json().await.unwrap();
        assert_eq!(updated_rule["operation"], "Write");
        
        // Test certificate management through REST API
        
        // 5. Issue certificate via POST /api/v1/certificates/issue
        let cert_request_payload = serde_json::json!({
            "common_name": "api-test-server",
            "organization": "RustMQ API Test",
            "organizational_unit": "Testing",
            "subject_alt_names": ["api-test-server.local", "localhost"],
            "key_type": "Rsa2048",
            "key_usage": ["DigitalSignature", "KeyEncipherment"],
            "extended_key_usage": ["ServerAuth"],
            "validity_days": 30,
            "role": "Server"
        });
        
        let cert_response = http_client
            .post(&format!("{}/api/v1/certificates/issue", base_url))
            .json(&cert_request_payload)
            .send()
            .await
            .unwrap();
        
        assert_eq!(cert_response.status(), 201);
        let cert_result: Value = cert_response.json().await.unwrap();
        let cert_serial = cert_result["serial_number"].as_str().unwrap();
        assert!(!cert_serial.is_empty());
        assert_eq!(cert_result["common_name"], "api-test-server");
        
        // 6. List certificates via GET /api/v1/certificates
        let cert_list_response = http_client
            .get(&format!("{}/api/v1/certificates", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(cert_list_response.status(), 200);
        let cert_list: Value = cert_list_response.json().await.unwrap();
        let certificates = cert_list["certificates"].as_array().unwrap();
        
        let issued_cert = certificates.iter()
            .find(|cert| cert["serial_number"].as_str().unwrap() == cert_serial)
            .unwrap();
        assert_eq!(issued_cert["common_name"], "api-test-server");
        assert_eq!(issued_cert["role"], "Server");
        
        // 7. Revoke certificate via POST /api/v1/certificates/{serial}/revoke
        let revoke_payload = serde_json::json!({
            "reason": "KeyCompromise",
            "description": "API test revocation"
        });
        
        let revoke_response = http_client
            .post(&format!("{}/api/v1/certificates/{}/revoke", base_url, cert_serial))
            .json(&revoke_payload)
            .send()
            .await
            .unwrap();
        
        assert_eq!(revoke_response.status(), 200);
        let revoke_result: Value = revoke_response.json().await.unwrap();
        assert!(revoke_result["success"].as_bool().unwrap());
        
        // 8. Check certificate status via GET /api/v1/certificates/{serial}/status
        let status_response = http_client
            .get(&format!("{}/api/v1/certificates/{}/status", base_url, cert_serial))
            .send()
            .await
            .unwrap();
        
        assert_eq!(status_response.status(), 200);
        let status_result: Value = status_response.json().await.unwrap();
        assert!(status_result["is_revoked"].as_bool().unwrap());
        assert_eq!(status_result["revocation_reason"], "KeyCompromise");
        
        // Test error handling
        
        // 9. Test invalid ACL rule creation
        let invalid_payload = serde_json::json!({
            "principal": "", // Invalid empty principal
            "resource_pattern": {
                "Topic": "invalid.*"
            },
            "operation": "InvalidOperation", // Invalid operation
            "effect": "Allow"
        });
        
        let invalid_response = http_client
            .post(&format!("{}/api/v1/acl/rules", base_url))
            .json(&invalid_payload)
            .send()
            .await
            .unwrap();
        
        assert_eq!(invalid_response.status(), 400);
        let error_result: Value = invalid_response.json().await.unwrap();
        assert!(!error_result["error"].as_str().unwrap().is_empty());
        
        // 10. Test non-existent resource access
        let not_found_response = http_client
            .get(&format!("{}/api/v1/acl/rules/non-existent-id", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(not_found_response.status(), 404);
        
        // Clean up
        api_handle.abort();
        cluster.shutdown().await.unwrap();
    }

    /// Test 2: Admin CLI to REST API to security components workflow
    #[tokio::test]
    async fn test_admin_cli_to_api_workflow() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        // Start Admin API server
        let admin_api_port = cluster.config.base_port + 8001;
        let admin_api = AdminApi::new(
            cluster.acl_manager.clone(),
            cluster.cert_manager.clone(),
            format!("127.0.0.1:{}", admin_api_port),
        ).await.unwrap();
        
        let api_handle = tokio::spawn(async move {
            admin_api.start().await.unwrap();
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let api_url = format!("http://127.0.0.1:{}", admin_api_port);
        
        // Simulate CLI commands through API calls
        
        // CLI Command: rustmq-admin acl create-rule --principal "cli-test-user" --resource "cli.*" --operation "read" --effect "allow"
        let cli_create_rule = async {
            let http_client = HttpClient::new();
            let payload = serde_json::json!({
                "principal": "cli-test-user",
                "resource_pattern": {
                    "Topic": "cli.*"
                },
                "operation": "Read",
                "effect": "Allow",
                "created_by": "admin-cli",
                "description": "Rule created via CLI"
            });
            
            http_client
                .post(&format!("{}/api/v1/acl/rules", api_url))
                .json(&payload)
                .send()
                .await
        };
        
        let create_response = cli_create_rule.await.unwrap();
        assert_eq!(create_response.status(), 201);
        let create_result: Value = create_response.json().await.unwrap();
        let rule_id = create_result["rule_id"].as_str().unwrap().to_string();
        
        // CLI Command: rustmq-admin acl list-rules --principal "cli-test-user"
        let cli_list_rules = async {
            let http_client = HttpClient::new();
            http_client
                .get(&format!("{}/api/v1/acl/rules?principal=cli-test-user", api_url))
                .send()
                .await
        };
        
        let list_response = cli_list_rules.await.unwrap();
        assert_eq!(list_response.status(), 200);
        let list_result: Value = list_response.json().await.unwrap();
        let rules = list_result["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0]["principal"], "cli-test-user");
        
        // CLI Command: rustmq-admin certificate issue --common-name "cli-test-cert" --role "client"
        let cli_issue_cert = async {
            let http_client = HttpClient::new();
            let payload = serde_json::json!({
                "common_name": "cli-test-cert",
                "organization": "RustMQ CLI Test",
                "role": "Client",
                "key_type": "Rsa2048",
                "validity_days": 30,
                "issued_by": "admin-cli"
            });
            
            http_client
                .post(&format!("{}/api/v1/certificates/issue", api_url))
                .json(&payload)
                .send()
                .await
        };
        
        let cert_response = cli_issue_cert.await.unwrap();
        assert_eq!(cert_response.status(), 201);
        let cert_result: Value = cert_response.json().await.unwrap();
        let cert_serial = cert_result["serial_number"].as_str().unwrap().to_string();
        
        // CLI Command: rustmq-admin certificate list --role "client"
        let cli_list_certs = async {
            let http_client = HttpClient::new();
            http_client
                .get(&format!("{}/api/v1/certificates?role=Client", api_url))
                .send()
                .await
        };
        
        let cert_list_response = cli_list_certs.await.unwrap();
        assert_eq!(cert_list_response.status(), 200);
        let cert_list_result: Value = cert_list_response.json().await.unwrap();
        let certificates = cert_list_result["certificates"].as_array().unwrap();
        
        let cli_cert = certificates.iter()
            .find(|cert| cert["serial_number"].as_str().unwrap() == cert_serial)
            .unwrap();
        assert_eq!(cli_cert["common_name"], "cli-test-cert");
        
        // CLI Command: rustmq-admin security audit --operation "create" --since "1h"
        let cli_audit = async {
            let http_client = HttpClient::new();
            let since_time = SystemTime::now() - Duration::from_secs(3600);
            let since_timestamp = since_time.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
            
            http_client
                .get(&format!("{}/api/v1/security/audit?operation=create&since={}", api_url, since_timestamp))
                .send()
                .await
        };
        
        let audit_response = cli_audit.await.unwrap();
        assert_eq!(audit_response.status(), 200);
        let audit_result: Value = audit_response.json().await.unwrap();
        let audit_entries = audit_result["entries"].as_array().unwrap();
        assert!(audit_entries.len() >= 2); // At least ACL rule and certificate creation
        
        // Test CLI batch operations
        
        // CLI Command: rustmq-admin acl batch-create --file "rules.json"
        let batch_rules = vec![
            serde_json::json!({
                "principal": "batch-user-1",
                "resource_pattern": {"Topic": "batch.topic1.*"},
                "operation": "Read",
                "effect": "Allow"
            }),
            serde_json::json!({
                "principal": "batch-user-2",
                "resource_pattern": {"Topic": "batch.topic2.*"},
                "operation": "Write",
                "effect": "Allow"
            }),
        ];
        
        let cli_batch_create = async {
            let http_client = HttpClient::new();
            let payload = serde_json::json!({
                "rules": batch_rules,
                "created_by": "admin-cli-batch"
            });
            
            http_client
                .post(&format!("{}/api/v1/acl/rules/batch", api_url))
                .json(&payload)
                .send()
                .await
        };
        
        let batch_response = cli_batch_create.await.unwrap();
        assert_eq!(batch_response.status(), 201);
        let batch_result: Value = batch_response.json().await.unwrap();
        let created_rules = batch_result["created_rules"].as_array().unwrap();
        assert_eq!(created_rules.len(), 2);
        
        // Test CLI error handling and validation
        
        // CLI Command with invalid input
        let cli_invalid_command = async {
            let http_client = HttpClient::new();
            let payload = serde_json::json!({
                "principal": "invalid-user",
                "resource_pattern": {"InvalidType": "invalid.*"}, // Invalid resource type
                "operation": "Read",
                "effect": "Allow"
            });
            
            http_client
                .post(&format!("{}/api/v1/acl/rules", api_url))
                .json(&payload)
                .send()
                .await
        };
        
        let invalid_response = cli_invalid_command.await.unwrap();
        assert_eq!(invalid_response.status(), 400);
        let error_result: Value = invalid_response.json().await.unwrap();
        assert!(error_result["error"].as_str().unwrap().contains("Invalid"));
        
        // Test CLI help and documentation endpoints
        let cli_help = async {
            let http_client = HttpClient::new();
            http_client
                .get(&format!("{}/api/v1/help/commands", api_url))
                .send()
                .await
        };
        
        let help_response = cli_help.await.unwrap();
        assert_eq!(help_response.status(), 200);
        let help_result: Value = help_response.json().await.unwrap();
        assert!(!help_result["commands"].as_array().unwrap().is_empty());
        
        // Clean up
        api_handle.abort();
        cluster.shutdown().await.unwrap();
    }

    /// Test 3: Security configuration hot updates through API
    #[tokio::test]
    async fn test_security_configuration_hot_updates() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let admin_api_port = cluster.config.base_port + 8002;
        let admin_api = AdminApi::new(
            cluster.acl_manager.clone(),
            cluster.cert_manager.clone(),
            format!("127.0.0.1:{}", admin_api_port),
        ).await.unwrap();
        
        let api_handle = tokio::spawn(async move {
            admin_api.start().await.unwrap();
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let http_client = HttpClient::new();
        let base_url = format!("http://127.0.0.1:{}", admin_api_port);
        
        // Test ACL configuration updates
        
        // 1. Get current ACL configuration
        let get_config_response = http_client
            .get(&format!("{}/api/v1/security/config/acl", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(get_config_response.status(), 200);
        let current_config: Value = get_config_response.json().await.unwrap();
        let original_cache_size = current_config["cache_size_mb"].as_u64().unwrap();
        
        // 2. Update ACL configuration
        let new_config = serde_json::json!({
            "cache_size_mb": original_cache_size * 2,
            "cache_ttl_seconds": 600,
            "l2_shard_count": 64,
            "bloom_filter_size": 2000000,
            "batch_fetch_size": 200,
            "enable_audit_logging": true,
            "negative_cache_enabled": true
        });
        
        let update_config_response = http_client
            .put(&format!("{}/api/v1/security/config/acl", base_url))
            .json(&new_config)
            .send()
            .await
            .unwrap();
        
        assert_eq!(update_config_response.status(), 200);
        let update_result: Value = update_config_response.json().await.unwrap();
        assert!(update_result["success"].as_bool().unwrap());
        assert!(update_result["hot_reload_applied"].as_bool().unwrap());
        
        // 3. Verify configuration was applied
        tokio::time::sleep(Duration::from_millis(50)).await; // Allow config propagation
        
        let verify_config_response = http_client
            .get(&format!("{}/api/v1/security/config/acl", base_url))
            .send()
            .await
            .unwrap();
        
        let updated_config: Value = verify_config_response.json().await.unwrap();
        assert_eq!(updated_config["cache_size_mb"].as_u64().unwrap(), original_cache_size * 2);
        assert_eq!(updated_config["cache_ttl_seconds"].as_u64().unwrap(), 600);
        
        // Test certificate configuration updates
        
        // 4. Get current certificate configuration
        let cert_config_response = http_client
            .get(&format!("{}/api/v1/security/config/certificates", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(cert_config_response.status(), 200);
        let cert_config: Value = cert_config_response.json().await.unwrap();
        let original_validity = cert_config["cert_validity_days"].as_u64().unwrap();
        
        // 5. Update certificate configuration
        let new_cert_config = serde_json::json!({
            "cert_validity_days": original_validity + 30,
            "auto_renew_before_expiry_days": 14,
            "crl_check_enabled": true,
            "ocsp_check_enabled": true
        });
        
        let update_cert_config_response = http_client
            .put(&format!("{}/api/v1/security/config/certificates", base_url))
            .json(&new_cert_config)
            .send()
            .await
            .unwrap();
        
        assert_eq!(update_cert_config_response.status(), 200);
        let cert_update_result: Value = update_cert_config_response.json().await.unwrap();
        assert!(cert_update_result["success"].as_bool().unwrap());
        
        // Test audit configuration updates
        
        // 6. Update audit configuration
        let audit_config = serde_json::json!({
            "enabled": true,
            "log_authentication_events": true,
            "log_authorization_events": true,
            "log_certificate_events": true,
            "log_failed_attempts": true,
            "max_log_size_mb": 500
        });
        
        let update_audit_response = http_client
            .put(&format!("{}/api/v1/security/config/audit", base_url))
            .json(&audit_config)
            .send()
            .await
            .unwrap();
        
        assert_eq!(update_audit_response.status(), 200);
        
        // Test configuration validation
        
        // 7. Try to set invalid configuration
        let invalid_config = serde_json::json!({
            "cache_size_mb": -1, // Invalid negative value
            "cache_ttl_seconds": "invalid", // Invalid type
            "bloom_filter_size": 0 // Invalid zero value
        });
        
        let invalid_config_response = http_client
            .put(&format!("{}/api/v1/security/config/acl", base_url))
            .json(&invalid_config)
            .send()
            .await
            .unwrap();
        
        assert_eq!(invalid_config_response.status(), 400);
        let validation_error: Value = invalid_config_response.json().await.unwrap();
        assert!(validation_error["error"].as_str().unwrap().contains("validation"));
        
        // Test configuration rollback
        
        // 8. Rollback to previous configuration
        let rollback_response = http_client
            .post(&format!("{}/api/v1/security/config/rollback", base_url))
            .json(&serde_json::json!({"component": "acl"}))
            .send()
            .await
            .unwrap();
        
        assert_eq!(rollback_response.status(), 200);
        let rollback_result: Value = rollback_response.json().await.unwrap();
        assert!(rollback_result["success"].as_bool().unwrap());
        
        // 9. Verify rollback was applied
        let final_config_response = http_client
            .get(&format!("{}/api/v1/security/config/acl", base_url))
            .send()
            .await
            .unwrap();
        
        let final_config: Value = final_config_response.json().await.unwrap();
        assert_eq!(final_config["cache_size_mb"].as_u64().unwrap(), original_cache_size);
        
        // Test configuration export and import
        
        // 10. Export security configuration
        let export_response = http_client
            .get(&format!("{}/api/v1/security/config/export", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(export_response.status(), 200);
        let export_result: Value = export_response.json().await.unwrap();
        assert!(!export_result["configuration"].as_object().unwrap().is_empty());
        assert!(!export_result["export_timestamp"].as_str().unwrap().is_empty());
        
        // 11. Import security configuration
        let import_config = export_result["configuration"].clone();
        let import_response = http_client
            .post(&format!("{}/api/v1/security/config/import", base_url))
            .json(&serde_json::json!({"configuration": import_config}))
            .send()
            .await
            .unwrap();
        
        assert_eq!(import_response.status(), 200);
        let import_result: Value = import_response.json().await.unwrap();
        assert!(import_result["success"].as_bool().unwrap());
        
        api_handle.abort();
        cluster.shutdown().await.unwrap();
    }

    /// Test 4: Security metrics aggregation and reporting through API
    #[tokio::test]
    async fn test_security_metrics_aggregation_and_reporting() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let admin_api_port = cluster.config.base_port + 8003;
        let admin_api = AdminApi::new(
            cluster.acl_manager.clone(),
            cluster.cert_manager.clone(),
            format!("127.0.0.1:{}", admin_api_port),
        ).await.unwrap();
        
        let api_handle = tokio::spawn(async move {
            admin_api.start().await.unwrap();
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let http_client = HttpClient::new();
        let base_url = format!("http://127.0.0.1:{}", admin_api_port);
        
        // Generate some security activity for metrics
        
        // Create ACL rules
        for i in 0..10 {
            let rule_payload = serde_json::json!({
                "principal": format!("metrics-user-{}", i),
                "resource_pattern": {
                    "Topic": format!("metrics.topic{}.*", i)
                },
                "operation": "Read",
                "effect": "Allow"
            });
            
            http_client
                .post(&format!("{}/api/v1/acl/rules", base_url))
                .json(&rule_payload)
                .send()
                .await
                .unwrap();
        }
        
        // Issue certificates
        for i in 0..5 {
            let cert_payload = serde_json::json!({
                "common_name": format!("metrics-cert-{}", i),
                "role": "Client",
                "validity_days": 30
            });
            
            http_client
                .post(&format!("{}/api/v1/certificates/issue", base_url))
                .json(&cert_payload)
                .send()
                .await
                .unwrap();
        }
        
        // Wait for metrics to accumulate
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Test authentication metrics
        
        // 1. Get authentication metrics
        let auth_metrics_response = http_client
            .get(&format!("{}/api/v1/security/metrics/authentication", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(auth_metrics_response.status(), 200);
        let auth_metrics: Value = auth_metrics_response.json().await.unwrap();
        
        assert!(auth_metrics["total_attempts"].as_u64().unwrap() >= 0);
        assert!(auth_metrics["successful_attempts"].as_u64().unwrap() >= 0);
        assert!(auth_metrics["failed_attempts"].as_u64().unwrap() >= 0);
        assert!(auth_metrics["average_latency_ms"].as_f64().unwrap() >= 0.0);
        
        // Test authorization metrics
        
        // 2. Get authorization metrics
        let authz_metrics_response = http_client
            .get(&format!("{}/api/v1/security/metrics/authorization", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(authz_metrics_response.status(), 200);
        let authz_metrics: Value = authz_metrics_response.json().await.unwrap();
        
        assert!(authz_metrics["total_checks"].as_u64().unwrap() >= 0);
        assert!(authz_metrics["allowed_requests"].as_u64().unwrap() >= 0);
        assert!(authz_metrics["denied_requests"].as_u64().unwrap() >= 0);
        assert!(authz_metrics["cache_hit_ratio"].as_f64().unwrap() >= 0.0);
        
        // Test certificate metrics
        
        // 3. Get certificate metrics
        let cert_metrics_response = http_client
            .get(&format!("{}/api/v1/security/metrics/certificates", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(cert_metrics_response.status(), 200);
        let cert_metrics: Value = cert_metrics_response.json().await.unwrap();
        
        assert!(cert_metrics["total_issued"].as_u64().unwrap() >= 5);
        assert!(cert_metrics["total_revoked"].as_u64().unwrap() >= 0);
        assert!(cert_metrics["expiring_soon"].as_u64().unwrap() >= 0);
        assert!(cert_metrics["average_issuance_time_ms"].as_f64().unwrap() >= 0.0);
        
        // Test ACL metrics
        
        // 4. Get ACL metrics
        let acl_metrics_response = http_client
            .get(&format!("{}/api/v1/security/metrics/acl", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(acl_metrics_response.status(), 200);
        let acl_metrics: Value = acl_metrics_response.json().await.unwrap();
        
        assert!(acl_metrics["total_rules"].as_u64().unwrap() >= 10);
        assert!(acl_metrics["rules_by_effect"]["Allow"].as_u64().unwrap() >= 10);
        assert!(acl_metrics["rules_by_effect"]["Deny"].as_u64().unwrap_or(0) >= 0);
        
        // Test aggregated security overview
        
        // 5. Get security overview
        let overview_response = http_client
            .get(&format!("{}/api/v1/security/metrics/overview", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(overview_response.status(), 200);
        let overview: Value = overview_response.json().await.unwrap();
        
        assert!(!overview["security_status"].as_str().unwrap().is_empty());
        assert!(overview["authentication"]["enabled"].as_bool().unwrap());
        assert!(overview["authorization"]["enabled"].as_bool().unwrap());
        assert!(overview["certificate_management"]["enabled"].as_bool().unwrap());
        
        // Test historical metrics
        
        // 6. Get historical metrics
        let historical_response = http_client
            .get(&format!("{}/api/v1/security/metrics/historical?period=1h&granularity=5m", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(historical_response.status(), 200);
        let historical: Value = historical_response.json().await.unwrap();
        
        let data_points = historical["data_points"].as_array().unwrap();
        assert!(!data_points.is_empty());
        
        for point in data_points {
            assert!(!point["timestamp"].as_str().unwrap().is_empty());
            assert!(point["authentication_attempts"].as_u64().unwrap() >= 0);
            assert!(point["authorization_checks"].as_u64().unwrap() >= 0);
        }
        
        // Test custom metrics queries
        
        // 7. Query metrics with filters
        let filtered_metrics_response = http_client
            .get(&format!("{}/api/v1/security/metrics/query?metric=authentication_latency&principal=metrics-user-1&time_range=1h", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(filtered_metrics_response.status(), 200);
        let filtered_metrics: Value = filtered_metrics_response.json().await.unwrap();
        
        assert!(!filtered_metrics["metric_name"].as_str().unwrap().is_empty());
        assert!(filtered_metrics["data_points"].as_array().unwrap().len() >= 0);
        
        // Test metrics export
        
        // 8. Export metrics in different formats
        let export_formats = vec!["json", "csv", "prometheus"];
        
        for format in export_formats {
            let export_response = http_client
                .get(&format!("{}/api/v1/security/metrics/export?format={}&period=1h", base_url, format))
                .send()
                .await
                .unwrap();
            
            assert_eq!(export_response.status(), 200);
            
            let content_type = export_response.headers().get("content-type").unwrap();
            match format {
                "json" => assert!(content_type.to_str().unwrap().contains("application/json")),
                "csv" => assert!(content_type.to_str().unwrap().contains("text/csv")),
                "prometheus" => assert!(content_type.to_str().unwrap().contains("text/plain")),
                _ => {}
            }
            
            let body = export_response.text().await.unwrap();
            assert!(!body.is_empty());
        }
        
        // Test real-time metrics streaming
        
        // 9. Set up metrics streaming endpoint
        let streaming_response = http_client
            .get(&format!("{}/api/v1/security/metrics/stream?interval=1s", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(streaming_response.status(), 200);
        
        // Test alerting integration
        
        // 10. Configure metrics alerts
        let alert_config = serde_json::json!({
            "alerts": [
                {
                    "name": "high_authentication_failures",
                    "metric": "authentication_failure_rate",
                    "threshold": 0.1,
                    "operator": "greater_than",
                    "duration": "5m"
                },
                {
                    "name": "certificate_expiry_warning",
                    "metric": "certificates_expiring_soon",
                    "threshold": 10,
                    "operator": "greater_than",
                    "duration": "1m"
                }
            ]
        });
        
        let alert_config_response = http_client
            .post(&format!("{}/api/v1/security/metrics/alerts", base_url))
            .json(&alert_config)
            .send()
            .await
            .unwrap();
        
        assert_eq!(alert_config_response.status(), 200);
        let alert_result: Value = alert_config_response.json().await.unwrap();
        assert!(alert_result["success"].as_bool().unwrap());
        
        api_handle.abort();
        cluster.shutdown().await.unwrap();
    }

    /// Test 5: Security audit log integration across all components
    #[tokio::test]
    async fn test_security_audit_log_integration() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let admin_api_port = cluster.config.base_port + 8004;
        let admin_api = AdminApi::new(
            cluster.acl_manager.clone(),
            cluster.cert_manager.clone(),
            format!("127.0.0.1:{}", admin_api_port),
        ).await.unwrap();
        
        let api_handle = tokio::spawn(async move {
            admin_api.start().await.unwrap();
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let http_client = HttpClient::new();
        let base_url = format!("http://127.0.0.1:{}", admin_api_port);
        
        // Generate comprehensive audit events
        
        // 1. Authentication events
        let auth_client = cluster.create_test_client("audit-test-user").await.unwrap();
        let _auth_connection = auth_client.connect_to_broker(0).await.unwrap();
        
        // 2. ACL rule management events
        let acl_rule_payload = serde_json::json!({
            "principal": "audit-test-user",
            "resource_pattern": {
                "Topic": "audit.*"
            },
            "operation": "Read",
            "effect": "Allow",
            "created_by": "audit-tester",
            "description": "Audit test rule"
        });
        
        let acl_response = http_client
            .post(&format!("{}/api/v1/acl/rules", base_url))
            .json(&acl_rule_payload)
            .send()
            .await
            .unwrap();
        let acl_result: Value = acl_response.json().await.unwrap();
        let rule_id = acl_result["rule_id"].as_str().unwrap();
        
        // 3. Certificate management events
        let cert_payload = serde_json::json!({
            "common_name": "audit-test-cert",
            "role": "Client",
            "validity_days": 30,
            "issued_by": "audit-tester"
        });
        
        let cert_response = http_client
            .post(&format!("{}/api/v1/certificates/issue", base_url))
            .json(&cert_payload)
            .send()
            .await
            .unwrap();
        let cert_result: Value = cert_response.json().await.unwrap();
        let cert_serial = cert_result["serial_number"].as_str().unwrap();
        
        // 4. Authorization events (through actual authorization checks)
        let authz_connection = auth_client.connect_to_broker(0).await.unwrap();
        let _authz_result = authz_connection.authorize("audit.test.topic", Permission::Read).await.unwrap();
        
        // 5. Certificate revocation event
        let revoke_payload = serde_json::json!({
            "reason": "KeyCompromise",
            "description": "Audit test revocation",
            "revoked_by": "audit-tester"
        });
        
        http_client
            .post(&format!("{}/api/v1/certificates/{}/revoke", base_url, cert_serial))
            .json(&revoke_payload)
            .send()
            .await
            .unwrap();
        
        // Wait for audit events to be recorded
        tokio::time::sleep(Duration::from_millis(200)).await;
        
        // Test comprehensive audit log retrieval
        
        // 6. Get all audit events
        let all_audit_response = http_client
            .get(&format!("{}/api/v1/security/audit", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(all_audit_response.status(), 200);
        let all_audit: Value = all_audit_response.json().await.unwrap();
        let entries = all_audit["entries"].as_array().unwrap();
        assert!(entries.len() >= 4); // At least ACL, cert issuance, authz, revocation
        
        // Verify audit entry structure
        for entry in entries {
            assert!(!entry["id"].as_str().unwrap().is_empty());
            assert!(!entry["timestamp"].as_str().unwrap().is_empty());
            assert!(!entry["event_type"].as_str().unwrap().is_empty());
            assert!(!entry["component"].as_str().unwrap().is_empty());
            
            if let Some(principal) = entry["principal"].as_str() {
                assert!(!principal.is_empty());
            }
            
            if let Some(operator) = entry["operator"].as_str() {
                assert!(!operator.is_empty());
            }
        }
        
        // Test audit log filtering
        
        // 7. Filter by event type
        let acl_audit_response = http_client
            .get(&format!("{}/api/v1/security/audit?event_type=acl_rule_created", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(acl_audit_response.status(), 200);
        let acl_audit: Value = acl_audit_response.json().await.unwrap();
        let acl_entries = acl_audit["entries"].as_array().unwrap();
        assert!(acl_entries.len() >= 1);
        
        for entry in acl_entries {
            assert_eq!(entry["event_type"], "acl_rule_created");
        }
        
        // 8. Filter by principal
        let principal_audit_response = http_client
            .get(&format!("{}/api/v1/security/audit?principal=audit-test-user", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(principal_audit_response.status(), 200);
        let principal_audit: Value = principal_audit_response.json().await.unwrap();
        let principal_entries = principal_audit["entries"].as_array().unwrap();
        assert!(principal_entries.len() >= 1);
        
        for entry in principal_entries {
            if let Some(principal) = entry["principal"].as_str() {
                assert_eq!(principal, "audit-test-user");
            }
        }
        
        // 9. Filter by time range
        let now = SystemTime::now();
        let one_hour_ago = now - Duration::from_secs(3600);
        let since_timestamp = one_hour_ago.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        let until_timestamp = now.duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
        
        let time_audit_response = http_client
            .get(&format!("{}/api/v1/security/audit?since={}&until={}", base_url, since_timestamp, until_timestamp))
            .send()
            .await
            .unwrap();
        
        assert_eq!(time_audit_response.status(), 200);
        let time_audit: Value = time_audit_response.json().await.unwrap();
        let time_entries = time_audit["entries"].as_array().unwrap();
        assert!(time_entries.len() >= 1);
        
        // Test audit log analysis
        
        // 10. Get audit summary
        let summary_response = http_client
            .get(&format!("{}/api/v1/security/audit/summary?period=1h", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(summary_response.status(), 200);
        let summary: Value = summary_response.json().await.unwrap();
        
        assert!(summary["total_events"].as_u64().unwrap() >= 4);
        assert!(!summary["events_by_type"].as_object().unwrap().is_empty());
        assert!(!summary["events_by_component"].as_object().unwrap().is_empty());
        
        // 11. Get security incidents
        let incidents_response = http_client
            .get(&format!("{}/api/v1/security/audit/incidents", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(incidents_response.status(), 200);
        let incidents: Value = incidents_response.json().await.unwrap();
        let incident_list = incidents["incidents"].as_array().unwrap();
        
        // Check if certificate revocation was flagged as incident
        let revocation_incident = incident_list.iter()
            .find(|incident| incident["event_type"].as_str().unwrap() == "certificate_revoked");
        
        if let Some(incident) = revocation_incident {
            assert_eq!(incident["severity"], "medium");
            assert!(!incident["description"].as_str().unwrap().is_empty());
        }
        
        // Test audit log export
        
        // 12. Export audit log
        let export_response = http_client
            .get(&format!("{}/api/v1/security/audit/export?format=json&period=1h", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(export_response.status(), 200);
        let export_body = export_response.text().await.unwrap();
        assert!(!export_body.is_empty());
        
        let export_data: Value = serde_json::from_str(&export_body).unwrap();
        assert!(!export_data["audit_entries"].as_array().unwrap().is_empty());
        assert!(!export_data["export_metadata"]["timestamp"].as_str().unwrap().is_empty());
        
        // Test audit log integrity
        
        // 13. Verify audit log integrity
        let integrity_response = http_client
            .get(&format!("{}/api/v1/security/audit/integrity", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(integrity_response.status(), 200);
        let integrity: Value = integrity_response.json().await.unwrap();
        
        assert!(integrity["is_valid"].as_bool().unwrap());
        assert!(!integrity["checksum"].as_str().unwrap().is_empty());
        assert!(integrity["entry_count"].as_u64().unwrap() >= 4);
        
        // Test audit alerting
        
        // 14. Configure audit alerts
        let alert_config = serde_json::json!({
            "alerts": [
                {
                    "name": "certificate_revocation_alert",
                    "event_type": "certificate_revoked",
                    "severity": "high",
                    "notification_channels": ["email", "webhook"]
                },
                {
                    "name": "authentication_failure_spike",
                    "event_type": "authentication_failed",
                    "threshold": 10,
                    "time_window": "5m",
                    "severity": "medium"
                }
            ]
        });
        
        let alert_response = http_client
            .post(&format!("{}/api/v1/security/audit/alerts", base_url))
            .json(&alert_config)
            .send()
            .await
            .unwrap();
        
        assert_eq!(alert_response.status(), 200);
        let alert_result: Value = alert_response.json().await.unwrap();
        assert!(alert_result["success"].as_bool().unwrap());
        
        // 15. Check triggered alerts
        let triggered_alerts_response = http_client
            .get(&format!("{}/api/v1/security/audit/alerts/triggered", base_url))
            .send()
            .await
            .unwrap();
        
        assert_eq!(triggered_alerts_response.status(), 200);
        let triggered_alerts: Value = triggered_alerts_response.json().await.unwrap();
        let alerts = triggered_alerts["alerts"].as_array().unwrap();
        
        // Should have certificate revocation alert
        let revocation_alert = alerts.iter()
            .find(|alert| alert["name"].as_str().unwrap() == "certificate_revocation_alert");
        
        if let Some(alert) = revocation_alert {
            assert_eq!(alert["severity"], "high");
            assert!(!alert["triggered_at"].as_str().unwrap().is_empty());
        }
        
        api_handle.abort();
        cluster.shutdown().await.unwrap();
    }
}

#[cfg(test)]
mod security_api_stress_tests {
    use super::*;
    
    /// Stress test: Security API performance under concurrent load
    #[tokio::test]
    async fn test_security_api_performance_under_load() {
        let mut cluster = SecurityTestCluster::new().await.unwrap();
        cluster.start().await.unwrap();
        
        let admin_api_port = cluster.config.base_port + 8005;
        let admin_api = AdminApi::new(
            cluster.acl_manager.clone(),
            cluster.cert_manager.clone(),
            format!("127.0.0.1:{}", admin_api_port),
        ).await.unwrap();
        
        let api_handle = tokio::spawn(async move {
            admin_api.start().await.unwrap();
        });
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        let base_url = format!("http://127.0.0.1:{}", admin_api_port);
        
        // Test concurrent ACL rule creation
        let concurrent_count = 100;
        let mut handles = Vec::new();
        
        let start_time = tokio::time::Instant::now();
        
        for i in 0..concurrent_count {
            let base_url_clone = base_url.clone();
            let handle = tokio::spawn(async move {
                let http_client = HttpClient::new();
                let payload = serde_json::json!({
                    "principal": format!("stress-user-{}", i),
                    "resource_pattern": {
                        "Topic": format!("stress.topic{}.*", i)
                    },
                    "operation": "Read",
                    "effect": "Allow"
                });
                
                let response = http_client
                    .post(&format!("{}/api/v1/acl/rules", base_url_clone))
                    .json(&payload)
                    .send()
                    .await
                    .unwrap();
                
                (response.status().as_u16(), i)
            });
            
            handles.push(handle);
        }
        
        let mut success_count = 0;
        for handle in handles {
            let (status_code, _i) = handle.await.unwrap();
            if status_code == 201 {
                success_count += 1;
            }
        }
        
        let total_time = start_time.elapsed();
        let throughput = concurrent_count as f64 / total_time.as_secs_f64();
        
        // Performance assertions
        assert!(success_count >= concurrent_count * 95 / 100, 
               "Success rate should be >= 95%, got {}/{}", success_count, concurrent_count);
        assert!(throughput > 20.0, 
               "API throughput should be > 20 requests/sec, got {:.2}", throughput);
        assert!(total_time < Duration::from_secs(10), 
               "Total time should be < 10s, got {:?}", total_time);
        
        println!("Security API stress test results:");
        println!("  Concurrent requests: {}", concurrent_count);
        println!("  Successful requests: {}", success_count);
        println!("  Total time: {:?}", total_time);
        println!("  Throughput: {:.2} requests/sec", throughput);
        
        api_handle.abort();
        cluster.shutdown().await.unwrap();
    }
}