//! Secure Consumer Example with mTLS Authentication
//!
//! This example demonstrates how to create a secure consumer using mTLS authentication
//! with the RustMQ Rust SDK, including ACL-based topic authorization.

use rustmq_client::{
    ClientConfig, TlsConfig, AuthConfig, ConsumerConfig, 
    RustMqClient,
    Result,
    config::StartPosition,
};
use std::time::Duration;
use tokio;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    // Create TLS configuration for mTLS
    let tls_config = TlsConfig::secure_config(
        // CA certificate (replace with your CA certificate)
        include_str!("../../../certs/ca.pem").to_string(),
        
        // Client certificate (replace with your client certificate)
        Some(include_str!("../../../certs/consumer.pem").to_string()),
        
        // Client private key (replace with your client private key)
        Some(include_str!("../../../certs/consumer.key").to_string()),
        
        // Server name for SNI
        "localhost".to_string(),
    );

    // Create authentication configuration using mTLS
    let auth_config = AuthConfig::mtls_config(
        include_str!("../../../certs/ca.pem").to_string(),
        include_str!("../../../certs/consumer.pem").to_string(),
        include_str!("../../../certs/consumer.key").to_string(),
        Some("localhost".to_string()),
    );

    // Create client configuration with security enabled
    let client_config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("secure-consumer-example".to_string()),
        tls_config: Some(tls_config),
        auth: Some(auth_config),
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        ..Default::default()
    };

    println!("Creating secure RustMQ client with mTLS authentication...");
    
    // Create client
    let client = RustMqClient::new(client_config).await?;
    
    // Verify security is enabled and check permissions
    if let Some(connection) = client.get_connection().await {
        if connection.is_security_enabled() {
            println!("✓ Security is enabled");
            
            if let Some(security_context) = connection.get_security_context().await {
                println!("✓ Authenticated as principal: {}", security_context.principal);
                
                if let Some(cert_info) = &security_context.certificate_info {
                    println!("✓ Certificate subject: {}", cert_info.subject);
                    println!("✓ Certificate issuer: {}", cert_info.issuer);
                    
                    // Show certificate attributes
                    for (key, value) in &cert_info.attributes {
                        println!("  Certificate {}: {}", key, value);
                    }
                }
                
                // Check permissions for the topic we want to consume
                let topic = "secure-topic";
                if security_context.permissions.can_read_topic(topic) {
                    println!("✓ Permission granted to read from topic: {}", topic);
                } else {
                    println!("✗ Permission denied to read from topic: {}", topic);
                    println!("Available read topics: {:?}", security_context.permissions.read_topics);
                }
            }
        } else {
            println!("⚠ Security is not enabled");
        }
    }

    // Create consumer configuration
    let consumer_config = ConsumerConfig {
        consumer_id: Some("secure-consumer".to_string()),
        consumer_group: "secure-consumer-group".to_string(),
        start_position: StartPosition::Earliest,
        fetch_timeout: Duration::from_secs(1),
        enable_auto_commit: true,
        auto_commit_interval: Duration::from_secs(5),
        ..Default::default()
    };

    println!("Creating secure consumer...");
    
    // Create consumer
    let consumer = client.create_consumer("secure-topic", &consumer_config.consumer_group).await?;

    println!("Starting secure message consumption...");
    println!("Press Ctrl+C to stop consuming messages");
    
    // Consume messages with security context
    let mut message_count = 0;
    
    loop {
        match consumer.receive().await {
            Ok(Some(message)) => {
                message_count += 1;
                
                println!("✓ Received secure message {} from authenticated source:", message_count);
                println!("  Key: {:?}", message.message.key);
                println!("  Value: {}", String::from_utf8_lossy(&message.message.payload));
                println!("  Partition: {}", message.message.partition);
                println!("  Offset: {}", message.message.offset);
                
                // Show message headers
                for (key, value) in &message.message.headers {
                    println!("  Header {}: {}", key, value);
                }
                
                // Acknowledge message processing
                message.ack().await?;
                
                // Demonstrate periodic security context refresh
                if message_count % 10 == 0 {
                    if let Some(connection) = client.get_connection().await {
                        println!("Refreshing security context...");
                        connection.refresh_security_context().await?;
                        println!("✓ Security context refreshed");
                    }
                }
            }
            Ok(None) => {
                // No message available, continue polling
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
                
                // Check if it's a security-related error
                match &e {
                    rustmq_client::ClientError::AuthorizationDenied(msg) => {
                        eprintln!("Authorization denied: {}", msg);
                        break;
                    }
                    rustmq_client::ClientError::InvalidCertificate { reason } => {
                        eprintln!("Certificate error: {}", reason);
                        break;
                    }
                    _ => {
                        // Continue for other types of errors
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                }
            }
        }
        
        // Add a small delay to avoid overwhelming output
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    
    println!("Consumed {} messages securely", message_count);

    Ok(())
}

