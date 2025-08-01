use rustmq_client::{
    client::RustMqClient,
    config::{ClientConfig, ConsumerConfig, ProducerConfig},
    error::{ClientError, Result},
    message::{Message, MessageBuilder},
    stream::{
        MessageStream, StreamConfig, StreamMode, ErrorStrategy, MessageProcessor
    },
};
use async_trait::async_trait;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use tokio::sync::Mutex;
use std::collections::HashMap;
use std::time::Duration;

/// Test message processor that counts processed messages
struct TestProcessor {
    processed_count: Arc<AtomicU64>,
    should_fail: Arc<Mutex<bool>>,
}

impl TestProcessor {
    fn new() -> Self {
        Self {
            processed_count: Arc::new(AtomicU64::new(0)),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    async fn with_failure_mode(self, should_fail: bool) -> Self {
        *self.should_fail.lock().await = should_fail;
        self
    }

    fn processed_count(&self) -> u64 {
        self.processed_count.load(Ordering::Relaxed)
    }

    async fn set_failure_mode(&self, should_fail: bool) {
        *self.should_fail.lock().await = should_fail;
    }
}

#[async_trait]
impl MessageProcessor for TestProcessor {
    async fn process(&self, message: &Message) -> Result<Option<Message>> {
        if *self.should_fail.lock().await {
            return Err(ClientError::Stream("Test processing failure".to_string()));
        }

        self.processed_count.fetch_add(1, Ordering::Relaxed);
        
        // Transform the message by adding a prefix
        let mut processed_message = message.clone();
        processed_message.payload = format!("processed_{}", 
            String::from_utf8_lossy(&message.payload)).into();
        
        Ok(Some(processed_message))
    }

    async fn process_batch(&self, messages: &[Message]) -> Result<Vec<Option<Message>>> {
        if *self.should_fail.lock().await {
            return Err(ClientError::Stream("Test batch processing failure".to_string()));
        }

        let mut results = Vec::with_capacity(messages.len());
        for message in messages {
            self.processed_count.fetch_add(1, Ordering::Relaxed);
            
            let mut processed_message = message.clone();
            processed_message.payload = format!("batch_processed_{}", 
                String::from_utf8_lossy(&message.payload)).into();
            
            results.push(Some(processed_message));
        }
        
        Ok(results)
    }
}

/// Mock client for testing
struct MockClient {
    messages: Arc<Mutex<Vec<Message>>>,
    consumers: Arc<Mutex<HashMap<String, Vec<Message>>>>,
    producers: Arc<Mutex<HashMap<String, Vec<Message>>>>,
}

impl MockClient {
    fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            consumers: Arc::new(Mutex::new(HashMap::new())),
            producers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn add_messages_for_topic(&self, topic: &str, messages: Vec<Message>) {
        let mut consumers = self.consumers.lock().await;
        consumers.entry(topic.to_string()).or_insert_with(Vec::new).extend(messages);
    }

    async fn get_produced_messages(&self, topic: &str) -> Vec<Message> {
        let producers = self.producers.lock().await;
        producers.get(topic).cloned().unwrap_or_default()
    }
}

#[tokio::test]
async fn test_stream_individual_processing() {
    let mut config = StreamConfig::default();
    config.input_topics = vec!["test_input".to_string()];
    config.output_topic = Some("test_output".to_string());
    config.mode = StreamMode::Individual;
    config.error_strategy = ErrorStrategy::Skip;

    // Create test messages
    let test_messages = vec![
        MessageBuilder::new()
            .id("msg1")
            .topic("test_input")
            .payload(b"hello".to_vec())
            .build().unwrap(),
        MessageBuilder::new()
            .id("msg2")
            .topic("test_input")
            .payload(b"world".to_vec())
            .build().unwrap(),
    ];

    let processor = TestProcessor::new();
    let initial_count = processor.processed_count();

    // For actual testing, we'd need a real client
    // This is a basic structure test
    assert_eq!(initial_count, 0);
    assert!(config.input_topics.contains(&"test_input".to_string()));
    assert_eq!(config.output_topic, Some("test_output".to_string()));
    
    // Test processor directly
    let result = processor.process(&test_messages[0]).await;
    assert!(result.is_ok());
    assert_eq!(processor.processed_count(), 1);
}

#[tokio::test]
async fn test_stream_batch_processing() {
    let mut config = StreamConfig::default();
    config.mode = StreamMode::Batch { 
        batch_size: 3, 
        batch_timeout: Duration::from_millis(1000) 
    };

    let processor = TestProcessor::new();
    
    let test_messages = vec![
        MessageBuilder::new()
            .id("msg1")
            .topic("test_input")
            .payload(b"hello".to_vec())
            .build().unwrap(),
        MessageBuilder::new()
            .id("msg2")
            .topic("test_input")
            .payload(b"world".to_vec())
            .build().unwrap(),
        MessageBuilder::new()
            .id("msg3")
            .topic("test_input")
            .payload(b"test".to_vec())
            .build().unwrap(),
    ];

    let result = processor.process_batch(&test_messages).await;
    assert!(result.is_ok());
    assert_eq!(processor.processed_count(), 3);

    let processed_messages = result.unwrap();
    assert_eq!(processed_messages.len(), 3);
    
    for (i, msg_opt) in processed_messages.iter().enumerate() {
        assert!(msg_opt.is_some());
        let msg = msg_opt.as_ref().unwrap();
        assert!(String::from_utf8_lossy(&msg.payload).starts_with("batch_processed_"));
    }
}

#[tokio::test]
async fn test_stream_windowed_processing() {
    let mut config = StreamConfig::default();
    config.mode = StreamMode::Windowed { 
        window_size: Duration::from_millis(100),
        slide_interval: Duration::from_millis(50)
    };

    // Test configuration
    if let StreamMode::Windowed { window_size, slide_interval } = &config.mode {
        assert_eq!(*window_size, Duration::from_millis(100));
        assert_eq!(*slide_interval, Duration::from_millis(50));
    } else {
        panic!("Expected windowed mode");
    }
}

#[tokio::test]
async fn test_error_strategy_skip() {
    let config = StreamConfig {
        error_strategy: ErrorStrategy::Skip,
        ..Default::default()
    };

    let processor = TestProcessor::new().with_failure_mode(true).await;
    
    // Test that the processor would fail
    let test_message = MessageBuilder::new()
        .id("fail_msg")
        .topic("test_input")
        .payload(b"test".to_vec())
        .build().unwrap();

    let result = processor.process(&test_message).await;
    assert!(result.is_err());
    assert_eq!(processor.processed_count(), 0);
}

#[tokio::test]
async fn test_error_strategy_retry() {
    let config = StreamConfig {
        error_strategy: ErrorStrategy::Retry { 
            max_attempts: 3, 
            backoff_ms: 100 
        },
        ..Default::default()
    };

    if let ErrorStrategy::Retry { max_attempts, backoff_ms } = &config.error_strategy {
        assert_eq!(*max_attempts, 3);
        assert_eq!(*backoff_ms, 100);
    } else {
        panic!("Expected retry strategy");
    }
}

#[tokio::test]
async fn test_error_strategy_dead_letter() {
    let config = StreamConfig {
        error_strategy: ErrorStrategy::DeadLetter { 
            topic: "dead_letter_queue".to_string() 
        },
        ..Default::default()
    };

    if let ErrorStrategy::DeadLetter { topic } = &config.error_strategy {
        assert_eq!(topic, "dead_letter_queue");
    } else {
        panic!("Expected dead letter strategy");
    }
}

#[tokio::test]
async fn test_error_strategy_stop() {
    let config = StreamConfig {
        error_strategy: ErrorStrategy::Stop,
        ..Default::default()
    };

    matches!(config.error_strategy, ErrorStrategy::Stop);
}

#[tokio::test]
async fn test_stream_config_default() {
    let config = StreamConfig::default();
    
    assert_eq!(config.input_topics, vec!["input".to_string()]);
    assert_eq!(config.output_topic, Some("output".to_string()));
    assert_eq!(config.consumer_group, "stream-processor");
    assert_eq!(config.parallelism, 1);
    assert_eq!(config.processing_timeout, Duration::from_secs(30));
    assert_eq!(config.exactly_once, false);
    assert_eq!(config.max_in_flight, 100);
    assert!(matches!(config.error_strategy, ErrorStrategy::Skip));
    assert!(matches!(config.mode, StreamMode::Individual));
}

#[tokio::test]
async fn test_processor_state_management() {
    let processor = TestProcessor::new();
    
    // Test initial state
    assert_eq!(processor.processed_count(), 0);
    assert!(!*processor.should_fail.lock().await);
    
    // Test state changes
    processor.set_failure_mode(true).await;
    assert!(*processor.should_fail.lock().await);
    
    processor.set_failure_mode(false).await;
    assert!(!*processor.should_fail.lock().await);
}

#[tokio::test]
async fn test_message_transformation() {
    let processor = TestProcessor::new();
    
    let original_message = MessageBuilder::new()
        .id("transform_test")
        .topic("test_input")
        .payload(b"original_data".to_vec())
        .build().unwrap();

    let result = processor.process(&original_message).await;
    assert!(result.is_ok());
    
    let processed_message = result.unwrap().unwrap();
    let processed_payload = String::from_utf8_lossy(&processed_message.payload);
    assert_eq!(processed_payload, "processed_original_data");
    assert_eq!(processed_message.id, "transform_test");
}

#[tokio::test]
async fn test_batch_vs_individual_processing() {
    let processor = TestProcessor::new();
    
    let test_messages = vec![
        MessageBuilder::new()
            .id("msg1")
            .topic("test_input")
            .payload(b"test1".to_vec())
            .build().unwrap(),
        MessageBuilder::new()
            .id("msg2")
            .topic("test_input")
            .payload(b"test2".to_vec())
            .build().unwrap(),
    ];

    // Test individual processing
    let individual_result1 = processor.process(&test_messages[0]).await;
    let individual_result2 = processor.process(&test_messages[1]).await;
    
    assert!(individual_result1.is_ok());
    assert!(individual_result2.is_ok());
    
    let processed1 = individual_result1.unwrap().unwrap();
    let processed2 = individual_result2.unwrap().unwrap();
    
    assert!(String::from_utf8_lossy(&processed1.payload).starts_with("processed_"));
    assert!(String::from_utf8_lossy(&processed2.payload).starts_with("processed_"));
    
    // Reset processor for batch test
    let batch_processor = TestProcessor::new();
    
    // Test batch processing
    let batch_result = batch_processor.process_batch(&test_messages).await;
    assert!(batch_result.is_ok());
    
    let batch_processed = batch_result.unwrap();
    assert_eq!(batch_processed.len(), 2);
    
    for msg_opt in batch_processed {
        assert!(msg_opt.is_some());
        let msg = msg_opt.unwrap();
        assert!(String::from_utf8_lossy(&msg.payload).starts_with("batch_processed_"));
    }
}

#[tokio::test]
async fn test_concurrent_processing() {
    let processor = Arc::new(TestProcessor::new());
    let mut handles = Vec::new();

    // Spawn multiple tasks to process messages concurrently
    for i in 0..10 {
        let processor_clone = processor.clone();
        let handle = tokio::spawn(async move {
            let message = MessageBuilder::new()
                .id(format!("concurrent_msg_{}", i))
                .topic("test_input")
                .payload(format!("data_{}", i).as_bytes().to_vec())
                .build().unwrap();
            processor_clone.process(&message).await
        });
        handles.push(handle);
    }

    // Wait for all tasks to complete
    let mut success_count = 0;
    for handle in handles {
        let result = handle.await.unwrap();
        if result.is_ok() {
            success_count += 1;
        }
    }

    assert_eq!(success_count, 10);
    assert_eq!(processor.processed_count(), 10);
}

#[tokio::test]
async fn test_error_handling_resilience() {
    let processor = TestProcessor::new();
    
    // Test successful processing
    let success_message = MessageBuilder::new()
        .id("success")
        .topic("test_input")
        .payload(b"good_data".to_vec())
        .build().unwrap();
    
    let result = processor.process(&success_message).await;
    assert!(result.is_ok());
    assert_eq!(processor.processed_count(), 1);
    
    // Test failure mode
    processor.set_failure_mode(true).await;
    
    let fail_message = MessageBuilder::new()
        .id("failure")
        .topic("test_input")
        .payload(b"bad_data".to_vec())
        .build().unwrap();
    
    let result = processor.process(&fail_message).await;
    assert!(result.is_err());
    assert_eq!(processor.processed_count(), 1); // Should not increment on failure
    
    // Test recovery
    processor.set_failure_mode(false).await;
    
    let recovery_message = MessageBuilder::new()
        .id("recovery")
        .topic("test_input")
        .payload(b"recovery_data".to_vec())
        .build().unwrap();
    
    let result = processor.process(&recovery_message).await;
    assert!(result.is_ok());
    assert_eq!(processor.processed_count(), 2); // Should increment again
}