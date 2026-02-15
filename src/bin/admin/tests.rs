use super::*;
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::api_client::{
        ApiResponse, GenerateCaRequest, IssueCertificateRequest, RevokeCertificateRequest,
    };
    use super::formatters::{OutputFormat, csv_escape_value};
    use super::*;

    #[test]
    fn test_output_format_parsing() {
        assert_eq!(
            "table".parse::<OutputFormat>().unwrap(),
            OutputFormat::Table
        );
        assert_eq!("json".parse::<OutputFormat>().unwrap(), OutputFormat::Json);
        assert_eq!("yaml".parse::<OutputFormat>().unwrap(), OutputFormat::Yaml);
        assert_eq!("csv".parse::<OutputFormat>().unwrap(), OutputFormat::Csv);

        assert!("invalid".parse::<OutputFormat>().is_err());
    }

    #[test]
    fn test_json_formatting() {
        let data = json!({
            "name": "test",
            "value": 42
        });

        let formatted = format_output(&data, OutputFormat::Json, false).unwrap();
        assert!(formatted.contains("\"name\": \"test\""));
        assert!(formatted.contains("\"value\": 42"));
    }

    #[test]
    fn test_yaml_formatting() {
        let data = json!({
            "name": "test",
            "value": 42
        });

        let formatted = format_output(&data, OutputFormat::Yaml, false).unwrap();
        assert!(formatted.contains("name: \"test\""));
        assert!(formatted.contains("value: 42"));
    }

    #[test]
    fn test_csv_formatting_array() {
        let data = json!([
            {"name": "test1", "value": 42},
            {"name": "test2", "value": 43}
        ]);

        let formatted = format_output(&data, OutputFormat::Csv, false).unwrap();
        assert!(formatted.contains("name,value"));
        assert!(formatted.contains("test1,42"));
        assert!(formatted.contains("test2,43"));
    }

    #[test]
    fn test_csv_formatting_object() {
        let data = json!({
            "name": "test",
            "value": 42
        });

        let formatted = format_output(&data, OutputFormat::Csv, false).unwrap();
        assert!(formatted.contains("field,value"));
        assert!(formatted.contains("name,test"));
        assert!(formatted.contains("value,42"));
    }

    #[test]
    fn test_table_formatting_array() {
        let data = json!([
            {"name": "test1", "status": "active"},
            {"name": "test2", "status": "inactive"}
        ]);

        let formatted = format_output(&data, OutputFormat::Table, true).unwrap();
        assert!(formatted.contains("NAME"));
        assert!(formatted.contains("STATUS"));
        assert!(formatted.contains("test1"));
        assert!(formatted.contains("test2"));
        assert!(formatted.contains("active"));
        assert!(formatted.contains("inactive"));
    }

    #[test]
    fn test_table_formatting_object() {
        let data = json!({
            "certificate_id": "cert_123",
            "status": "active",
            "expires_in": 30
        });

        let formatted = format_output(&data, OutputFormat::Table, true).unwrap();
        assert!(formatted.contains("FIELD"));
        assert!(formatted.contains("VALUE"));
        assert!(formatted.contains("certificate_id"));
        assert!(formatted.contains("cert_123"));
        assert!(formatted.contains("status"));
        assert!(formatted.contains("active"));
    }

    #[test]
    fn test_api_response_serialization() {
        let response = ApiResponse {
            success: true,
            data: Some(json!({"test": "value"})),
            error: None,
            leader_hint: Some("leader-1".to_string()),
        };

        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains("\"success\":true"));
        assert!(serialized.contains("\"test\":\"value\""));
        assert!(serialized.contains("\"leader_hint\":\"leader-1\""));
    }

    #[test]
    fn test_generate_ca_request() {
        let request = GenerateCaRequest {
            common_name: "RustMQ Root CA".to_string(),
            organization: Some("RustMQ Corp".to_string()),
            country: Some("US".to_string()),
            validity_days: Some(3650),
            key_size: Some(4096),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"common_name\":\"RustMQ Root CA\""));
        assert!(serialized.contains("\"organization\":\"RustMQ Corp\""));
        assert!(serialized.contains("\"country\":\"US\""));
        assert!(serialized.contains("\"validity_days\":3650"));
        assert!(serialized.contains("\"key_size\":4096"));
    }

    #[test]
    fn test_issue_certificate_request() {
        let request = IssueCertificateRequest {
            ca_id: "root_ca_1".to_string(),
            common_name: "broker-01.rustmq.com".to_string(),
            subject_alt_names: Some(vec!["broker-01".to_string(), "192.168.1.100".to_string()]),
            organization: Some("RustMQ Corp".to_string()),
            role: Some("broker".to_string()),
            validity_days: Some(365),
            key_usage: Some(vec![
                "digital_signature".to_string(),
                "key_encipherment".to_string(),
            ]),
            extended_key_usage: Some(vec!["server_auth".to_string(), "client_auth".to_string()]),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"ca_id\":\"root_ca_1\""));
        assert!(serialized.contains("\"common_name\":\"broker-01.rustmq.com\""));
        assert!(serialized.contains("\"role\":\"broker\""));
        assert!(serialized.contains("\"validity_days\":365"));
    }

    #[test]
    fn test_create_acl_rule_request() {
        let mut conditions = HashMap::new();
        conditions.insert("source_ip".to_string(), "192.168.1.0/24".to_string());

        let request = CreateAclRuleRequest {
            principal: "user@domain.com".to_string(),
            resource_pattern: "topic.users.*".to_string(),
            resource_type: "topic".to_string(),
            operation: "read".to_string(),
            effect: "allow".to_string(),
            conditions: Some(conditions),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"principal\":\"user@domain.com\""));
        assert!(serialized.contains("\"resource_pattern\":\"topic.users.*\""));
        assert!(serialized.contains("\"operation\":\"read\""));
        assert!(serialized.contains("\"effect\":\"allow\""));
        assert!(serialized.contains("\"source_ip\":\"192.168.1.0/24\""));
    }

    #[test]
    fn test_acl_evaluation_request() {
        let mut context = HashMap::new();
        context.insert("client_ip".to_string(), "192.168.1.100".to_string());

        let request = AclEvaluationRequest {
            principal: "user@domain.com".to_string(),
            resource: "topic.users.data".to_string(),
            operation: "read".to_string(),
            context: Some(context),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"principal\":\"user@domain.com\""));
        assert!(serialized.contains("\"resource\":\"topic.users.data\""));
        assert!(serialized.contains("\"operation\":\"read\""));
        assert!(serialized.contains("\"client_ip\":\"192.168.1.100\""));
    }

    #[test]
    fn test_bulk_acl_evaluation_request() {
        let evaluations = vec![
            AclEvaluationRequest {
                principal: "user1@domain.com".to_string(),
                resource: "topic.users.data".to_string(),
                operation: "read".to_string(),
                context: None,
            },
            AclEvaluationRequest {
                principal: "user2@domain.com".to_string(),
                resource: "topic.admin.logs".to_string(),
                operation: "write".to_string(),
                context: None,
            },
        ];

        let request = BulkAclEvaluationRequest { evaluations };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"principal\":\"user1@domain.com\""));
        assert!(serialized.contains("\"principal\":\"user2@domain.com\""));
        assert!(serialized.contains("\"operation\":\"read\""));
        assert!(serialized.contains("\"operation\":\"write\""));
    }

    #[test]
    fn test_certificate_validation_request() {
        let request = CertificateValidationRequest {
            certificate_pem: "-----BEGIN CERTIFICATE-----\nMIIC...\n-----END CERTIFICATE-----"
                .to_string(),
            chain_pem: Some(vec![
                "-----BEGIN CERTIFICATE-----\nMIIC...\n-----END CERTIFICATE-----".to_string(),
            ]),
            check_revocation: Some(true),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"certificate_pem\":\"-----BEGIN CERTIFICATE-----"));
        assert!(serialized.contains("\"check_revocation\":true"));
    }

    #[test]
    fn test_maintenance_request() {
        let mut parameters = HashMap::new();
        parameters.insert("dry_run".to_string(), "true".to_string());
        parameters.insert("max_age_days".to_string(), "30".to_string());

        let request = MaintenanceRequest {
            operation: "cleanup_expired_certificates".to_string(),
            parameters: Some(parameters),
        };

        let serialized = serde_json::to_string(&request).unwrap();
        assert!(serialized.contains("\"operation\":\"cleanup_expired_certificates\""));
        assert!(serialized.contains("\"dry_run\":\"true\""));
        assert!(serialized.contains("\"max_age_days\":\"30\""));
    }

    #[test]
    fn test_format_duration_days() {
        assert_eq!(format_duration_days(-5), "5 days ago");
        assert_eq!(format_duration_days(0), "today");
        assert_eq!(format_duration_days(1), "1 day");
        assert_eq!(format_duration_days(15), "15 days");
        assert_eq!(format_duration_days(60), "2 months");
        assert_eq!(format_duration_days(400), "1 years");
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(2097152), "2.0 MB");
        assert_eq!(format_bytes(3221225472), "3.0 GB");
    }

    #[test]
    fn test_csv_escape_value() {
        let simple_value = json!("simple");
        assert_eq!(csv_escape_value(&simple_value), "simple");

        let comma_value = json!("value,with,commas");
        assert_eq!(csv_escape_value(&comma_value), "\"value,with,commas\"");

        let quote_value = json!("value\"with\"quotes");
        assert_eq!(
            csv_escape_value(&quote_value),
            "\"value\"\"with\"\"quotes\""
        );

        let newline_value = json!("value\nwith\nnewlines");
        assert_eq!(
            csv_escape_value(&newline_value),
            "\"value\nwith\nnewlines\""
        );

        let null_value = json!(null);
        assert_eq!(csv_escape_value(&null_value), "");

        let bool_value = json!(true);
        assert_eq!(csv_escape_value(&bool_value), "true");

        let number_value = json!(42);
        assert_eq!(csv_escape_value(&number_value), "42");
    }

    #[test]
    fn test_url_encoding() {
        use super::api_client::urlencoding;

        assert_eq!(urlencoding::encode("simple"), "simple");
        assert_eq!(urlencoding::encode("hello world"), "hello%20world");
        assert_eq!(urlencoding::encode("user@domain.com"), "user%40domain.com");
        assert_eq!(urlencoding::encode("topic.users.*"), "topic.users.%2A");
        assert_eq!(urlencoding::encode("192.168.1.0/24"), "192.168.1.0%2F24");
    }

    #[test]
    fn test_empty_data_formatting() {
        // Test empty array
        let empty_array = json!([]);
        let table_output = format_output(&empty_array, OutputFormat::Table, true).unwrap();
        assert_eq!(table_output, "No data available");

        let csv_output = format_output(&empty_array, OutputFormat::Csv, true).unwrap();
        assert_eq!(csv_output, "");

        // Test empty object
        let empty_object = json!({});
        let yaml_output = format_output(&empty_object, OutputFormat::Yaml, true).unwrap();
        assert_eq!(yaml_output, "{}");
    }

    #[test]
    fn test_complex_nested_data_formatting() {
        let complex_data = json!({
            "certificate": {
                "id": "cert_123",
                "subject": "CN=broker-01.rustmq.com,O=RustMQ Corp",
                "issuer": "CN=RustMQ Root CA,O=RustMQ Corp",
                "validity": {
                    "not_before": "2024-01-01T00:00:00Z",
                    "not_after": "2025-01-01T00:00:00Z"
                },
                "extensions": {
                    "key_usage": ["digital_signature", "key_encipherment"],
                    "extended_key_usage": ["server_auth", "client_auth"],
                    "subject_alt_names": ["broker-01.rustmq.com", "192.168.1.100"]
                }
            },
            "status": "active",
            "metrics": {
                "usage_count": 1500,
                "last_used": "2024-06-15T10:30:00Z"
            }
        });

        // Test JSON formatting preserves structure
        let json_output = format_output(&complex_data, OutputFormat::Json, true).unwrap();
        assert!(json_output.contains("\"certificate\""));
        assert!(json_output.contains("\"validity\""));
        assert!(json_output.contains("\"extensions\""));

        // Test YAML formatting creates hierarchical structure
        let yaml_output = format_output(&complex_data, OutputFormat::Yaml, true).unwrap();
        assert!(yaml_output.contains("certificate:"));
        assert!(yaml_output.contains("  validity:"));
        assert!(yaml_output.contains("    not_before:"));

        // Test table formatting handles nested objects
        let table_output = format_output(&complex_data, OutputFormat::Table, true).unwrap();
        assert!(table_output.contains("FIELD"));
        assert!(table_output.contains("VALUE"));
        assert!(table_output.contains("certificate"));
        assert!(table_output.contains("{object}"));
    }

    #[test]
    fn test_large_array_table_formatting() {
        let large_data = json!([
            {"id": "1", "name": "item1", "status": "active", "created": "2024-01-01"},
            {"id": "2", "name": "item2", "status": "inactive", "created": "2024-01-02"},
            {"id": "3", "name": "item3", "status": "active", "created": "2024-01-03"},
            {"id": "4", "name": "item4", "status": "pending", "created": "2024-01-04"},
            {"id": "5", "name": "item5", "status": "active", "created": "2024-01-05"}
        ]);

        let table_output = format_output(&large_data, OutputFormat::Table, true).unwrap();

        // Check headers
        assert!(table_output.contains("ID"));
        assert!(table_output.contains("NAME"));
        assert!(table_output.contains("STATUS"));
        assert!(table_output.contains("CREATED"));

        // Check separator line
        assert!(table_output.contains("=+="));

        // Check all data rows
        for i in 1..=5 {
            assert!(table_output.contains(&format!("item{}", i)));
            assert!(table_output.contains(&format!("2024-01-0{}", i)));
        }

        // Check statuses
        assert!(table_output.contains("active"));
        assert!(table_output.contains("inactive"));
        assert!(table_output.contains("pending"));
    }

    #[test]
    fn test_api_response_error_handling() {
        // Test successful response
        let success_response = ApiResponse {
            success: true,
            data: Some(json!({"message": "Operation completed"})),
            error: None,
            leader_hint: None,
        };
        assert!(success_response.success);
        assert!(success_response.error.is_none());

        // Test error response
        let error_response = ApiResponse::<serde_json::Value> {
            success: false,
            data: None,
            error: Some("Resource not found".to_string()),
            leader_hint: Some("try leader-2".to_string()),
        };
        assert!(!error_response.success);
        assert!(error_response.error.is_some());
        assert_eq!(error_response.error.unwrap(), "Resource not found");
    }
}
