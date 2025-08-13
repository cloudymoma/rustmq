use rustmq_client::{
    client::RustMqClient,
    config::{ClientConfig, ConsumerConfig, StartPosition},
    consumer::{Consumer, ConsumerBuilder},
    error::Result,
};
use std::time::Duration;
use tracing::{info, error, warn};
use tokio::signal;

/// Example demonstrating the enhanced consumer features including:
/// - Memory leak fixes (no more per-message task spawning)
/// - Message caching for effective retry functionality
/// - Circuit breaker for broker resilience
/// - Batch processing for improved performance
/// - Proper task lifecycle management
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::init();

    // Create client configuration
    let client_config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        connect_timeout: Duration::from_secs(10),
        request_timeout: Duration::from_secs(30),
        ..Default::default()
    };

    // Create client
    let client = RustMqClient::new(client_config).await?;

    // Create enhanced consumer configuration
    let consumer_config = ConsumerConfig {
        consumer_id: Some("enhanced-consumer-1".to_string()),
        consumer_group: "enhanced-group".to_string(),
        auto_commit_interval: Duration::from_secs(5),
        enable_auto_commit: true,
        fetch_size: 100,
        fetch_timeout: Duration::from_millis(500),
        start_position: StartPosition::Latest,
        max_retry_attempts: 3,
        dead_letter_queue: Some("failed-messages".to_string()),
    };

    // Build the enhanced consumer
    let consumer = ConsumerBuilder::new()
        .topic("orders")
        .consumer_group("order-processors")
        .config(consumer_config)
        .client(client)
        .build()
        .await?;

    info!("Enhanced consumer created with features:");
    info!("- No memory leaks (fixed task spawning)");
    info!("- Message caching for retry functionality");
    info!("- Circuit breaker for broker resilience");
    info!("- Batch processing for acknowledgments");
    info!("- Proper task lifecycle management");

    // Example 1: Basic message consumption with enhanced error handling
    basic_consumption_example(&consumer).await?;

    // Example 2: Batch processing demonstration
    batch_processing_example(&consumer).await?;

    // Example 3: Circuit breaker demonstration
    circuit_breaker_example(&consumer).await?;

    // Example 4: Advanced partition management
    partition_management_example(&consumer).await?;

    // Example 5: Graceful shutdown demonstration
    graceful_shutdown_example(consumer).await?;

    Ok(())
}

/// Basic consumption with enhanced error handling and retry
async fn basic_consumption_example(consumer: &Consumer) -> Result<()> {
    info!("=== Basic Consumption Example ===");

    // Consume messages with automatic retry and DLQ support
    for i in 0..10 {
        if let Some(consumer_message) = consumer.receive().await? {
            let message = &consumer_message.message;
            
            info!("Received message {}: {} from partition {}", 
                  i, 
                  message.payload_as_string().unwrap_or_default(),
                  message.partition);

            // Simulate processing
            tokio::time::sleep(Duration::from_millis(10)).await;

            // Demonstrate smart acknowledgment
            if i % 7 == 0 {
                // Simulate processing failure - will trigger retry with exponential backoff
                warn!("Simulating processing failure for message {}", i);
                consumer_message.nack().await?;
            } else {
                // Successful processing
                consumer_message.ack().await?;
            }
        }
    }

    Ok(())
}

/// Demonstrate batch processing capabilities
async fn batch_processing_example(consumer: &Consumer) -> Result<()> {
    info!("=== Batch Processing Example ===");

    // The enhanced consumer automatically batches acknowledgments for better performance
    let mut messages = Vec::new();
    
    // Collect a batch of messages
    for _ in 0..5 {
        if let Some(consumer_message) = consumer.receive().await? {
            messages.push(consumer_message);
        }
    }

    // Process batch
    info!("Processing batch of {} messages", messages.len());
    
    // Acknowledge all messages in the batch
    // The consumer will automatically batch these acknowledgments for efficiency
    for message in messages {
        message.ack().await?;
    }

    info!("Batch processed successfully - acknowledgments were batched internally");

    Ok(())
}

/// Demonstrate circuit breaker functionality
async fn circuit_breaker_example(consumer: &Consumer) -> Result<()> {
    info!("=== Circuit Breaker Example ===");

    // The circuit breaker automatically protects against broker failures
    // When broker becomes unavailable, the circuit breaker will:
    // 1. Detect consecutive failures
    // 2. Open the circuit (stop sending requests)
    // 3. Periodically test recovery (half-open state)
    // 4. Close circuit when broker recovers

    info!("Consumer automatically protected by circuit breaker");
    info!("- Failure threshold: 5 consecutive failures");
    info!("- Recovery timeout: 60 seconds");
    info!("- Will skip fetch operations when circuit is open");

    // Continue consuming - circuit breaker works transparently
    for _ in 0..3 {
        if let Some(message) = consumer.receive().await? {
            message.ack().await?;
        }
    }

    Ok(())
}

/// Demonstrate advanced partition management
async fn partition_management_example(consumer: &Consumer) -> Result<()> {
    info!("=== Partition Management Example ===");

    // Get assigned partitions
    let assigned_partitions = consumer.assigned_partitions().await;
    info!("Assigned partitions: {:?}", assigned_partitions);

    // Get partition assignment details
    if let Some(assignment) = consumer.partition_assignment().await {
        info!("Assignment ID: {}", assignment.assignment_id);
        info!("Assigned at: {:?}", assignment.assigned_at);
    }

    // Pause specific partitions
    if !assigned_partitions.is_empty() {
        let partition_to_pause = assigned_partitions[0];
        consumer.pause_partitions(vec![partition_to_pause]).await?;
        info!("Paused partition: {}", partition_to_pause);

        // Check paused partitions
        let paused = consumer.paused_partitions().await;
        info!("Currently paused partitions: {:?}", paused);

        // Resume partition
        tokio::time::sleep(Duration::from_secs(1)).await;
        consumer.resume_partitions(vec![partition_to_pause]).await?;
        info!("Resumed partition: {}", partition_to_pause);
    }

    // Get consumer lag
    match consumer.get_lag().await {
        Ok(lag_map) => {
            info!("Consumer lag by partition:");
            for (partition, lag) in lag_map {
                info!("  Partition {}: {} messages behind", partition, lag);
            }
        }
        Err(e) => warn!("Could not get lag: {}", e),
    }

    Ok(())
}

/// Demonstrate graceful shutdown with proper resource cleanup
async fn graceful_shutdown_example(consumer: Consumer) -> Result<()> {
    info!("=== Graceful Shutdown Example ===");

    // Set up signal handling for graceful shutdown
    tokio::spawn(async move {
        // Wait for shutdown signal
        signal::ctrl_c().await.expect("Failed to listen for ctrl+c");
        info!("Received shutdown signal");
    });

    // The enhanced consumer provides proper task lifecycle management
    info!("Enhanced consumer features for shutdown:");
    info!("- Automatic cancellation of background tasks");
    info!("- Flush pending acknowledgment batches");
    info!("- Final offset commit before closing");
    info!("- Wait for task completion");
    info!("- Proper resource cleanup");

    // Simulate some work before shutdown
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Close consumer with enhanced cleanup
    info!("Closing consumer with enhanced cleanup...");
    consumer.close().await?;
    info!("Consumer closed successfully - all resources cleaned up");

    Ok(())
}

/// Utility function to demonstrate consumer metrics
#[allow(dead_code)]
async fn print_consumer_metrics(consumer: &Consumer) {
    let metrics = consumer.metrics().await;
    
    info!("Consumer Metrics:");
    info!("  Messages received: {}", metrics.messages_received.load(std::sync::atomic::Ordering::Relaxed));
    info!("  Messages processed: {}", metrics.messages_processed.load(std::sync::atomic::Ordering::Relaxed));
    info!("  Messages failed: {}", metrics.messages_failed.load(std::sync::atomic::Ordering::Relaxed));
    info!("  Bytes received: {}", metrics.bytes_received.load(std::sync::atomic::Ordering::Relaxed));
    
    let lag = metrics.lag.read().await;
    info!("  Current lag: {} ms", *lag);
    
    let last_receive = metrics.last_receive_time.read().await;
    if let Some(time) = *last_receive {
        let since_last = std::time::Instant::now().duration_since(time);
        info!("  Time since last receive: {:?}", since_last);
    }
    
    let processing_time = metrics.processing_time_ms.read().await;
    info!("  Average processing time: {:.2} ms", *processing_time);
}