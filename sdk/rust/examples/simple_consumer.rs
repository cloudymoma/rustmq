use rustmq_client::*;
use tokio;
use tracing_subscriber;
use tokio::signal;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create client configuration
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("simple-consumer".to_string()),
        ..Default::default()
    };

    // Create client
    let client = RustMqClient::new(config).await?;
    println!("Connected to RustMQ cluster");

    // Create consumer
    let consumer = client.create_consumer("example-topic", "example-group").await?;
    println!("Created consumer for topic: example-topic, group: example-group");

    // Handle Ctrl+C gracefully
    let consumer_clone = consumer.clone();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to install Ctrl+C handler");
        println!("\nReceived Ctrl+C, shutting down...");
        consumer_clone.close().await.ok();
    });

    // Consume messages
    let mut message_count = 0;
    loop {
        match consumer.receive().await {
            Ok(Some(consumer_message)) => {
                message_count += 1;
                
                let message = &consumer_message.message;
                println!("Received message #{}: id={}, offset={}, partition={}", 
                    message_count, message.id, message.offset, message.partition);
                
                if let Ok(payload) = message.payload_as_string() {
                    println!("  Payload: {}", payload);
                }
                
                // Print headers
                for (key, value) in &message.headers {
                    println!("  Header {}: {}", key, value);
                }
                
                // Acknowledge the message
                if let Err(e) = consumer_message.ack().await {
                    eprintln!("Failed to acknowledge message: {}", e);
                }
                
                println!("  Message acknowledged");
            }
            Ok(None) => {
                println!("Consumer closed");
                break;
            }
            Err(e) => {
                eprintln!("Error receiving message: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
            }
        }
    }

    // Close client
    client.close().await?;
    println!("Consumer and client closed. Total messages processed: {}", message_count);

    Ok(())
}