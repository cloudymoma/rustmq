use crate::{
    client::RustMqClient,
    error::{ClientError, Result},
    message::Message,
    consumer::{Consumer, ConsumerMessage},
    producer::Producer,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use futures::{Stream, StreamExt};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};
use async_trait::async_trait;

/// High-level streaming interface for real-time message processing
pub struct MessageStream {
    config: StreamConfig,
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
        // TODO: Implement batch processing
        todo!("Implement batch processing")
    }

    /// Start windowed processing
    async fn start_windowed_processing(
        &self,
        window_size: std::time::Duration,
        slide_interval: std::time::Duration,
    ) -> Result<()> {
        // TODO: Implement windowed processing
        todo!("Implement windowed processing")
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
                consumer_message.ack().await.ok();
                metrics.messages_skipped.fetch_add(1, Ordering::Relaxed);
            }
            ErrorStrategy::Retry { max_attempts, backoff_ms } => {
                // TODO: Implement retry logic
                consumer_message.nack().await.ok();
            }
            ErrorStrategy::DeadLetter { topic } => {
                // TODO: Send to dead letter queue
                consumer_message.ack().await.ok();
            }
            ErrorStrategy::Stop => {
                // TODO: Stop the stream
                consumer_message.nack().await.ok();
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