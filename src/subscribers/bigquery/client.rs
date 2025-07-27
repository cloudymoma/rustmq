use async_trait::async_trait;
use gcp_bigquery_client::{Client as BQClient, model::{table_data_insert_all_request::TableDataInsertAllRequest, table_data_insert_all_request_rows::TableDataInsertAllRequestRows}};
use google_cloud_auth::{project::Config as ProjectConfig, token::DefaultTokenSourceProvider};
use serde_json::Value;
use std::sync::Arc;
use tokio::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

use crate::subscribers::bigquery::{
    config::{BigQuerySubscriberConfig, WriteMethod, AuthConfig, AuthMethod},
    error::{BigQueryError, Result},
    types::{BigQueryBatch, BigQueryMessage, InsertResult, InsertError, InsertStats},
};

/// BigQuery client wrapper that handles different write methods
#[async_trait]
pub trait BigQueryWriter: Send + Sync {
    /// Insert a batch of messages into BigQuery
    async fn insert_batch(&self, batch: BigQueryBatch) -> Result<InsertResult>;
    
    /// Validate that the target table exists and is accessible
    async fn validate_table(&self) -> Result<()>;
    
    /// Get table schema
    async fn get_table_schema(&self) -> Result<Value>;
    
    /// Create table if it doesn't exist (if auto_create_table is enabled)
    async fn create_table_if_not_exists(&self) -> Result<()>;
}

/// Streaming Inserts implementation of BigQueryWriter
pub struct StreamingInsertsClient {
    client: Arc<BQClient>,
    config: BigQuerySubscriberConfig,
}

/// Storage Write API implementation of BigQueryWriter (placeholder for future implementation)
pub struct StorageWriteClient {
    config: BigQuerySubscriberConfig,
}

impl StreamingInsertsClient {
    /// Create a new streaming inserts client
    pub async fn new(config: BigQuerySubscriberConfig) -> Result<Self> {
        let client = Self::create_client(&config.auth).await?;
        
        Ok(Self {
            client: Arc::new(client),
            config,
        })
    }
    
    /// Create BigQuery client with proper authentication
    async fn create_client(auth_config: &AuthConfig) -> Result<BQClient> {
        let token_source_provider = match auth_config.method {
            AuthMethod::ServiceAccount => {
                if let Some(key_file) = &auth_config.service_account_key_file {
                    let config = ProjectConfig::from_file(key_file).await?;
                    DefaultTokenSourceProvider::new(config).await?
                } else {
                    return Err(BigQueryError::Config(
                        "Service account key file required for service account auth".to_string()
                    ));
                }
            }
            AuthMethod::MetadataServer => {
                let config = ProjectConfig::from_metadata_server().await?;
                DefaultTokenSourceProvider::new(config).await?
            }
            AuthMethod::ApplicationDefault => {
                let config = ProjectConfig::default().with_scopes(&auth_config.scopes);
                DefaultTokenSourceProvider::new(config).await?
            }
        };
        
        let client = BQClient::from_token_source_provider(Box::new(token_source_provider)).await?;
        Ok(client)
    }
}

#[async_trait]
impl BigQueryWriter for StreamingInsertsClient {
    async fn insert_batch(&self, batch: BigQueryBatch) -> Result<InsertResult> {
        let start_time = Instant::now();
        let transformation_start = Instant::now();
        
        // Transform messages into BigQuery format
        let mut rows = Vec::new();
        let mut transformation_errors = Vec::new();
        
        for (index, message) in batch.messages.iter().enumerate() {
            match self.transform_message(message) {
                Ok(row) => rows.push(row),
                Err(e) => {
                    transformation_errors.push(InsertError {
                        message: format!("Transformation failed: {}", e),
                        code: Some("TRANSFORMATION_ERROR".to_string()),
                        row_index: Some(index),
                        field: None,
                        retryable: false,
                    });
                }
            }
        }
        
        let transformation_time = transformation_start.elapsed();
        
        if !transformation_errors.is_empty() {
            warn!(
                "Failed to transform {} out of {} messages", 
                transformation_errors.len(), 
                batch.messages.len()
            );
        }
        
        if rows.is_empty() {
            let stats = InsertStats {
                duration_ms: start_time.elapsed().as_millis() as u64,
                transformation_time_ms: transformation_time.as_millis() as u64,
                api_time_ms: 0,
                bytes_sent: 0,
                retry_count: batch.metadata.retry_attempt,
            };
            
            return Ok(InsertResult::failure(transformation_errors, stats));
        }
        
        // Prepare the insert request
        let write_method = &self.config.write_method;
        let (skip_invalid_rows, ignore_unknown_values, template_suffix) = match write_method {
            WriteMethod::StreamingInserts { 
                skip_invalid_rows, 
                ignore_unknown_values, 
                template_suffix 
            } => (*skip_invalid_rows, *ignore_unknown_values, template_suffix.clone()),
            _ => (false, false, None),
        };
        
        let mut request = TableDataInsertAllRequest {
            ignore_unknown_values: Some(ignore_unknown_values),
            skip_invalid_rows: Some(skip_invalid_rows),
            template_suffix,
            rows: Some(rows),
            ..Default::default()
        };
        
        // Perform the API call
        let api_start = Instant::now();
        let result = self.client
            .tabledata()
            .insert_all(
                &self.config.project_id,
                &self.config.dataset,
                &self.config.table,
                request,
            )
            .await;
        
        let api_time = api_start.elapsed();
        let total_time = start_time.elapsed();
        
        let stats = InsertStats {
            duration_ms: total_time.as_millis() as u64,
            transformation_time_ms: transformation_time.as_millis() as u64,
            api_time_ms: api_time.as_millis() as u64,
            bytes_sent: batch.metadata.size_bytes,
            retry_count: batch.metadata.retry_attempt,
        };
        
        match result {
            Ok(response) => {
                if let Some(insert_errors) = response.insert_errors {
                    if !insert_errors.is_empty() {
                        let errors = insert_errors
                            .into_iter()
                            .map(|err| InsertError {
                                message: err.errors
                                    .map(|errs| errs.into_iter()
                                        .map(|e| e.message.unwrap_or_default())
                                        .collect::<Vec<_>>()
                                        .join("; "))
                                    .unwrap_or_default(),
                                code: None,
                                row_index: err.index.map(|i| i as usize),
                                field: None,
                                retryable: true,
                            })
                            .collect();
                        
                        warn!("BigQuery insert had {} errors", errors.len());
                        return Ok(InsertResult::failure(errors, stats));
                    }
                }
                
                let rows_inserted = batch.messages.len() - transformation_errors.len();
                info!(
                    "Successfully inserted {} rows to {}.{}.{} in {}ms",
                    rows_inserted,
                    self.config.project_id,
                    self.config.dataset,
                    self.config.table,
                    stats.duration_ms
                );
                
                Ok(InsertResult::success(rows_inserted, stats))
            }
            Err(e) => {
                error!("BigQuery API call failed: {}", e);
                let errors = vec![InsertError {
                    message: e.to_string(),
                    code: None,
                    row_index: None,
                    field: None,
                    retryable: Self::is_retryable_error(&e),
                }];
                
                Ok(InsertResult::failure(errors, stats))
            }
        }
    }
    
    async fn validate_table(&self) -> Result<()> {
        debug!(
            "Validating table {}.{}.{}",
            self.config.project_id, self.config.dataset, self.config.table
        );
        
        match self.client
            .table()
            .get(&self.config.project_id, &self.config.dataset, &self.config.table)
            .await
        {
            Ok(_) => {
                debug!("Table validation successful");
                Ok(())
            }
            Err(e) => {
                error!("Table validation failed: {}", e);
                Err(BigQueryError::TableNotFound {
                    project: self.config.project_id.clone(),
                    dataset: self.config.dataset.clone(),
                    table: self.config.table.clone(),
                })
            }
        }
    }
    
    async fn get_table_schema(&self) -> Result<Value> {
        let table = self.client
            .table()
            .get(&self.config.project_id, &self.config.dataset, &self.config.table)
            .await?;
        
        Ok(serde_json::to_value(table.schema)?)
    }
    
    async fn create_table_if_not_exists(&self) -> Result<()> {
        if !self.config.schema.auto_create_table {
            return Ok(());
        }
        
        // Check if table exists first
        if self.validate_table().await.is_ok() {
            return Ok(());
        }
        
        // Table doesn't exist, create it if we have a schema
        if let Some(table_schema) = &self.config.schema.table_schema {
            info!(
                "Creating table {}.{}.{}",
                self.config.project_id, self.config.dataset, self.config.table
            );
            
            // This would require additional BigQuery table creation logic
            // For now, return an error indicating manual table creation is required
            return Err(BigQueryError::Config(
                "Auto table creation not yet implemented. Please create the table manually.".to_string()
            ));
        }
        
        Err(BigQueryError::Config(
            "Cannot create table: no schema provided in configuration".to_string()
        ))
    }
}

impl StreamingInsertsClient {
    /// Transform a RustMQ message into BigQuery row format
    fn transform_message(&self, message: &BigQueryMessage) -> Result<TableDataInsertAllRequestRows> {
        let data = if let Some(transformed) = &message.transformed_data {
            transformed.clone()
        } else {
            // Apply schema transformation based on configuration
            self.apply_schema_transformation(&message.data)?
        };
        
        let mut row = TableDataInsertAllRequestRows {
            json: Some(data),
            ..Default::default()
        };
        
        // Set insert ID for deduplication if available
        if let Some(insert_id) = &message.insert_id {
            row.insert_id = Some(insert_id.clone());
        }
        
        Ok(row)
    }
    
    /// Apply schema transformation based on configuration
    fn apply_schema_transformation(&self, data: &Value) -> Result<Value> {
        use crate::subscribers::bigquery::config::SchemaMappingStrategy;
        
        match &self.config.schema.mapping {
            SchemaMappingStrategy::Direct => {
                // Use data as-is
                Ok(data.clone())
            }
            SchemaMappingStrategy::Custom => {
                // Apply custom field mappings
                let mut result = serde_json::Map::new();
                
                for (source_field, target_field) in &self.config.schema.column_mappings {
                    if let Some(value) = data.get(source_field) {
                        result.insert(target_field.clone(), value.clone());
                    }
                }
                
                // Add default values for missing fields
                for (field, default_value) in &self.config.schema.default_values {
                    if !result.contains_key(field) {
                        result.insert(field.clone(), default_value.clone());
                    }
                }
                
                Ok(Value::Object(result))
            }
            SchemaMappingStrategy::Nested { root_field } => {
                // Extract nested data
                if let Some(nested_data) = data.get(root_field) {
                    Ok(nested_data.clone())
                } else {
                    Err(BigQueryError::Transformation(
                        format!("Root field '{}' not found in message", root_field)
                    ))
                }
            }
        }
    }
    
    /// Check if an error is retryable
    fn is_retryable_error(error: &gcp_bigquery_client::error::BQError) -> bool {
        // Implement logic to determine if error is retryable
        // This is a simplified implementation
        match error {
            gcp_bigquery_client::error::BQError::RequestError(_) => true,
            gcp_bigquery_client::error::BQError::ResponseError { .. } => false,
            _ => false,
        }
    }
}

#[async_trait]
impl BigQueryWriter for StorageWriteClient {
    async fn insert_batch(&self, _batch: BigQueryBatch) -> Result<InsertResult> {
        // Placeholder for Storage Write API implementation
        Err(BigQueryError::Config(
            "Storage Write API not yet implemented. Use streaming_inserts method.".to_string()
        ))
    }
    
    async fn validate_table(&self) -> Result<()> {
        Err(BigQueryError::Config(
            "Storage Write API not yet implemented".to_string()
        ))
    }
    
    async fn get_table_schema(&self) -> Result<Value> {
        Err(BigQueryError::Config(
            "Storage Write API not yet implemented".to_string()
        ))
    }
    
    async fn create_table_if_not_exists(&self) -> Result<()> {
        Err(BigQueryError::Config(
            "Storage Write API not yet implemented".to_string()
        ))
    }
}

/// Factory function to create the appropriate BigQuery writer
pub async fn create_bigquery_writer(config: BigQuerySubscriberConfig) -> Result<Box<dyn BigQueryWriter>> {
    match &config.write_method {
        WriteMethod::StreamingInserts { .. } => {
            let client = StreamingInsertsClient::new(config).await?;
            Ok(Box::new(client))
        }
        WriteMethod::StorageWrite { .. } => {
            let client = StorageWriteClient { config };
            Ok(Box::new(client))
        }
    }
}