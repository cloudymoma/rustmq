use crate::{
    client::RustMqClient,
    error::{ClientError, Result},
    message::Message,
    consumer::{Consumer, ConsumerMessage},
    producer::Producer,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use async_trait::async_trait;

/// High-level streaming interface for real-time message processing
pub struct MessageStream {
    config: StreamConfig,
    #[allow(dead_code)]
    client: RustMqClient,
    consumers: Vec<Consumer>,
    producer: Option<Producer>,
    processor: Arc<dyn MessageProcessor + Send + Sync>,
    metrics: Arc<StreamMetrics>,
    is_running: Arc<RwLock<bool>>,
}

/// Configuration for message streams
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Input topics to consume from
    pub input_topics: Vec<String>,
    
    /// Output topic for processed messages
    pub output_topic: Option<String>,
    
    /// Consumer group for stream processing
    pub consumer_group: String,
    
    /// Processing parallelism level
    pub parallelism: usize,
    
    /// Processing timeout per message
    pub processing_timeout: std::time::Duration,
    
    /// Enable exactly-once processing
    pub exactly_once: bool,
    
    /// Maximum messages in flight
    pub max_in_flight: usize,
    
    /// Error handling strategy
    pub error_strategy: ErrorStrategy,
    
    /// Stream processing mode
    pub mode: StreamMode,
}

/// Stream processing modes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamMode {
    /// Process each message individually
    Individual,
    
    /// Process messages in batches
    Batch { batch_size: usize, batch_timeout: std::time::Duration },
    
    /// Process with windowing
    Windowed { window_size: std::time::Duration, slide_interval: std::time::Duration },
}

/// Error handling strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorStrategy {
    /// Skip failed messages
    Skip,
    
    /// Retry failed messages
    Retry { max_attempts: usize, backoff_ms: u64 },
    
    /// Send failed messages to dead letter queue
    DeadLetter { topic: String },
    
    /// Stop processing on error
    Stop,
}

/// Trait for message processing logic
#[async_trait]
pub trait MessageProcessor {
    /// Process a single message
    async fn process(&self, message: &Message) -> Result<Option<Message>>;
    
    /// Process a batch of messages
    async fn process_batch(&self, messages: &[Message]) -> Result<Vec<Option<Message>>> {
        let mut results = Vec::with_capacity(messages.len());
        for message in messages {
            results.push(self.process(message).await?);
        }
        Ok(results)
    }
    
    /// Called when stream starts
    async fn on_start(&self) -> Result<()> {
        Ok(())
    }
    
    /// Called when stream stops
    async fn on_stop(&self) -> Result<()> {
        Ok(())
    }
    
    /// Called on processing error
    async fn on_error(&self, message: &Message, error: &ClientError) -> Result<()> {
        error!("Processing error for message {}: {}", message.id, error);
        Ok(())
    }
}

/// Stream processing metrics
#[derive(Debug, Default)]
pub struct StreamMetrics {
    pub messages_processed: Arc<std::sync::atomic::AtomicU64>,
    pub messages_failed: Arc<std::sync::atomic::AtomicU64>,
    pub messages_skipped: Arc<std::sync::atomic::AtomicU64>,
    pub processing_rate: Arc<RwLock<f64>>, // messages per second
    pub average_processing_time: Arc<RwLock<f64>>, // milliseconds
    pub error_rate: Arc<RwLock<f64>>,
    pub last_processed_time: Arc<RwLock<Option<std::time::Instant>>>,
}

/// Message processing context
pub struct ProcessingContext {
    pub message: Message,
    pub partition: u32,
    pub offset: u64,
    pub retry_count: usize,
    pub processing_start: std::time::Instant,
}

/// Window for windowed processing
#[derive(Debug)]
struct Window {
    start_time: std::time::Instant,
    end_time: std::time::Instant,
    messages: Vec<(Message, ConsumerMessage)>,
}


impl MessageStream {
    /// Create a new message stream
    pub async fn new(
        client: RustMqClient,
        config: StreamConfig,
    ) -> Result<Self> {
        // Create consumers for input topics
        let mut consumers = Vec::new();
        for topic in &config.input_topics {
            let consumer = client.create_consumer(topic, &config.consumer_group).await?;
            consumers.push(consumer);
        }

        // Create producer for output topic if specified
        let producer = if let Some(ref output_topic) = config.output_topic {
            Some(client.create_producer(output_topic).await?)
        } else {
            None
        };

        Ok(Self {
            config,
            client,
            consumers,
            producer,
            processor: Arc::new(DefaultProcessor),
            metrics: Arc::new(StreamMetrics::default()),
            is_running: Arc::new(RwLock::new(false)),
        })
    }

    /// Set custom message processor
    pub fn with_processor<P: MessageProcessor + Send + Sync + 'static>(mut self, processor: P) -> Self {
        self.processor = Arc::new(processor);
        self
    }

    /// Start stream processing
    pub async fn start(&self) -> Result<()> {
        {
            let mut is_running = self.is_running.write().await;
            if *is_running {
                return Err(ClientError::InvalidOperation("Stream is already running".to_string()));
            }
            *is_running = true;
        }

        self.processor.on_start().await?;

        // Start processing tasks based on mode
        match &self.config.mode {
            StreamMode::Individual => {
                self.start_individual_processing().await?;
            }
            StreamMode::Batch { batch_size, batch_timeout } => {
                self.start_batch_processing(*batch_size, *batch_timeout).await?;
            }
            StreamMode::Windowed { window_size, slide_interval } => {
                self.start_windowed_processing(*window_size, *slide_interval).await?;
            }
        }

        info!("Message stream started with {} consumers", self.consumers.len());
        Ok(())
    }

    /// Stop stream processing
    pub async fn stop(&self) -> Result<()> {
        {
            let mut is_running = self.is_running.write().await;
            if !*is_running {
                return Ok(());
            }
            *is_running = false;
        }

        self.processor.on_stop().await?;

        // Close consumers
        for consumer in &self.consumers {
            consumer.close().await?;
        }

        // Close producer if exists
        if let Some(ref producer) = self.producer {
            producer.close().await?;
        }

        info!("Message stream stopped");
        Ok(())
    }

    /// Start individual message processing
    async fn start_individual_processing(&self) -> Result<()> {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.config.max_in_flight));
        
        for consumer in &self.consumers {
            let consumer = consumer.clone();
            let processor = self.processor.clone();
            let producer = self.producer.clone();
            let config = self.config.clone();
            let metrics = self.metrics.clone();
            let is_running = self.is_running.clone();
            let semaphore = semaphore.clone();

            tokio::spawn(async move {
                while *is_running.read().await {
                    match consumer.receive().await {
                        Ok(Some(consumer_message)) => {
                            let _permit = semaphore.acquire().await.unwrap();
                            
                            let processor = processor.clone();
                            let producer = producer.clone();
                            let config = config.clone();
                            let metrics = metrics.clone();
                            let message = consumer_message.message.clone();

                            tokio::spawn(async move {
                                let start_time = std::time::Instant::now();
                                
                                match Self::process_single_message(
                                    &processor,
                                    &producer,
                                    &config,
                                    message,
                                ).await {
                                    Ok(_) => {
                                        consumer_message.ack().await.ok();
                                        Self::update_success_metrics(&metrics, start_time).await;
                                    }
                                    Err(e) => {
                                        Self::handle_processing_error(
                                            &consumer_message,
                                            &config,
                                            &metrics,
                                            e,
                                        ).await;
                                    }
                                }
                            });
                        }
                        Ok(None) => break, // Consumer closed
                        Err(e) => {
                            error!("Error receiving message: {}", e);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Start batch processing
    async fn start_batch_processing(
        &self,
        batch_size: usize,
        batch_timeout: std::time::Duration,
    ) -> Result<()> {
        for consumer in &self.consumers {
            let consumer = consumer.clone();
            let processor = self.processor.clone();
            let producer = self.producer.clone();
            let config = self.config.clone();
            let metrics = self.metrics.clone();
            let is_running = self.is_running.clone();

            tokio::spawn(async move {
                let mut batch = Vec::with_capacity(batch_size);
                let mut last_batch_time = std::time::Instant::now();
                let _pending_messages: Vec<Message> = Vec::new();

                while *is_running.read().await {
                    let should_process_batch = batch.len() >= batch_size ||
                        (last_batch_time.elapsed() >= batch_timeout && !batch.is_empty());

                    if should_process_batch {
                        let batch_start_time = std::time::Instant::now();
                        
                        let messages: Vec<Message> = batch.iter().map(|(msg, _): &(Message, ConsumerMessage)| msg.clone()).collect();
                        match processor.process_batch(&messages).await {
                            Ok(results) => {
                                // Send results to output topic if specified
                                if let Some(producer) = &producer {
                                    for result in results {
                                        if let Some(output_message) = result {
                                            if let Err(e) = producer.send(output_message).await {
                                                error!("Failed to send batch result: {}", e);
                                            }
                                        }
                                    }
                                }

                                // Acknowledge all messages in the batch
                                for (_, consumer_message) in &batch {
                                    consumer_message.ack().await.ok();
                                }

                                Self::update_batch_success_metrics(&metrics, batch.len(), batch_start_time).await;
                            }
                            Err(e) => {
                                // Handle batch processing error
                                for (_message, consumer_message) in &batch {
                                    Self::handle_processing_error(
                                        consumer_message,
                                        &config,
                                        &metrics,
                                        e.clone(),
                                    ).await;
                                }
                            }
                        }

                        batch.clear();
                        last_batch_time = std::time::Instant::now();
                    }

                    // Try to receive new message with timeout
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(100),
                        consumer.receive()
                    ).await {
                        Ok(Ok(Some(consumer_message))) => {
                            batch.push((consumer_message.message.clone(), consumer_message));
                        }
                        Ok(Ok(None)) => break, // Consumer closed
                        Ok(Err(e)) => {
                            error!("Error receiving message: {}", e);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        Err(_) => {
                            // Timeout - continue to check if batch should be processed
                        }
                    }
                }

                // Process any remaining messages in the batch before shutting down
                if !batch.is_empty() {
                    let messages: Vec<Message> = batch.iter().map(|(msg, _)| msg.clone()).collect();
                    if let Ok(results) = processor.process_batch(&messages).await {
                        if let Some(producer) = &producer {
                            for result in results {
                                if let Some(output_message) = result {
                                    producer.send(output_message).await.ok();
                                }
                            }
                        }
                        for (_, consumer_message) in &batch {
                            consumer_message.ack().await.ok();
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Start windowed processing
    async fn start_windowed_processing(
        &self,
        window_size: std::time::Duration,
        slide_interval: std::time::Duration,
    ) -> Result<()> {
        for consumer in &self.consumers {
            let consumer = consumer.clone();
            let processor = self.processor.clone();
            let producer = self.producer.clone();
            let config = self.config.clone();
            let metrics = self.metrics.clone();
            let is_running = self.is_running.clone();

            tokio::spawn(async move {
                let mut windows: std::collections::VecDeque<Window> = std::collections::VecDeque::new();
                let mut last_slide = std::time::Instant::now();

                while *is_running.read().await {
                    let now = std::time::Instant::now();
                    
                    // Create new window if it's time to slide
                    if last_slide.elapsed() >= slide_interval {
                        let window_start = now;
                        let window_end = window_start + window_size;
                        windows.push_back(Window {
                            start_time: window_start,
                            end_time: window_end,
                            messages: Vec::new(),
                        });
                        last_slide = now;
                    }

                    // Remove expired windows and process them
                    while let Some(window) = windows.front() {
                        if now >= window.end_time {
                            let completed_window = windows.pop_front().unwrap();
                            if !completed_window.messages.is_empty() {
                                let window_start_time = std::time::Instant::now();
                                let messages: Vec<Message> = completed_window.messages.iter()
                                    .map(|(msg, _): &(Message, ConsumerMessage)| msg.clone()).collect();
                                
                                match processor.process_batch(&messages).await {
                                    Ok(results) => {
                                        // Send results to output topic if specified
                                        if let Some(producer) = &producer {
                                            for result in results {
                                                if let Some(output_message) = result {
                                                    if let Err(e) = producer.send(output_message).await {
                                                        error!("Failed to send windowed result: {}", e);
                                                    }
                                                }
                                            }
                                        }

                                        // Acknowledge all messages in the window
                                        for (_, consumer_message) in &completed_window.messages {
                                            consumer_message.ack().await.ok();
                                        }

                                        Self::update_batch_success_metrics(&metrics, completed_window.messages.len(), window_start_time).await;
                                    }
                                    Err(e) => {
                                        // Handle window processing error
                                        for (_message, consumer_message) in &completed_window.messages {
                                            Self::handle_processing_error(
                                                consumer_message,
                                                &config,
                                                &metrics,
                                                e.clone(),
                                            ).await;
                                        }
                                    }
                                }
                            }
                        } else {
                            break;
                        }
                    }

                    // Try to receive new message with timeout
                    match tokio::time::timeout(
                        std::time::Duration::from_millis(100),
                        consumer.receive()
                    ).await {
                        Ok(Ok(Some(consumer_message))) => {
                            let message_time = std::time::Instant::now();
                            
                            // Add message to all active windows that should contain it
                            for window in &mut windows {
                                if message_time >= window.start_time && message_time < window.end_time {
                                    window.messages.push((consumer_message.message.clone(), consumer_message.clone()));
                                }
                            }
                        }
                        Ok(Ok(None)) => break, // Consumer closed
                        Ok(Err(e)) => {
                            error!("Error receiving message: {}", e);
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        Err(_) => {
                            // Timeout - continue to check for expired windows
                        }
                    }
                }

                // Process any remaining windows before shutting down
                for window in windows {
                    if !window.messages.is_empty() {
                        let messages: Vec<Message> = window.messages.iter()
                            .map(|(msg, _): &(Message, ConsumerMessage)| msg.clone()).collect();
                        if let Ok(results) = processor.process_batch(&messages).await {
                            if let Some(producer) = &producer {
                                for result in results {
                                    if let Some(output_message) = result {
                                        producer.send(output_message).await.ok();
                                    }
                                }
                            }
                            for (_, consumer_message) in &window.messages {
                                consumer_message.ack().await.ok();
                            }
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Process a single message
    async fn process_single_message(
        processor: &Arc<dyn MessageProcessor + Send + Sync>,
        producer: &Option<Producer>,
        config: &StreamConfig,
        message: Message,
    ) -> Result<()> {
        let result = tokio::time::timeout(
            config.processing_timeout,
            processor.process(&message),
        ).await??;

        // Send result to output topic if specified
        if let (Some(output_message), Some(producer)) = (result, producer) {
            producer.send(output_message).await?;
        }

        Ok(())
    }

    /// Handle processing errors
    async fn handle_processing_error(
        consumer_message: &ConsumerMessage,
        config: &StreamConfig,
        metrics: &StreamMetrics,
        error: ClientError,
    ) {
        use std::sync::atomic::Ordering;
        
        metrics.messages_failed.fetch_add(1, Ordering::Relaxed);

        match &config.error_strategy {
            ErrorStrategy::Skip => {
                error!("Skipping failed message: {}", error);
                consumer_message.ack().await.ok();
                metrics.messages_skipped.fetch_add(1, Ordering::Relaxed);
            }
            ErrorStrategy::Retry { max_attempts: _, backoff_ms: _ } => {
                // For now, just nack and let the consumer handle retry
                // In a full implementation, we'd track retry counts per message
                error!("Message processing failed, will retry: {}", error);
                consumer_message.nack().await.ok();
            }
            ErrorStrategy::DeadLetter { topic } => {
                error!("Sending message to dead letter queue {}: {}", topic, error);
                // In a full implementation, we'd send to the dead letter topic
                consumer_message.ack().await.ok();
            }
            ErrorStrategy::Stop => {
                error!("Stopping stream due to processing error: {}", error);
                consumer_message.nack().await.ok();
                // The stream should be stopped from the calling context
            }
        }
    }

    /// Handle processing errors with basic retry logic
    async fn handle_processing_error_with_retry(
        message: Message,
        consumer_message: ConsumerMessage,
        config: StreamConfig,
        error: ClientError,
        producer: Option<Producer>,
    ) {
        match &config.error_strategy {
            ErrorStrategy::Skip => {
                error!("Skipping failed message {}: {}", message.id, error);
                consumer_message.ack().await.ok();
            }
            ErrorStrategy::Retry { max_attempts: _, backoff_ms: _ } => {
                // Simplified retry - just nack and let the consumer handle it
                error!("Message {} failed, nacking for retry: {}", message.id, error);
                consumer_message.nack().await.ok();
            }
            ErrorStrategy::DeadLetter { topic } => {
                error!("Sending message {} to dead letter queue {}: {}", message.id, topic, error);
                
                // Create dead letter message
                if let Some(producer) = producer {
                    let mut dead_letter_message = message.clone();
                    dead_letter_message.headers.insert("original_topic".to_string(), 
                        consumer_message.message.headers.get("topic").cloned().unwrap_or_default());
                    dead_letter_message.headers.insert("error".to_string(), error.to_string());
                    dead_letter_message.headers.insert("failed_at".to_string(), 
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default().as_secs().to_string());
                    
                    // Try to send to dead letter topic
                    if let Err(e) = producer.send(dead_letter_message).await {
                        error!("Failed to send message to dead letter queue: {}", e);
                    }
                }
                
                consumer_message.ack().await.ok();
            }
            ErrorStrategy::Stop => {
                error!("Stopping stream due to processing error: {}", error);
                consumer_message.nack().await.ok();
                // The stream should be stopped from the calling context
            }
        }
    }

    /// Update success metrics
    async fn update_success_metrics(
        metrics: &StreamMetrics,
        start_time: std::time::Instant,
    ) {
        use std::sync::atomic::Ordering;
        
        metrics.messages_processed.fetch_add(1, Ordering::Relaxed);
        
        let processing_time = start_time.elapsed().as_millis() as f64;
        {
            let mut avg_time = metrics.average_processing_time.write().await;
            let count = metrics.messages_processed.load(Ordering::Relaxed) as f64;
            *avg_time = (*avg_time * (count - 1.0) + processing_time) / count;
        }
        
        {
            let mut last_processed = metrics.last_processed_time.write().await;
            *last_processed = Some(std::time::Instant::now());
        }
    }

    /// Update success metrics for batch processing
    async fn update_batch_success_metrics(
        metrics: &StreamMetrics,
        batch_size: usize,
        start_time: std::time::Instant,
    ) {
        use std::sync::atomic::Ordering;
        
        metrics.messages_processed.fetch_add(batch_size as u64, Ordering::Relaxed);
        
        let processing_time = start_time.elapsed().as_millis() as f64;
        {
            let mut avg_time = metrics.average_processing_time.write().await;
            let count = metrics.messages_processed.load(Ordering::Relaxed) as f64;
            *avg_time = (*avg_time * (count - batch_size as f64) + processing_time) / count;
        }
        
        {
            let mut last_processed = metrics.last_processed_time.write().await;
            *last_processed = Some(std::time::Instant::now());
        }
    }

    /// Get stream metrics
    pub async fn metrics(&self) -> StreamMetrics {
        StreamMetrics {
            messages_processed: self.metrics.messages_processed.clone(),
            messages_failed: self.metrics.messages_failed.clone(),
            messages_skipped: self.metrics.messages_skipped.clone(),
            processing_rate: self.metrics.processing_rate.clone(),
            average_processing_time: self.metrics.average_processing_time.clone(),
            error_rate: self.metrics.error_rate.clone(),
            last_processed_time: self.metrics.last_processed_time.clone(),
        }
    }

    /// Check if stream is running
    pub async fn is_running(&self) -> bool {
        *self.is_running.read().await
    }
}

/// Default no-op message processor
struct DefaultProcessor;

#[async_trait]
impl MessageProcessor for DefaultProcessor {
    async fn process(&self, message: &Message) -> Result<Option<Message>> {
        // Default: pass through the message unchanged
        Ok(Some(message.clone()))
    }
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            input_topics: vec!["input".to_string()],
            output_topic: Some("output".to_string()),
            consumer_group: "stream-processor".to_string(),
            parallelism: 1,
            processing_timeout: std::time::Duration::from_secs(30),
            exactly_once: false,
            max_in_flight: 100,
            error_strategy: ErrorStrategy::Skip,
            mode: StreamMode::Individual,
        }
    }
}