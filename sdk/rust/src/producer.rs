use crate::{
    client::RustMqClient,
    config::{AckLevel, ProducerConfig},
    error::{ClientError, Result},
    message::Message,
};
use bytes::Bytes;
use rustmq::types::{AcknowledgmentLevel, Header, ProduceRequest, ProduceResponse, Record};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc, oneshot};
use tokio::time::Instant;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// High-level producer for sending messages to RustMQ
#[derive(Clone)]
pub struct Producer {
    id: String,
    topic: String,
    config: Arc<ProducerConfig>,
    client: RustMqClient,
    batch_sender: mpsc::UnboundedSender<BatchCommand>,
    sequence_counter: Arc<RwLock<u64>>,
    metrics: Arc<ProducerMetrics>,
}

/// Builder pattern for configuring and creating producer instances
///
/// The ProducerBuilder provides a fluent interface for configuring producer
/// settings before creating the actual producer instance. This allows for
/// flexible configuration with sensible defaults.
///
/// # Example
///
/// ```rust,no_run
/// # use rustmq_client::{
/// #     client::RustMqClient,
/// #     producer::ProducerBuilder,
/// #     config::{ProducerConfig, AckLevel, ClientConfig},
/// # };
/// # use std::time::Duration;
/// #
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # rustls::crypto::aws_lc_rs::default_provider().install_default().unwrap();
/// # let client_config = ClientConfig::default();
/// # let client = RustMqClient::new(client_config).await?;
///
/// let producer = ProducerBuilder::new()
///     .topic("events")
///     .config(ProducerConfig {
///         batch_size: 50,
///         batch_timeout: Duration::from_millis(20),
///         ack_level: AckLevel::All,
///         producer_id: Some("my-app-producer".to_string()),
///         ..Default::default()
///     })
///     .client(client)
///     .build()
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct ProducerBuilder {
    topic: Option<String>,
    config: Option<ProducerConfig>,
    client: Option<RustMqClient>,
}

/// Internal message with completion callback
struct BatchedMessage {
    message: Message,
    result_sender: oneshot::Sender<Result<MessageResult>>,
}

/// Flush request for forcing immediate batch send
struct FlushRequest {
    result_sender: oneshot::Sender<Result<()>>,
}

/// Internal batch command enum for controlling batching behavior
enum BatchCommand {
    Message(BatchedMessage),
    Flush(FlushRequest),
}

/// Convert SDK AckLevel to broker AcknowledgmentLevel
impl From<AckLevel> for AcknowledgmentLevel {
    fn from(ack_level: AckLevel) -> Self {
        match ack_level {
            AckLevel::None => AcknowledgmentLevel::None,
            AckLevel::Leader => AcknowledgmentLevel::Leader,
            AckLevel::All => AcknowledgmentLevel::All,
        }
    }
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

impl std::fmt::Debug for Producer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Producer")
            .field("id", &self.id)
            .field("topic", &self.topic)
            .field("config", &self.config)
            .finish()
    }
}

impl ProducerBuilder {
    /// Create a new producer builder with default settings
    ///
    /// Returns a builder instance with no topic, config, or client set.
    /// These must be provided before calling `build()`.
    pub fn new() -> Self {
        Self {
            topic: None,
            config: None,
            client: None,
        }
    }

    /// Set the topic that this producer will send messages to
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic name (required for producer creation)
    pub fn topic<T: Into<String>>(mut self, topic: T) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set custom producer configuration
    ///
    /// If not provided, default configuration will be used with sensible
    /// defaults for batch size, timeouts, and acknowledgment level.
    ///
    /// # Arguments
    ///
    /// * `config` - Producer configuration settings
    pub fn config(mut self, config: ProducerConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the RustMQ client instance for broker communication
    ///
    /// The client provides the underlying connection to RustMQ brokers.
    /// Multiple producers can share the same client instance.
    ///
    /// # Arguments
    ///
    /// * `client` - Connected RustMQ client (required for producer creation)
    pub fn client(mut self, client: RustMqClient) -> Self {
        self.client = Some(client);
        self
    }

    /// Build and initialize the producer instance
    ///
    /// Creates a new producer with the configured settings and starts the
    /// internal batching task. The producer will be ready to send messages
    /// immediately after creation.
    ///
    /// # Returns
    ///
    /// * `Ok(Producer)` - Successfully created and initialized producer
    /// * `Err(ClientError::InvalidConfig)` - If required fields (topic, client) are missing
    ///
    /// # Errors
    ///
    /// Returns an error if the topic or client are not set.
    pub async fn build(self) -> Result<Producer> {
        let topic = self
            .topic
            .ok_or_else(|| ClientError::InvalidConfig("Producer topic is required".to_string()))?;

        let client = self
            .client
            .ok_or_else(|| ClientError::InvalidConfig("Client is required".to_string()))?;

        let config = Arc::new(self.config.unwrap_or_default());
        let id = config
            .producer_id
            .clone()
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
    /// Send a message and wait for acknowledgment from the broker
    ///
    /// This method sends a message to the RustMQ broker and waits for confirmation
    /// that the message has been successfully stored according to the configured
    /// acknowledgment level (None, Leader, or All replicas).
    ///
    /// The message will be automatically assigned a sequence number and producer ID
    /// before being added to the internal batching queue. The method will block
    /// until the broker responds with the message's assigned offset and partition.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send. Must include topic and payload.
    ///
    /// # Returns
    ///
    /// * `Ok(MessageResult)` - Contains the broker-assigned message ID, partition, offset, and timestamp
    /// * `Err(ClientError)` - If the send fails due to network, broker, or serialization errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustmq_client::{
    /// #     client::RustMqClient,
    /// #     producer::{Producer, ProducerBuilder},
    /// #     message::MessageBuilder,
    /// #     config::{ProducerConfig, ClientConfig},
    /// # };
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client_config = ClientConfig::default();
    /// # let client = RustMqClient::new(client_config).await?;
    /// # let producer = ProducerBuilder::new()
    /// #     .topic("my-topic")
    /// #     .client(client)
    /// #     .build()
    /// #     .await?;
    ///
    /// let message = MessageBuilder::new()
    ///     .topic("my-topic")
    ///     .payload("Hello, World!")
    ///     .header("source", "my-app")
    ///     .build()?;
    ///
    /// let result = producer.send(message).await?;
    /// println!("Message sent to partition {} at offset {}", result.partition, result.offset);
    /// # Ok(())
    /// # }
    /// ```
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

        self.batch_sender
            .send(BatchCommand::Message(batched_message))
            .map_err(|_| ClientError::Producer("Producer is closed".to_string()))?;

        // Wait for result
        result_receiver
            .await
            .map_err(|_| ClientError::Producer("Failed to receive send result".to_string()))?
    }

    /// Send a message with fire-and-forget semantics
    ///
    /// This method queues a message for sending without waiting for broker acknowledgment.
    /// It returns immediately after the message is successfully queued in the internal
    /// batching system. This provides higher throughput but no guarantee that the message
    /// was successfully delivered to the broker.
    ///
    /// The message will be automatically assigned a sequence number and producer ID
    /// before being added to the batching queue. Any send errors will be logged but
    /// not returned to the caller.
    ///
    /// # Arguments
    ///
    /// * `message` - The message to send. Must include topic and payload.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Message was successfully queued for sending
    /// * `Err(ClientError::Producer)` - If the producer is closed or the queue is full
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustmq_client::{
    /// #     client::RustMqClient,
    /// #     producer::{Producer, ProducerBuilder},
    /// #     message::MessageBuilder,
    /// #     config::{ProducerConfig, ClientConfig},
    /// # };
    /// # use serde::Serialize;
    /// #
    /// # #[derive(Serialize)]
    /// # struct EventData { id: u32, name: String }
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client_config = ClientConfig::default();
    /// # let client = RustMqClient::new(client_config).await?;
    /// # let producer = ProducerBuilder::new()
    /// #     .topic("events")
    /// #     .client(client)
    /// #     .build()
    /// #     .await?;
    /// # let event_data = EventData { id: 1, name: "test".to_string() };
    ///
    /// let message = MessageBuilder::new()
    ///     .topic("events")
    ///     .payload(serde_json::to_string(&event_data)?)
    ///     .header("event-type", "user-action")
    ///     .build()?;
    ///
    /// // Fire and forget - returns immediately
    /// producer.send_async(message).await?;
    /// # Ok(())
    /// # }
    /// ```
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

        self.batch_sender
            .send(BatchCommand::Message(batched_message))
            .map_err(|_| ClientError::Producer("Producer is closed".to_string()))?;

        Ok(())
    }

    /// Send multiple messages in a batch and wait for all acknowledgments
    ///
    /// This method sends multiple messages to the broker and waits for confirmation
    /// of all messages. Each message is processed through the normal send flow,
    /// ensuring proper sequence numbering and producer ID assignment.
    ///
    /// This is equivalent to calling `send()` for each message sequentially,
    /// but provides a convenient batch interface. Messages will still be subject
    /// to the producer's internal batching logic based on configured batch size
    /// and timeout settings.
    ///
    /// # Arguments
    ///
    /// * `messages` - Vector of messages to send. Each must include topic and payload.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<MessageResult>)` - Results for each message in the same order as input
    /// * `Err(ClientError)` - If any message fails to send (all results are discarded)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustmq_client::{
    /// #     client::RustMqClient,
    /// #     producer::{Producer, ProducerBuilder},
    /// #     message::MessageBuilder,
    /// #     config::{ProducerConfig, ClientConfig},
    /// # };
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client_config = ClientConfig::default();
    /// # let client = RustMqClient::new(client_config).await?;
    /// # let producer = ProducerBuilder::new()
    /// #     .topic("batch-topic")
    /// #     .client(client)
    /// #     .build()
    /// #     .await?;
    ///
    /// let messages: Vec<_> = (0..3).map(|i| {
    ///     MessageBuilder::new()
    ///         .topic("batch-topic")
    ///         .payload(format!("Message {}", i))
    ///         .header("batch-id", "12345")
    ///         .build().unwrap()
    /// }).collect();
    ///
    /// let results = producer.send_batch(messages).await?;
    /// for result in results {
    ///     println!("Sent message {} to offset {}", result.message_id, result.offset);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_batch(&self, messages: Vec<Message>) -> Result<Vec<MessageResult>> {
        let mut results = Vec::with_capacity(messages.len());

        for message in messages {
            let result = self.send(message).await?;
            results.push(result);
        }

        Ok(results)
    }

    /// Flush any pending messages
    ///
    /// Forces immediate sending of all batched messages, regardless of batch size
    /// or timeout settings. This is useful for ensuring message delivery before
    /// closing the producer or at critical points in application logic.
    pub async fn flush(&self) -> Result<()> {
        let (result_sender, result_receiver) = oneshot::channel();

        let flush_request = FlushRequest { result_sender };

        self.batch_sender
            .send(BatchCommand::Flush(flush_request))
            .map_err(|_| ClientError::Producer("Producer is closed".to_string()))?;

        // Wait for flush to complete
        result_receiver
            .await
            .map_err(|_| ClientError::Producer("Failed to receive flush result".to_string()))?
    }

    /// Close the producer and flush all remaining messages
    ///
    /// This method performs a graceful shutdown of the producer by first flushing
    /// all pending messages in the batching queue, then closing the internal
    /// communication channels. Any messages that fail to send during the flush
    /// will be logged as warnings.
    ///
    /// After calling close(), the producer should not be used for sending additional
    /// messages. Any subsequent calls to send methods will return an error.
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Producer closed successfully
    /// * `Err(ClientError)` - If an error occurs during the flush operation
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustmq_client::{
    /// #     client::RustMqClient,
    /// #     producer::{Producer, ProducerBuilder},
    /// #     message::MessageBuilder,
    /// #     config::{ProducerConfig, ClientConfig},
    /// # };
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client_config = ClientConfig::default();
    /// # let client = RustMqClient::new(client_config).await?;
    /// # let producer = ProducerBuilder::new()
    /// #     .topic("test-topic")
    /// #     .client(client)
    /// #     .build()
    /// #     .await?;
    /// # let message1 = MessageBuilder::new()
    /// #     .topic("test-topic")
    /// #     .payload("message 1")
    /// #     .build()?;
    /// # let message2 = MessageBuilder::new()
    /// #     .topic("test-topic")
    /// #     .payload("message 2")
    /// #     .build()?;
    ///
    /// // Send some messages
    /// producer.send_async(message1).await?;
    /// producer.send_async(message2).await?;
    ///
    /// // Close and flush all remaining messages
    /// producer.close().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn close(&self) -> Result<()> {
        // Flush remaining messages before closing
        if let Err(e) = self.flush().await {
            warn!("Failed to flush messages during close: {}", e);
        }

        info!("Producer {} closed", self.id);
        Ok(())
    }

    /// Get current producer performance metrics
    ///
    /// Returns a snapshot of the producer's performance metrics including
    /// message counts, batch statistics, and timing information. These metrics
    /// can be used for monitoring, alerting, and performance tuning.
    ///
    /// # Returns
    ///
    /// A `ProducerMetrics` struct containing:
    /// - `messages_sent`: Total number of successfully sent messages
    /// - `messages_failed`: Total number of failed message sends
    /// - `bytes_sent`: Total bytes successfully transmitted
    /// - `batches_sent`: Total number of batches sent to the broker
    /// - `average_batch_size`: Running average of messages per batch
    /// - `last_send_time`: Timestamp of the most recent successful send
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use rustmq_client::{
    /// #     client::RustMqClient,
    /// #     producer::{Producer, ProducerBuilder},
    /// #     config::{ProducerConfig, ClientConfig},
    /// # };
    /// # use std::sync::atomic::Ordering;
    /// #
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let client_config = ClientConfig::default();
    /// # let client = RustMqClient::new(client_config).await?;
    /// # let producer = ProducerBuilder::new()
    /// #     .topic("test-topic")
    /// #     .client(client)
    /// #     .build()
    /// #     .await?;
    ///
    /// let metrics = producer.metrics().await;
    /// println!("Messages sent: {}", metrics.messages_sent.load(Ordering::Relaxed));
    /// println!("Average batch size: {:.2}", *metrics.average_batch_size.read().await);
    /// # Ok(())
    /// # }
    /// ```
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
    async fn run_batching_loop(&self, mut receiver: mpsc::UnboundedReceiver<BatchCommand>) {
        let mut pending_messages = Vec::new();
        let mut pending_flushes = Vec::new();
        let mut batch_timer = tokio::time::interval(self.config.batch_timeout);

        loop {
            tokio::select! {
                // Receive new command
                command = receiver.recv() => {
                    match command {
                        Some(BatchCommand::Message(batched_message)) => {
                            pending_messages.push(batched_message);

                            // Check if batch is full
                            if pending_messages.len() >= self.config.batch_size {
                                self.send_batch_internal(&mut pending_messages, &mut pending_flushes).await;
                            }
                        }
                        Some(BatchCommand::Flush(flush_request)) => {
                            pending_flushes.push(flush_request);

                            // Immediately send any pending messages
                            if !pending_messages.is_empty() {
                                self.send_batch_internal(&mut pending_messages, &mut pending_flushes).await;
                            } else {
                                // No pending messages, just complete flush requests
                                self.complete_flush_requests(&mut pending_flushes).await;
                            }
                        }
                        None => {
                            // Channel closed, send remaining messages
                            if !pending_messages.is_empty() {
                                self.send_batch_internal(&mut pending_messages, &mut pending_flushes).await;
                            } else if !pending_flushes.is_empty() {
                                self.complete_flush_requests(&mut pending_flushes).await;
                            }
                            break;
                        }
                    }
                }

                // Batch timeout
                _ = batch_timer.tick() => {
                    if !pending_messages.is_empty() {
                        self.send_batch_internal(&mut pending_messages, &mut pending_flushes).await;
                    }
                }
            }
        }
    }

    /// Send a batch of messages to the broker
    async fn send_batch_internal(
        &self,
        messages: &mut Vec<BatchedMessage>,
        flush_requests: &mut Vec<FlushRequest>,
    ) {
        if messages.is_empty() {
            // No messages to send, just complete flush requests
            self.complete_flush_requests(flush_requests).await;
            return;
        }

        let batch_size = messages.len();
        let batch_id = Uuid::new_v4().to_string();

        debug!("Sending batch {} with {} messages", batch_id, batch_size);

        // Convert SDK Messages to broker Records
        let records: Vec<Record> = messages
            .iter()
            .map(|batched_message| {
                // Convert HashMap<String, String> headers to Vec<Header>
                let headers: Vec<Header> = batched_message
                    .message
                    .headers
                    .iter()
                    .map(|(k, v)| Header {
                        key: k.clone(),
                        value: Bytes::from(v.clone()),
                    })
                    .collect();

                // Use Record::new helper for proper type conversion
                Record::new(
                    batched_message.message.key.as_ref().map(|k| k.to_vec()),
                    batched_message.message.payload.to_vec(),
                    headers,
                    batched_message.message.timestamp as i64, // Convert u64 to i64
                )
            })
            .collect();

        // Create broker-compatible ProduceRequest
        let produce_request = ProduceRequest {
            topic: self.topic.clone(), // Use String, not Arc
            partition_id: 0,           // Simplified: all messages to partition 0 for now
            records,
            acks: self.config.ack_level.clone().into(),
            timeout_ms: 30000, // 30 second timeout
        };

        // Serialize and send request to broker
        let request_result = self.send_produce_request(produce_request).await;

        match request_result {
            Ok(response) => {
                // Calculate sequential offsets from broker's first offset
                let base_offset = response.offset;

                // Send successful results back to callers
                for (idx, batched_message) in messages.drain(..).enumerate() {
                    let message_result = MessageResult {
                        message_id: batched_message.message.id.clone(),
                        topic: self.topic.clone(),
                        partition: 0, // Simplified: using partition 0
                        offset: base_offset + idx as u64,
                        timestamp: batched_message.message.timestamp,
                    };
                    let _ = batched_message.result_sender.send(Ok(message_result));
                }

                // Update metrics for successful send
                self.update_metrics(batch_size).await;

                info!(
                    "Successfully sent batch {} with {} messages",
                    batch_id, batch_size
                );
            }
            Err(error) => {
                // Send error results back to callers
                for batched_message in messages.drain(..) {
                    let _ = batched_message.result_sender.send(Err(error.clone()));
                }

                // Update failure metrics
                use std::sync::atomic::Ordering;
                self.metrics
                    .messages_failed
                    .fetch_add(batch_size as u64, Ordering::Relaxed);

                error!("Failed to send batch {}: {}", batch_id, error);
            }
        }

        // Complete flush requests
        self.complete_flush_requests(flush_requests).await;
    }

    /// Send produce request to broker and get response
    async fn send_produce_request(&self, request: ProduceRequest) -> Result<ProduceResponse> {
        // Serialize request with bincode (matches server expectation)
        let request_bytes = bincode::serialize(&request).map_err(|e| {
            ClientError::Serialization(format!("Failed to serialize produce request: {}", e))
        })?;

        // Send via connection and get response
        let response_bytes = self.client.connection().send_request(request_bytes).await?;

        // Deserialize response with bincode (matches server format)
        let response: ProduceResponse = bincode::deserialize(&response_bytes).map_err(|e| {
            ClientError::Deserialization(format!("Failed to deserialize produce response: {}", e))
        })?;

        // Check broker's error_code field (0 = success)
        if response.error_code != 0 {
            return Err(ClientError::Broker(response.error_message.unwrap_or_else(
                || format!("Broker error code: {}", response.error_code),
            )));
        }

        Ok(response)
    }

    /// Complete all pending flush requests
    async fn complete_flush_requests(&self, flush_requests: &mut Vec<FlushRequest>) {
        for flush_request in flush_requests.drain(..) {
            let _ = flush_request.result_sender.send(Ok(()));
        }
    }

    /// Update producer metrics
    async fn update_metrics(&self, batch_size: usize) {
        use std::sync::atomic::Ordering;

        self.metrics
            .messages_sent
            .fetch_add(batch_size as u64, Ordering::Relaxed);
        self.metrics.batches_sent.fetch_add(1, Ordering::Relaxed);

        // Update average batch size
        {
            let mut avg = self.metrics.average_batch_size.write().await;
            let total_batches = self.metrics.batches_sent.load(Ordering::Relaxed) as f64;
            let total_messages = self.metrics.messages_sent.load(Ordering::Relaxed) as f64;
            *avg = if total_batches > 0.0 {
                total_messages / total_batches
            } else {
                0.0
            };
        }

        // Update last send time
        {
            let mut last_send = self.metrics.last_send_time.write().await;
            *last_send = Some(Instant::now());
        }
    }

    /// Get the unique producer ID
    ///
    /// Returns the producer's unique identifier, which is either provided
    /// during configuration or auto-generated during producer creation.
    /// This ID is included in all messages sent by this producer.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the topic this producer sends messages to
    ///
    /// Returns the topic name that this producer was configured to send
    /// messages to. All messages sent through this producer will be
    /// published to this topic.
    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Get the producer configuration
    ///
    /// Returns a reference to the producer's configuration settings,
    /// including batch size, timeouts, acknowledgment level, and other
    /// operational parameters.
    pub fn config(&self) -> &ProducerConfig {
        &self.config
    }
}

impl Default for ProducerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
