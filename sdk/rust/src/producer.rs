use crate::{
    client::RustMqClient,
    config::{ProducerConfig, AckLevel},
    error::{ClientError, Result},
    message::{Message, MessageBatch},
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// High-level producer for sending messages to RustMQ
#[derive(Clone)]
pub struct Producer {
    id: String,
    topic: String,
    config: Arc<ProducerConfig>,
    client: RustMqClient,
    batch_sender: mpsc::UnboundedSender<BatchedMessage>,
    sequence_counter: Arc<RwLock<u64>>,
    metrics: Arc<ProducerMetrics>,
}

/// Builder for creating producers
pub struct ProducerBuilder {
    topic: Option<String>,
    config: Option<ProducerConfig>,
    client: Option<RustMqClient>,
}

/// Internal message with completion callback
struct BatchedMessage {
    message: Message,
    result_sender: tokio::sync::oneshot::Sender<Result<MessageResult>>,
}

/// Result of sending a message
#[derive(Debug, Clone)]
pub struct MessageResult {
    pub message_id: String,
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: u64,
}

/// Producer performance metrics
#[derive(Debug, Default)]
pub struct ProducerMetrics {
    pub messages_sent: Arc<std::sync::atomic::AtomicU64>,
    pub messages_failed: Arc<std::sync::atomic::AtomicU64>,
    pub bytes_sent: Arc<std::sync::atomic::AtomicU64>,
    pub batches_sent: Arc<std::sync::atomic::AtomicU64>,
    pub average_batch_size: Arc<RwLock<f64>>,
    pub last_send_time: Arc<RwLock<Option<Instant>>>,
}

impl ProducerBuilder {
    /// Create a new producer builder
    pub fn new() -> Self {
        Self {
            topic: None,
            config: None,
            client: None,
        }
    }

    /// Set topic for the producer
    pub fn topic<T: Into<String>>(mut self, topic: T) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set producer configuration
    pub fn config(mut self, config: ProducerConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set client instance
    pub fn client(mut self, client: RustMqClient) -> Self {
        self.client = Some(client);
        self
    }

    /// Build the producer
    pub async fn build(self) -> Result<Producer> {
        let topic = self.topic.ok_or_else(|| {
            ClientError::InvalidConfig("Producer topic is required".to_string())
        })?;
        
        let client = self.client.ok_or_else(|| {
            ClientError::InvalidConfig("Client is required".to_string())
        })?;
        
        let config = Arc::new(self.config.unwrap_or_default());
        let id = config.producer_id.clone()
            .unwrap_or_else(|| format!("producer-{}", Uuid::new_v4()));

        // Create batching channel
        let (batch_sender, batch_receiver) = mpsc::unbounded_channel();
        
        let producer = Producer {
            id: id.clone(),
            topic: topic.clone(),
            config: config.clone(),
            client: client.clone(),
            batch_sender,
            sequence_counter: Arc::new(RwLock::new(0)),
            metrics: Arc::new(ProducerMetrics::default()),
        };

        // Start background batching task
        let batching_producer = producer.clone();
        tokio::spawn(async move {
            batching_producer.run_batching_loop(batch_receiver).await;
        });

        info!("Created producer {} for topic {}", id, topic);
        Ok(producer)
    }
}

impl Producer {
    /// Send a message asynchronously
    pub async fn send(&self, message: Message) -> Result<MessageResult> {
        let (result_sender, result_receiver) = tokio::sync::oneshot::channel();
        
        // Prepare message with sequence number
        let mut message = message;
        message.producer_id = Some(self.id.clone());
        
        {
            let mut sequence = self.sequence_counter.write().await;
            *sequence += 1;
            message.sequence = Some(*sequence);
        }

        // Send to batching queue
        let batched_message = BatchedMessage {
            message,
            result_sender,
        };

        self.batch_sender.send(batched_message)
            .map_err(|_| ClientError::Producer("Producer is closed".to_string()))?;

        // Wait for result
        result_receiver.await
            .map_err(|_| ClientError::Producer("Failed to receive send result".to_string()))?
    }

    /// Send a message with fire-and-forget semantics
    pub async fn send_async(&self, message: Message) -> Result<()> {
        let (result_sender, _) = tokio::sync::oneshot::channel();
        
        let mut message = message;
        message.producer_id = Some(self.id.clone());
        
        {
            let mut sequence = self.sequence_counter.write().await;
            *sequence += 1;
            message.sequence = Some(*sequence);
        }

        let batched_message = BatchedMessage {
            message,
            result_sender,
        };

        self.batch_sender.send(batched_message)
            .map_err(|_| ClientError::Producer("Producer is closed".to_string()))?;

        Ok(())
    }

    /// Send multiple messages in a batch
    pub async fn send_batch(&self, messages: Vec<Message>) -> Result<Vec<MessageResult>> {
        let mut results = Vec::with_capacity(messages.len());
        
        for message in messages {
            let result = self.send(message).await?;
            results.push(result);
        }
        
        Ok(results)
    }

    /// Flush any pending messages
    pub async fn flush(&self) -> Result<()> {
        // TODO: Implement flush mechanism
        todo!("Implement producer flush")
    }

    /// Close the producer
    pub async fn close(&self) -> Result<()> {
        // Flush remaining messages
        self.flush().await?;
        
        info!("Producer {} closed", self.id);
        Ok(())
    }

    /// Get producer metrics
    pub async fn metrics(&self) -> ProducerMetrics {
        ProducerMetrics {
            messages_sent: self.metrics.messages_sent.clone(),
            messages_failed: self.metrics.messages_failed.clone(),
            bytes_sent: self.metrics.bytes_sent.clone(),
            batches_sent: self.metrics.batches_sent.clone(),
            average_batch_size: self.metrics.average_batch_size.clone(),
            last_send_time: self.metrics.last_send_time.clone(),
        }
    }

    /// Background task for batching messages
    async fn run_batching_loop(&self, mut receiver: mpsc::UnboundedReceiver<BatchedMessage>) {
        let mut pending_messages = Vec::new();
        let mut batch_timer = tokio::time::interval(self.config.batch_timeout);
        
        loop {
            tokio::select! {
                // Receive new message
                message = receiver.recv() => {
                    match message {
                        Some(batched_message) => {
                            pending_messages.push(batched_message);
                            
                            // Check if batch is full
                            if pending_messages.len() >= self.config.batch_size {
                                self.send_batch_internal(&mut pending_messages).await;
                            }
                        }
                        None => {
                            // Channel closed, send remaining messages
                            if !pending_messages.is_empty() {
                                self.send_batch_internal(&mut pending_messages).await;
                            }
                            break;
                        }
                    }
                }
                
                // Batch timeout
                _ = batch_timer.tick() => {
                    if !pending_messages.is_empty() {
                        self.send_batch_internal(&mut pending_messages).await;
                    }
                }
            }
        }
    }

    /// Send a batch of messages to the broker
    async fn send_batch_internal(&self, messages: &mut Vec<BatchedMessage>) {
        if messages.is_empty() {
            return;
        }

        let batch_size = messages.len();
        let batch_id = Uuid::new_v4().to_string();
        
        debug!("Sending batch {} with {} messages", batch_id, batch_size);

        // TODO: Implement actual message sending to broker
        let results: Vec<Result<MessageResult>> = messages.iter().map(|batched_message| {
            // Placeholder: simulate successful send
            Ok(MessageResult {
                message_id: batched_message.message.id.clone(),
                topic: batched_message.message.topic.clone(),
                partition: batched_message.message.partition,
                offset: 0, // Would be assigned by broker
                timestamp: batched_message.message.timestamp,
            })
        }).collect();

        // Send results back to callers
        for (batched_message, result) in messages.drain(..).zip(results) {
            let _ = batched_message.result_sender.send(result);
        }

        // Update metrics
        self.update_metrics(batch_size).await;
    }

    /// Update producer metrics
    async fn update_metrics(&self, batch_size: usize) {
        use std::sync::atomic::Ordering;
        
        self.metrics.messages_sent.fetch_add(batch_size as u64, Ordering::Relaxed);
        self.metrics.batches_sent.fetch_add(1, Ordering::Relaxed);
        
        // Update average batch size
        {
            let mut avg = self.metrics.average_batch_size.write().await;
            let total_batches = self.metrics.batches_sent.load(Ordering::Relaxed) as f64;
            let total_messages = self.metrics.messages_sent.load(Ordering::Relaxed) as f64;
            *avg = if total_batches > 0.0 { total_messages / total_batches } else { 0.0 };
        }
        
        // Update last send time
        {
            let mut last_send = self.metrics.last_send_time.write().await;
            *last_send = Some(Instant::now());
        }
    }

    /// Get producer ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get topic
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Get configuration
    pub fn config(&self) -> &ProducerConfig {
        &self.config
    }
}

impl Default for ProducerBuilder {
    fn default() -> Self {
        Self::new()
    }
}