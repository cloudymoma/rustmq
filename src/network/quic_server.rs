use crate::{Result, types::*, config::NetworkConfig};
use bytes::Bytes;
use std::sync::Arc;
use std::net::SocketAddr;
use tokio::sync::RwLock;
use std::collections::HashMap;
use quinn::{Endpoint, ServerConfig, Connection};
use std::time::Duration;

pub struct QuicServer {
    endpoint: Endpoint,
    connection_pool: Arc<ConnectionPool>,
    request_router: Arc<RequestRouter>,
    config: NetworkConfig,
}

pub struct ConnectionPool {
    connections: RwLock<HashMap<String, Connection>>,
    max_connections: usize,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            max_connections,
        }
    }

    pub async fn get_connection(&self, client_id: &str) -> Option<Connection> {
        let connections = self.connections.read().await;
        connections.get(client_id).cloned()
    }

    pub async fn add_connection(&self, client_id: String, connection: Connection) {
        let mut connections = self.connections.write().await;
        if connections.len() >= self.max_connections {
            if let Some((oldest_id, _)) = connections.iter().next() {
                let oldest_id = oldest_id.clone();
                connections.remove(&oldest_id);
            }
        }
        connections.insert(client_id, connection);
    }

    pub async fn remove_connection(&self, client_id: &str) {
        let mut connections = self.connections.write().await;
        connections.remove(client_id);
    }
}

pub struct RequestRouter {
    producer_handler: Arc<dyn ProduceHandler>,
    consumer_handler: Arc<dyn FetchHandler>,
    metadata_handler: Arc<dyn MetadataHandler>,
}

#[async_trait::async_trait]
pub trait ProduceHandler: Send + Sync {
    async fn handle_produce(&self, request: ProduceRequest) -> Result<ProduceResponse>;
}

#[async_trait::async_trait]
pub trait FetchHandler: Send + Sync {
    async fn handle_fetch(&self, request: FetchRequest) -> Result<FetchResponse>;
}

#[async_trait::async_trait]
pub trait MetadataHandler: Send + Sync {
    async fn handle_metadata(&self, request: MetadataRequest) -> Result<MetadataResponse>;
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetadataRequest {
    pub topics: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MetadataResponse {
    pub brokers: Vec<BrokerInfo>,
    pub topics_metadata: Vec<TopicMetadata>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicMetadata {
    pub name: String,
    pub partitions: Vec<PartitionMetadata>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TopicPartitionMetadata {
    pub partition_id: u32,
    pub leader: Option<BrokerId>,
    pub replicas: Vec<BrokerId>,
    pub in_sync_replicas: Vec<BrokerId>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PartitionMetadata {
    pub partition_id: u32,
    pub leader: Option<BrokerId>,
    pub replicas: Vec<BrokerId>,
}

impl RequestRouter {
    pub fn new(
        producer_handler: Arc<dyn ProduceHandler>,
        consumer_handler: Arc<dyn FetchHandler>,
        metadata_handler: Arc<dyn MetadataHandler>,
    ) -> Self {
        Self {
            producer_handler,
            consumer_handler,
            metadata_handler,
        }
    }

    pub async fn route_request(&self, request_type: RequestType, data: Bytes) -> Result<Bytes> {
        match request_type {
            RequestType::Produce => {
                let request: ProduceRequest = bincode::deserialize(&data)?;
                let response = self.producer_handler.handle_produce(request).await?;
                let response_data = bincode::serialize(&response)?;
                Ok(Bytes::from(response_data))
            }
            RequestType::Fetch => {
                let request: FetchRequest = bincode::deserialize(&data)?;
                let response = self.consumer_handler.handle_fetch(request).await?;
                let response_data = bincode::serialize(&response)?;
                Ok(Bytes::from(response_data))
            }
            RequestType::Metadata => {
                let request: MetadataRequest = bincode::deserialize(&data)?;
                let response = self.metadata_handler.handle_metadata(request).await?;
                let response_data = bincode::serialize(&response)?;
                Ok(Bytes::from(response_data))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RequestType {
    Produce = 1,
    Fetch = 2,
    Metadata = 3,
}

impl TryFrom<u8> for RequestType {
    type Error = crate::error::RustMqError;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            1 => Ok(RequestType::Produce),
            2 => Ok(RequestType::Fetch),
            3 => Ok(RequestType::Metadata),
            _ => Err(crate::error::RustMqError::Network(format!("Invalid request type: {value}"))),
        }
    }
}

impl QuicServer {
    pub async fn new(
        config: NetworkConfig,
        producer_handler: Arc<dyn ProduceHandler>,
        consumer_handler: Arc<dyn FetchHandler>,
        metadata_handler: Arc<dyn MetadataHandler>,
    ) -> Result<Self> {
        let server_config = Self::create_server_config()?;
        let addr: SocketAddr = config.quic_listen.parse()
            .map_err(|e| crate::error::RustMqError::Config(format!("Invalid QUIC address: {e}")))?;
        
        let endpoint = Endpoint::server(server_config, addr)?;
        
        let connection_pool = Arc::new(ConnectionPool::new(config.max_connections));
        let request_router = Arc::new(RequestRouter::new(
            producer_handler,
            consumer_handler,
            metadata_handler,
        ));

        Ok(Self {
            endpoint,
            connection_pool,
            request_router,
            config,
        })
    }

    fn create_server_config() -> Result<ServerConfig> {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
        let cert_der = cert.serialize_der()?;
        let priv_key = cert.serialize_private_key_der();

        let mut server_config = ServerConfig::with_single_cert(
            vec![rustls::Certificate(cert_der)],
            rustls::PrivateKey(priv_key),
        )?;

        let transport_config = Arc::get_mut(&mut server_config.transport).unwrap();
        transport_config.max_concurrent_uni_streams(1000_u32.into());
        transport_config.max_concurrent_bidi_streams(1000_u32.into());
        transport_config.max_idle_timeout(Some(Duration::from_secs(30).try_into().unwrap()));

        Ok(server_config)
    }

    pub async fn start(&self) -> Result<()> {
        tracing::info!("Starting QUIC server on {}", self.config.quic_listen);

        while let Some(conn) = self.endpoint.accept().await {
            let connection = conn.await?;
            let client_id = format!("{}", connection.remote_address());
            
            self.connection_pool.add_connection(client_id.clone(), connection.clone()).await;
            
            let router = self.request_router.clone();
            let pool = self.connection_pool.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(connection, router).await {
                    tracing::error!("Connection error: {}", e);
                }
                pool.remove_connection(&client_id).await;
            });
        }

        Ok(())
    }

    async fn handle_connection(
        connection: Connection,
        router: Arc<RequestRouter>,
    ) -> Result<()> {
        while let Ok((mut send, mut recv)) = connection.accept_bi().await {
            let router = router.clone();
            
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 8192];
                
                if let Ok(Some(size)) = recv.read(&mut buffer).await {
                    if size >= 1 {
                        let request_type = RequestType::try_from(buffer[0])?;
                        let request_data = Bytes::from(buffer[1..size].to_vec());
                        
                        match router.route_request(request_type, request_data).await {
                            Ok(response_data) => {
                                let mut response_with_type = vec![request_type as u8];
                                response_with_type.extend_from_slice(&response_data);
                                
                                if let Err(e) = send.write_all(&response_with_type).await {
                                    tracing::error!("Failed to send response: {}", e);
                                }
                                
                                if let Err(e) = send.finish().await {
                                    tracing::error!("Failed to finish stream: {}", e);
                                }
                            }
                            Err(e) => {
                                tracing::error!("Request processing error: {}", e);
                                let error_response = format!("Error: {e}");
                                let mut error_with_type = vec![255u8]; // Error indicator
                                error_with_type.extend_from_slice(error_response.as_bytes());
                                
                                let _ = send.write_all(&error_with_type).await;
                                let _ = send.finish().await;
                            }
                        }
                    }
                }
                
                Ok::<(), crate::error::RustMqError>(())
            });
        }

        Ok(())
    }
}

pub struct MockProduceHandler;

#[async_trait::async_trait]
impl ProduceHandler for MockProduceHandler {
    async fn handle_produce(&self, _request: ProduceRequest) -> Result<ProduceResponse> {
        Ok(ProduceResponse {
            offset: 123,
            error_code: 0,
            error_message: None,
        })
    }
}

pub struct MockFetchHandler;

#[async_trait::async_trait]
impl FetchHandler for MockFetchHandler {
    async fn handle_fetch(&self, _request: FetchRequest) -> Result<FetchResponse> {
        Ok(FetchResponse {
            records: vec![],
            high_watermark: 456,
            error_code: 0,
            error_message: None,
        })
    }
}

pub struct MockMetadataHandler;

#[async_trait::async_trait]
impl MetadataHandler for MockMetadataHandler {
    async fn handle_metadata(&self, _request: MetadataRequest) -> Result<MetadataResponse> {
        Ok(MetadataResponse {
            brokers: vec![BrokerInfo {
                id: "broker-1".to_string(),
                host: "localhost".to_string(),
                port_quic: 9092,
                port_rpc: 9093,
                rack_id: "rack-1".to_string(),
            }],
            topics_metadata: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connection_pool() {
        let pool = ConnectionPool::new(2);
        
        assert!(pool.get_connection("client1").await.is_none());
        
        // We can't easily create real QUIC connections in tests,
        // so we'll just test the pool logic structure
        assert_eq!(pool.connections.read().await.len(), 0);
    }

    #[tokio::test]
    async fn test_request_type_conversion() {
        assert!(matches!(RequestType::try_from(1).unwrap(), RequestType::Produce));
        assert!(matches!(RequestType::try_from(2).unwrap(), RequestType::Fetch));
        assert!(matches!(RequestType::try_from(3).unwrap(), RequestType::Metadata));
        assert!(RequestType::try_from(99).is_err());
    }

    #[tokio::test]
    async fn test_request_router() {
        let router = RequestRouter::new(
            Arc::new(MockProduceHandler),
            Arc::new(MockFetchHandler),
            Arc::new(MockMetadataHandler),
        );

        let produce_request = ProduceRequest {
            topic: "test-topic".to_string(),
            partition_id: 0,
            records: vec![],
            acks: AcknowledgmentLevel::Leader,
            timeout_ms: 5000,
        };

        let request_data = bincode::serialize(&produce_request).unwrap();
        let response = router
            .route_request(RequestType::Produce, Bytes::from(request_data))
            .await
            .unwrap();

        let response: ProduceResponse = bincode::deserialize(&response).unwrap();
        assert_eq!(response.offset, 123);
        assert_eq!(response.error_code, 0);
    }
}