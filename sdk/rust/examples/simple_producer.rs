use rustmq_client::*;
use tokio;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create client configuration
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("simple-producer".to_string()),
        ..Default::default()
    };

    // Create client
    let client = RustMqClient::new(config).await?;
    println!("Connected to RustMQ cluster");

    // Create producer
    let producer = client.create_producer("example-topic").await?;
    println!("Created producer for topic: example-topic");

    // Send some messages
    for i in 0..10 {
        let message = Message::builder()
            .topic("example-topic")
            .payload(format!("Hello, World! Message #{}", i))
            .header("message-id", &format!("msg-{}", i))
            .header("timestamp", &chrono::Utc::now().to_rfc3339())
            .build()?;

        match producer.send(message).await {
            Ok(result) => {
                println!("Message {} sent successfully: offset={}, partition={}", 
                    i, result.offset, result.partition);
            }
            Err(e) => {
                eprintln!("Failed to send message {}: {}", i, e);
            }
        }

        // Small delay between messages
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Flush remaining messages
    producer.flush().await?;
    println!("All messages flushed");

    // Close producer and client
    producer.close().await?;
    client.close().await?;
    println!("Producer and client closed");

    Ok(())
}