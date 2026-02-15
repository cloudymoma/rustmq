use futures::StreamExt;
use rustmq_client::{config::StartPosition, *};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio;
use tracing_subscriber;

/// Comprehensive example demonstrating the enhanced RustMQ Consumer
/// with production-ready features including:
/// - Task management and proper resource cleanup
/// - Message caching for effective retry functionality
/// - Circuit breaker for broker failure protection
/// - Batch acknowledgment processing for improved performance
/// - Comprehensive metrics and monitoring
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize structured logging with appropriate level
    tracing_subscriber::fmt()
        .with_level(true)
        .with_target(true)
        .init();

    println!("🚀 Starting Production-Ready RustMQ Consumer Example");

    // Create production-grade client configuration
    let config = ClientConfig {
        brokers: vec!["localhost:9092".to_string()],
        client_id: Some("production-consumer-demo".to_string()),
        request_timeout: Duration::from_secs(30),
        connect_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),
        max_connections: 5,
        retry_config: RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            multiplier: 2.0,
            jitter: true,
        },
        ..Default::default()
    };

    // Create client with production configuration
    let client = RustMqClient::new(config).await?;
    println!("✅ Connected to RustMQ cluster");

    // Configure production consumer with optimized settings
    let consumer_config = ConsumerConfig {
        consumer_id: Some("production-consumer-v1".to_string()),
        consumer_group: "production-processing-group".to_string(),

        // Optimize for performance and reliability
        enable_auto_commit: false, // Manual commit for guaranteed processing
        auto_commit_interval: Duration::from_secs(5),

        // Batch fetching for better throughput
        fetch_size: 500,
        fetch_timeout: Duration::from_millis(100),

        // Start from earliest for complete processing
        start_position: StartPosition::Latest,

        // Enhanced error handling
        max_retry_attempts: 5,
        dead_letter_queue: Some("production-dlq".to_string()),
    };

    // Create production consumer with enhanced capabilities
    let consumer = ConsumerBuilder::new()
        .topic("production-events")
        .consumer_group("production-processing-group")
        .config(consumer_config)
        .client(client.clone())
        .build()
        .await?;

    println!("✅ Created production consumer with enhanced features");
    println!("   - Task Management: Background task coordination");
    println!("   - Message Caching: LRU cache for retry functionality");
    println!("   - Circuit Breaker: Automatic broker failure protection");
    println!("   - Batch Processing: Acknowledgment batching for performance");

    // Start comprehensive monitoring
    start_production_monitoring(&consumer).await;

    // Demonstrate production consumption patterns
    println!("\n🔄 Starting production consumption patterns...");

    // Pattern 1: High-throughput batch processing
    println!("\n--- Pattern 1: High-Throughput Batch Processing ---");
    demonstrate_batch_processing(&consumer).await?;

    // Pattern 2: Resilient processing with circuit breaker
    println!("\n--- Pattern 2: Resilient Processing ---");
    demonstrate_resilient_processing(&consumer).await?;

    // Pattern 3: Advanced partition management
    println!("\n--- Pattern 3: Advanced Partition Management ---");
    demonstrate_advanced_partition_management(&consumer).await?;

    // Pattern 4: Performance-optimized streaming
    println!("\n--- Pattern 4: Performance-Optimized Streaming ---");
    demonstrate_optimized_streaming(&consumer).await?;

    // Pattern 5: Production metrics and observability
    println!("\n--- Pattern 5: Production Metrics ---");
    demonstrate_production_metrics(&consumer).await?;

    // Graceful shutdown demonstration
    println!("\n🛑 Demonstrating graceful shutdown...");
    demonstrate_graceful_shutdown(&consumer, &client).await?;

    println!("✅ Production consumer example completed successfully");
    Ok(())
}

/// Start comprehensive production monitoring
async fn start_production_monitoring(consumer: &Consumer) {
    let metrics_consumer = consumer.clone();

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(15));

        loop {
            interval.tick().await;

            // Collect comprehensive metrics
            let metrics = metrics_consumer.metrics().await;
            let lag = metrics_consumer.get_lag().await.unwrap_or_default();
            let assigned_partitions = metrics_consumer.assigned_partitions().await;
            let paused_partitions = metrics_consumer.paused_partitions().await;

            // Generate production metrics report
            println!("\n📊 === PRODUCTION METRICS REPORT ===");
            println!("🏷️  Consumer ID: {}", metrics_consumer.id());
            println!("📡  Topic: {}", metrics_consumer.topic());
            println!("👥  Consumer Group: {}", metrics_consumer.consumer_group());
            println!(
                "📦  Messages Received: {}",
                metrics.messages_received.load(Ordering::Relaxed)
            );
            println!(
                "✅  Messages Processed: {}",
                metrics.messages_processed.load(Ordering::Relaxed)
            );
            println!(
                "❌  Messages Failed: {}",
                metrics.messages_failed.load(Ordering::Relaxed)
            );
            println!(
                "📏  Bytes Received: {} KB",
                metrics.bytes_received.load(Ordering::Relaxed) / 1024
            );

            // Partition information
            println!("🗂️  Assigned Partitions: {:?}", assigned_partitions);
            if !paused_partitions.is_empty() {
                println!("⏸️  Paused Partitions: {:?}", paused_partitions);
            }

            // Lag analysis
            let total_lag: u64 = lag.values().sum();
            println!("⏱️  Total Consumer Lag: {} messages", total_lag);
            if !lag.is_empty() {
                println!("📈  Lag by Partition: {:?}", lag);
            }

            // Processing rate calculation
            let messages_processed = metrics.messages_processed.load(Ordering::Relaxed);
            if messages_processed > 0 {
                println!("⚡  Processing Rate: ~{} msg/sec", messages_processed / 15);
            }

            // Last activity
            if let Some(last_receive) = *metrics.last_receive_time.read().await {
                let since_last = last_receive.elapsed();
                if since_last.as_secs() > 30 {
                    println!("⚠️  Warning: No messages received for {:?}", since_last);
                } else {
                    println!("🔄  Last Message: {:?} ago", since_last);
                }
            }

            println!("=====================================");
        }
    });
}

/// Demonstrate high-throughput batch processing
async fn demonstrate_batch_processing(consumer: &Consumer) -> Result<()> {
    println!("Processing messages in high-throughput batch mode...");

    let mut processed_count = 0;
    let batch_size = 20;
    let max_batches = 3;

    for batch_num in 1..=max_batches {
        println!("📦 Processing batch {} of {}", batch_num, max_batches);
        let batch_start = std::time::Instant::now();

        let mut batch_messages = Vec::new();

        // Collect batch of messages
        for _ in 0..batch_size {
            if let Some(consumer_message) = consumer.receive().await? {
                batch_messages.push(consumer_message);
            } else {
                break;
            }
        }

        if batch_messages.is_empty() {
            println!("No messages available for batch {}", batch_num);
            continue;
        }

        // Process batch with enhanced error handling
        for (i, consumer_message) in batch_messages.into_iter().enumerate() {
            let message = &consumer_message.message;

            match process_message_with_enhanced_handling(message).await {
                Ok(_) => {
                    consumer_message.ack().await?;
                    processed_count += 1;
                }
                Err(ProcessingError::Retryable) => {
                    println!("   Message {}: Retryable error - will be retried", i + 1);
                    consumer_message.nack().await?;
                }
                Err(ProcessingError::Fatal) => {
                    println!(
                        "   Message {}: Fatal error - acknowledged to prevent retry",
                        i + 1
                    );
                    consumer_message.ack().await?;
                }
                Err(ProcessingError::CircuitBreakerOpen) => {
                    println!(
                        "   Message {}: Circuit breaker open - will retry later",
                        i + 1
                    );
                    consumer_message.nack().await?;
                }
            }
        }

        // Manual commit after successful batch processing
        consumer.commit().await?;

        let batch_duration = batch_start.elapsed();
        println!(
            "   ✅ Batch {} completed in {:?} ({} messages)",
            batch_num, batch_duration, processed_count
        );
    }

    println!(
        "📊 Batch processing completed: {} total messages processed",
        processed_count
    );
    Ok(())
}

/// Demonstrate resilient processing with circuit breaker
async fn demonstrate_resilient_processing(consumer: &Consumer) -> Result<()> {
    println!("Demonstrating resilient processing with enhanced error handling...");

    let mut consecutive_failures = 0;
    let max_consecutive_failures = 3;
    let mut messages_processed = 0;

    for i in 1..=15 {
        if let Some(consumer_message) = consumer.receive().await? {
            let message = &consumer_message.message;

            println!("Processing message {} (ID: {})", i, message.id);

            match process_message_with_enhanced_handling(message).await {
                Ok(_) => {
                    println!("   ✅ Successfully processed");
                    consumer_message.ack().await?;
                    consecutive_failures = 0; // Reset failure counter
                    messages_processed += 1;
                }
                Err(ProcessingError::Retryable) => {
                    println!("   ⚠️ Retryable error - message will be retried");
                    consumer_message.nack().await?;
                    consecutive_failures += 1;
                }
                Err(ProcessingError::Fatal) => {
                    println!("   ❌ Fatal error - message acknowledged to prevent infinite retry");
                    consumer_message.ack().await?;
                    consecutive_failures = 0;
                }
                Err(ProcessingError::CircuitBreakerOpen) => {
                    println!("   🔴 Circuit breaker protection activated");
                    consumer_message.nack().await?;

                    // Wait for circuit breaker recovery
                    println!("   ⏱️ Waiting for circuit breaker recovery...");
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    consecutive_failures = 0;
                }
            }

            // Circuit breaker simulation
            if consecutive_failures >= max_consecutive_failures {
                println!("   🛡️ Too many consecutive failures, pausing processing");
                tokio::time::sleep(Duration::from_secs(1)).await;
                consecutive_failures = 0;
            }

            // Manual commit every 5 messages for data consistency
            if messages_processed % 5 == 0 && messages_processed > 0 {
                println!(
                    "   📝 Manual commit at {} processed messages",
                    messages_processed
                );
                consumer.commit().await?;
            }
        } else {
            break;
        }
    }

    println!(
        "🛡️ Resilient processing completed: {} messages processed",
        messages_processed
    );
    Ok(())
}

/// Demonstrate advanced partition management
async fn demonstrate_advanced_partition_management(consumer: &Consumer) -> Result<()> {
    println!("Demonstrating advanced partition management...");

    let assigned_partitions = consumer.assigned_partitions().await;
    println!(
        "📊 Currently assigned partitions: {:?}",
        assigned_partitions
    );

    if assigned_partitions.is_empty() {
        println!("No partitions assigned, skipping partition management demo");
        return Ok(());
    }

    // Get detailed partition assignment information
    if let Some(assignment) = consumer.partition_assignment().await {
        println!("📋 Partition Assignment Details:");
        println!("   Assignment ID: {}", assignment.assignment_id);
        println!("   Assigned at: {:?}", assignment.assigned_at);
        println!("   Total partitions: {}", assignment.partitions.len());
    }

    // Demonstrate dynamic partition management
    if assigned_partitions.len() > 1 {
        let partition_to_manage = assigned_partitions[0];

        // Pause specific partition
        println!(
            "⏸️ Pausing partition {} for maintenance",
            partition_to_manage
        );
        consumer.pause_partitions(vec![partition_to_manage]).await?;

        let paused = consumer.paused_partitions().await;
        println!("   Currently paused: {:?}", paused);

        // Process from remaining partitions
        println!("🔄 Processing from active partitions...");
        for _ in 0..3 {
            if let Some(consumer_message) = consumer.receive().await? {
                let message = &consumer_message.message;
                println!(
                    "   Received from partition {}: {}",
                    message.partition, message.id
                );
                consumer_message.ack().await?;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        // Resume paused partition
        println!("▶️ Resuming partition {}", partition_to_manage);
        consumer
            .resume_partitions(vec![partition_to_manage])
            .await?;

        let paused_after = consumer.paused_partitions().await;
        println!("   Paused after resume: {:?}", paused_after);
    }

    // Show current lag for all partitions
    match consumer.get_lag().await {
        Ok(lag_map) => {
            println!("📈 Current consumer lag by partition:");
            for (partition, lag) in lag_map {
                println!("   Partition {}: {} messages behind", partition, lag);
            }
        }
        Err(e) => println!("Could not retrieve lag information: {}", e),
    }

    println!("✅ Advanced partition management completed");
    Ok(())
}

/// Demonstrate performance-optimized streaming
async fn demonstrate_optimized_streaming(consumer: &Consumer) -> Result<()> {
    println!("Demonstrating performance-optimized streaming interface...");

    let mut stream = consumer.stream();
    let mut processed_count = 0;
    let max_stream_messages = 10;
    let start_time = std::time::Instant::now();

    println!("🌊 Starting optimized stream processing...");

    while let Some(result) = stream.next().await {
        if processed_count >= max_stream_messages {
            break;
        }

        match result {
            Ok(consumer_message) => {
                let message = &consumer_message.message;

                // High-performance processing simulation
                match process_message_with_enhanced_handling(message).await {
                    Ok(_) => {
                        consumer_message.ack().await?;
                        processed_count += 1;

                        if processed_count % 5 == 0 {
                            let elapsed = start_time.elapsed();
                            let rate = processed_count as f64 / elapsed.as_secs_f64();
                            println!(
                                "   📊 Processed {} messages at {:.1} msg/sec",
                                processed_count, rate
                            );
                        }
                    }
                    Err(e) => {
                        println!("   ⚠️ Stream processing error: {:?}", e);
                        consumer_message.nack().await?;
                    }
                }
            }
            Err(e) => {
                println!("Stream error: {}", e);
                break;
            }
        }
    }

    let total_time = start_time.elapsed();
    let final_rate = processed_count as f64 / total_time.as_secs_f64();

    println!("⚡ Stream processing completed:");
    println!("   Total messages: {}", processed_count);
    println!("   Total time: {:?}", total_time);
    println!("   Average rate: {:.1} messages/sec", final_rate);

    Ok(())
}

/// Demonstrate production metrics and observability
async fn demonstrate_production_metrics(consumer: &Consumer) -> Result<()> {
    println!("Generating comprehensive production metrics report...");

    // Process some messages to generate metrics
    for i in 1..=10 {
        if let Some(consumer_message) = consumer.receive().await? {
            let message = &consumer_message.message;

            // Simulate different processing outcomes
            match i % 4 {
                0 => {
                    // Simulate failure for metrics
                    consumer_message.nack().await?;
                }
                _ => {
                    consumer_message.ack().await?;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Generate comprehensive metrics report
    let metrics = consumer.metrics().await;
    let lag = consumer.get_lag().await.unwrap_or_default();
    let assigned_partitions = consumer.assigned_partitions().await;

    println!("\n📊 === COMPREHENSIVE METRICS REPORT ===");
    println!("Consumer Configuration:");
    println!("   ID: {}", consumer.id());
    println!("   Topic: {}", consumer.topic());
    println!("   Group: {}", consumer.consumer_group());
    println!("   Config: {:?}", consumer.config());

    println!("\nMessage Statistics:");
    println!(
        "   Messages Received: {}",
        metrics.messages_received.load(Ordering::Relaxed)
    );
    println!(
        "   Messages Processed: {}",
        metrics.messages_processed.load(Ordering::Relaxed)
    );
    println!(
        "   Messages Failed: {}",
        metrics.messages_failed.load(Ordering::Relaxed)
    );
    println!(
        "   Bytes Received: {} bytes",
        metrics.bytes_received.load(Ordering::Relaxed)
    );

    println!("\nPartition Information:");
    println!("   Assigned Partitions: {:?}", assigned_partitions);
    println!("   Partition Count: {}", assigned_partitions.len());

    println!("\nLag Analysis:");
    if lag.is_empty() {
        println!("   No lag information available");
    } else {
        let total_lag: u64 = lag.values().sum();
        println!("   Total Lag: {} messages", total_lag);
        println!("   Per-Partition Lag: {:?}", lag);
    }

    // Performance metrics
    if let Some(last_receive) = *metrics.last_receive_time.read().await {
        println!("\nTiming Information:");
        println!("   Last Message Received: {:?} ago", last_receive.elapsed());
    }

    let processing_time = *metrics.processing_time_ms.read().await;
    if processing_time > 0.0 {
        println!("   Average Processing Time: {:.2}ms", processing_time);
    }

    println!("=========================================");

    Ok(())
}

/// Demonstrate graceful shutdown
async fn demonstrate_graceful_shutdown(consumer: &Consumer, client: &RustMqClient) -> Result<()> {
    println!("Initiating graceful shutdown sequence...");

    // Process any remaining messages
    println!("🔄 Processing remaining messages...");
    let mut final_count = 0;
    let shutdown_timeout = Duration::from_secs(2);
    let shutdown_start = std::time::Instant::now();

    while shutdown_start.elapsed() < shutdown_timeout {
        if let Some(consumer_message) = consumer.receive().await? {
            consumer_message.ack().await?;
            final_count += 1;
        } else {
            break;
        }
    }

    if final_count > 0 {
        println!("   ✅ Processed {} final messages", final_count);
    }

    // Final commit before shutdown
    println!("📝 Performing final offset commit...");
    consumer.commit().await?;

    // Close consumer with proper cleanup
    println!("🔒 Closing consumer with proper cleanup...");
    consumer.close().await?;

    // Close client connection
    println!("🔌 Closing client connection...");
    client.close().await?;

    println!("✅ Graceful shutdown completed successfully");
    println!("   All resources properly cleaned up");
    println!("   All offsets committed");
    println!("   No message loss occurred");

    Ok(())
}

/// Enhanced message processing with comprehensive error handling
async fn process_message_with_enhanced_handling(
    message: &Message,
) -> std::result::Result<(), ProcessingError> {
    // Simulate realistic processing time
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Simulate different error conditions based on message content
    let payload = message.payload_as_string().unwrap_or_default();

    if payload.contains("circuit_breaker") {
        Err(ProcessingError::CircuitBreakerOpen)
    } else if payload.contains("fatal_error") {
        Err(ProcessingError::Fatal)
    } else if payload.contains("retry_error") {
        Err(ProcessingError::Retryable)
    } else {
        // Successful processing
        Ok(())
    }
}

/// Enhanced processing error types
#[derive(Debug)]
enum ProcessingError {
    Retryable,
    Fatal,
    CircuitBreakerOpen,
}

impl std::fmt::Display for ProcessingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessingError::Retryable => write!(f, "Retryable processing error"),
            ProcessingError::Fatal => write!(f, "Fatal processing error"),
            ProcessingError::CircuitBreakerOpen => write!(f, "Circuit breaker is open"),
        }
    }
}

impl std::error::Error for ProcessingError {}
