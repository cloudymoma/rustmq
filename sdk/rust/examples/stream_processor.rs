use async_trait::async_trait;
use rustmq_client::{
    stream::{ErrorStrategy, MessageProcessor, StreamMode},
    *,
};
use tokio;
use tracing_subscriber;

// Custom message processor that transforms messages
struct UppercaseProcessor;

#[async_trait]
impl MessageProcessor for UppercaseProcessor {
    async fn process(&self, message: &Message) -> Result<Option<Message>> {
        // Transform payload to uppercase
        let uppercase_payload = match message.payload_as_string() {
            Ok(text) => text.to_uppercase(),
            Err(_) => return Ok(None), // Skip non-UTF8 messages
        };

        // Create transformed message
        let transformed = Message::builder()
            .topic("processed-topic")
            .payload(uppercase_payload)
            .header("processor", "uppercase")
            .header("original-topic", &message.topic)
            .header(
                "processed-at",
                &std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
                    .to_string(),
            )
            .build()
            .map_err(|e| ClientError::Stream(e))?;

        Ok(Some(transformed))
    }

    async fn on_start(&self) -> Result<()> {
        println!("UppercaseProcessor started");
        Ok(())
    }

    async fn on_stop(&self) -> Result<()> {
        println!("UppercaseProcessor stopped");
        Ok(())
    }

    async fn on_error(&self, message: &Message, error: &ClientError) -> Result<()> {
        eprintln!("Processing error for message {}: {}", message.id, error);
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create client configuration
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("stream-processor".to_string()),
        ..Default::default()
    };

    // Create client
    let client = RustMqClient::new(config).await?;
    println!("Connected to RustMQ cluster");

    // Configure stream processing
    let stream_config = StreamConfig {
        input_topics: vec!["input-topic".to_string()],
        output_topic: Some("output-topic".to_string()),
        consumer_group: "stream-processors".to_string(),
        parallelism: 4,
        processing_timeout: std::time::Duration::from_secs(30),
        exactly_once: false,
        max_in_flight: 100,
        error_strategy: ErrorStrategy::Retry {
            max_attempts: 3,
            backoff_ms: 1000,
        },
        mode: StreamMode::Individual,
    };

    // Create message stream with custom processor
    let stream = client
        .create_stream(stream_config)
        .await?
        .with_processor(UppercaseProcessor);

    println!("Created stream processor: input-topic -> output-topic");

    // Start processing
    stream.start().await?;
    println!("Stream processing started");

    // Note: Metrics monitoring would require MessageStream to implement Clone
    // For this example, we'll proceed without metrics monitoring
    println!("Stream processing active - metrics monitoring would go here");

    // Wait for Ctrl+C
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    println!("\nReceived Ctrl+C, shutting down stream processor...");

    // Stop processing
    stream.stop().await?;
    println!("Stream processing stopped");

    // Close client
    client.close().await?;
    println!("Client closed");

    Ok(())
}
