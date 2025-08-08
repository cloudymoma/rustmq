//! Token Authentication Example
//!
//! This example demonstrates how to use JWT token-based authentication
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

    // Create TLS configuration for server authentication only
    let tls_config = TlsConfig::secure_config(
        // CA certificate (replace with your CA certificate)
        include_str!("../../../certs/ca.pem").to_string(),
        
        // No client certificate for token auth
        None,
        None,
        
        // Server name for SNI
        "localhost".to_string(),
    );

    // Create authentication configuration using JWT token
    let auth_config = AuthConfig::token_config(
        // JWT token (replace with your actual token)
        "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0b2tlbi11c2VyIiwiaWF0IjoxNjQwOTk1MjAwLCJleHAiOjE2NzI1MzEyMDAsInJvbGVzIjpbInByb2R1Y2VyIl0sInRvcGljcyI6WyJ0b2tlbi10b3BpYyJdfQ.example".to_string()
    );

    // Create client configuration with token authentication
    let client_config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("token-auth-example".to_string()),
        tls_config: Some(tls_config),
        auth: Some(auth_config),
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        ..Default::default()
    };

    println!("Creating RustMQ client with JWT token authentication...");
    
    // Create client
    let client = RustMqClient::new(client_config).await?;
    
    // Verify authentication
    if let Some(connection) = client.get_connection().await {
        if connection.is_security_enabled() {
            println!("✓ Security is enabled");
            
            if let Some(security_context) = connection.get_security_context().await {
                println!("✓ Authenticated as principal: {}", security_context.principal);
                
                // Show token-based permissions
                let permissions = &security_context.permissions;
                if !permissions.read_topics.is_empty() {
                    println!("✓ Read access to topics: {:?}", permissions.read_topics);
                }
                if !permissions.write_topics.is_empty() {
                    println!("✓ Write access to topics: {:?}", permissions.write_topics);
                }
                if !permissions.admin_operations.is_empty() {
                    println!("✓ Admin access to operations: {:?}", permissions.admin_operations);
                }
            }
        } else {
            println!("⚠ Security is not enabled");
        }
    }

    // Create producer configuration
    let producer_config = ProducerConfig {
        producer_id: Some("token-producer".to_string()),
        batch_size: 5,
        batch_timeout: Duration::from_millis(100),
        ..Default::default()
    };

    println!("Creating producer with token authentication...");
    
    // Create producer
    let producer = client.create_producer("token-topic", producer_config).await?;

    println!("Sending messages with token authentication...");
    
    // Send messages
    for i in 0..5 {
        let message = MessageBuilder::new()
            .key(format!("token-key-{}", i))
            .value(format!("Message {} sent with JWT token authentication", i))
            .header("auth-method", "jwt-token")
            .header("timestamp", &chrono::Utc::now().to_rfc3339())
            .build();

        match producer.send(message).await {
            Ok(metadata) => {
                println!("✓ Message {} sent successfully - Partition: {}, Offset: {}", 
                    i, metadata.partition, metadata.offset);
            }
            Err(e) => {
                eprintln!("✗ Failed to send message {}: {}", i, e);
                
                // Check for token-related errors
                match &e {
                    rustmq_client::ClientError::AuthorizationDenied(msg) => {
                        eprintln!("Token authorization failed: {}", msg);
                        break;
                    }
                    rustmq_client::ClientError::Authentication(msg) => {
                        eprintln!("Token authentication failed: {}", msg);
                        break;
                    }
                    _ => {
                        // Continue for other errors
                    }
                }
            }
        }
    }

    // Flush any remaining messages
    producer.flush().await?;
    
    println!("All messages sent successfully with token authentication!");

    // Demonstrate token refresh (if supported by the server)
    if let Some(connection) = client.get_connection().await {
        println!("Attempting to refresh authentication token...");
        match connection.refresh_security_context().await {
            Ok(_) => println!("✓ Authentication token refreshed"),
            Err(e) => println!("✗ Token refresh failed: {}", e),
        }
    }

    Ok(())
}

#[cfg(not(feature = "integration-tests"))]
fn main() {
    println!("This example requires a valid JWT token and a running RustMQ broker with token authentication enabled.");
    println!("To run this example:");
    println!("1. Configure RustMQ broker with JWT token validation");
    println!("2. Generate a valid JWT token with appropriate claims");
    println!("3. Update the token in this example");
    println!("4. Run with: cargo run --example token_authentication --features integration-tests");
    
    println!("\nExample JWT token payload:");
    println!("{{");
    println!("  \"sub\": \"user-id\",");
    println!("  \"iat\": 1640995200,");
    println!("  \"exp\": 1672531200,");
    println!("  \"roles\": [\"producer\", \"consumer\"],");
    println!("  \"topics\": [\"token-topic\", \"logs.*\"]");
    println!("}}");
}