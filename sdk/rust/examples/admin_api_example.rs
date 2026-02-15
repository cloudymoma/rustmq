//! RustMQ Admin API Client Example
//!
//! This example demonstrates how to use the RustMQ Admin API client for:
//! - Cluster management (health, brokers, topics)
//! - Security operations (CA management, certificate issuance, ACL management)
//! - Audit log queries
//!
//! Run with: cargo run --example admin_api_example

use rustmq_client::admin::{
    AclEvaluationRequest, AdminClient, AdminConfig, AuditLogRequest, CreateAclRuleRequest,
    CreateTopicRequest, GenerateCaRequest, IssueCertificateRequest,
};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing for logs
    tracing_subscriber::fmt::init();

    println!("=== RustMQ Admin API Client Example ===\n");

    // Create admin client configuration
    let config = AdminConfig::new("http://localhost:8080")
        .with_timeout(Duration::from_secs(30))
        .with_tls_verify(false); // For local testing only

    // Create admin client
    let client = AdminClient::new(config)?;
    println!("✓ Admin client created successfully\n");

    // ==================== Health & Cluster Management ====================
    println!("--- Health & Cluster Management ---");

    // Check cluster health
    match client.health().await {
        Ok(health) => {
            println!("✓ Health check:");
            println!("  Status: {}", health.status);
            println!("  Version: {}", health.version);
            println!("  Uptime: {} seconds", health.uptime_seconds);
            println!("  Is Leader: {}", health.is_leader);
            println!("  Raft Term: {}\n", health.raft_term);
        }
        Err(e) => println!("✗ Health check failed: {}\n", e),
    }

    // Get cluster status
    match client.cluster_status().await {
        Ok(status) => {
            println!("✓ Cluster status:");
            println!("  Brokers: {}", status.brokers.len());
            println!("  Topics: {}", status.topics.len());
            println!("  Leader: {:?}", status.leader);
            println!("  Term: {}", status.term);
            println!("  Healthy: {}\n", status.healthy);

            for broker in &status.brokers {
                println!(
                    "  Broker: {} ({}:{})",
                    broker.id, broker.host, broker.port_quic
                );
                println!("    Online: {}", broker.online);
                println!("    Rack: {}\n", broker.rack_id);
            }
        }
        Err(e) => println!("✗ Cluster status failed: {}\n", e),
    }

    // List brokers
    match client.list_brokers().await {
        Ok(brokers) => {
            println!("✓ Brokers ({})", brokers.len());
            for broker in brokers {
                println!(
                    "  - {} @ {}:{} ({})",
                    broker.id,
                    broker.host,
                    broker.port_quic,
                    if broker.online { "online" } else { "offline" }
                );
            }
            println!();
        }
        Err(e) => println!("✗ List brokers failed: {}\n", e),
    }

    // ==================== Topic Management ====================
    println!("--- Topic Management ---");

    // List topics
    match client.list_topics().await {
        Ok(topics) => {
            println!("✓ Topics ({})", topics.len());
            for topic in &topics {
                println!(
                    "  - {} (partitions: {}, replication: {})",
                    topic.name, topic.partitions, topic.replication_factor
                );
            }
            println!();
        }
        Err(e) => println!("✗ List topics failed: {}\n", e),
    }

    // Create a new topic
    let create_topic_req = CreateTopicRequest {
        name: "admin-test-topic".to_string(),
        partitions: 3,
        replication_factor: 2,
        retention_ms: Some(86400000),    // 24 hours
        segment_bytes: Some(1073741824), // 1 GB
        compression_type: Some("lz4".to_string()),
    };

    match client.create_topic(create_topic_req).await {
        Ok(result) => println!("✓ Topic created: {}\n", result),
        Err(e) => println!("✗ Create topic failed: {}\n", e),
    }

    // Describe topic
    match client.describe_topic("admin-test-topic").await {
        Ok(topic) => {
            println!("✓ Topic details:");
            println!("  Name: {}", topic.name);
            println!("  Partitions: {}", topic.partitions);
            println!("  Replication Factor: {}", topic.replication_factor);
            println!("  Created: {}", topic.created_at);
            println!(
                "  Partition Assignments: {}\n",
                topic.partition_assignments.len()
            );
        }
        Err(e) => println!("✗ Describe topic failed: {}\n", e),
    }

    // ==================== Security - CA Management ====================
    println!("--- CA Management ---");

    // Generate a new CA
    let ca_req = GenerateCaRequest {
        common_name: "RustMQ Admin Example CA".to_string(),
        organization: Some("Example Org".to_string()),
        country: Some("US".to_string()),
        validity_days: Some(365),
        key_size: Some(2048),
    };

    match client.generate_ca(ca_req).await {
        Ok(ca_info) => {
            println!("✓ CA generated:");
            println!("  CA ID: {}", ca_info.ca_id);
            println!("  Common Name: {}", ca_info.common_name);
            println!("  Subject: {}", ca_info.subject);
            println!("  Valid From: {}", ca_info.not_before);
            println!("  Valid Until: {}", ca_info.not_after);
            println!("  Fingerprint: {}\n", ca_info.fingerprint);
        }
        Err(e) => println!("✗ Generate CA failed: {}\n", e),
    }

    // List CAs
    match client.list_cas().await {
        Ok(cas) => {
            println!("✓ Certificate Authorities ({})", cas.len());
            for ca in &cas {
                println!("  - {} ({})", ca.common_name, ca.ca_id);
                println!("    Valid: {} to {}", ca.not_before, ca.not_after);
            }
            println!();
        }
        Err(e) => println!("✗ List CAs failed: {}\n", e),
    }

    // ==================== Security - Certificate Management ====================
    println!("--- Certificate Management ---");

    // Issue a certificate (using first CA if available)
    if let Ok(cas) = client.list_cas().await {
        if let Some(ca) = cas.first() {
            let cert_req = IssueCertificateRequest {
                ca_id: ca.ca_id.clone(),
                common_name: "broker-1.example.com".to_string(),
                subject_alt_names: Some(vec![
                    "broker-1.local".to_string(),
                    "192.168.1.100".to_string(),
                ]),
                organization: Some("Example Org".to_string()),
                role: Some("broker".to_string()),
                validity_days: Some(90),
                key_usage: Some(vec![
                    "digitalSignature".to_string(),
                    "keyEncipherment".to_string(),
                ]),
                extended_key_usage: Some(vec!["serverAuth".to_string(), "clientAuth".to_string()]),
            };

            match client.issue_certificate(cert_req).await {
                Ok(cert) => {
                    println!("✓ Certificate issued:");
                    println!("  Certificate ID: {}", cert.certificate_id);
                    println!("  Common Name: {}", cert.common_name);
                    println!("  Serial: {}", cert.serial_number);
                    println!("  Valid From: {}", cert.not_before);
                    println!("  Valid Until: {}", cert.not_after);
                    println!("  Status: {}", cert.status);
                    println!("  Role: {:?}\n", cert.role);
                }
                Err(e) => println!("✗ Issue certificate failed: {}\n", e),
            }
        }
    }

    // List certificates
    match client.list_certificates(None).await {
        Ok(certs) => {
            println!("✓ Certificates ({})", certs.len());
            for cert in certs.iter().take(5) {
                println!("  - {} ({})", cert.common_name, cert.certificate_id);
                println!("    Status: {}, Expires: {}", cert.status, cert.not_after);
            }
            if certs.len() > 5 {
                println!("  ... and {} more", certs.len() - 5);
            }
            println!();
        }
        Err(e) => println!("✗ List certificates failed: {}\n", e),
    }

    // ==================== Security - ACL Management ====================
    println!("--- ACL Management ---");

    // Create ACL rule
    let acl_req = CreateAclRuleRequest {
        principal: "user@example.com".to_string(),
        resource_pattern: "topic.events.*".to_string(),
        resource_type: "topic".to_string(),
        operation: "read".to_string(),
        effect: "allow".to_string(),
        conditions: Some(HashMap::from([(
            "source_ip".to_string(),
            "192.168.1.0/24".to_string(),
        )])),
    };

    match client.create_acl_rule(acl_req).await {
        Ok(rule) => {
            println!("✓ ACL rule created:");
            println!("  Rule ID: {}", rule.rule_id);
            println!("  Principal: {}", rule.principal);
            println!("  Resource: {}", rule.resource_pattern);
            println!("  Operation: {}", rule.operation);
            println!("  Effect: {}", rule.effect);
            println!("  Created: {}\n", rule.created_at);
        }
        Err(e) => println!("✗ Create ACL rule failed: {}\n", e),
    }

    // List ACL rules
    match client.list_acl_rules(None).await {
        Ok(rules) => {
            println!("✓ ACL Rules ({})", rules.len());
            for rule in rules.iter().take(5) {
                println!(
                    "  - {} {} on {}",
                    rule.principal, rule.operation, rule.resource_pattern
                );
            }
            if rules.len() > 5 {
                println!("  ... and {} more", rules.len() - 5);
            }
            println!();
        }
        Err(e) => println!("✗ List ACL rules failed: {}\n", e),
    }

    // Evaluate ACL permission
    let eval_req = AclEvaluationRequest {
        principal: "user@example.com".to_string(),
        resource: "topic.events.payments".to_string(),
        operation: "read".to_string(),
        context: Some(HashMap::from([(
            "source_ip".to_string(),
            "192.168.1.100".to_string(),
        )])),
    };

    match client.evaluate_acl(eval_req).await {
        Ok(result) => {
            println!("✓ ACL evaluation:");
            println!("  Allowed: {}", result.allowed);
            println!("  Effect: {}", result.effect);
            println!("  Matched Rules: {:?}", result.matched_rules);
            println!("  Evaluation Time: {} ns\n", result.evaluation_time_ns);
        }
        Err(e) => println!("✗ ACL evaluation failed: {}\n", e),
    }

    // ==================== Security - Audit Logs ====================
    println!("--- Audit Logs ---");

    // Query audit logs
    let audit_req = AuditLogRequest {
        start_time: None,
        end_time: None,
        event_type: Some("authentication".to_string()),
        principal: None,
        limit: Some(10),
        offset: Some(0),
    };

    match client.get_audit_logs(audit_req).await {
        Ok(logs) => {
            println!("✓ Audit Logs ({})", logs.total_count);
            for entry in logs.events.iter().take(5) {
                println!(
                    "  [{} {}] {} - {}",
                    entry.timestamp,
                    entry.event_type,
                    entry.principal.as_ref().unwrap_or(&"N/A".to_string()),
                    entry.result
                );
            }
            if logs.events.len() > 5 {
                println!("  ... and {} more", logs.events.len() - 5);
            }
            println!();
        }
        Err(e) => println!("✗ Get audit logs failed: {}\n", e),
    }

    // Get audit trail for specific principal
    match client.get_audit_trail("user@example.com").await {
        Ok(entries) => {
            println!("✓ Audit trail for user@example.com ({})", entries.len());
            for entry in entries.iter().take(3) {
                println!(
                    "  [{} {}] {} on {:?}",
                    entry.timestamp,
                    entry.event_type,
                    entry.operation.as_ref().unwrap_or(&"N/A".to_string()),
                    entry.resource
                );
            }
            if entries.len() > 3 {
                println!("  ... and {} more", entries.len() - 3);
            }
            println!();
        }
        Err(e) => println!("✗ Get audit trail failed: {}\n", e),
    }

    // ==================== Cleanup ====================
    println!("--- Cleanup ---");

    // Delete test topic
    match client.delete_topic("admin-test-topic").await {
        Ok(result) => println!("✓ {}\n", result),
        Err(e) => println!("✗ Delete topic failed: {}\n", e),
    }

    println!("=== Example Complete ===");
    Ok(())
}
