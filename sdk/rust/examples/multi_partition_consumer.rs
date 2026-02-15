use futures::StreamExt;
use rustmq_client::{config::StartPosition, *};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio;
use tracing_subscriber;

/// Comprehensive multi-partition Consumer example demonstrating:
/// - Multi-partition assignment and management
/// - Per-partition offset tracking and commits
/// - Partition-specific seeking capabilities
/// - Error handling and recovery strategies
/// - Performance monitoring and metrics
/// - Advanced partition control (pause/resume)

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Create client configuration optimized for multi-partition workloads
    let config = ClientConfig {
        brokers: vec![
            "localhost:9092".to_string(),
            "localhost:9093".to_string(), // Multiple brokers for reliability
            "localhost:9094".to_string(),
        ],
        client_id: Some("multi-partition-consumer-demo".to_string()),

        // Optimize for multi-partition performance
        max_connections: 15, // More connections for concurrent partition fetching
        request_timeout: Duration::from_secs(10),
        keep_alive_interval: Duration::from_secs(30),

        ..Default::default()
    };

    // Create client
    let client = RustMqClient::new(config).await?;
    println!("🚀 Connected to RustMQ cluster with multi-partition support");

    // Configure consumer for optimal multi-partition performance
    let consumer_config = ConsumerConfig {
        consumer_id: Some("mp-consumer-1".to_string()),
        consumer_group: "multi-partition-demo-group".to_string(),

        // Multi-partition optimized settings
        enable_auto_commit: false, // Manual commits for precise control
        auto_commit_interval: Duration::from_secs(30),

        // Larger batches for efficiency across multiple partitions
        fetch_size: 1000,
        fetch_timeout: Duration::from_millis(500), // Responsive polling

        // Start from earliest to demonstrate full functionality
        start_position: StartPosition::Earliest,

        // Robust error handling
        max_retry_attempts: 5,
        dead_letter_queue: Some("mp-demo-dlq".to_string()),
    };

    // Create consumer
    let consumer = ConsumerBuilder::new()
        .topic("multi-partition-demo-topic")
        .consumer_group("multi-partition-demo-group")
        .config(consumer_config)
        .client(client.clone())
        .build()
        .await?;

    println!("📊 Created multi-partition consumer");

    // Start comprehensive monitoring task
    let metrics_consumer = consumer.clone();
    let metrics_handle = tokio::spawn(async move {
        multi_partition_monitoring(&metrics_consumer).await;
    });

    // Demonstrate multi-partition capabilities
    println!("\n🎯 Starting multi-partition consumption demonstration...");

    // Step 1: Partition Discovery and Assignment
    println!("\n--- Step 1: Partition Discovery ---");
    demonstrate_partition_discovery(&consumer).await?;

    // Step 2: Multi-Partition Message Processing
    println!("\n--- Step 2: Multi-Partition Message Processing ---");
    demonstrate_multi_partition_processing(&consumer).await?;

    // Step 3: Advanced Seeking Capabilities
    println!("\n--- Step 3: Advanced Seeking ---");
    demonstrate_advanced_seeking(&consumer).await?;

    // Step 4: Partition Management
    println!("\n--- Step 4: Partition Management ---");
    demonstrate_partition_management(&consumer).await?;

    // Step 5: Error Handling and Recovery
    println!("\n--- Step 5: Error Handling ---");
    demonstrate_error_handling(&consumer).await?;

    // Step 6: Performance Optimization
    println!("\n--- Step 6: Performance Optimization ---");
    demonstrate_performance_optimization(&consumer).await?;

    // Clean shutdown
    println!("\n🛑 Shutting down multi-partition consumer...");
    metrics_handle.abort();

    // Final commit of all partition offsets
    println!("💾 Final commit of all partition offsets...");
    consumer.commit().await?;

    consumer.close().await?;
    client.close().await?;

    println!("✅ Multi-partition consumer demonstration completed!");
    Ok(())
}

/// Demonstrate partition discovery and assignment
async fn demonstrate_partition_discovery(consumer: &Consumer) -> Result<()> {
    // Get assigned partitions
    let partitions = consumer.assigned_partitions().await;
    println!("🔍 Assigned partitions: {:?}", partitions);

    if partitions.is_empty() {
        println!("⚠️  No partitions assigned - this might be due to:");
        println!("   • Topic doesn't exist");
        println!("   • No available partitions");
        println!("   • Consumer group rebalancing in progress");
        return Ok(());
    }

    // Get detailed partition assignment information
    if let Some(assignment) = consumer.partition_assignment().await {
        println!("📋 Partition Assignment Details:");
        println!("   Assignment ID: {}", assignment.assignment_id);
        println!("   Assigned at: {:?}", assignment.assigned_at);
        println!("   Total partitions: {}", assignment.partitions.len());

        // Show per-partition status
        for &partition in &assignment.partitions {
            match consumer.committed_offset(partition).await {
                Ok(offset) => println!("   Partition {}: committed offset {}", partition, offset),
                Err(e) => println!("   Partition {}: error getting offset - {}", partition, e),
            }
        }
    }

    Ok(())
}

/// Demonstrate multi-partition message processing with proper acknowledgment
async fn demonstrate_multi_partition_processing(consumer: &Consumer) -> Result<()> {
    let mut processed_count = 0;
    let max_messages = 20;
    let mut partition_counts = HashMap::new();

    println!("📨 Processing messages from all assigned partitions...");

    while processed_count < max_messages {
        match consumer.receive().await? {
            Some(consumer_message) => {
                let message = &consumer_message.message;

                // Track per-partition processing
                *partition_counts.entry(message.partition).or_insert(0) += 1;

                println!(
                    "📬 Message {} from partition {} (offset: {})",
                    message.id, message.partition, message.offset
                );
                println!(
                    "   Payload: {}",
                    message
                        .payload_as_string()
                        .unwrap_or("<binary>".to_string())
                );
                println!("   Age: {}ms", message.age_ms());

                // Simulate processing with different outcomes
                match simulate_message_processing(message).await {
                    Ok(_) => {
                        println!("   ✅ Processed successfully");
                        consumer_message.ack().await?;
                    }
                    Err(ProcessingError::Retryable) => {
                        println!("   🔄 Processing failed (retryable)");
                        consumer_message.nack().await?;
                    }
                    Err(ProcessingError::Fatal) => {
                        println!("   ❌ Fatal error - acknowledging to prevent infinite retry");
                        consumer_message.ack().await?;
                    }
                }

                processed_count += 1;

                // Commit every 5 messages for demonstration
                if processed_count % 5 == 0 {
                    println!(
                        "   💾 Manual commit (processed {} messages)",
                        processed_count
                    );
                    consumer.commit().await?;
                }
            }
            None => {
                println!("📭 No messages available, waiting...");
                tokio::time::sleep(Duration::from_millis(100)).await;
                break;
            }
        }
    }

    // Show processing summary
    println!("\n📊 Processing Summary:");
    for (partition, count) in partition_counts {
        println!("   Partition {}: {} messages processed", partition, count);
    }

    Ok(())
}

/// Demonstrate advanced seeking capabilities across multiple partitions
async fn demonstrate_advanced_seeking(consumer: &Consumer) -> Result<()> {
    let assigned_partitions = consumer.assigned_partitions().await;

    if assigned_partitions.is_empty() {
        println!("⚠️  No partitions to demonstrate seeking");
        return Ok(());
    }

    println!("🎯 Demonstrating multi-partition seeking capabilities...");

    // Demonstrate 1: Individual partition seeking
    if let Some(&first_partition) = assigned_partitions.first() {
        println!("🔍 Seeking partition {} to offset 10", first_partition);
        match consumer.seek(first_partition, 10).await {
            Ok(_) => println!("   ✅ Successfully seeked partition {}", first_partition),
            Err(e) => println!("   ❌ Seek failed: {}", e),
        }
    }

    // Demonstrate 2: Seeking all partitions to same offset
    println!("🔍 Seeking all partitions to offset 5");
    match consumer.seek_all(5).await {
        Ok(_) => println!("   ✅ Successfully seeked all partitions"),
        Err(e) => println!("   ❌ Bulk seek failed: {}", e),
    }

    // Demonstrate 3: Timestamp-based seeking
    let one_hour_ago = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 3600;

    if let Some(&first_partition) = assigned_partitions.first() {
        println!(
            "🕐 Seeking partition {} to timestamp {} (1 hour ago)",
            first_partition, one_hour_ago
        );
        match consumer
            .seek_to_timestamp(first_partition, one_hour_ago)
            .await
        {
            Ok(_) => println!("   ✅ Successfully seeked to timestamp"),
            Err(e) => println!("   ❌ Timestamp seek failed: {}", e),
        }
    }

    // Demonstrate 4: Seeking all partitions to timestamp
    println!("🕐 Seeking all partitions to timestamp {}", one_hour_ago);
    match consumer.seek_all_to_timestamp(one_hour_ago).await {
        Ok(partition_offsets) => {
            println!("   ✅ Successfully seeked all partitions to timestamp:");
            for (partition, offset) in partition_offsets {
                println!("     Partition {}: offset {}", partition, offset);
            }
        }
        Err(e) => println!("   ❌ Bulk timestamp seek failed: {}", e),
    }

    // Show updated committed offsets
    println!("\n📍 Current committed offsets after seeking:");
    for &partition in &assigned_partitions {
        match consumer.committed_offset(partition).await {
            Ok(offset) => println!("   Partition {}: offset {}", partition, offset),
            Err(e) => println!("   Partition {}: error - {}", partition, e),
        }
    }

    Ok(())
}

/// Demonstrate partition management (pause/resume)
async fn demonstrate_partition_management(consumer: &Consumer) -> Result<()> {
    let assigned_partitions = consumer.assigned_partitions().await;

    if assigned_partitions.len() < 2 {
        println!("⚠️  Need at least 2 partitions to demonstrate pause/resume");
        return Ok(());
    }

    println!("⏸️  Demonstrating partition pause/resume functionality...");

    // Pause half of the partitions
    let partitions_to_pause: Vec<u32> = assigned_partitions
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0) // Pause every other partition
        .map(|(_, &p)| p)
        .collect();

    println!("⏸️  Pausing partitions: {:?}", partitions_to_pause);
    consumer
        .pause_partitions(partitions_to_pause.clone())
        .await?;

    let paused = consumer.paused_partitions().await;
    println!("   📋 Currently paused: {:?}", paused);

    // Process messages only from active partitions
    println!("📨 Processing from active partitions only...");
    let mut active_processing_count = 0;
    while active_processing_count < 5 {
        match consumer.receive().await? {
            Some(consumer_message) => {
                let message = &consumer_message.message;
                println!("   📬 Message from active partition {}", message.partition);

                // Verify message is from non-paused partition
                if paused.contains(&message.partition) {
                    println!("   ❌ ERROR: Received message from paused partition!");
                } else {
                    println!("   ✅ Correctly received from active partition");
                }

                consumer_message.ack().await?;
                active_processing_count += 1;
            }
            None => {
                println!("   📭 No messages from active partitions");
                break;
            }
        }
    }

    // Resume paused partitions
    println!("▶️  Resuming paused partitions: {:?}", partitions_to_pause);
    consumer.resume_partitions(partitions_to_pause).await?;

    let paused_after_resume = consumer.paused_partitions().await;
    println!("   📋 Paused after resume: {:?}", paused_after_resume);

    // Verify all partitions are active
    if paused_after_resume.is_empty() {
        println!("   ✅ All partitions are now active");
    } else {
        println!(
            "   ⚠️  Some partitions still paused: {:?}",
            paused_after_resume
        );
    }

    Ok(())
}

/// Demonstrate error handling strategies for multi-partition scenarios
async fn demonstrate_error_handling(consumer: &Consumer) -> Result<()> {
    println!("🚨 Demonstrating multi-partition error handling...");

    // Get consumer lag to identify potential issues
    match consumer.get_lag().await {
        Ok(lag_map) => {
            println!("📊 Current consumer lag by partition:");
            let mut total_lag = 0u64;
            for (partition, lag) in &lag_map {
                println!("   Partition {}: {} messages behind", partition, lag);
                total_lag += lag;

                // Alert on high lag
                if *lag > 1000 {
                    println!("   ⚠️  HIGH LAG WARNING for partition {}", partition);
                }
            }
            println!("   Total lag across all partitions: {} messages", total_lag);
        }
        Err(e) => {
            println!("❌ Failed to get consumer lag: {}", e);
            println!("   This could indicate:");
            println!("   • Network connectivity issues");
            println!("   • Broker communication problems");
            println!("   • Consumer group coordination issues");
        }
    }

    // Demonstrate retry logic with simulated errors
    println!("\n🔄 Testing error recovery mechanisms...");
    let mut error_count = 0;
    while error_count < 3 {
        match consumer.receive().await {
            Ok(Some(consumer_message)) => {
                let message = &consumer_message.message;

                // Simulate different error scenarios
                if message
                    .payload_as_string()
                    .unwrap_or_default()
                    .contains("error")
                {
                    println!("❌ Simulated processing error for message {}", message.id);
                    consumer_message.nack().await?;
                    error_count += 1;
                } else {
                    println!("✅ Normal processing for message {}", message.id);
                    consumer_message.ack().await?;
                }
            }
            Ok(None) => {
                println!("📭 No more messages for error testing");
                break;
            }
            Err(e) => {
                println!("❌ Consumer error: {}", e);

                // Implement error categorization
                match e {
                    ClientError::Connection(_) => {
                        println!("   🔌 Connection error - implementing backoff retry");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    ClientError::Timeout { .. } => {
                        println!("   ⏰ Timeout error - adjusting fetch parameters");
                    }
                    ClientError::OffsetOutOfRange { .. } => {
                        println!("   📍 Offset out of range - seeking to latest");
                        // In practice, you might seek to latest available offset
                    }
                    _ => {
                        println!("   ❓ Other error - applying general retry logic");
                    }
                }
                error_count += 1;
            }
        }
    }

    Ok(())
}

/// Demonstrate performance optimization techniques
async fn demonstrate_performance_optimization(consumer: &Consumer) -> Result<()> {
    println!("⚡ Demonstrating performance optimization for multi-partition consumption...");

    // Collect baseline metrics
    let start_time = std::time::Instant::now();
    let initial_metrics = consumer.metrics().await;
    let initial_processed = initial_metrics.messages_processed.load(Ordering::Relaxed);

    // Process messages with performance monitoring
    let mut optimization_count = 0;
    let optimization_target = 10;

    while optimization_count < optimization_target {
        match consumer.receive().await? {
            Some(consumer_message) => {
                let process_start = std::time::Instant::now();

                // Simulate optimized processing
                simulate_optimized_processing(&consumer_message.message).await?;

                let process_duration = process_start.elapsed();
                consumer_message.ack().await?;

                optimization_count += 1;

                if optimization_count % 3 == 0 {
                    println!(
                        "⚡ Processed message {} in {:?}",
                        optimization_count, process_duration
                    );
                }
            }
            None => break,
        }
    }

    // Calculate performance metrics
    let total_duration = start_time.elapsed();
    let final_metrics = consumer.metrics().await;
    let final_processed = final_metrics.messages_processed.load(Ordering::Relaxed);
    let messages_processed = final_processed - initial_processed;

    println!("\n📈 Performance Summary:");
    println!("   Total time: {:?}", total_duration);
    println!("   Messages processed: {}", messages_processed);

    if total_duration.as_millis() > 0 {
        let throughput = (messages_processed as f64 * 1000.0) / total_duration.as_millis() as f64;
        println!("   Throughput: {:.2} messages/second", throughput);
    }

    // Show optimization recommendations
    println!("\n💡 Multi-partition optimization tips:");
    println!("   • Use larger fetch sizes (500-2000) for better batch efficiency");
    println!("   • Enable manual commits for precise offset control");
    println!("   • Monitor per-partition lag to identify bottlenecks");
    println!("   • Use partition pause/resume for load balancing");
    println!("   • Implement parallel processing for independent partitions");

    Ok(())
}

/// Continuous monitoring of multi-partition consumer metrics
async fn multi_partition_monitoring(consumer: &Consumer) {
    let mut interval = tokio::time::interval(Duration::from_secs(15));

    loop {
        interval.tick().await;

        println!("\n📊 === Multi-Partition Consumer Metrics ===");

        // Basic metrics
        let metrics = consumer.metrics().await;
        println!(
            "📈 Messages received: {}",
            metrics.messages_received.load(Ordering::Relaxed)
        );
        println!(
            "✅ Messages processed: {}",
            metrics.messages_processed.load(Ordering::Relaxed)
        );
        println!(
            "❌ Messages failed: {}",
            metrics.messages_failed.load(Ordering::Relaxed)
        );
        println!(
            "📦 Bytes received: {}",
            metrics.bytes_received.load(Ordering::Relaxed)
        );

        // Partition-specific metrics
        let assigned_partitions = consumer.assigned_partitions().await;
        println!(
            "🔢 Assigned partitions: {} ({:?})",
            assigned_partitions.len(),
            assigned_partitions
        );

        // Consumer lag per partition
        match consumer.get_lag().await {
            Ok(lag_map) => {
                if !lag_map.is_empty() {
                    println!("📊 Consumer lag by partition:");
                    for (partition, lag) in lag_map {
                        let status = if lag > 100 {
                            "⚠️ HIGH"
                        } else if lag > 10 {
                            "⚡ MEDIUM"
                        } else {
                            "✅ LOW"
                        };
                        println!(
                            "   Partition {}: {} messages ({} lag)",
                            partition, lag, status
                        );
                    }
                } else {
                    println!("📊 No lag data available");
                }
            }
            Err(e) => println!("❌ Failed to get lag data: {}", e),
        }

        // Paused partitions
        let paused = consumer.paused_partitions().await;
        if !paused.is_empty() {
            println!("⏸️  Paused partitions: {:?}", paused);
        }

        // Processing efficiency
        let received = metrics.messages_received.load(Ordering::Relaxed);
        let processed = metrics.messages_processed.load(Ordering::Relaxed);
        if received > 0 {
            let efficiency = (processed as f64 / received as f64) * 100.0;
            println!("🎯 Processing efficiency: {:.1}%", efficiency);
        }

        println!("=====================================");
    }
}

/// Simulate realistic message processing with potential failures
async fn simulate_message_processing(
    message: &Message,
) -> std::result::Result<(), ProcessingError> {
    // Simulate processing time
    tokio::time::sleep(Duration::from_millis(10)).await;

    let payload = message.payload_as_string().unwrap_or_default();

    // Simulate different outcomes based on message content
    if payload.contains("fatal_error") {
        Err(ProcessingError::Fatal)
    } else if payload.contains("retry") || payload.contains("temporary") {
        Err(ProcessingError::Retryable)
    } else {
        Ok(())
    }
}

/// Simulate optimized message processing
async fn simulate_optimized_processing(message: &Message) -> Result<()> {
    // Simulate zero-copy processing techniques
    let _payload_ref = &message.payload; // No copying, just reference

    // Simulate batch processing optimization
    tokio::time::sleep(Duration::from_millis(1)).await; // Optimized processing

    Ok(())
}

/// Processing error types for demonstration
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
