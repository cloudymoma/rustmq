use crate::{
    client::RustMqClient,
    config::{ConsumerConfig, StartPosition},
    error::{ClientError, Result},
    message::Message,
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock, Mutex};
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

/// High-level consumer for receiving messages from RustMQ
#[derive(Clone)]
pub struct Consumer {
    id: String,
    topic: String,
    consumer_group: String,
    config: Arc<ConsumerConfig>,
    client: RustMqClient,
    message_receiver: Arc<Mutex<Option<mpsc::UnboundedReceiver<Message>>>>,
    offset_tracker: Arc<RwLock<OffsetTracker>>,
    metrics: Arc<ConsumerMetrics>,
    is_closed: Arc<RwLock<bool>>,
}

/// Builder for creating consumers
pub struct ConsumerBuilder {
    topic: Option<String>,
    consumer_group: Option<String>,
    config: Option<ConsumerConfig>,
    client: Option<RustMqClient>,
}

/// Tracks message offsets for acknowledgment
#[derive(Debug, Default)]
struct OffsetTracker {
    committed_offset: u64,
    pending_offsets: std::collections::BTreeSet<u64>,
    last_commit_time: Option<Instant>,
}

/// Consumer performance metrics
#[derive(Debug, Default)]
pub struct ConsumerMetrics {
    pub messages_received: Arc<std::sync::atomic::AtomicU64>,
    pub messages_processed: Arc<std::sync::atomic::AtomicU64>,
    pub messages_failed: Arc<std::sync::atomic::AtomicU64>,
    pub bytes_received: Arc<std::sync::atomic::AtomicU64>,
    pub lag: Arc<RwLock<u64>>,
    pub last_receive_time: Arc<RwLock<Option<Instant>>>,
    pub processing_time_ms: Arc<RwLock<f64>>,
}

/// Consumer message with acknowledgment capability
pub struct ConsumerMessage {
    pub message: Message,
    ack_sender: mpsc::UnboundedSender<AckRequest>,
}

/// Acknowledgment request
struct AckRequest {
    offset: u64,
    success: bool,
}

impl ConsumerBuilder {
    /// Create a new consumer builder
    pub fn new() -> Self {
        Self {
            topic: None,
            consumer_group: None,
            config: None,
            client: None,
        }
    }

    /// Set topic for the consumer
    pub fn topic<T: Into<String>>(mut self, topic: T) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set consumer group
    pub fn consumer_group<T: Into<String>>(mut self, consumer_group: T) -> Self {
        self.consumer_group = Some(consumer_group.into());
        self
    }

    /// Set consumer configuration
    pub fn config(mut self, config: ConsumerConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set client instance
    pub fn client(mut self, client: RustMqClient) -> Self {
        self.client = Some(client);
        self
    }

    /// Build the consumer
    pub async fn build(self) -> Result<Consumer> {
        let topic = self.topic.ok_or_else(|| {
            ClientError::InvalidConfig("Consumer topic is required".to_string())
        })?;
        
        let consumer_group = self.consumer_group.ok_or_else(|| {
            ClientError::InvalidConfig("Consumer group is required".to_string())
        })?;
        
        let client = self.client.ok_or_else(|| {
            ClientError::InvalidConfig("Client is required".to_string())
        })?;
        
        let config = Arc::new(self.config.unwrap_or_default());
        let id = config.consumer_id.clone()
            .unwrap_or_else(|| format!("consumer-{}", Uuid::new_v4()));

        // Create message channel
        let (message_sender, message_receiver) = mpsc::unbounded_channel();
        
        let consumer = Consumer {
            id: id.clone(),
            topic: topic.clone(),
            consumer_group: consumer_group.clone(),
            config: config.clone(),
            client: client.clone(),
            message_receiver: Arc::new(Mutex::new(Some(message_receiver))),
            offset_tracker: Arc::new(RwLock::new(OffsetTracker::default())),
            metrics: Arc::new(ConsumerMetrics::default()),
            is_closed: Arc::new(RwLock::new(false)),
        };

        // Start background consumption task
        let consumption_consumer = consumer.clone();
        tokio::spawn(async move {
            consumption_consumer.run_consumption_loop(message_sender).await;
        });

        // Start auto-commit task if enabled
        if config.enable_auto_commit {
            let commit_consumer = consumer.clone();
            tokio::spawn(async move {
                commit_consumer.run_auto_commit_loop().await;
            });
        }

        info!("Created consumer {} for topic {} in group {}", id, topic, consumer_group);
        Ok(consumer)
    }
}

impl Consumer {
    /// Receive the next message
    pub async fn receive(&self) -> Result<Option<ConsumerMessage>> {
        let mut receiver_guard = self.message_receiver.lock().await;
        if let Some(ref mut receiver) = *receiver_guard {
            match receiver.recv().await {
                Some(message) => {
                    self.update_receive_metrics(&message).await;
                    
                    // Create acknowledgment channel
                    let (ack_sender, ack_receiver) = mpsc::unbounded_channel();
                    
                    // Start acknowledgment handling
                    let ack_consumer = self.clone();
                    tokio::spawn(async move {
                        ack_consumer.handle_ack_requests(ack_receiver).await;
                    });

                    Ok(Some(ConsumerMessage {
                        message,
                        ack_sender,
                    }))
                }
                None => Ok(None), // Consumer closed
            }
        } else {
            Err(ClientError::Consumer("Consumer is closed".to_string()))
        }
    }

    /// Create a stream of messages
    pub fn stream(&self) -> impl Stream<Item = Result<ConsumerMessage>> + '_ {
        ConsumerStream::new(self)
    }

    /// Commit current offset manually
    pub async fn commit(&self) -> Result<()> {
        let offset_tracker = self.offset_tracker.read().await;
        let offset = offset_tracker.committed_offset;
        
        // TODO: Implement actual offset commit to broker
        debug!("Committing offset {} for consumer {}", offset, self.id);
        
        Ok(())
    }

    /// Seek to specific offset
    pub async fn seek(&self, offset: u64) -> Result<()> {
        // TODO: Implement seek functionality
        todo!("Implement consumer seek")
    }

    /// Seek to specific timestamp
    pub async fn seek_to_timestamp(&self, timestamp: u64) -> Result<()> {
        // TODO: Implement timestamp-based seek
        todo!("Implement timestamp seek")
    }

    /// Close the consumer
    pub async fn close(&self) -> Result<()> {
        let mut is_closed = self.is_closed.write().await;
        if *is_closed {
            return Ok(());
        }
        
        *is_closed = true;
        
        // Commit final offsets
        self.commit().await?;
        
        // Close message receiver
        let mut receiver_guard = self.message_receiver.lock().await;
        *receiver_guard = None;
        
        info!("Consumer {} closed", self.id);
        Ok(())
    }

    /// Get consumer metrics
    pub async fn metrics(&self) -> ConsumerMetrics {
        ConsumerMetrics {
            messages_received: self.metrics.messages_received.clone(),
            messages_processed: self.metrics.messages_processed.clone(),
            messages_failed: self.metrics.messages_failed.clone(),
            bytes_received: self.metrics.bytes_received.clone(),
            lag: self.metrics.lag.clone(),
            last_receive_time: self.metrics.last_receive_time.clone(),
            processing_time_ms: self.metrics.processing_time_ms.clone(),
        }
    }

    /// Background task for consuming messages
    async fn run_consumption_loop(&self, sender: mpsc::UnboundedSender<Message>) {
        let mut fetch_interval = tokio::time::interval(Duration::from_millis(100));
        
        loop {
            // Check if consumer is closed
            {
                let is_closed = self.is_closed.read().await;
                if *is_closed {
                    break;
                }
            }
            
            // TODO: Implement actual message fetching from broker
            // For now, simulate receiving messages
            fetch_interval.tick().await;
            
            // Simulate message fetching
            // In real implementation, this would fetch from broker
        }
    }

    /// Background task for auto-committing offsets
    async fn run_auto_commit_loop(&self) {
        let mut commit_interval = tokio::time::interval(self.config.auto_commit_interval);
        
        loop {
            commit_interval.tick().await;
            
            // Check if consumer is closed
            {
                let is_closed = self.is_closed.read().await;
                if *is_closed {
                    break;
                }
            }
            
            if let Err(e) = self.commit().await {
                warn!("Auto-commit failed for consumer {}: {}", self.id, e);
            }
        }
    }

    /// Handle acknowledgment requests
    async fn handle_ack_requests(&self, mut receiver: mpsc::UnboundedReceiver<AckRequest>) {
        while let Some(ack_request) = receiver.recv().await {
            let mut offset_tracker = self.offset_tracker.write().await;
            
            if ack_request.success {
                offset_tracker.pending_offsets.remove(&ack_request.offset);
                
                // Update committed offset to highest consecutive offset
                while let Some(&next_offset) = offset_tracker.pending_offsets.first() {
                    if next_offset == offset_tracker.committed_offset + 1 {
                        offset_tracker.committed_offset = next_offset;
                        offset_tracker.pending_offsets.remove(&next_offset);
                    } else {
                        break;
                    }
                }
                
                self.metrics.messages_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                self.metrics.messages_failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                
                // TODO: Handle failed message (retry, dead letter queue, etc.)
            }
        }
    }

    /// Update metrics when receiving a message
    async fn update_receive_metrics(&self, message: &Message) {
        use std::sync::atomic::Ordering;
        
        self.metrics.messages_received.fetch_add(1, Ordering::Relaxed);
        self.metrics.bytes_received.fetch_add(message.size as u64, Ordering::Relaxed);
        
        {
            let mut last_receive = self.metrics.last_receive_time.write().await;
            *last_receive = Some(Instant::now());
        }
        
        // Calculate lag (simplified)
        {
            let mut lag = self.metrics.lag.write().await;
            *lag = message.age_ms();
        }
    }

    /// Get consumer ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get topic
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Get consumer group
    pub fn consumer_group(&self) -> &str {
        &self.consumer_group
    }

    /// Get configuration
    pub fn config(&self) -> &ConsumerConfig {
        &self.config
    }
}

impl ConsumerMessage {
    /// Acknowledge successful processing
    pub async fn ack(&self) -> Result<()> {
        let ack_request = AckRequest {
            offset: self.message.offset,
            success: true,
        };
        
        self.ack_sender.send(ack_request)
            .map_err(|_| ClientError::Consumer("Failed to send acknowledgment".to_string()))?;
        
        Ok(())
    }

    /// Negative acknowledge (message processing failed)
    pub async fn nack(&self) -> Result<()> {
        let ack_request = AckRequest {
            offset: self.message.offset,
            success: false,
        };
        
        self.ack_sender.send(ack_request)
            .map_err(|_| ClientError::Consumer("Failed to send negative acknowledgment".to_string()))?;
        
        Ok(())
    }
}

/// Stream implementation for consumer
struct ConsumerStream<'a> {
    consumer: &'a Consumer,
}

impl<'a> ConsumerStream<'a> {
    fn new(consumer: &'a Consumer) -> Self {
        Self { consumer }
    }
}

impl<'a> Stream for ConsumerStream<'a> {
    type Item = Result<ConsumerMessage>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // TODO: Implement proper async stream polling
        // This is a simplified implementation
        Poll::Pending
    }
}

impl Default for ConsumerBuilder {
    fn default() -> Self {
        Self::new()
    }
}