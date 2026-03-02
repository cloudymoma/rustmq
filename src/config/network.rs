use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub quic_listen: String,
    pub rpc_listen: String,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub quic_config: QuicConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuicConfig {
    pub max_concurrent_uni_streams: u32,
    pub max_concurrent_bidi_streams: u32,
    pub max_idle_timeout_ms: u64,
    pub max_stream_data: u64,
    pub max_connection_data: u64,
}

impl Default for QuicConfig {
    fn default() -> Self {
        Self {
            max_concurrent_uni_streams: 1000,
            max_concurrent_bidi_streams: 1000,
            max_idle_timeout_ms: 30_000,     // 30 seconds
            max_stream_data: 1_024_000,      // 1MB per stream
            max_connection_data: 10_240_000, // 10MB per connection
        }
    }
}
