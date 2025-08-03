// WAL (Write-Ahead Log) module with multiple I/O backend support

pub mod async_file;
pub mod direct_io_wal;     // Original implementation
pub mod optimized_wal;     // New io_uring optimized implementation

pub use async_file::{
    AsyncWalFile, AsyncWalFileFactory, PlatformCapabilities, TokioWalFile
};

#[cfg(all(target_os = "linux", feature = "io-uring"))]
pub use async_file::IoUringWalFile;

// Re-export the original implementation for backward compatibility
pub use direct_io_wal::DirectIOWal;

// Export the optimized implementation
pub use optimized_wal::OptimizedDirectIOWal;

use crate::{Result, config::WalConfig, storage::traits::BufferPool};
use std::sync::Arc;

/// Factory for creating optimal WAL implementations based on platform capabilities
pub struct WalFactory;

impl WalFactory {
    /// Create the optimal WAL implementation for the current platform
    pub async fn create_optimal_wal(
        config: WalConfig, 
        buffer_pool: Arc<dyn BufferPool>
    ) -> Result<Box<dyn crate::storage::traits::WriteAheadLog>> {
        let capabilities = PlatformCapabilities::detect();
        
        if capabilities.io_uring_available {
            tracing::info!("Creating optimized WAL with io_uring support");
            Ok(Box::new(OptimizedDirectIOWal::new(config, buffer_pool).await?))
        } else {
            tracing::info!("Creating standard WAL with tokio::fs backend");
            Ok(Box::new(DirectIOWal::new(config, buffer_pool).await?))
        }
    }
    
    /// Create the original DirectIOWal implementation (for compatibility/testing)
    pub async fn create_standard_wal(
        config: WalConfig, 
        buffer_pool: Arc<dyn BufferPool>
    ) -> Result<DirectIOWal> {
        DirectIOWal::new(config, buffer_pool).await
    }
    
    /// Create the optimized WAL implementation (forces new implementation)
    pub async fn create_optimized_wal(
        config: WalConfig, 
        buffer_pool: Arc<dyn BufferPool>
    ) -> Result<OptimizedDirectIOWal> {
        OptimizedDirectIOWal::new(config, buffer_pool).await
    }
    
    /// Get platform capabilities for decision making
    pub fn get_platform_capabilities() -> PlatformCapabilities {
        PlatformCapabilities::detect()
    }
}