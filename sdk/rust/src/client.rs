use crate::{
    config::ClientConfig,
    connection::Connection,
    consumer::{Consumer, ConsumerBuilder},
    error::Result,
    producer::{Producer, ProducerBuilder},
    stream::{MessageStream, StreamConfig},
};
use dashmap::DashMap;
use std::sync::Arc;

/// Main RustMQ client for managing connections and creating producers/consumers
#[derive(Clone)]
pub struct RustMqClient {
    config: Arc<ClientConfig>,
    connection: Arc<Connection>,
    producers: Arc<DashMap<String, Producer>>,
    consumers: Arc<DashMap<String, Consumer>>,
}

impl RustMqClient {
    /// Create a new RustMQ client
    pub async fn new(config: ClientConfig) -> Result<Self> {
        let connection = Connection::new(&config).await?;

        Ok(Self {
            config: Arc::new(config),
            connection: Arc::new(connection),
            producers: Arc::new(DashMap::new()),
            consumers: Arc::new(DashMap::new()),
        })
    }

    /// Create a new producer for the specified topic
    pub async fn create_producer(&self, topic: &str) -> Result<Producer> {
        let producer = ProducerBuilder::new()
            .topic(topic.to_string())
            .client(self.clone())
            .build()
            .await?;

        self.producers.insert(topic.to_string(), producer.clone());
        Ok(producer)
    }

    /// Create a new consumer for the specified topic
    pub async fn create_consumer(&self, topic: &str, consumer_group: &str) -> Result<Consumer> {
        let consumer = ConsumerBuilder::new()
            .topic(topic.to_string())
            .consumer_group(consumer_group.to_string())
            .client(self.clone())
            .build()
            .await?;

        let key = format!("{}:{}", topic, consumer_group);
        self.consumers.insert(key, consumer.clone());
        Ok(consumer)
    }

    /// Create a streaming interface for real-time message processing
    pub async fn create_stream(&self, config: StreamConfig) -> Result<MessageStream> {
        MessageStream::new(self.clone(), config).await
    }

    /// Get or create a producer for the specified topic
    pub async fn producer(&self, topic: &str) -> Result<Producer> {
        if let Some(producer) = self.producers.get(topic) {
            Ok(producer.clone())
        } else {
            self.create_producer(topic).await
        }
    }

    /// Get or create a consumer for the specified topic and group
    pub async fn consumer(&self, topic: &str, consumer_group: &str) -> Result<Consumer> {
        let key = format!("{}:{}", topic, consumer_group);
        if let Some(consumer) = self.consumers.get(&key) {
            Ok(consumer.clone())
        } else {
            self.create_consumer(topic, consumer_group).await
        }
    }

    /// Close the client and all associated resources
    pub async fn close(&self) -> Result<()> {
        // Close all producers
        for producer in self.producers.iter() {
            producer.close().await?;
        }

        // Close all consumers
        for consumer in self.consumers.iter() {
            consumer.close().await?;
        }

        // Close connection
        self.connection.close().await?;

        Ok(())
    }

    /// Get client configuration
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Get underlying connection
    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Check if client is connected
    pub async fn is_connected(&self) -> bool {
        self.connection.is_connected().await
    }

    /// Get client health status
    pub async fn health_check(&self) -> Result<bool> {
        self.connection.health_check().await
    }

    /// Get a connection if available
    pub async fn get_connection(&self) -> Option<&Connection> {
        if self.connection.is_connected().await {
            Some(&self.connection)
        } else {
            None
        }
    }
}
