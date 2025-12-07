use crate::{
    config::{ClientConfig, TlsConfig},
    error::{ClientError, Result},
    security::{SecurityManager, SecurityContext},
};
use quinn::{
    Connection as QuicConnection, Endpoint, ClientConfig as QuinnClientConfig,
    TransportConfig, VarInt,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::{
    collections::HashMap,
    net::{SocketAddr, ToSocketAddrs},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{RwLock, Semaphore},
    time::{sleep, timeout},
};
use tracing::{debug, error, info, warn, instrument};
use uuid::Uuid;

/// Connection pool entry with metadata
#[derive(Clone, Debug)]
struct ConnectionEntry {
    connection: QuicConnection,
    broker_addr: SocketAddr,
    created_at: Instant,
    last_used: Arc<RwLock<Instant>>,
    error_count: Arc<AtomicUsize>,
}

/// QUIC-based connection to RustMQ brokers with security support
pub struct Connection {
    config: Arc<ClientConfig>,
    endpoint: Arc<Endpoint>,
    connections: Arc<RwLock<HashMap<SocketAddr, ConnectionEntry>>>,
    current_index: Arc<AtomicUsize>,
    connection_semaphore: Arc<Semaphore>,
    health_check_interval: Duration,
    security_manager: Option<Arc<SecurityManager>>,
    security_context: Arc<RwLock<Option<SecurityContext>>>,
}

impl Connection {
    /// Create a new connection to RustMQ brokers
    pub async fn new(config: &ClientConfig) -> Result<Self> {
        info!("Creating new QUIC connection with {} brokers", config.brokers.len());
        
        let endpoint = Self::create_endpoint(config).await?;
        let connections = Self::establish_connections(&endpoint, config).await?;
        
        // Initialize security manager if security is enabled
        let (security_manager, security_context) = if let Some(auth_config) = &config.auth {
            if auth_config.security.as_ref().map(|s| s.enabled).unwrap_or(false) {
                let security_manager = Arc::new(SecurityManager::new(
                    auth_config.security.clone().unwrap()
                )?);
                
                // Perform initial authentication
                let default_tls_config = TlsConfig::default();
                let tls_config = config.tls_config.as_ref().unwrap_or(&default_tls_config);
                let context = security_manager.authenticate(tls_config, auth_config).await?;
                
                (Some(security_manager), Arc::new(RwLock::new(Some(context))))
            } else {
                (None, Arc::new(RwLock::new(None)))
            }
        } else {
            (None, Arc::new(RwLock::new(None)))
        };
        
        let connection = Self {
            config: Arc::new(config.clone()),
            endpoint: Arc::new(endpoint),
            connections: Arc::new(RwLock::new(connections)),
            current_index: Arc::new(AtomicUsize::new(0)),
            connection_semaphore: Arc::new(Semaphore::new(config.max_connections)),
            health_check_interval: config.keep_alive_interval,
            security_manager,
            security_context,
        };
        
        // Start background health check task
        connection.start_health_check_task();
        
        Ok(connection)
    }

    /// Create QUIC endpoint with configuration
    #[instrument(skip(config))]
    async fn create_endpoint(config: &ClientConfig) -> Result<Endpoint> {
        debug!("Creating QUIC endpoint with TLS enabled: {}", config.enable_tls);
        
        // Configure transport parameters for message queue workloads
        let mut transport_config = TransportConfig::default();
        
        // Optimize for message queue patterns
        transport_config
            .max_concurrent_bidi_streams(VarInt::from_u32(1000))
            .max_concurrent_uni_streams(VarInt::from_u32(1000))
            .stream_receive_window(VarInt::from_u32(1024 * 1024)) // 1MB
            .receive_window(VarInt::from_u32(8 * 1024 * 1024)) // 8MB
            .send_window(8 * 1024 * 1024) // 8MB
            .max_idle_timeout(Some(config.keep_alive_interval.try_into().map_err(|_| {
                ClientError::InvalidConfig("Invalid keep alive interval".to_string())
            })?))
            .keep_alive_interval(Some(config.keep_alive_interval / 2));
        
        let mut client_config = if config.enable_tls {
            Self::create_tls_client_config(config)?
        } else {
            // Create insecure config for development
            warn!("Creating insecure QUIC endpoint - not recommended for production");
            Self::create_insecure_client_config()?
        };
        
        client_config.transport_config(Arc::new(transport_config));
        
        // Bind to any available port
        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().map_err(|e| {
            ClientError::InvalidConfig(format!("Invalid bind address: {}", e))
        })?)?;
        
        endpoint.set_default_client_config(client_config);
        
        info!("QUIC endpoint created successfully");
        Ok(endpoint)
    }
    
    /// Create TLS client configuration with security support
    fn create_tls_client_config(config: &ClientConfig) -> Result<QuinnClientConfig> {
        let default_tls_config = TlsConfig::default();
        let tls_config = config.tls_config.as_ref().unwrap_or(&default_tls_config);
        
        if !tls_config.is_enabled() {
            return Self::create_insecure_client_config();
        }
        
        // Validate TLS configuration
        tls_config.validate().map_err(|e| {
            ClientError::InvalidConfig(format!("Invalid TLS configuration: {}", e))
        })?;
        
        // Create security manager for TLS configuration if needed
        let rustls_config = if let Some(auth_config) = &config.auth {
            if auth_config.security.as_ref().map(|s| s.enabled).unwrap_or(false) {
                let security_manager = SecurityManager::new(
                    auth_config.security.clone().unwrap()
                )?;
                security_manager.create_rustls_config(tls_config)?
            } else {
                Self::create_basic_tls_config(tls_config)?
            }
        } else {
            Self::create_basic_tls_config(tls_config)?
        };
        
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(rustls_config).map_err(|e| {
            ClientError::Tls(format!("Failed to create QUIC client config: {}", e))
        })?;
        
        Ok(QuinnClientConfig::new(Arc::new(crypto)))
    }
    
    /// Create basic TLS configuration without full security manager
    fn create_basic_tls_config(tls_config: &TlsConfig) -> Result<rustls::ClientConfig> {
        let mut root_store = rustls::RootCertStore::empty();
        
        if let Some(ca_cert_pem) = &tls_config.ca_cert {
            // Load custom CA certificates
            let mut reader = std::io::BufReader::new(ca_cert_pem.as_bytes());
            let certs: std::result::Result<Vec<_>, _> = rustls_pemfile::certs(&mut reader).collect();
            let certs = certs.map_err(|e| {
                ClientError::InvalidCertificate {
                    reason: format!("Failed to parse CA certificate: {}", e),
                }
            })?;
            
            for cert in certs {
                root_store.add(CertificateDer::from(cert)).map_err(|e| {
                    ClientError::InvalidCertificate {
                        reason: format!("Failed to add CA certificate: {}", e),
                    }
                })?;
            }
        } else if !tls_config.insecure_skip_verify {
            // Use system root certificates
            root_store.extend(
                webpki_roots::TLS_SERVER_ROOTS.iter().cloned()
            );
        }
        
        let config_builder = rustls::ClientConfig::builder()
            .with_root_certificates(root_store);
        
        // Configure client authentication if mTLS is enabled
        let rustls_config = if tls_config.requires_client_cert() {
            let client_cert_chain = Self::load_client_certificate_chain(tls_config)?;
            let client_key = Self::load_client_private_key(tls_config)?;
            
            config_builder
                .with_client_auth_cert(client_cert_chain, client_key)
                .map_err(|e| ClientError::Tls(format!("Failed to configure client certificate: {}", e)))?
        } else {
            config_builder.with_no_client_auth()
        };
        
        Ok(rustls_config)
    }
    
    /// Load client certificate chain from TLS config
    fn load_client_certificate_chain(tls_config: &TlsConfig) -> Result<Vec<CertificateDer<'static>>> {
        let client_cert_pem = tls_config.client_cert.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Client certificate required for mTLS".to_string())
        })?;
        
        let mut reader = std::io::BufReader::new(client_cert_pem.as_bytes());
        let certs: std::result::Result<Vec<_>, _> = rustls_pemfile::certs(&mut reader).collect();
        let certs = certs.map_err(|e| {
            ClientError::InvalidCertificate {
                reason: format!("Failed to parse client certificate: {}", e),
            }
        })?;
        
        Ok(certs.into_iter().map(CertificateDer::from).collect())
    }
    
    /// Load client private key from TLS config
    fn load_client_private_key(tls_config: &TlsConfig) -> Result<PrivateKeyDer<'static>> {
        let client_key_pem = tls_config.client_key.as_ref().ok_or_else(|| {
            ClientError::InvalidConfig("Client private key required for mTLS".to_string())
        })?;
        
        let mut reader = std::io::BufReader::new(client_key_pem.as_bytes());
        
        // Try PKCS#8 format first
        let keys: std::result::Result<Vec<_>, _> = rustls_pemfile::pkcs8_private_keys(&mut reader).collect();
        if let Ok(mut keys) = keys {
            if !keys.is_empty() {
                return Ok(PrivateKeyDer::from(keys.remove(0)));
            }
        }
        
        // Try RSA format
        let mut reader = std::io::BufReader::new(client_key_pem.as_bytes());
        let keys: std::result::Result<Vec<_>, _> = rustls_pemfile::rsa_private_keys(&mut reader).collect();
        if let Ok(mut keys) = keys {
            if !keys.is_empty() {
                return Ok(PrivateKeyDer::from(keys.remove(0)));
            }
        }
        
        Err(ClientError::InvalidCertificate {
            reason: "Failed to parse client private key".to_string(),
        })
    }
    
    /// Create insecure client configuration for development
    fn create_insecure_client_config() -> Result<QuinnClientConfig> {
        let crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(InsecureVerifier))
            .with_no_client_auth();
        
        let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(crypto).map_err(|e| {
            ClientError::Tls(format!("Failed to create insecure QUIC client config: {}", e))
        })?;
        
        Ok(QuinnClientConfig::new(Arc::new(crypto)))
    }

    /// Establish connections to all brokers
    #[instrument(skip(endpoint, config))]
    async fn establish_connections(
        endpoint: &Endpoint,
        config: &ClientConfig,
    ) -> Result<HashMap<SocketAddr, ConnectionEntry>> {
        let mut connections = HashMap::new();
        let mut connection_futures = Vec::new();
        
        for broker_addr_str in &config.brokers {
            let addr = broker_addr_str.to_socket_addrs()
                .map_err(|e| ClientError::InvalidConfig(format!("Invalid broker address '{}': {}", broker_addr_str, e)))?
                .next()
                .ok_or_else(|| ClientError::InvalidConfig(format!("No valid address for broker: {}", broker_addr_str)))?;
            
            let endpoint = endpoint.clone();
            let server_name = config.tls_config.as_ref()
                .and_then(|tls| tls.server_name.clone())
                .unwrap_or_else(|| "localhost".to_string());
            let connect_timeout = config.connect_timeout;
            
            connection_futures.push(async move {
                Self::connect_with_retry(&endpoint, addr, &server_name, connect_timeout, config).await
                    .map(|conn| (addr, conn))
            });
        }
        
        // Connect to all brokers concurrently
        let results = futures::future::join_all(connection_futures).await;
        
        for result in results {
            match result {
                Ok((addr, connection_entry)) => {
                    info!("Successfully connected to broker: {}", addr);
                    connections.insert(addr, connection_entry);
                }
                Err(e) => {
                    error!("Failed to connect to broker: {}", e);
                    // Continue with other connections
                }
            }
        }
        
        if connections.is_empty() {
            return Err(ClientError::NoConnectionsAvailable);
        }
        
        info!("Established {} broker connections", connections.len());
        Ok(connections)
    }
    
    /// Connect to a single broker with retry logic
    #[instrument(skip(endpoint, config))]
    async fn connect_with_retry(
        endpoint: &Endpoint,
        addr: SocketAddr,
        server_name: &str,
        connect_timeout: Duration,
        config: &ClientConfig,
    ) -> Result<ConnectionEntry> {
        let retry_config = &config.retry_config;
        let mut attempt = 0;
        let mut delay = retry_config.base_delay;
        
        loop {
            attempt += 1;
            
            let connecting = match endpoint.connect(addr, server_name) {
                Ok(connecting) => connecting,
                Err(e) => {
                    warn!("Connection attempt {} to {} failed: {}", attempt, addr, e);
                    continue;
                }
            };
            
            match timeout(connect_timeout, connecting).await {
                Ok(Ok(connection)) => {
                    debug!("Connected to broker {} on attempt {}", addr, attempt);
                    return Ok(ConnectionEntry {
                        connection,
                        broker_addr: addr,
                        created_at: Instant::now(),
                        last_used: Arc::new(RwLock::new(Instant::now())),
                        error_count: Arc::new(AtomicUsize::new(0)),
                    });
                }
                Ok(Err(e)) => {
                    warn!("Connection attempt {} to {} failed: {}", attempt, addr, e);
                }
                Err(_) => {
                    warn!("Connection attempt {} to {} timed out", attempt, addr);
                }
            }
            
            if attempt >= retry_config.max_retries {
                return Err(ClientError::Connection(
                    format!("Failed to connect to {} after {} attempts", addr, attempt)
                ));
            }
            
            // Exponential backoff with jitter
            let actual_delay = if retry_config.jitter {
                let jitter = (delay.as_millis() as f64 * 0.1 * simple_rand()) as u64;
                delay + Duration::from_millis(jitter)
            } else {
                delay
            };
            
            sleep(actual_delay).await;
            delay = std::cmp::min(
                Duration::from_millis((delay.as_millis() as f64 * retry_config.multiplier) as u64),
                retry_config.max_delay,
            );
        }
    }

    /// Get the next available connection (round-robin)
    #[instrument(skip(self))]
    pub async fn get_connection(&self) -> Result<QuicConnection> {
        let connections = self.connections.read().await;
        if connections.is_empty() {
            return Err(ClientError::NoConnectionsAvailable);
        }
        
        let connection_entries: Vec<_> = connections.values().collect();
        let index = self.current_index.fetch_add(1, Ordering::Relaxed) % connection_entries.len();
        let entry = &connection_entries[index];
        
        // Check if connection is still alive
        if let Some(error) = entry.connection.close_reason() {
            warn!("Connection to {} is closed: {:?}", entry.broker_addr, error);
            // Try to reconnect in background
            let addr = entry.broker_addr;
            let connections_clone = Arc::clone(&self.connections);
            let endpoint_clone = Arc::clone(&self.endpoint);
            let config_clone = Arc::clone(&self.config);
            
            tokio::spawn(async move {
                if let Err(e) = Self::reconnect_broker(connections_clone, endpoint_clone, config_clone, addr).await {
                    error!("Failed to reconnect to broker {}: {}", addr, e);
                }
            });
            
            return Err(ClientError::BrokerNotAvailable { broker: addr.to_string() });
        }
        
        // Update last used timestamp
        *entry.last_used.write().await = Instant::now();
        
        Ok(entry.connection.clone())
    }
    
    /// Reconnect to a specific broker
    async fn reconnect_broker(
        connections: Arc<RwLock<HashMap<SocketAddr, ConnectionEntry>>>,
        endpoint: Arc<Endpoint>,
        config: Arc<ClientConfig>,
        addr: SocketAddr,
    ) -> Result<()> {
        let server_name = config.tls_config.as_ref()
            .and_then(|tls| tls.server_name.clone())
            .unwrap_or_else(|| "localhost".to_string());
        
        let new_entry = Self::connect_with_retry(
            &endpoint,
            addr,
            &server_name,
            config.connect_timeout,
            &config,
        ).await?;
        
        let mut connections = connections.write().await;
        connections.insert(addr, new_entry);
        
        info!("Successfully reconnected to broker: {}", addr);
        Ok(())
    }

    /// Send a request and wait for response with security context
    #[instrument(skip(self, request))]
    pub async fn send_request(&self, request: Vec<u8>) -> Result<Vec<u8>> {
        // Check authorization if security is enabled
        if let Some(_security_manager) = &self.security_manager {
            self.check_request_authorization(&request).await?;
        }
        
        self.send_request_internal(request).await
    }
    
    /// Internal method to send request without authorization check
    async fn send_request_internal(&self, request: Vec<u8>) -> Result<Vec<u8>> {
        let _permit = self.connection_semaphore.acquire().await.map_err(|_| {
            ClientError::ResourceExhausted { resource: "connection_pool".to_string() }
        })?;
        
        let connection = self.get_connection().await?;
        let request_id = Uuid::new_v4();
        
        debug!("Sending request {} ({} bytes)", request_id, request.len());
        
        // Open a bidirectional stream
        let (mut send_stream, mut recv_stream) = connection.open_bi().await.map_err(|e| {
            ClientError::QuicTransport(format!("Failed to open bidirectional stream: {}", e))
        })?;
        
        // Create request frame with request type byte (1 = Produce)
        // Protocol: [1-byte request type][request data]
        let mut frame = Vec::with_capacity(1 + request.len());
        frame.push(1u8); // RequestType::Produce
        frame.extend_from_slice(&request);
        
        // Send request with timeout
        let send_result = timeout(
            self.config.request_timeout,
            send_stream.write_all(&frame)
        ).await;
        
        match send_result {
            Ok(Ok(_)) => {
                send_stream.finish().map_err(|e| {
                    ClientError::QuicTransport(format!("Failed to finish send stream: {}", e))
                })?;
                debug!("Request {} sent successfully", request_id);
            }
            Ok(Err(e)) => {
                return Err(ClientError::QuicTransport(format!("Failed to send request: {}", e)));
            }
            Err(_) => {
                return Err(ClientError::Timeout { 
                    timeout_ms: self.config.request_timeout.as_millis() as u64 
                });
            }
        }
        
        // Read response with timeout
        let recv_result = timeout(
            self.config.request_timeout,
            Self::read_response(&mut recv_stream, request_id)
        ).await;
        
        match recv_result {
            Ok(Ok(response)) => {
                debug!("Received response for request {} ({} bytes)", request_id, response.len());
                Ok(response)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(ClientError::Timeout { 
                timeout_ms: self.config.request_timeout.as_millis() as u64 
            }),
        }
    }
    
    /// Read response from stream
    async fn read_response(
        recv_stream: &mut quinn::RecvStream,
        _expected_request_id: Uuid, // Not used in simplified protocol
    ) -> Result<Vec<u8>> {
        // Read response type byte (1 = Produce response)
        // Protocol: [1-byte request type][response data]
        let mut type_byte = [0u8; 1];
        recv_stream.read_exact(&mut type_byte).await.map_err(|e| {
            ClientError::QuicTransport(format!("Failed to read response type: {}", e))
        })?;

        // Check response type
        if type_byte[0] == 255 {
            // Error response - read error message
            let error_data = recv_stream.read_to_end(16 * 1024).await
                .map_err(|e| match e {
                    quinn::ReadToEndError::TooLong => ClientError::MessageTooLarge {
                        size: 16 * 1024,
                        max_size: 16 * 1024
                    },
                    quinn::ReadToEndError::Read(read_err) => {
                        ClientError::QuicTransport(format!("Failed to read error data: {}", read_err))
                    }
                })?;

            let error_msg = String::from_utf8_lossy(&error_data);
            return Err(ClientError::Broker(format!("Broker error: {}", error_msg)));
        }

        // Verify it's a Produce response (type 1)
        if type_byte[0] != 1 {
            return Err(ClientError::Protocol(
                format!("Unexpected response type: expected 1 (Produce), got {}", type_byte[0])
            ));
        }

        // Read the rest of the response data (no length prefix - read until EOF)
        // Quinn's read_to_end(size_limit) enforces max size and returns Vec<u8>
        let response_data = recv_stream.read_to_end(16 * 1024 * 1024).await
            .map_err(|e| match e {
                quinn::ReadToEndError::TooLong => ClientError::MessageTooLarge {
                    size: 16 * 1024 * 1024,
                    max_size: 16 * 1024 * 1024
                },
                quinn::ReadToEndError::Read(read_err) => {
                    ClientError::QuicTransport(format!("Failed to read response data: {}", read_err))
                }
            })?;

        Ok(response_data)
    }

    /// Send a message without waiting for response
    #[instrument(skip(self, message))]
    pub async fn send_message(&self, message: Vec<u8>) -> Result<()> {
        let _permit = self.connection_semaphore.acquire().await.map_err(|_| {
            ClientError::ResourceExhausted { resource: "connection_pool".to_string() }
        })?;
        
        let connection = self.get_connection().await?;
        let message_id = Uuid::new_v4();
        
        debug!("Sending message {} ({} bytes)", message_id, message.len());
        
        // Open a unidirectional stream
        let mut send_stream = connection.open_uni().await.map_err(|e| {
            ClientError::QuicTransport(format!("Failed to open unidirectional stream: {}", e))
        })?;
        
        // Create message frame with header
        let mut frame = Vec::new();
        frame.extend_from_slice(&message_id.as_bytes()[..]);
        frame.extend_from_slice(&(message.len() as u32).to_be_bytes());
        frame.extend_from_slice(&message);
        
        // Send message with timeout
        let send_result = timeout(
            self.config.request_timeout,
            async {
                send_stream.write_all(&frame).await.map_err(|e| ClientError::from(e))?;
                send_stream.finish().map_err(|e| ClientError::from(e))
            }
        ).await;
        
        match send_result {
            Ok(Ok(_)) => {
                debug!("Message {} sent successfully", message_id);
                Ok(())
            }
            Ok(Err(e)) => {
                Err(ClientError::QuicTransport(format!("Failed to send message: {}", e)))
            }
            Err(_) => {
                Err(ClientError::Timeout { 
                    timeout_ms: self.config.request_timeout.as_millis() as u64 
                })
            }
        }
    }
    
    /// Send multiple messages in a batch for efficiency
    #[instrument(skip(self, messages))]
    pub async fn send_messages_batch(&self, messages: Vec<Vec<u8>>) -> Result<Vec<Result<()>>> {
        if messages.is_empty() {
            return Ok(vec![]);
        }
        
        let batch_size = std::cmp::min(messages.len(), 10); // Max 10 concurrent streams
        let mut results = Vec::with_capacity(messages.len());
        
        for chunk in messages.chunks(batch_size) {
            let mut futures = Vec::new();
            
            for message in chunk {
                let message = message.clone();
                let connection_clone = self.clone();
                futures.push(async move {
                    connection_clone.send_message(message).await
                });
            }
            
            let chunk_results = futures::future::join_all(futures).await;
            results.extend(chunk_results);
        }
        
        Ok(results)
    }

    /// Check if connection is healthy
    pub async fn is_connected(&self) -> bool {
        let connections = self.connections.read().await;
        !connections.is_empty() && connections.values().any(|entry| entry.connection.close_reason().is_none())
    }

    /// Perform health check on connections
    #[instrument(skip(self))]
    pub async fn health_check(&self) -> Result<bool> {
        let connections = self.connections.read().await;
        
        if connections.is_empty() {
            return Ok(false);
        }
        
        let mut healthy_count = 0;
        let mut check_futures = Vec::new();
        
        for (addr, entry) in connections.iter() {
            let connection = entry.connection.clone();
            let addr = *addr;
            
            check_futures.push(async move {
                Self::check_connection_health(connection, addr).await
            });
        }
        
        let results = futures::future::join_all(check_futures).await;
        
        for (result, (addr, _entry)) in results.iter().zip(connections.iter()) {
            match result {
                Ok(true) => {
                    healthy_count += 1;
                    debug!("Broker {} is healthy", addr);
                }
                Ok(false) => {
                    warn!("Broker {} failed health check", addr);
                }
                Err(e) => {
                    error!("Health check error for broker {}: {}", addr, e);
                }
            }
        }
        
        let health_ratio = healthy_count as f64 / connections.len() as f64;
        let is_healthy = health_ratio >= 0.5; // At least 50% of connections must be healthy
        
        info!("Health check completed: {}/{} brokers healthy ({}%)", 
            healthy_count, connections.len(), (health_ratio * 100.0) as u32);
        
        Ok(is_healthy)
    }
    
    /// Check health of a single connection
    async fn check_connection_health(connection: QuicConnection, _addr: SocketAddr) -> Result<bool> {
        // Check if connection is closed
        if connection.close_reason().is_some() {
            return Ok(false);
        }
        
        // Send a ping frame
        let ping_data = b"rustmq_ping";
        let timeout_duration = Duration::from_secs(5);
        
        let ping_result = timeout(timeout_duration, async {
            let (mut send_stream, mut recv_stream) = connection.open_bi().await?;
            
            // Send ping
            send_stream.write_all(ping_data).await?;
            send_stream.finish()?;
            
            // Read pong
            let mut response = vec![0u8; ping_data.len()];
            recv_stream.read_exact(&mut response).await?;
            
            Ok::<bool, ClientError>(response == ping_data)
        }).await;
        
        match ping_result {
            Ok(Ok(pong_received)) => Ok(pong_received),
            Ok(Err(_)) => Ok(false),
            Err(_) => Ok(false), // Timeout
        }
    }
    
    /// Start background health check task
    fn start_health_check_task(&self) {
        let connection_clone = self.clone();
        let interval = self.health_check_interval;
        
        tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            
            loop {
                interval_timer.tick().await;
                
                if let Err(e) = connection_clone.health_check().await {
                    error!("Background health check failed: {}", e);
                }
                
                // Clean up failed connections
                connection_clone.cleanup_failed_connections().await;
            }
        });
    }
    
    /// Clean up failed connections
    async fn cleanup_failed_connections(&self) {
        let mut connections = self.connections.write().await;
        let mut to_remove = Vec::new();
        
        for (addr, entry) in connections.iter() {
            if entry.connection.close_reason().is_some() {
                let error_count = entry.error_count.load(Ordering::Relaxed);
                if error_count > 5 { // Remove after 5 consecutive errors
                    to_remove.push(*addr);
                }
            }
        }
        
        for addr in to_remove {
            connections.remove(&addr);
            warn!("Removed failed connection to broker: {}", addr);
        }
    }

    /// Reconnect to brokers
    #[instrument(skip(self))]
    pub async fn reconnect(&self) -> Result<()> {
        info!("Reconnecting to all brokers");
        
        let new_connections = Self::establish_connections(&self.endpoint, &self.config).await?;
        
        let mut connections = self.connections.write().await;
        
        // Close existing connections gracefully
        for (_addr, entry) in connections.iter() {
            entry.connection.close(0u8.into(), b"Reconnecting");
        }
        
        *connections = new_connections;
        
        info!("Successfully reconnected to {} brokers", connections.len());
        Ok(())
    }

    /// Close all connections
    pub async fn close(&self) -> Result<()> {
        let connections = self.connections.read().await;
        for (_addr, entry) in connections.iter() {
            entry.connection.close(0u8.into(), b"Client closing");
        }
        
        // Wait for connections to close gracefully
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        
        Ok(())
    }

    /// Get connection statistics
    pub async fn stats(&self) -> ConnectionStats {
        let connections = self.connections.read().await;
        let total_connections = connections.len();
        let active_connections = connections.iter()
            .filter(|(_addr, entry)| entry.connection.close_reason().is_none())
            .count();
        
        let broker_stats: Vec<BrokerConnectionInfo> = connections.iter()
            .map(|(addr, entry)| BrokerConnectionInfo {
                address: addr.to_string(),
                is_connected: entry.connection.close_reason().is_none(),
                created_at: entry.created_at,
                last_used: entry.last_used.try_read().map(|t| *t).unwrap_or(entry.created_at),
                error_count: entry.error_count.load(Ordering::Relaxed),
            })
            .collect();
        
        ConnectionStats {
            total_connections,
            active_connections,
            brokers: self.config.brokers.clone(),
            broker_stats,
        }
    }
}

impl Clone for Connection {
    fn clone(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            endpoint: Arc::clone(&self.endpoint),
            connections: Arc::clone(&self.connections),
            current_index: Arc::clone(&self.current_index),
            connection_semaphore: Arc::clone(&self.connection_semaphore),
            health_check_interval: self.health_check_interval,
            security_manager: self.security_manager.clone(),
            security_context: Arc::clone(&self.security_context),
        }
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub active_connections: usize,
    pub brokers: Vec<String>,
    pub broker_stats: Vec<BrokerConnectionInfo>,
}

/// Individual broker connection information
#[derive(Debug, Clone)]
pub struct BrokerConnectionInfo {
    pub address: String,
    pub is_connected: bool,
    pub created_at: Instant,
    pub last_used: Instant,
    pub error_count: usize,
}

/// Insecure certificate verifier for development
#[derive(Debug)]
struct InsecureVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA1,
            rustls::SignatureScheme::ECDSA_SHA1_Legacy,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::ED448,
        ]
    }
}

impl Connection {
    /// Check request authorization using security context
    async fn check_request_authorization(&self, _request: &[u8]) -> Result<()> {
        let security_context = self.security_context.read().await;
        
        if let Some(context) = security_context.as_ref() {
            // TODO: Parse request to determine required permissions
            // For now, just check if we have a valid security context
            if context.auth_time.elapsed() > Duration::from_secs(3600) {
                return Err(ClientError::AuthorizationDenied(
                    "Security context expired".to_string()
                ));
            }
            
            // TODO: Implement ACL-based authorization checks
            // - Parse request to extract topic/operation
            // - Check permissions against ACL rules
            // - Log authorization attempts for audit
            
            debug!("Request authorized for principal: {}", context.principal);
        }
        
        Ok(())
    }
    
    /// Get current security context
    pub async fn get_security_context(&self) -> Option<SecurityContext> {
        self.security_context.read().await.clone()
    }
    
    /// Refresh security context (re-authenticate)
    pub async fn refresh_security_context(&self) -> Result<()> {
        if let (Some(security_manager), Some(auth_config)) = (&self.security_manager, &self.config.auth) {
            let default_tls_config = TlsConfig::default();
            let tls_config = self.config.tls_config.as_ref().unwrap_or(&default_tls_config);
            let new_context = security_manager.authenticate(tls_config, auth_config).await?;
            
            *self.security_context.write().await = Some(new_context);
            info!("Security context refreshed successfully");
        }
        
        Ok(())
    }
    
    /// Check if security is enabled for this connection
    pub fn is_security_enabled(&self) -> bool {
        self.security_manager.is_some()
    }
}

/// Simple random number generator for jitter
pub(crate) fn simple_rand() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos();
    (nanos % 1000) as f64 / 1000.0
}

#[cfg(test)]
mod tests;