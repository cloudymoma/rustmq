use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use futures::stream::{Stream, StreamExt};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, Mutex, Semaphore};
use tokio::time::{interval, sleep, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::subscribers::bigquery::{
    client::{create_bigquery_writer, BigQueryWriter},
    config::{BigQuerySubscriberConfig, RetryBackoffStrategy, DeadLetterAction},
    error::{BigQueryError, Result},
    types::{
        BigQueryBatch, BigQueryMessage, MessageMetadata, HealthStatus, HealthState,
        ConnectionStatus, SubscriberMetrics, InsertResult,
    },
};

/// Main BigQuery subscriber service that manages the entire pipeline
pub struct BigQuerySubscriberService {
    config: BigQuerySubscriberConfig,
    bigquery_writer: Box<dyn BigQueryWriter>,
    metrics: Arc<RwLock<SubscriberMetrics>>,
    message_queue: Arc<Mutex<VecDeque<BigQueryMessage>>>,
    active_batches: Arc<DashMap<String, BigQueryBatch>>,
    batch_semaphore: Arc<Semaphore>,
    shutdown_tx: Option<mpsc::Sender<()>>,
    health_status: Arc<RwLock<HealthStatus>>,
}

/// Message processor that handles batching and retries
struct MessageProcessor {
    config: BigQuerySubscriberConfig,
    bigquery_writer: Arc<dyn BigQueryWriter>,
    metrics: Arc<RwLock<SubscriberMetrics>>,
    message_queue: Arc<Mutex<VecDeque<BigQueryMessage>>>,
    active_batches: Arc<DashMap<String, BigQueryBatch>>,
    batch_semaphore: Arc<Semaphore>,
    health_status: Arc<RwLock<HealthStatus>>,
}

/// RustMQ consumer client (simplified interface)
#[async_trait]
pub trait RustMqConsumer: Send + Sync {
    /// Start consuming messages from the configured topic
    async fn start_consuming(&mut self) -> Result<Box<dyn Stream<Item = Result<RawMessage>> + Unpin + Send>>;
    
    /// Commit offset for a message
    async fn commit_offset(&self, topic: &str, partition: u32, offset: u64) -> Result<()>;
    
    /// Get consumer health status
    async fn health_status(&self) -> ConnectionStatus;
}

/// Raw message from RustMQ
#[derive(Debug, Clone)]
pub struct RawMessage {
    pub topic: String,
    pub partition: u32,
    pub offset: u64,
    pub timestamp: chrono::DateTime<Utc>,
    pub key: Option<Vec<u8>>,
    pub value: Vec<u8>,
    pub headers: std::collections::HashMap<String, String>,
}

impl BigQuerySubscriberService {
    /// Create a new BigQuery subscriber service
    pub async fn new(config: BigQuerySubscriberConfig) -> Result<Self> {
        // Validate configuration
        Self::validate_config(&config)?;
        
        // Create BigQuery writer
        let bigquery_writer = create_bigquery_writer(config.clone()).await?;
        
        // Validate BigQuery table access
        bigquery_writer.validate_table().await?;
        
        // Create table if auto-creation is enabled
        bigquery_writer.create_table_if_not_exists().await?;
        
        let batch_semaphore = Arc::new(Semaphore::new(config.batching.max_concurrent_batches));
        
        let health_status = Arc::new(RwLock::new(HealthStatus {
            status: HealthState::Healthy,
            last_successful_insert: None,
            last_error: None,
            backlog_size: 0,
            messages_per_minute: 0,
            successful_inserts_per_minute: 0,
            failed_inserts_per_minute: 0,
            error_rate: 0.0,
            bigquery_connection: ConnectionStatus::Connected,
            rustmq_connection: ConnectionStatus::Disconnected,
        }));
        
        Ok(Self {
            config,
            bigquery_writer,
            metrics: Arc::new(RwLock::new(SubscriberMetrics::default())),
            message_queue: Arc::new(Mutex::new(VecDeque::new())),
            active_batches: Arc::new(DashMap::new()),
            batch_semaphore,
            shutdown_tx: None,
            health_status,
        })
    }
    
    /// Start the subscriber service
    pub async fn start<C: RustMqConsumer + 'static>(&mut self, mut consumer: C) -> Result<()> {
        info!("Starting BigQuery subscriber service");
        
        // Update health status
        {
            let mut health = self.health_status.write();
            health.rustmq_connection = consumer.health_status().await;
        }
        
        // Create shutdown channel
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel(1);
        self.shutdown_tx = Some(shutdown_tx);
        
        // Start consuming messages from RustMQ
        let mut message_stream = consumer.start_consuming().await?;
        
        // Create message processor
        let processor = MessageProcessor {
            config: self.config.clone(),
            bigquery_writer: Arc::from(self.bigquery_writer.as_ref()),
            metrics: self.metrics.clone(),
            message_queue: self.message_queue.clone(),
            active_batches: self.active_batches.clone(),
            batch_semaphore: self.batch_semaphore.clone(),
            health_status: self.health_status.clone(),
        };
        
        // Start background tasks
        let batch_processor_task = processor.start_batch_processor();
        let metrics_updater_task = self.start_metrics_updater();
        let health_monitor_task = self.start_health_monitor();
        
        // Main message consumption loop
        tokio::select! {
            _ = async {
                while let Some(message_result) = message_stream.next().await {
                    match message_result {
                        Ok(raw_message) => {
                            if let Err(e) = self.process_raw_message(raw_message, &consumer).await {
                                error!("Failed to process message: {}", e);
                                self.update_error_metrics(&e).await;
                            }
                        }
                        Err(e) => {
                            error!("Error receiving message from RustMQ: {}", e);
                            self.update_error_metrics(&e).await;
                        }
                    }
                }
            } => {
                warn!("Message stream ended");
            }
            
            _ = shutdown_rx.recv() => {
                info!("Shutdown signal received");
            }
        }
        
        // Graceful shutdown
        info!("Shutting down BigQuery subscriber service");
        
        // Cancel background tasks
        batch_processor_task.abort();
        metrics_updater_task.abort();
        health_monitor_task.abort();
        
        // Process remaining messages
        self.flush_remaining_messages().await?;
        
        Ok(())
    }
    
    /// Stop the subscriber service gracefully
    pub async fn stop(&self) -> Result<()> {
        if let Some(tx) = &self.shutdown_tx {
            let _ = tx.send(()).await;
        }
        Ok(())
    }
    
    /// Get current health status
    pub fn health_status(&self) -> HealthStatus {
        self.health_status.read().clone()
    }
    
    /// Get current metrics
    pub fn metrics(&self) -> SubscriberMetrics {
        self.metrics.read().clone()
    }
    
    /// Validate the configuration
    fn validate_config(config: &BigQuerySubscriberConfig) -> Result<()> {
        if config.project_id.is_empty() {
            return Err(BigQueryError::Config("project_id is required".to_string()));
        }
        
        if config.dataset.is_empty() {
            return Err(BigQueryError::Config("dataset is required".to_string()));
        }
        
        if config.table.is_empty() {
            return Err(BigQueryError::Config("table is required".to_string()));
        }
        
        if config.subscription.topic.is_empty() {
            return Err(BigQueryError::Config("subscription topic is required".to_string()));
        }
        
        if config.subscription.broker_endpoints.is_empty() {
            return Err(BigQueryError::Config("at least one broker endpoint is required".to_string()));
        }
        
        if config.batching.max_rows_per_batch == 0 {
            return Err(BigQueryError::Config("max_rows_per_batch must be greater than 0".to_string()));
        }
        
        if config.batching.max_batch_size_bytes == 0 {
            return Err(BigQueryError::Config("max_batch_size_bytes must be greater than 0".to_string()));
        }
        
        Ok(())
    }
    
    /// Process a raw message from RustMQ
    async fn process_raw_message<C: RustMqConsumer>(
        &self,
        raw_message: RawMessage,
        consumer: &C,
    ) -> Result<()> {
        // Convert raw message to BigQuery message
        let metadata = MessageMetadata {
            topic: raw_message.topic.clone(),
            partition_id: raw_message.partition,
            offset: raw_message.offset,
            timestamp: raw_message.timestamp,
            key: raw_message.key.as_ref().map(|k| String::from_utf8_lossy(k).to_string()),
            headers: raw_message.headers,
            producer_id: None, // Would need to be extracted from RustMQ message
            sequence_number: None,
        };
        
        // Parse message value as JSON
        let data: serde_json::Value = serde_json::from_slice(&raw_message.value)
            .map_err(|e| BigQueryError::Serialization(e))?;
        
        let mut message = BigQueryMessage::new(data, metadata);
        message.generate_insert_id();
        
        // Add to processing queue
        {
            let mut queue = self.message_queue.lock().await;
            queue.push_back(message);
            
            // Update metrics
            let mut metrics = self.metrics.write();
            metrics.messages_received += 1;
            metrics.current_backlog_size = queue.len();
            if queue.len() > metrics.peak_backlog_size {
                metrics.peak_backlog_size = queue.len();
            }
        }
        
        // Commit offset
        consumer.commit_offset(&raw_message.topic, raw_message.partition, raw_message.offset).await?;
        
        Ok(())
    }
    
    /// Update error metrics
    async fn update_error_metrics<E: std::fmt::Display>(&self, error: &E) {
        let mut metrics = self.metrics.write();
        metrics.messages_failed += 1;
        
        let mut health = self.health_status.write();
        health.last_error = Some(Utc::now());
        health.failed_inserts_per_minute += 1;
    }
    
    /// Flush remaining messages during shutdown
    async fn flush_remaining_messages(&self) -> Result<()> {
        info!("Flushing remaining messages");
        
        let remaining_messages = {
            let mut queue = self.message_queue.lock().await;
            queue.drain(..).collect::<Vec<_>>()
        };
        
        if !remaining_messages.is_empty() {
            warn!("Processing {} remaining messages", remaining_messages.len());
            
            // Create a final batch
            let mut batch = BigQueryBatch::new(Uuid::new_v4().to_string());
            for message in remaining_messages {
                batch.add_message(message);
            }
            
            // Process the batch
            match self.bigquery_writer.insert_batch(batch).await {
                Ok(result) => {
                    if result.success {
                        info!("Successfully flushed remaining messages");
                    } else {
                        warn!("Some messages failed during flush: {:?}", result.errors);
                    }
                }
                Err(e) => {
                    error!("Failed to flush remaining messages: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Start metrics updater task
    fn start_metrics_updater(&self) -> tokio::task::JoinHandle<()> {
        let metrics = self.metrics.clone();
        let health_status = self.health_status.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60));
            
            loop {
                interval.tick().await;
                
                // Update rate metrics (simplified implementation)
                let mut health = health_status.write();
                let metrics_read = metrics.read();
                
                // Calculate error rate
                let total_messages = metrics_read.messages_processed + metrics_read.messages_failed;
                if total_messages > 0 {
                    health.error_rate = (metrics_read.messages_failed as f64 / total_messages as f64) * 100.0;
                }
                
                // Update health status based on error rate
                health.status = if health.error_rate > 10.0 {
                    HealthState::Unhealthy
                } else if health.error_rate > 5.0 {
                    HealthState::Degraded
                } else {
                    HealthState::Healthy
                };
            }
        })
    }
    
    /// Start health monitor task
    fn start_health_monitor(&self) -> tokio::task::JoinHandle<()> {
        let health_status = self.health_status.clone();
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30));
            
            loop {
                interval.tick().await;
                
                // Monitor connection health, update backlog status, etc.
                // This is a simplified implementation
                debug!("Health monitor tick");
            }
        })
    }
}

impl MessageProcessor {
    /// Start the batch processor
    fn start_batch_processor(&self) -> tokio::task::JoinHandle<()> {
        let processor = self.clone();
        
        tokio::spawn(async move {
            processor.run_batch_processor().await;
        })
    }
    
    /// Main batch processing loop
    async fn run_batch_processor(&self) {
        let mut batch_timer = interval(Duration::from_millis(100)); // Check every 100ms
        
        loop {
            batch_timer.tick().await;
            
            // Check if we should create a new batch
            if let Err(e) = self.process_pending_messages().await {
                error!("Error processing pending messages: {}", e);
            }
            
            // Check for aged batches
            if let Err(e) = self.check_aged_batches().await {
                error!("Error checking aged batches: {}", e);
            }
        }
    }
    
    /// Process pending messages in the queue
    async fn process_pending_messages(&self) -> Result<()> {
        let batch_config = &self.config.batching;
        
        // Try to acquire a semaphore permit
        if let Ok(permit) = self.batch_semaphore.try_acquire() {
            let mut messages_to_process = Vec::new();
            
            // Collect messages for a batch
            {
                let mut queue = self.message_queue.lock().await;
                let max_messages = std::cmp::min(batch_config.max_rows_per_batch, queue.len());
                
                for _ in 0..max_messages {
                    if let Some(message) = queue.pop_front() {
                        messages_to_process.push(message);
                    } else {
                        break;
                    }
                }
            }
            
            if !messages_to_process.is_empty() {
                let batch_id = Uuid::new_v4().to_string();
                let mut batch = BigQueryBatch::new(batch_id.clone());
                
                for message in messages_to_process {
                    batch.add_message(message);
                    if batch.is_full(batch_config.max_rows_per_batch, batch_config.max_batch_size_bytes) {
                        break;
                    }
                }
                
                // Store the batch and process it
                self.active_batches.insert(batch_id.clone(), batch.clone());
                self.process_batch(batch, permit).await;
            } else {
                // No messages to process, release the permit
                drop(permit);
            }
        }
        
        Ok(())
    }
    
    /// Check for batches that have aged beyond the maximum latency
    async fn check_aged_batches(&self) -> Result<()> {
        let max_latency = self.config.batching.max_batch_latency_ms;
        let mut aged_batches = Vec::new();
        
        for entry in self.active_batches.iter() {
            let batch = entry.value();
            if batch.is_aged(max_latency) {
                aged_batches.push(entry.key().clone());
            }
        }
        
        for batch_id in aged_batches {
            if let Some((_, batch)) = self.active_batches.remove(&batch_id) {
                if let Ok(permit) = self.batch_semaphore.try_acquire() {
                    self.process_batch(batch, permit).await;
                } else {
                    // Put the batch back if we can't get a permit
                    self.active_batches.insert(batch_id, batch);
                }
            }
        }
        
        Ok(())
    }
    
    /// Process a single batch
    async fn process_batch(&self, batch: BigQueryBatch, _permit: tokio::sync::SemaphorePermit<'_>) {
        let batch_id = batch.metadata.batch_id.clone();
        let message_count = batch.messages.len();
        
        debug!("Processing batch {} with {} messages", batch_id, message_count);
        
        let result = self.process_batch_with_retries(batch).await;
        
        match result {
            Ok(insert_result) => {
                if insert_result.success {
                    info!("Successfully processed batch {} ({} messages)", batch_id, message_count);
                    
                    // Update metrics
                    let mut metrics = self.metrics.write();
                    metrics.messages_processed += insert_result.rows_inserted as u64;
                    metrics.successful_api_calls += 1;
                    metrics.batches_sent += 1;
                    metrics.bytes_processed += insert_result.stats.bytes_sent as u64;
                    
                    // Update health status
                    let mut health = self.health_status.write();
                    health.last_successful_insert = Some(Utc::now());
                    health.successful_inserts_per_minute += insert_result.rows_inserted as u64;
                } else {
                    warn!("Batch {} failed with errors: {:?}", batch_id, insert_result.errors);
                    
                    // Update metrics
                    let mut metrics = self.metrics.write();
                    metrics.messages_failed += message_count as u64;
                    metrics.failed_api_calls += 1;
                }
            }
            Err(e) => {
                error!("Failed to process batch {}: {}", batch_id, e);
                
                // Update metrics
                let mut metrics = self.metrics.write();
                metrics.messages_failed += message_count as u64;
                metrics.failed_api_calls += 1;
            }
        }
        
        // The permit is automatically dropped here, allowing another batch to be processed
    }
    
    /// Process a batch with retries
    async fn process_batch_with_retries(&self, mut batch: BigQueryBatch) -> Result<InsertResult> {
        let max_retries = self.config.error_handling.max_retries;
        
        for attempt in 0..=max_retries {
            batch.metadata.retry_attempt = attempt;
            
            match self.bigquery_writer.insert_batch(batch.clone()).await {
                Ok(result) => {
                    if result.success || !self.should_retry(&result.errors) {
                        return Ok(result);
                    }
                    
                    if attempt < max_retries {
                        let delay = self.calculate_retry_delay(attempt);
                        warn!(
                            "Batch {} failed (attempt {}), retrying in {}ms", 
                            batch.metadata.batch_id, 
                            attempt + 1, 
                            delay.as_millis()
                        );
                        sleep(delay).await;
                    } else {
                        warn!("Batch {} failed after {} attempts", batch.metadata.batch_id, max_retries + 1);
                        self.handle_dead_letter(&batch).await;
                        return Ok(result);
                    }
                }
                Err(e) => {
                    if attempt < max_retries && self.is_retryable_error(&e) {
                        let delay = self.calculate_retry_delay(attempt);
                        warn!(
                            "Batch {} error (attempt {}), retrying in {}ms: {}", 
                            batch.metadata.batch_id, 
                            attempt + 1, 
                            delay.as_millis(),
                            e
                        );
                        sleep(delay).await;
                    } else {
                        error!("Batch {} failed permanently: {}", batch.metadata.batch_id, e);
                        self.handle_dead_letter(&batch).await;
                        return Err(e);
                    }
                }
            }
        }
        
        unreachable!()
    }
    
    /// Check if errors are retryable
    fn should_retry(&self, errors: &[crate::subscribers::bigquery::types::InsertError]) -> bool {
        errors.iter().any(|e| e.retryable)
    }
    
    /// Check if an error is retryable
    fn is_retryable_error(&self, error: &BigQueryError) -> bool {
        match error {
            BigQueryError::Client(_) => true,
            BigQueryError::QuotaExceeded(_) => true,
            BigQueryError::StreamingInsertFailed(_) => true,
            _ => false,
        }
    }
    
    /// Calculate retry delay based on backoff strategy
    fn calculate_retry_delay(&self, attempt: usize) -> Duration {
        match &self.config.error_handling.retry_backoff {
            RetryBackoffStrategy::Linear { increment_ms } => {
                Duration::from_millis(increment_ms * (attempt as u64 + 1))
            }
            RetryBackoffStrategy::Exponential { base_ms, max_ms } => {
                let delay = base_ms * 2_u64.pow(attempt as u32);
                Duration::from_millis(std::cmp::min(delay, *max_ms))
            }
            RetryBackoffStrategy::Fixed { delay_ms } => {
                Duration::from_millis(*delay_ms)
            }
        }
    }
    
    /// Handle dead letter messages
    async fn handle_dead_letter(&self, batch: &BigQueryBatch) {
        match &self.config.error_handling.dead_letter_action {
            DeadLetterAction::Drop => {
                warn!("Dropping {} messages from failed batch {}", 
                     batch.messages.len(), batch.metadata.batch_id);
            }
            DeadLetterAction::Log => {
                error!("Dead letter batch {}: {} messages", 
                      batch.metadata.batch_id, batch.messages.len());
                for (i, message) in batch.messages.iter().enumerate() {
                    error!("Dead letter message {}: {:?}", i, message.metadata);
                }
            }
            DeadLetterAction::DeadLetterQueue => {
                // TODO: Implement dead letter queue
                warn!("Dead letter queue not implemented, logging instead");
            }
            DeadLetterAction::File { path } => {
                // TODO: Implement file-based dead letter handling
                warn!("File-based dead letter handling not implemented, logging instead");
            }
        }
    }
}

impl Clone for MessageProcessor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            bigquery_writer: self.bigquery_writer.clone(),
            metrics: self.metrics.clone(),
            message_queue: self.message_queue.clone(),
            active_batches: self.active_batches.clone(),
            batch_semaphore: self.batch_semaphore.clone(),
            health_status: self.health_status.clone(),
        }
    }
}