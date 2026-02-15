use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// A message in the RustMQ system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID
    pub id: String,

    /// Topic the message belongs to
    pub topic: String,

    /// Partition ID
    pub partition: u32,

    /// Message offset within partition
    pub offset: u64,

    /// Message key for partitioning
    pub key: Option<Bytes>,

    /// Message payload
    pub payload: Bytes,

    /// Message headers/properties
    pub headers: HashMap<String, String>,

    /// Timestamp when message was produced
    pub timestamp: u64,

    /// Producer ID
    pub producer_id: Option<String>,

    /// Message sequence number
    pub sequence: Option<u64>,

    /// Compression algorithm used
    pub compression: Option<String>,

    /// Message size in bytes
    pub size: usize,
}

/// Builder for creating messages
#[derive(Debug, Default)]
pub struct MessageBuilder {
    id: Option<String>,
    topic: Option<String>,
    partition: Option<u32>,
    key: Option<Bytes>,
    payload: Option<Bytes>,
    headers: HashMap<String, String>,
    producer_id: Option<String>,
}

impl MessageBuilder {
    /// Create a new message builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set message ID
    pub fn id<T: Into<String>>(mut self, id: T) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set topic
    pub fn topic<T: Into<String>>(mut self, topic: T) -> Self {
        self.topic = Some(topic.into());
        self
    }

    /// Set partition
    pub fn partition(mut self, partition: u32) -> Self {
        self.partition = Some(partition);
        self
    }

    /// Set message key
    pub fn key<T: Into<Bytes>>(mut self, key: T) -> Self {
        self.key = Some(key.into());
        self
    }

    /// Set message payload
    pub fn payload<T: Into<Bytes>>(mut self, payload: T) -> Self {
        self.payload = Some(payload.into());
        self
    }

    /// Add a header
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add multiple headers
    pub fn headers<I, K, V>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (key, value) in headers {
            self.headers.insert(key.into(), value.into());
        }
        self
    }

    /// Set producer ID
    pub fn producer_id<T: Into<String>>(mut self, producer_id: T) -> Self {
        self.producer_id = Some(producer_id.into());
        self
    }

    /// Build the message
    pub fn build(self) -> Result<Message, String> {
        let payload = self.payload.ok_or("Message payload is required")?;
        let topic = self.topic.ok_or("Message topic is required")?;

        let id = self.id.unwrap_or_else(|| Uuid::new_v4().to_string());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let size = payload.len() + self.key.as_ref().map_or(0, |k| k.len());

        Ok(Message {
            id,
            topic,
            partition: self.partition.unwrap_or(0),
            offset: 0, // Will be set by broker
            key: self.key,
            payload,
            headers: self.headers,
            timestamp,
            producer_id: self.producer_id,
            sequence: None,    // Will be set by producer
            compression: None, // Will be set based on producer config
            size,
        })
    }
}

impl Message {
    /// Create a new message builder
    pub fn builder() -> MessageBuilder {
        MessageBuilder::new()
    }

    /// Get message as JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Create message from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Get message payload as string (UTF-8)
    pub fn payload_as_string(&self) -> Result<String, std::string::FromUtf8Error> {
        String::from_utf8(self.payload.to_vec())
    }

    /// Get message key as string (UTF-8)
    pub fn key_as_string(&self) -> Option<Result<String, std::string::FromUtf8Error>> {
        self.key.as_ref().map(|k| String::from_utf8(k.to_vec()))
    }

    /// Check if message has a specific header
    pub fn has_header(&self, key: &str) -> bool {
        self.headers.contains_key(key)
    }

    /// Get header value
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }

    /// Get all header keys
    pub fn header_keys(&self) -> Vec<&String> {
        self.headers.keys().collect()
    }

    /// Calculate message age in milliseconds
    pub fn age_ms(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        now.saturating_sub(self.timestamp)
    }

    /// Check if message is compressed
    pub fn is_compressed(&self) -> bool {
        self.compression.is_some()
    }

    /// Get message size including headers
    pub fn total_size(&self) -> usize {
        let headers_size: usize = self.headers.iter().map(|(k, v)| k.len() + v.len()).sum();

        self.size + headers_size + self.id.len() + self.topic.len()
    }
}

/// Message metadata for lightweight operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageMetadata {
    pub id: String,
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: u64,
    pub size: usize,
    pub headers: HashMap<String, String>,
}

impl From<&Message> for MessageMetadata {
    fn from(message: &Message) -> Self {
        Self {
            id: message.id.clone(),
            topic: message.topic.clone(),
            partition: message.partition,
            offset: message.offset,
            timestamp: message.timestamp,
            size: message.size,
            headers: message.headers.clone(),
        }
    }
}

/// Batch of messages for efficient processing
#[derive(Debug, Clone)]
pub struct MessageBatch {
    pub messages: Vec<Message>,
    pub batch_id: String,
    pub total_size: usize,
    pub created_at: u64,
}

impl MessageBatch {
    /// Create a new message batch
    pub fn new(messages: Vec<Message>) -> Self {
        let total_size = messages.iter().map(|m| m.total_size()).sum();
        let batch_id = Uuid::new_v4().to_string();
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        Self {
            messages,
            batch_id,
            total_size,
            created_at,
        }
    }

    /// Get number of messages in batch
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if batch is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Split batch into smaller batches
    pub fn split_by_size(self, max_size: usize) -> Vec<MessageBatch> {
        let mut batches = Vec::new();
        let mut current_batch = Vec::new();
        let mut current_size = 0;

        for message in self.messages {
            let message_size = message.total_size();

            if current_size + message_size > max_size && !current_batch.is_empty() {
                batches.push(MessageBatch::new(current_batch));
                current_batch = Vec::new();
                current_size = 0;
            }

            current_batch.push(message);
            current_size += message_size;
        }

        if !current_batch.is_empty() {
            batches.push(MessageBatch::new(current_batch));
        }

        batches
    }
}
