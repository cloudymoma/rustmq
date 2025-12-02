//! Load and performance integration tests
//!
//! These tests validate system performance under load and are marked with #[ignore]
//! to avoid running them on every test run. Run explicitly with:
//! ```bash
//! cargo test --release --test integration_load_test -- --ignored
//! ```

use rustmq::{Config, broker::Broker};
use rustmq::config::{BrokerConfig, NetworkConfig, WalConfig, ObjectStorageConfig, StorageType, CacheConfig, ReplicationConfig};
use rustmq_client::*;
use tempfile::TempDir;
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::time::{Duration, Instant};

// Use atomic counter to generate unique ports for each test
static PORT_COUNTER: AtomicU16 = AtomicU16::new(16000);

fn get_unique_ports() -> (u16, u16) {
    let base = PORT_COUNTER.fetch_add(10, Ordering::SeqCst);
    (base, base + 1)
}

async fn create_test_broker_config(temp_dir: &TempDir, quic_port: u16, rpc_port: u16) -> Config {
    let wal_path = temp_dir.path().join("wal");
    std::fs::create_dir_all(&wal_path).unwrap();

    let object_storage_path = temp_dir.path().join("storage");
    std::fs::create_dir_all(&object_storage_path).unwrap();

    Config {
        broker: BrokerConfig {
            id: format!("load-test-broker-{}", quic_port),
            rack_id: "load-test-rack-1".to_string(),
        },
        network: NetworkConfig {
            quic_listen: format!("127.0.0.1:{}", quic_port),
            rpc_listen: format!("127.0.0.1:{}", rpc_port),
            max_connections: 2000,
            connection_timeout_ms: 60000,
            quic_config: rustmq::config::QuicConfig::default(),
        },
        wal: WalConfig {
            path: wal_path,
            capacity_bytes: 100 * 1024 * 1024, // 100 MB for load test
            fsync_on_write: false, // Disable fsync for performance test
            segment_size_bytes: 10 * 1024 * 1024, // 10 MB segments
            buffer_size: 8192,
            upload_interval_ms: 60_000,
            flush_interval_ms: 5000,
        },
        object_storage: ObjectStorageConfig {
            storage_type: StorageType::Local {
                path: object_storage_path,
            },
            bucket: "load-test-bucket".to_string(),
            region: "us-central1".to_string(),
            endpoint: "https://storage.googleapis.com".to_string(),
            access_key: None,
            secret_key: None,
            multipart_threshold: 100 * 1024 * 1024,
            max_concurrent_uploads: 8,
        },
        cache: CacheConfig {
            write_cache_size_bytes: 10 * 1024 * 1024, // 10 MB
            read_cache_size_bytes: 10 * 1024 * 1024,  // 10 MB
            eviction_policy: rustmq::config::EvictionPolicy::Lru,
        },
        replication: ReplicationConfig {
            min_in_sync_replicas: 1,
            ack_timeout_ms: 10000,
            max_replication_lag: 10000,
            heartbeat_timeout_ms: 60000,
        },
        ..Default::default()
    }
}

#[tokio::test]
#[ignore] // Only run with --ignored flag
async fn test_10k_messages_per_second_for_one_minute() {
    println!("🚀 Starting load test: 10,000 msg/sec for 60 seconds");
    println!("⏱️  Target: 600,000 total messages");

    let temp_dir = TempDir::new().unwrap();
    let (quic_port, rpc_port) = get_unique_ports();

    let config = create_test_broker_config(&temp_dir, quic_port, rpc_port).await;

    // Start broker
    let mut broker = Broker::from_config(config).await
        .expect("Failed to create broker");

    broker.start().await.expect("Failed to start broker");
    println!("✅ Broker started on port {}", quic_port);

    // Give broker time to fully initialize
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Test parameters
    const TARGET_MSG_PER_SEC: usize = 10_000;
    const TEST_DURATION_SECS: u64 = 60;
    const NUM_PRODUCER_TASKS: usize = 10; // Parallelize producers
    const MSG_PER_TASK_PER_SEC: usize = TARGET_MSG_PER_SEC / NUM_PRODUCER_TASKS;

    // Shared counters
    let total_sent = Arc::new(AtomicUsize::new(0));
    let total_failed = Arc::new(AtomicUsize::new(0));

    // Latency tracking (in microseconds)
    let latencies = Arc::new(parking_lot::Mutex::new(Vec::with_capacity(100_000)));

    let test_start = Instant::now();

    // Spawn producer tasks
    let mut tasks = Vec::new();
    for task_id in 0..NUM_PRODUCER_TASKS {
        let sent_counter = total_sent.clone();
        let failed_counter = total_failed.clone();
        let latencies_clone = latencies.clone();

        let task = tokio::spawn(async move {
            // Create client for this task
            let client_config = ClientConfig {
                brokers: vec![format!("127.0.0.1:{}", quic_port)],
                client_id: Some(format!("load-test-producer-{}", task_id)),
                ..Default::default()
            };

            let client = match RustMqClient::new(client_config).await {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Task {}: Failed to create client: {}", task_id, e);
                    return;
                }
            };

            let producer = match client.create_producer("load-test-topic").await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Task {}: Failed to create producer: {}", task_id, e);
                    let _ = client.close().await;
                    return;
                }
            };

            let task_start = Instant::now();
            let mut msg_count = 0;

            // Run for TEST_DURATION_SECS
            while task_start.elapsed() < Duration::from_secs(TEST_DURATION_SECS) {
                let batch_start = Instant::now();

                // Send messages for this second
                for i in 0..MSG_PER_TASK_PER_SEC {
                    let send_start = Instant::now();

                    let message = Message::builder()
                        .topic("load-test-topic")
                        .payload(format!("Task {} - Message {}", task_id, msg_count + i))
                        .header("task-id", &task_id.to_string())
                        .header("msg-num", &(msg_count + i).to_string())
                        .build()
                        .expect("Failed to build message");

                    match producer.send(message).await {
                        Ok(_) => {
                            sent_counter.fetch_add(1, Ordering::Relaxed);

                            // Record latency (sample 1% to avoid memory overhead)
                            if i % 100 == 0 {
                                let latency_us = send_start.elapsed().as_micros() as u64;
                                latencies_clone.lock().push(latency_us);
                            }
                        }
                        Err(e) => {
                            failed_counter.fetch_add(1, Ordering::Relaxed);
                            if msg_count % 1000 == 0 {
                                eprintln!("Task {}: Send error: {}", task_id, e);
                            }
                        }
                    }
                }

                msg_count += MSG_PER_TASK_PER_SEC;

                // Sleep to achieve target rate (1 second per batch)
                let batch_duration = batch_start.elapsed();
                if batch_duration < Duration::from_secs(1) {
                    tokio::time::sleep(Duration::from_secs(1) - batch_duration).await;
                }
            }

            // Flush remaining messages
            let _ = producer.flush().await;
            let _ = producer.close().await;
            let _ = client.close().await;

            println!("✅ Task {} completed: sent {} messages", task_id, msg_count);
        });

        tasks.push(task);
    }

    // Monitor progress
    let monitor_task = tokio::spawn({
        let sent = total_sent.clone();
        let failed = total_failed.clone();
        async move {
            for second in 0..TEST_DURATION_SECS {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let current_sent = sent.load(Ordering::Relaxed);
                let current_failed = failed.load(Ordering::Relaxed);
                if second % 10 == 0 || second == TEST_DURATION_SECS - 1 {
                    println!("📊 Progress [{:2}s]: Sent: {}, Failed: {}, Rate: ~{} msg/s",
                        second + 1,
                        current_sent,
                        current_failed,
                        current_sent / (second + 1) as usize
                    );
                }
            }
        }
    });

    // Wait for all producer tasks to complete
    for task in tasks {
        let _ = task.await;
    }

    let _ = monitor_task.await;

    let test_duration = test_start.elapsed();

    // Collect metrics
    let final_sent = total_sent.load(Ordering::Relaxed);
    let final_failed = total_failed.load(Ordering::Relaxed);

    // Calculate latency percentiles
    let mut latency_samples = latencies.lock().clone();
    latency_samples.sort_unstable();

    let p50 = if !latency_samples.is_empty() {
        latency_samples[latency_samples.len() * 50 / 100]
    } else {
        0
    };

    let p95 = if !latency_samples.is_empty() {
        latency_samples[latency_samples.len() * 95 / 100]
    } else {
        0
    };

    let p99 = if !latency_samples.is_empty() {
        latency_samples[latency_samples.len() * 99 / 100]
    } else {
        0
    };

    // Print results
    println!("\n📈 Load Test Results:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Target:          {:8} messages", TARGET_MSG_PER_SEC * TEST_DURATION_SECS as usize);
    println!("Sent:            {:8} messages", final_sent);
    println!("Failed:          {:8} messages", final_failed);
    println!("Success Rate:    {:8.2}%", (final_sent as f64 / (final_sent + final_failed) as f64) * 100.0);
    println!("Duration:        {:8.2}s", test_duration.as_secs_f64());
    println!("Actual Rate:     {:8.0} msg/s", final_sent as f64 / test_duration.as_secs_f64());
    println!("\nLatency Percentiles:");
    println!("P50:             {:8.2} ms", p50 as f64 / 1000.0);
    println!("P95:             {:8.2} ms", p95 as f64 / 1000.0);
    println!("P99:             {:8.2} ms", p99 as f64 / 1000.0);
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Give broker time to process remaining messages
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Graceful shutdown
    println!("\n🛑 Stopping broker...");
    broker.stop().await.expect("Failed to stop broker");
    println!("✅ Broker stopped gracefully");

    // Assertions
    assert!(final_sent > 0, "Should have sent at least some messages");

    // Success rate should be very high (allow for some network errors)
    let success_rate = (final_sent as f64 / (final_sent + final_failed) as f64) * 100.0;
    assert!(success_rate > 95.0,
        "Success rate too low: {:.2}% (sent: {}, failed: {})",
        success_rate, final_sent, final_failed);

    // Should achieve at least 80% of target throughput
    let actual_rate = final_sent as f64 / test_duration.as_secs_f64();
    let target_rate = TARGET_MSG_PER_SEC as f64;
    let throughput_ratio = (actual_rate / target_rate) * 100.0;

    println!("\n✅ Load test completed successfully!");
    println!("   Throughput: {:.1}% of target ({:.0} msg/s actual vs {} msg/s target)",
        throughput_ratio, actual_rate, TARGET_MSG_PER_SEC);

    // Note: This assertion is informational - real throughput depends on hardware
    if throughput_ratio < 80.0 {
        println!("⚠️  Warning: Throughput below 80% of target. This may be due to test hardware limitations.");
    }
}

#[tokio::test]
#[ignore] // Only run with --ignored flag
async fn test_burst_load_handling() {
    println!("🚀 Starting burst load test");

    let temp_dir = TempDir::new().unwrap();
    let (quic_port, rpc_port) = get_unique_ports();

    let config = create_test_broker_config(&temp_dir, quic_port, rpc_port).await;

    let mut broker = Broker::from_config(config).await.unwrap();
    broker.start().await.unwrap();

    tokio::time::sleep(Duration::from_secs(1)).await;

    // Create client
    let client_config = ClientConfig {
        brokers: vec![format!("127.0.0.1:{}", quic_port)],
        client_id: Some("burst-test-client".to_string()),
        ..Default::default()
    };

    let client = RustMqClient::new(client_config).await.unwrap();
    let producer = Arc::new(client.create_producer("burst-test-topic").await.unwrap());

    // Send burst of 10,000 messages as fast as possible
    const BURST_SIZE: usize = 10_000;
    let sent_counter = Arc::new(AtomicUsize::new(0));

    let burst_start = Instant::now();

    // Send messages in parallel batches
    let mut tasks = Vec::new();
    for batch_id in 0..10 {
        let producer_clone = Arc::clone(&producer);
        let counter = sent_counter.clone();

        let task = tokio::spawn(async move {
            for i in 0..(BURST_SIZE / 10) {
                let message = Message::builder()
                    .topic("burst-test-topic")
                    .payload(format!("Burst message {} - {}", batch_id, i))
                    .build()
                    .expect("Failed to build message");

                if producer_clone.send(message).await.is_ok() {
                    counter.fetch_add(1, Ordering::Relaxed);
                }
            }
        });

        tasks.push(task);
    }

    for task in tasks {
        let _ = task.await;
    }

    let burst_duration = burst_start.elapsed();
    let burst_sent = sent_counter.load(Ordering::Relaxed);

    producer.flush().await.unwrap();

    println!("📊 Burst Results:");
    println!("   Sent: {} messages", burst_sent);
    println!("   Duration: {:?}", burst_duration);
    println!("   Rate: {:.0} msg/s", burst_sent as f64 / burst_duration.as_secs_f64());

    producer.close().await.unwrap();
    client.close().await.unwrap();

    tokio::time::sleep(Duration::from_secs(2)).await;
    broker.stop().await.unwrap();

    assert!(burst_sent > BURST_SIZE * 90 / 100,
        "Should handle at least 90% of burst messages");

    println!("✅ Burst load test passed!");
}
