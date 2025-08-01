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
use serde::{Serialize, Deserialize};
use std::collections::{HashMap, VecDeque, BTreeSet};
use std::sync::atomic::Ordering;

/// High-level consumer for receiving messages from RustMQ
#[derive(Clone, Debug)]
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
    failed_messages: Arc<Mutex<VecDeque<FailedMessage>>>,
    // Multi-partition state management
    partition_assignment: Arc<RwLock<Option<PartitionAssignment>>>,
    partition_states: Arc<RwLock<HashMap<u32, PartitionState>>>,
    paused_partitions: Arc<RwLock<Vec<u32>>>,
}

/// Builder for creating consumers
pub struct ConsumerBuilder {
    topic: Option<String>,
    consumer_group: Option<String>,
    config: Option<ConsumerConfig>,
    client: Option<RustMqClient>,
}

/// Tracks message offsets for acknowledgment across multiple partitions
#[derive(Debug, Default)]
pub struct OffsetTracker {
    // Per-partition committed offsets
    committed_offsets: HashMap<u32, u64>,
    // Per-partition pending offsets (messages received but not yet acknowledged)
    pending_offsets: HashMap<u32, BTreeSet<u64>>,
    // Per-partition acknowledged offsets (messages that have been processed)
    acknowledged_offsets: HashMap<u32, BTreeSet<u64>>,
    // Last commit time per partition
    last_commit_times: HashMap<u32, Instant>,
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
#[derive(Debug, Clone)]
pub struct ConsumerMessage {
    pub message: Message,
    ack_sender: mpsc::UnboundedSender<AckRequest>,
}

/// Acknowledgment request
struct AckRequest {
    offset: u64,
    partition: u32,
    success: bool,
}

/// Consumer protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsumerRequest {
    /// Subscribe to topic with consumer group
    Subscribe {
        topic: String,
        consumer_group: String,
        consumer_id: String,
        start_position: StartPosition,
    },
    /// Fetch messages from broker
    Fetch {
        topic: String,
        partition: u32,
        offset: u64,
        max_messages: usize,
        timeout_ms: u64,
    },
    /// Commit offsets to broker (supports multiple partitions)
    CommitOffsets {
        topic: String,
        consumer_group: String,
        offsets: HashMap<u32, u64>, // partition -> offset
    },
    /// Seek to specific offset
    Seek {
        topic: String,
        partition: u32,
        offset: u64,
    },
    /// Seek to specific timestamp on specific partition
    SeekToTimestamp {
        topic: String,
        partition: u32,
        timestamp: u64,
    },
    /// Seek all partitions to specific timestamp
    SeekAllToTimestamp {
        topic: String,
        timestamp: u64,
    },
    /// Get consumer group metadata
    GetConsumerGroupMetadata {
        consumer_group: String,
    },
    /// Get partition metadata for topic
    GetPartitionMetadata {
        topic: String,
    },
    /// Unsubscribe from topic
    Unsubscribe {
        topic: String,
        consumer_group: String,
        consumer_id: String,
    },
    /// Request partition rebalance
    RequestRebalance {
        consumer_group: String,
        consumer_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsumerResponse {
    /// Subscription successful
    SubscribeOk {
        assigned_partitions: Vec<u32>,
    },
    /// Fetched messages
    FetchOk {
        messages: Vec<Message>,
        next_offset: u64,
        partition: u32,
    },
    /// Offsets committed successfully
    CommitOk {
        committed_partitions: Vec<u32>,
    },
    /// Seek successful
    SeekOk {
        partition: u32,
        new_offset: u64,
    },
    /// All partitions seek successful
    SeekAllOk {
        partition_offsets: HashMap<u32, u64>,
    },
    /// Consumer group metadata
    ConsumerGroupMetadata {
        partitions: HashMap<u32, u64>, // partition -> committed offset
    },
    /// Partition metadata
    PartitionMetadata {
        partitions: Vec<PartitionInfo>,
    },
    /// Rebalance triggered
    RebalanceTriggered {
        new_assignment: Vec<u32>,
    },
    /// Unsubscribe successful
    UnsubscribeOk,
    /// Error response
    Error {
        message: String,
        error_code: u32,
    },
}

/// Failed message with retry information
#[derive(Debug, Clone)]
struct FailedMessage {
    message: Message,
    retry_count: usize,
    last_error: String,
    next_retry_time: Instant,
    partition: u32,
}

/// Partition information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub partition_id: u32,
    pub leader_broker: String,
    pub replica_brokers: Vec<String>,
    pub high_watermark: u64,
    pub low_watermark: u64,
}

/// Partition assignment state
#[derive(Debug, Clone)]
pub struct PartitionAssignment {
    pub partitions: Vec<u32>,
    pub assignment_id: String,
    pub assigned_at: Instant,
}

/// Per-partition consumer state
#[derive(Debug, Clone)]
struct PartitionState {
    pub partition_id: u32,
    pub committed_offset: u64,
    pub pending_offsets: BTreeSet<u64>,
    pub last_fetch_time: Option<Instant>,
    pub is_paused: bool,
    pub failed_messages: VecDeque<FailedMessage>,
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
            failed_messages: Arc::new(Mutex::new(VecDeque::new())),
            // Multi-partition state management
            partition_assignment: Arc::new(RwLock::new(None)),
            partition_states: Arc::new(RwLock::new(HashMap::new())),
            paused_partitions: Arc::new(RwLock::new(Vec::new())),
        };
        
        // Subscribe to the topic
        consumer.subscribe().await?;

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
    pub fn stream(&self) -> ConsumerStream {
        ConsumerStream::new(self)
    }

    /// Commit current offsets for all assigned partitions manually
    pub async fn commit(&self) -> Result<()> {
        let offset_tracker = self.offset_tracker.read().await;
        let offsets = offset_tracker.get_all_committed_offsets();
        
        if offsets.is_empty() {
            debug!("No offsets to commit for consumer {}", self.id);
            return Ok(());
        }
        
        debug!("Committing offsets {:?} for consumer {}", offsets, self.id);
        
        // Create commit request with all partition offsets
        let request = ConsumerRequest::CommitOffsets {
            topic: self.topic.clone(),
            consumer_group: self.consumer_group.clone(),
            offsets,
        };
        
        // Serialize request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        // Send request to broker
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        // Deserialize response
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::CommitOk { committed_partitions } => {
                debug!("Successfully committed offsets for partitions {:?} for consumer {}", committed_partitions, self.id);
                Ok(())
            }
            ConsumerResponse::Error { message, error_code } => {
                error!("Failed to commit offsets: {} (code: {})", message, error_code);
                Err(ClientError::Consumer(format!("Commit failed: {}", message)))
            }
            _ => {
                Err(ClientError::Protocol("Unexpected response for commit request".to_string()))
            }
        }
    }

    /// Seek to specific offset on a specific partition
    pub async fn seek(&self, partition: u32, offset: u64) -> Result<()> {
        debug!("Seeking to offset {} on partition {} for consumer {}", offset, partition, self.id);
        
        // Create seek request
        let request = ConsumerRequest::Seek {
            topic: self.topic.clone(),
            partition,
            offset,
        };
        
        // Serialize request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        // Send request to broker
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        // Deserialize response
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::SeekOk { partition: response_partition, new_offset } => {
                // Update local offset tracker for the specific partition
                {
                    let mut offset_tracker = self.offset_tracker.write().await;
                    offset_tracker.set_committed_offset(response_partition, new_offset);
                    offset_tracker.clear_pending_offsets(response_partition);
                }
                
                debug!("Successfully seeked to offset {} on partition {} for consumer {}", new_offset, response_partition, self.id);
                Ok(())
            }
            ConsumerResponse::Error { message, error_code } => {
                error!("Failed to seek to offset {} on partition {}: {} (code: {})", offset, partition, message, error_code);
                Err(ClientError::Consumer(format!("Seek failed: {}", message)))
            }
            _ => {
                Err(ClientError::Protocol("Unexpected response for seek request".to_string()))
            }
        }
    }
    
    /// Seek all assigned partitions to specific offset
    pub async fn seek_all(&self, offset: u64) -> Result<()> {
        let assignment = self.partition_assignment.read().await;
        if let Some(ref assignment) = *assignment {
            let mut errors = Vec::new();
            
            for &partition in &assignment.partitions {
                if let Err(e) = self.seek(partition, offset).await {
                    errors.push(format!("Partition {}: {}", partition, e));
                }
            }
            
            if !errors.is_empty() {
                return Err(ClientError::Consumer(format!("Seek failed on some partitions: {}", errors.join(", "))));
            }
        }
        
        Ok(())
    }

    /// Seek specific partition to specific timestamp
    pub async fn seek_to_timestamp(&self, partition: u32, timestamp: u64) -> Result<()> {
        debug!("Seeking partition {} to timestamp {} for consumer {}", partition, timestamp, self.id);
        
        // Create timestamp seek request
        let request = ConsumerRequest::SeekToTimestamp {
            topic: self.topic.clone(),
            partition,
            timestamp,
        };
        
        // Serialize request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        // Send request to broker
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        // Deserialize response
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::SeekOk { partition: response_partition, new_offset } => {
                // Update local offset tracker for the specific partition
                {
                    let mut offset_tracker = self.offset_tracker.write().await;
                    offset_tracker.set_committed_offset(response_partition, new_offset);
                    offset_tracker.clear_pending_offsets(response_partition);
                }
                
                debug!("Successfully seeked partition {} to timestamp {} (offset {}) for consumer {}", response_partition, timestamp, new_offset, self.id);
                Ok(())
            }
            ConsumerResponse::Error { message, error_code } => {
                error!("Failed to seek partition {} to timestamp {}: {} (code: {})", partition, timestamp, message, error_code);
                Err(ClientError::Consumer(format!("Timestamp seek failed: {}", message)))
            }
            _ => {
                Err(ClientError::Protocol("Unexpected response for timestamp seek request".to_string()))
            }
        }
    }
    
    /// Seek all assigned partitions to specific timestamp
    pub async fn seek_all_to_timestamp(&self, timestamp: u64) -> Result<HashMap<u32, u64>> {
        debug!("Seeking all partitions to timestamp {} for consumer {}", timestamp, self.id);
        
        // Create timestamp seek request for all partitions
        let request = ConsumerRequest::SeekAllToTimestamp {
            topic: self.topic.clone(),
            timestamp,
        };
        
        // Serialize request
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        // Send request to broker
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        // Deserialize response
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::SeekAllOk { partition_offsets } => {
                // Update local offset tracker for all partitions
                {
                    let mut offset_tracker = self.offset_tracker.write().await;
                    for (partition, offset) in &partition_offsets {
                        offset_tracker.set_committed_offset(*partition, *offset);
                        offset_tracker.clear_pending_offsets(*partition);
                    }
                }
                
                debug!("Successfully seeked all partitions to timestamp {} with offsets {:?} for consumer {}", timestamp, partition_offsets, self.id);
                Ok(partition_offsets)
            }
            ConsumerResponse::Error { message, error_code } => {
                error!("Failed to seek all partitions to timestamp {}: {} (code: {})", timestamp, message, error_code);
                Err(ClientError::Consumer(format!("Timestamp seek failed: {}", message)))
            }
            _ => {
                Err(ClientError::Protocol("Unexpected response for timestamp seek request".to_string()))
            }
        }
    }

    /// Close the consumer
    pub async fn close(&self) -> Result<()> {
        let mut is_closed = self.is_closed.write().await;
        if *is_closed {
            return Ok(());
        }
        
        *is_closed = true;
        
        // Unsubscribe from broker
        let _ = self.unsubscribe().await;
        
        // Commit final offsets
        self.commit().await?;
        
        // Close message receiver
        let mut receiver_guard = self.message_receiver.lock().await;
        *receiver_guard = None;
        
        info!("Consumer {} closed", self.id);
        Ok(())
    }
    
    /// Unsubscribe from the topic
    async fn unsubscribe(&self) -> Result<()> {
        debug!("Unsubscribing consumer {} from topic {}", self.id, self.topic);
        
        let request = ConsumerRequest::Unsubscribe {
            topic: self.topic.clone(),
            consumer_group: self.consumer_group.clone(),
            consumer_id: self.id.clone(),
        };
        
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::UnsubscribeOk => {
                debug!("Successfully unsubscribed consumer {} from topic {}", self.id, self.topic);
                Ok(())
            }
            ConsumerResponse::Error { message, error_code } => {
                warn!("Failed to unsubscribe: {} (code: {})", message, error_code);
                Ok(()) // Don't fail close operation on unsubscribe error
            }
            _ => {
                warn!("Unexpected response for unsubscribe request");
                Ok(())
            }
        }
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

    /// Subscribe to the topic
    async fn subscribe(&self) -> Result<()> {
        debug!("Subscribing consumer {} to topic {} in group {}", self.id, self.topic, self.consumer_group);
        
        let request = ConsumerRequest::Subscribe {
            topic: self.topic.clone(),
            consumer_group: self.consumer_group.clone(),
            consumer_id: self.id.clone(),
            start_position: self.config.start_position.clone(),
        };
        
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::SubscribeOk { assigned_partitions } => {
                // Create partition assignment
                let assignment = PartitionAssignment {
                    partitions: assigned_partitions.clone(),
                    assignment_id: Uuid::new_v4().to_string(),
                    assigned_at: Instant::now(),
                };
                
                // Update partition assignment
                *self.partition_assignment.write().await = Some(assignment);
                
                // Initialize partition states
                {
                    let mut partition_states = self.partition_states.write().await;
                    for &partition_id in &assigned_partitions {
                        partition_states.insert(partition_id, PartitionState {
                            partition_id,
                            committed_offset: 0,
                            pending_offsets: BTreeSet::new(),
                            last_fetch_time: None,
                            is_paused: false,
                            failed_messages: VecDeque::new(),
                        });
                    }
                }
                
                info!("Consumer {} subscribed to topic {} with partitions {:?}", 
                      self.id, self.topic, assigned_partitions);
                Ok(())
            }
            ConsumerResponse::Error { message, error_code } => {
                Err(ClientError::Consumer(format!("Subscribe failed: {} (code: {})", message, error_code)))
            }
            _ => {
                Err(ClientError::Protocol("Unexpected response for subscribe request".to_string()))
            }
        }
    }
    
    /// Background task for consuming messages
    async fn run_consumption_loop(&self, sender: mpsc::UnboundedSender<Message>) {
        let mut fetch_interval = tokio::time::interval(self.config.fetch_timeout);
        
        loop {
            // Check if consumer is closed
            {
                let is_closed = self.is_closed.read().await;
                if *is_closed {
                    break;
                }
            }
            
            fetch_interval.tick().await;
            
            // Process failed messages for retry
            self.process_failed_messages(&sender).await;
            
            // Fetch messages from all assigned partitions
            let assignment = self.partition_assignment.read().await;
            if let Some(ref assignment) = *assignment {
                let paused_partitions = self.paused_partitions.read().await;
                
                for &partition in &assignment.partitions {
                    // Skip paused partitions
                    if paused_partitions.contains(&partition) {
                        continue;
                    }
                    
                    if let Err(e) = self.fetch_messages_for_partition(partition, &sender).await {
                        warn!("Failed to fetch messages from partition {}: {}", partition, e);
                    }
                }
            }
        }
    }
    
    /// Fetch messages for a specific partition
    async fn fetch_messages_for_partition(
        &self,
        partition: u32,
        sender: &mpsc::UnboundedSender<Message>,
    ) -> Result<()> {
        let current_offset = {
            let offset_tracker = self.offset_tracker.read().await;
            offset_tracker.get_committed_offset(partition) + 1
        };
        
        let request = ConsumerRequest::Fetch {
            topic: self.topic.clone(),
            partition,
            offset: current_offset,
            max_messages: self.config.fetch_size,
            timeout_ms: self.config.fetch_timeout.as_millis() as u64,
        };
        
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match self.client.connection().send_request(request_bytes).await {
            Ok(response_bytes) => {
                let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
                    .map_err(|e| ClientError::Serialization(e.to_string()))?;
                
                match response {
                    ConsumerResponse::FetchOk { messages, next_offset, partition: response_partition } => {
                        debug!("Fetched {} messages from partition {}, next offset: {}", 
                               messages.len(), response_partition, next_offset);
                        
                        // Update offset tracker with pending offsets for this partition
                        {
                            let mut offset_tracker = self.offset_tracker.write().await;
                            for message in &messages {
                                offset_tracker.add_pending_offset(response_partition, message.offset);
                            }
                        }
                        
                        // Update partition state
                        {
                            let mut partition_states = self.partition_states.write().await;
                            if let Some(state) = partition_states.get_mut(&response_partition) {
                                state.last_fetch_time = Some(Instant::now());
                            }
                        }
                        
                        // Send messages to receiver
                        for message in messages {
                            if let Err(_) = sender.send(message) {
                                warn!("Failed to send message to receiver - consumer may be closed");
                                break;
                            }
                        }
                    }
                    ConsumerResponse::Error { message, error_code } => {
                        if error_code == 404 { // No messages available
                            debug!("No messages available for partition {}", partition);
                        } else {
                            warn!("Error fetching messages from partition {}: {} (code: {})", 
                                  partition, message, error_code);
                        }
                    }
                    _ => {
                        warn!("Unexpected response for fetch request");
                    }
                }
            }
            Err(e) => {
                warn!("Failed to fetch messages from partition {}: {}", partition, e);
                return Err(e);
            }
        }
        
        Ok(())
    }
    
    /// Process failed messages for retry
    async fn process_failed_messages(&self, sender: &mpsc::UnboundedSender<Message>) {
        let now = Instant::now();
        let mut failed_messages = self.failed_messages.lock().await;
        let mut retry_messages = Vec::new();
        
        // Find messages ready for retry
        while let Some(failed_msg) = failed_messages.front() {
            if failed_msg.next_retry_time <= now {
                if let Some(failed_msg) = failed_messages.pop_front() {
                    retry_messages.push(failed_msg);
                }
            } else {
                break;
            }
        }
        
        // Send retry messages or move to DLQ
        for mut failed_msg in retry_messages {
            if failed_msg.retry_count < self.config.max_retry_attempts {
                // Retry the message
                failed_msg.retry_count += 1;
                failed_msg.next_retry_time = now + Duration::from_secs(1 << failed_msg.retry_count); // Exponential backoff
                
                debug!("Retrying message {} (attempt {})", failed_msg.message.id, failed_msg.retry_count);
                
                if sender.send(failed_msg.message.clone()).is_err() {
                    warn!("Failed to send retry message - consumer may be closed");
                    failed_messages.push_back(failed_msg);
                } else {
                    // Re-queue for potential future retry
                    failed_messages.push_back(failed_msg);
                }
            } else {
                // Send to dead letter queue if configured
                if let Some(ref dlq_topic) = self.config.dead_letter_queue {
                    if let Err(e) = self.send_to_dead_letter_queue(&failed_msg.message, dlq_topic).await {
                        error!("Failed to send message {} to DLQ {}: {}", failed_msg.message.id, dlq_topic, e);
                    } else {
                        info!("Sent message {} to dead letter queue {}", failed_msg.message.id, dlq_topic);
                    }
                } else {
                    warn!("Message {} exceeded max retry attempts and no DLQ configured - dropping", failed_msg.message.id);
                }
                
                self.metrics.messages_failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    
    /// Send failed message to dead letter queue
    async fn send_to_dead_letter_queue(&self, message: &Message, dlq_topic: &str) -> Result<()> {
        // Create a producer for the DLQ topic
        let producer = self.client.create_producer(dlq_topic).await?;
        
        // Create DLQ message with additional metadata
        let mut dlq_message = message.clone();
        dlq_message.topic = dlq_topic.to_string();
        dlq_message.headers.insert("original_topic".to_string(), message.topic.clone());
        dlq_message.headers.insert("failure_reason".to_string(), "max_retries_exceeded".to_string());
        dlq_message.headers.insert("consumer_group".to_string(), self.consumer_group.clone());
        
        producer.send(dlq_message).await?;
        Ok(())
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
            if ack_request.success {
                let mut offset_tracker = self.offset_tracker.write().await;
                offset_tracker.remove_pending_offset(ack_request.partition, ack_request.offset);
                
                // Update committed offset to highest consecutive offset for this partition
                offset_tracker.update_consecutive_committed(ack_request.partition);
                
                self.metrics.messages_processed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            } else {
                // Handle failed message processing
                if let Some(failed_message) = self.find_message_by_offset(ack_request.partition, ack_request.offset).await {
                    let failed_msg = FailedMessage {
                        message: failed_message,
                        retry_count: 0,
                        last_error: "Message processing failed".to_string(),
                        next_retry_time: Instant::now() + Duration::from_secs(1),
                        partition: ack_request.partition,
                    };
                    
                    let mut failed_messages = self.failed_messages.lock().await;
                    failed_messages.push_back(failed_msg);
                    
                    debug!("Added message with offset {} on partition {} to failed message queue", ack_request.offset, ack_request.partition);
                }
                
                self.metrics.messages_failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        }
    }
    
    /// Find message by partition and offset (this would typically come from a local cache)
    async fn find_message_by_offset(&self, partition: u32, offset: u64) -> Option<Message> {
        // In a real implementation, this would look up the message from a local cache
        // For now, we create a placeholder message
        use crate::message::MessageBuilder;
        
        MessageBuilder::new()
            .topic(&self.topic)
            .payload(format!("Retry message for partition {} offset {}", partition, offset))
            .build()
            .ok()
            .map(|mut msg| {
                msg.offset = offset;
                msg.partition = partition;
                msg
            })
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
    
    /// Get assigned partitions
    pub async fn assigned_partitions(&self) -> Vec<u32> {
        let assignment = self.partition_assignment.read().await;
        if let Some(ref assignment) = *assignment {
            assignment.partitions.clone()
        } else {
            Vec::new()
        }
    }
    
    /// Get partition assignment details
    pub async fn partition_assignment(&self) -> Option<PartitionAssignment> {
        self.partition_assignment.read().await.clone()
    }
    
    /// Get consumer lag for all partitions
    pub async fn get_lag(&self) -> Result<HashMap<u32, u64>> {
        let mut lag_map = HashMap::new();
        
        let request = ConsumerRequest::GetConsumerGroupMetadata {
            consumer_group: self.consumer_group.clone(),
        };
        
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::ConsumerGroupMetadata { partitions } => {
                let offset_tracker = self.offset_tracker.read().await;
                
                for (partition, broker_committed_offset) in partitions {
                    let local_committed_offset = offset_tracker.get_committed_offset(partition);
                    
                    // Calculate lag as difference between broker's committed offset and local committed offset
                    let lag = if broker_committed_offset > local_committed_offset {
                        broker_committed_offset - local_committed_offset
                    } else {
                        0
                    };
                    lag_map.insert(partition, lag);
                }
            }
            ConsumerResponse::Error { message, error_code } => {
                return Err(ClientError::Consumer(format!("Failed to get consumer group metadata: {} (code: {})", message, error_code)));
            }
            _ => {
                return Err(ClientError::Protocol("Unexpected response for consumer group metadata request".to_string()));
            }
        }
        
        Ok(lag_map)
    }
    
    /// Pause consumption from specific partitions
    pub async fn pause_partitions(&self, partitions: Vec<u32>) -> Result<()> {
        debug!("Pausing consumption from partitions: {:?}", partitions);
        
        let mut paused_partitions = self.paused_partitions.write().await;
        let mut partition_states = self.partition_states.write().await;
        
        for partition in partitions {
            if !paused_partitions.contains(&partition) {
                paused_partitions.push(partition);
            }
            
            // Update partition state
            if let Some(state) = partition_states.get_mut(&partition) {
                state.is_paused = true;
            }
        }
        
        Ok(())
    }
    
    /// Resume consumption from specific partitions
    pub async fn resume_partitions(&self, partitions: Vec<u32>) -> Result<()> {
        debug!("Resuming consumption from partitions: {:?}", partitions);
        
        let mut paused_partitions = self.paused_partitions.write().await;
        let mut partition_states = self.partition_states.write().await;
        
        for partition in partitions {
            paused_partitions.retain(|&p| p != partition);
            
            // Update partition state
            if let Some(state) = partition_states.get_mut(&partition) {
                state.is_paused = false;
            }
        }
        
        Ok(())
    }
    
    /// Get currently paused partitions
    pub async fn paused_partitions(&self) -> Vec<u32> {
        self.paused_partitions.read().await.clone()
    }
    
    /// Get committed offset for a specific partition
    pub async fn committed_offset(&self, partition: u32) -> Result<u64> {
        let request = ConsumerRequest::GetConsumerGroupMetadata {
            consumer_group: self.consumer_group.clone(),
        };
        
        let request_bytes = serde_json::to_vec(&request)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        let response_bytes = self.client.connection().send_request(request_bytes).await?;
        
        let response: ConsumerResponse = serde_json::from_slice(&response_bytes)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;
        
        match response {
            ConsumerResponse::ConsumerGroupMetadata { partitions } => {
                partitions.get(&partition)
                    .copied()
                    .ok_or_else(|| ClientError::PartitionNotFound { 
                        topic: self.topic.clone(), 
                        partition 
                    })
            }
            ConsumerResponse::Error { message, error_code } => {
                Err(ClientError::Consumer(format!("Failed to get committed offset: {} (code: {})", message, error_code)))
            }
            _ => {
                Err(ClientError::Protocol("Unexpected response for consumer group metadata request".to_string()))
            }
        }
    }
}

impl ConsumerMessage {
    /// Acknowledge successful processing
    pub async fn ack(&self) -> Result<()> {
        let ack_request = AckRequest {
            offset: self.message.offset,
            partition: self.message.partition,
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
            partition: self.message.partition,
            success: false,
        };
        
        self.ack_sender.send(ack_request)
            .map_err(|_| ClientError::Consumer("Failed to send negative acknowledgment".to_string()))?;
        
        Ok(())
    }
}

/// Stream implementation for consumer
pub struct ConsumerStream {
    consumer: Consumer,
    receiver_future: Option<Pin<Box<dyn std::future::Future<Output = Option<ConsumerMessage>> + Send>>>,
}

impl ConsumerStream {
    fn new(consumer: &Consumer) -> Self {
        Self { 
            consumer: consumer.clone(),
            receiver_future: None,
        }
    }
}

impl Stream for ConsumerStream {
    type Item = Result<ConsumerMessage>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // If we don't have a pending future, create one
        if self.receiver_future.is_none() {
            let consumer = self.consumer.clone();
            self.receiver_future = Some(Box::pin(async move {
                consumer.receive().await.ok().flatten()
            }));
        }

        // Poll the future
        if let Some(mut future) = self.receiver_future.take() {
            match future.as_mut().poll(cx) {
                Poll::Ready(Some(message)) => {
                    // Got a message, prepare for next poll
                    Poll::Ready(Some(Ok(message)))
                }
                Poll::Ready(None) => {
                    // Consumer is closed
                    Poll::Ready(None)
                }
                Poll::Pending => {
                    // Put the future back and return pending
                    self.receiver_future = Some(future);
                    Poll::Pending
                }
            }
        } else {
            Poll::Pending
        }
    }
}

impl OffsetTracker {
    /// Get the committed offset for a specific partition
    pub fn get_committed_offset(&self, partition: u32) -> u64 {
        self.committed_offsets.get(&partition).copied().unwrap_or(0)
    }
    
    /// Set the committed offset for a specific partition
    pub fn set_committed_offset(&mut self, partition: u32, offset: u64) {
        self.committed_offsets.insert(partition, offset);
        self.last_commit_times.insert(partition, Instant::now());
    }
    
    /// Add a pending offset for a specific partition
    pub fn add_pending_offset(&mut self, partition: u32, offset: u64) {
        self.pending_offsets.entry(partition).or_insert_with(BTreeSet::new).insert(offset);
    }
    
    /// Remove a pending offset for a specific partition (acknowledge a message)
    pub fn remove_pending_offset(&mut self, partition: u32, offset: u64) {
        if let Some(offsets) = self.pending_offsets.get_mut(&partition) {
            offsets.remove(&offset);
        }
        
        // Add to acknowledged offsets
        self.acknowledged_offsets.entry(partition).or_insert_with(BTreeSet::new).insert(offset);
    }
    
    /// Update committed offset to highest consecutive offset for a partition
    /// This should be called after acknowledging a message
    pub fn update_consecutive_committed(&mut self, partition: u32) {
        let current_committed = self.get_committed_offset(partition);
        let acknowledged_offsets = self.acknowledged_offsets.get(&partition);
        
        if let Some(acknowledged_offsets) = acknowledged_offsets {
            let mut new_committed = current_committed;
            let mut candidate = current_committed + 1;
            
            // Keep incrementing while we find consecutive acknowledged offsets
            while acknowledged_offsets.contains(&candidate) {
                new_committed = candidate;
                candidate += 1;
            }
            
            // Update if we found a higher consecutive committed offset
            if new_committed > current_committed {
                self.set_committed_offset(partition, new_committed);
                
                // Clean up acknowledged offsets that are now committed
                if let Some(ack_offsets) = self.acknowledged_offsets.get_mut(&partition) {
                    let to_remove: Vec<u64> = ack_offsets.iter()
                        .filter(|&&offset| offset <= new_committed)
                        .copied()
                        .collect();
                    
                    for offset in to_remove {
                        ack_offsets.remove(&offset);
                    }
                }
            }
        }
    }
    
    /// Clear all pending offsets for a partition (used during seek)
    pub fn clear_pending_offsets(&mut self, partition: u32) {
        self.pending_offsets.remove(&partition);
    }
    
    /// Get all committed offsets as a HashMap
    pub fn get_all_committed_offsets(&self) -> HashMap<u32, u64> {
        self.committed_offsets.clone()
    }
}

impl Default for ConsumerBuilder {
    fn default() -> Self {
        Self::new()
    }
}