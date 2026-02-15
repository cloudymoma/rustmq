use futures::StreamExt;
use rustmq_client::{config::StartPosition, *};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create client configuration
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("advanced-consumer-example".to_string()),
        ..Default::default()
    };

    // Create client
    let client = RustMqClient::new(config).await?;
    println!("Connected to RustMQ cluster");

    // Configure advanced consumer
    let consumer_config = ConsumerConfig {
        consumer_id: Some("advanced-consumer-1".to_string()),
        consumer_group: "advanced-processing-group".to_string(),

        // Disable auto-commit for manual control
        enable_auto_commit: false,
        auto_commit_interval: Duration::from_secs(10),

        // Optimize fetch settings
        fetch_size: 100,
        fetch_timeout: Duration::from_millis(500),

        // Start from earliest to process all messages
        start_position: StartPosition::Earliest,

        // Configure retry and DLQ
        max_retry_attempts: 3,
        dead_letter_queue: Some("failed-messages".to_string()),
    };

    // Create consumer
    let consumer = ConsumerBuilder::new()
        .topic("advanced-topic")
        .consumer_group("advanced-processing-group")
        .config(consumer_config)
        .client(client.clone())
        .build()
        .await?;

    println!("Created advanced consumer for topic: advanced-topic");

    // Start metrics monitoring
    let metrics_consumer = consumer.clone();
    let metrics_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));

        loop {
            interval.tick().await;

            let metrics = metrics_consumer.metrics().await;
            let lag = metrics_consumer.get_lag().await.unwrap_or_default();

            println!("=== Consumer Metrics ===");
            println!(
                "Messages received: {}",
                metrics.messages_received.load(Ordering::Relaxed)
            );
            println!(
                "Messages processed: {}",
                metrics.messages_processed.load(Ordering::Relaxed)
            );
            println!(
                "Messages failed: {}",
                metrics.messages_failed.load(Ordering::Relaxed)
            );
            println!(
                "Bytes received: {}",
                metrics.bytes_received.load(Ordering::Relaxed)
            );

            if let Some(last_receive) = *metrics.last_receive_time.read().await {
                let age = last_receive.elapsed();
                println!("Last message received: {:?} ago", age);
            }

            println!("Consumer lag by partition: {:?}", lag);
            println!("========================");
        }
    });

    // Demonstrate different consumption patterns
    println!("Starting advanced consumption patterns...");

    // Pattern 1: Basic consumption with manual ack/nack
    println!("\n--- Pattern 1: Basic consumption with error handling ---");
    consume_with_error_handling(&consumer).await?;

    // Pattern 2: Seeking demonstration
    println!("\n--- Pattern 2: Seeking to specific offset ---");
    demonstrate_seeking(&consumer).await?;

    // Pattern 3: Streaming interface
    println!("\n--- Pattern 3: Streaming interface ---");
    demonstrate_streaming(&consumer).await?;

    // Pattern 4: Manual offset management
    println!("\n--- Pattern 4: Manual offset management ---");
    demonstrate_manual_commits(&consumer).await?;

    // Pattern 5: Partition management
    println!("\n--- Pattern 5: Partition management ---");
    demonstrate_partition_management(&consumer).await?;

    // Clean shutdown
    metrics_handle.abort();
    consumer.close().await?;
    client.close().await?;
    println!("Advanced consumer example completed");

    Ok(())
}

/// Demonstrate basic consumption with error handling
async fn consume_with_error_handling(consumer: &Consumer) -> Result<()> {
    let mut processed_count = 0;
    let max_messages = 10;

    while processed_count < max_messages {
        if let Some(consumer_message) = consumer.receive().await? {
            let message = &consumer_message.message;

            println!(
                "Received message {} from partition {}",
                message.id, message.partition
            );
            println!(
                "  Payload: {}",
                message.payload_as_string().unwrap_or_default()
            );
            println!("  Headers: {:?}", message.headers);
            println!("  Age: {}ms", message.age_ms());

            // Simulate processing with potential failure
            match simulate_message_processing(message).await {
                Ok(_) => {
                    println!("  ✅ Processing successful");
                    consumer_message.ack().await?;
                }
                Err(ProcessingError::Retryable) => {
                    println!("  ⚠️ Processing failed (retryable) - will retry with backoff");
                    consumer_message.nack().await?;
                }
                Err(ProcessingError::Fatal) => {
                    println!(
                        "  ❌ Fatal processing error - acknowledging to prevent infinite retry"
                    );
                    consumer_message.ack().await?;
                }
            }

            processed_count += 1;
        } else {
            println!("No messages available, waiting...");
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    Ok(())
}

/// Demonstrate seeking capabilities
async fn demonstrate_seeking(consumer: &Consumer) -> Result<()> {
    let assigned_partitions = consumer.assigned_partitions().await;
    println!("Current assigned partitions: {:?}", assigned_partitions);

    if assigned_partitions.is_empty() {
        println!("No partitions assigned, skipping seek demonstration");
        return Ok(());
    }

    // Demonstrate single partition seeking
    let first_partition = assigned_partitions[0];
    println!("Seeking partition {} to offset 5...", first_partition);
    consumer.seek(first_partition, 5).await?;

    // Seek all partitions to a specific offset
    println!("Seeking all partitions to offset 10...");
    consumer.seek_all(10).await?;

    // Seek to timestamp (1 hour ago) on a specific partition
    let one_hour_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 3600;

    println!(
        "Seeking partition {} to timestamp: {} (1 hour ago)",
        first_partition, one_hour_ago
    );
    consumer
        .seek_to_timestamp(first_partition, one_hour_ago)
        .await?;

    // Seek all partitions to timestamp
    println!(
        "Seeking all partitions to timestamp: {} (1 hour ago)",
        one_hour_ago
    );
    match consumer.seek_all_to_timestamp(one_hour_ago).await {
        Ok(partition_offsets) => {
            println!(
                "Successfully seeked all partitions to timestamp with offsets: {:?}",
                partition_offsets
            );
        }
        Err(e) => println!("Could not seek all partitions to timestamp: {}", e),
    }

    // Get committed offsets for each partition
    for &partition in &assigned_partitions {
        match consumer.committed_offset(partition).await {
            Ok(offset) => println!(
                "Current committed offset for partition {}: {}",
                partition, offset
            ),
            Err(e) => println!(
                "Could not get committed offset for partition {}: {}",
                partition, e
            ),
        }
    }

    Ok(())
}

/// Demonstrate streaming interface
async fn demonstrate_streaming(consumer: &Consumer) -> Result<()> {
    let mut stream = consumer.stream();
    let mut count = 0;
    let max_stream_messages = 5;

    println!("Using streaming interface...");

    while let Some(result) = stream.next().await {
        if count >= max_stream_messages {
            break;
        }

        match result {
            Ok(consumer_message) => {
                let message = &consumer_message.message;
                println!(
                    "Stream received: {} ({})",
                    message.id,
                    message.payload_as_string().unwrap_or_default()
                );

                // Always ack in streaming mode for this example
                consumer_message.ack().await?;
                count += 1;
            }
            Err(e) => {
                println!("Stream error: {}", e);
                break;
            }
        }
    }

    Ok(())
}

/// Demonstrate manual offset management
async fn demonstrate_manual_commits(consumer: &Consumer) -> Result<()> {
    let mut processed_count = 0;
    let commit_interval = 3; // Commit every 3 messages

    println!(
        "Processing with manual commits every {} messages",
        commit_interval
    );

    while processed_count < 10 {
        if let Some(consumer_message) = consumer.receive().await? {
            let message = &consumer_message.message;

            println!("Manual processing message: {}", message.id);

            // Simulate processing
            tokio::time::sleep(Duration::from_millis(50)).await;
            consumer_message.ack().await?;

            processed_count += 1;

            // Manual commit at intervals
            if processed_count % commit_interval == 0 {
                println!("  📝 Manual commit at message {}", processed_count);
                consumer.commit().await?;
            }
        } else {
            break;
        }
    }

    // Final commit
    println!("  📝 Final manual commit");
    consumer.commit().await?;

    Ok(())
}

/// Demonstrate partition management capabilities
async fn demonstrate_partition_management(consumer: &Consumer) -> Result<()> {
    let assigned_partitions = consumer.assigned_partitions().await;
    println!("Currently assigned partitions: {:?}", assigned_partitions);

    if assigned_partitions.is_empty() {
        println!("No partitions assigned, skipping partition management demonstration");
        return Ok(());
    }

    // Get partition assignment details
    if let Some(assignment) = consumer.partition_assignment().await {
        println!("Partition assignment details:");
        println!("  Assignment ID: {}", assignment.assignment_id);
        println!("  Assigned at: {:?}", assignment.assigned_at);
        println!("  Partitions: {:?}", assignment.partitions);
    }

    // Demonstrate pausing and resuming partitions
    if assigned_partitions.len() > 1 {
        let partitions_to_pause = vec![assigned_partitions[0]];
        println!("Pausing partitions: {:?}", partitions_to_pause);
        consumer
            .pause_partitions(partitions_to_pause.clone())
            .await?;

        let paused = consumer.paused_partitions().await;
        println!("Currently paused partitions: {:?}", paused);

        // Simulate some processing time
        tokio::time::sleep(Duration::from_millis(100)).await;

        println!("Resuming partitions: {:?}", partitions_to_pause);
        consumer.resume_partitions(partitions_to_pause).await?;

        let paused_after_resume = consumer.paused_partitions().await;
        println!("Paused partitions after resume: {:?}", paused_after_resume);
    }

    // Display current consumer lag by partition
    match consumer.get_lag().await {
        Ok(lag_map) => {
            println!("Consumer lag by partition:");
            for (partition, lag) in lag_map {
                println!("  Partition {}: {} messages behind", partition, lag);
            }
        }
        Err(e) => println!("Could not get consumer lag: {}", e),
    }

    Ok(())
}

/// Simulate message processing with different outcomes
async fn simulate_message_processing(
    message: &Message,
) -> std::result::Result<(), ProcessingError> {
    // Simulate processing time
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Simulate different outcomes based on message content
    let payload = message.payload_as_string().unwrap_or_default();

    if payload.contains("error") {
        Err(ProcessingError::Fatal)
    } else if payload.contains("retry") {
        Err(ProcessingError::Retryable)
    } else {
        Ok(())
    }
}

/// Processing error types
#[derive(Debug)]
enum ProcessingError {
    Retryable,
    Fatal,
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingError::Retryable => write!(f, "Retryable processing error"),
            ProcessingError::Fatal => write!(f, "Fatal processing error"),
        }
    }
}

impl std::error::Error for ProcessingError {}
