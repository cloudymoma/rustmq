//! Secure Producer Example with mTLS Authentication
//!
//! This example demonstrates how to create a secure producer using mTLS authentication
//! with the RustMQ Rust SDK.

use rustmq_client::{
    ClientConfig, TlsConfig, AuthConfig, ProducerConfig, 
    RustMqClient, Message, MessageBuilder,
    Result,
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
        Some(include_str!("../../../certs/client.pem").to_string()),
        
        // Client private key (replace with your client private key)
        Some(include_str!("../../../certs/client.key").to_string()),
        
        // Server name for SNI
        "localhost".to_string(),
    );

    // Create authentication configuration using mTLS
    let auth_config = AuthConfig::mtls_config(
        include_str!("../../../certs/ca.pem").to_string(),
        include_str!("../../../certs/client.pem").to_string(),
        include_str!("../../../certs/client.key").to_string(),
        Some("localhost".to_string()),
    );

    // Create client configuration with security enabled
    let client_config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("secure-producer-example".to_string()),
        tls_config: Some(tls_config),
        auth: Some(auth_config),
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        ..Default::default()
    };

    println!("Creating secure RustMQ client with mTLS authentication...");
    
    // Create client
    let client = RustMqClient::new(client_config).await?;
    
    // Verify security is enabled
    if let Some(connection) = client.get_connection().await {
        if connection.is_security_enabled() {
            println!("✓ Security is enabled");
            
            if let Some(security_context) = connection.get_security_context().await {
                println!("✓ Authenticated as principal: {}", security_context.principal);
                
                if let Some(cert_info) = &security_context.certificate_info {
                    println!("✓ Certificate subject: {}", cert_info.subject);
                    println!("✓ Certificate issuer: {}", cert_info.issuer);
                    println!("✓ Certificate serial: {}", cert_info.serial_number);
                }
            }
        } else {
            println!("⚠ Security is not enabled");
        }
    }

    // Create producer configuration
    let producer_config = ProducerConfig {
        producer_id: Some("secure-producer".to_string()),
        batch_size: 10,
        batch_timeout: Duration::from_millis(100),
        ..Default::default()
    };

    println!("Creating secure producer...");
    
    // Create producer
    let producer = client.create_producer("secure-topic", producer_config).await?;

    println!("Sending secure messages...");
    
    // Send messages with security context
    for i in 0..10 {
        let message = MessageBuilder::new()
            .key(format!("key-{}", i))
            .value(format!("Secure message {} from mTLS authenticated client", i))
            .header("source", "secure-producer-example")
            .header("timestamp", &chrono::Utc::now().to_rfc3339())
            .build();

        match producer.send(message).await {
            Ok(metadata) => {
                println!("✓ Message {} sent successfully - Partition: {}, Offset: {}", 
                    i, metadata.partition, metadata.offset);
            }
            Err(e) => {
                eprintln!("✗ Failed to send message {}: {}", i, e);
            }
        }
    }

    // Flush any remaining messages
    producer.flush().await?;
    
    println!("All messages sent successfully!");
    
    // Demonstrate security context refresh
    if let Some(connection) = client.get_connection().await {
        println!("Refreshing security context...");
        connection.refresh_security_context().await?;
        println!("✓ Security context refreshed");
    }

    Ok(())
}

#[cfg(not(feature = "integration-tests"))]
fn main() {
    println!("This example requires certificate files and a running RustMQ broker with mTLS enabled.");
    println!("To run this example:");
    println!("1. Generate certificates (see docs/security/certificates.md)");
    println!("2. Start RustMQ broker with mTLS configuration");
    println!("3. Update certificate paths in this example");
    println!("4. Run with: cargo run --example secure_producer --features integration-tests");
}