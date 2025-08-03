// Abstract file interface for different I/O backends
use crate::Result;
use async_trait::async_trait;
use std::path::Path;

/// Platform capabilities for I/O backend selection
#[derive(Debug, Clone)]
pub struct PlatformCapabilities {
    pub io_uring_available: bool,
    pub kernel_version: Option<(u32, u32)>,
    pub registered_buffers_supported: bool,
}

impl PlatformCapabilities {
    /// Detect platform capabilities at runtime
    pub fn detect() -> Self {
        #[cfg(all(target_os = "linux", feature = "io-uring"))]
        {
            // Check if io_uring is available and supported
            if let Ok(true) = Self::check_io_uring_support() {
                return Self {
                    io_uring_available: true,
                    kernel_version: Self::get_kernel_version(),
                    registered_buffers_supported: Self::check_registered_buffers(),
                };
            }
        }
        
        // Fallback for non-Linux or when io-uring is not available
        Self {
            io_uring_available: false,
            kernel_version: None,
            registered_buffers_supported: false,
        }
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn check_io_uring_support() -> Result<bool> {
        // Simple check for io_uring availability
        // tokio-uring will handle the actual detection at runtime
        std::fs::metadata("/proc/sys/kernel/io_uring_disabled")
            .map(|_| true)
            .or_else(|_| Ok(true)) // Assume available if we can't check
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn get_kernel_version() -> Option<(u32, u32)> {
        use std::process::Command;
        
        if let Ok(output) = Command::new("uname").arg("-r").output() {
            if let Ok(version_str) = String::from_utf8(output.stdout) {
                let parts: Vec<&str> = version_str.split('.').collect();
                if parts.len() >= 2 {
                    if let (Ok(major), Ok(minor)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                        return Some((major, minor));
                    }
                }
            }
        }
        None
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn check_registered_buffers() -> bool {
        // For simplicity, assume registered buffers are supported if io_uring is available
        // In production, you might want more sophisticated detection
        true
    }
}

/// Abstraction for file operations that can be backed by different I/O implementations
#[async_trait]
pub trait AsyncWalFile: Send + Sync {
    /// Write data at a specific offset in the file
    /// Returns the buffer back to allow for buffer reuse
    async fn write_at(&self, data: Vec<u8>, offset: u64) -> Result<Vec<u8>>;
    
    /// Read data from a specific offset in the file
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>>;
    
    /// Sync all data to disk
    async fn sync_all(&self) -> Result<()>;
    
    /// Get current file size
    async fn file_size(&self) -> Result<u64>;
    
    /// Get the backend type for metrics/debugging
    fn backend_type(&self) -> &'static str;
}

/// Factory for creating AsyncWalFile instances with optimal backend
pub struct AsyncWalFileFactory {
    capabilities: PlatformCapabilities,
}

impl AsyncWalFileFactory {
    pub fn new() -> Self {
        Self {
            capabilities: PlatformCapabilities::detect(),
        }
    }

    pub fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    /// Create an optimal AsyncWalFile instance for the current platform
    pub async fn create_file<P: AsRef<Path>>(&self, path: P) -> Result<Box<dyn AsyncWalFile>> {
        #[cfg(all(target_os = "linux", feature = "io-uring"))]
        {
            if self.capabilities.io_uring_available {
                tracing::info!("Using io_uring backend for WAL file operations");
                return Ok(Box::new(IoUringWalFile::new(path).await?));
            }
        }
        
        tracing::info!("Using tokio::fs backend for WAL file operations");
        Ok(Box::new(TokioWalFile::new(path).await?))
    }
}

impl Default for AsyncWalFileFactory {
    fn default() -> Self {
        Self::new()
    }
}

// Tokio-based implementation (fallback)
pub struct TokioWalFile {
    file: tokio::fs::File,
}

impl TokioWalFile {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path.as_ref())
            .await?;
        
        Ok(Self { file })
    }
}

#[async_trait]
impl AsyncWalFile for TokioWalFile {
    async fn write_at(&self, data: Vec<u8>, offset: u64) -> Result<Vec<u8>> {
        use tokio::io::{AsyncWriteExt, AsyncSeekExt, SeekFrom};
        
        // Note: This requires a mutable reference to the file, which is challenging
        // with the current API design. In practice, we'll need to use interior mutability
        // or adjust the abstraction. For now, we'll implement it as a demonstration.
        
        // Create a clone of the file handle for this operation
        let mut file = self.file.try_clone().await?;
        file.seek(SeekFrom::Start(offset)).await?;
        file.write_all(&data).await?;
        
        // Return the data buffer for reuse
        Ok(data)
    }
    
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
        
        let mut file = self.file.try_clone().await?;
        file.seek(SeekFrom::Start(offset)).await?;
        
        let mut buffer = vec![0u8; len];
        file.read_exact(&mut buffer).await?;
        
        Ok(buffer)
    }
    
    async fn sync_all(&self) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        
        let mut file = self.file.try_clone().await?;
        file.sync_data().await?;
        Ok(())
    }
    
    async fn file_size(&self) -> Result<u64> {
        let metadata = self.file.metadata().await?;
        Ok(metadata.len())
    }
    
    fn backend_type(&self) -> &'static str {
        "tokio-fs"
    }
}

// io_uring-based implementation (Linux only)
// Note: Due to tokio-uring's design, we need to use message passing
// to interact with io_uring from a dedicated task
#[cfg(all(target_os = "linux", feature = "io-uring"))]
pub struct IoUringWalFile {
    // Channel for sending commands to the io_uring task
    tx: tokio::sync::mpsc::UnboundedSender<IoUringCommand>,
}

#[cfg(all(target_os = "linux", feature = "io-uring"))]
#[derive(Debug)]
enum IoUringCommand {
    WriteAt {
        data: Vec<u8>,
        offset: u64,
        response: tokio::sync::oneshot::Sender<Result<Vec<u8>>>,
    },
    ReadAt {
        offset: u64,
        len: usize,
        response: tokio::sync::oneshot::Sender<Result<Vec<u8>>>,
    },
    SyncAll {
        response: tokio::sync::oneshot::Sender<Result<()>>,
    },
    FileSize {
        response: tokio::sync::oneshot::Sender<Result<u64>>,
    },
    Shutdown,
}

#[cfg(all(target_os = "linux", feature = "io-uring"))]
impl IoUringWalFile {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        
        // Spawn the io_uring task
        let path_clone = path.clone();
        tokio_uring::spawn(async move {
            let file = match tokio_uring::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .read(true)
                .open(&path)
                .await
            {
                Ok(file) => file,
                Err(_) => return, // Failed to open file
            };
            
            let path = path_clone;
            
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    IoUringCommand::WriteAt { data, offset, response } => {
                        let result: std::result::Result<Vec<u8>, std::io::Error> = async {
                            let (result, returned_buf) = file.write_at(data, offset).await;
                            result?;
                            Ok(returned_buf)
                        }.await;
                        let _ = response.send(result.map_err(Into::into));
                    },
                    IoUringCommand::ReadAt { offset, len, response } => {
                        let result: std::result::Result<Vec<u8>, std::io::Error> = async {
                            let buffer = vec![0u8; len];
                            let (result, mut returned_buf) = file.read_at(buffer, offset).await;
                            let bytes_read = result?;
                            if bytes_read < len {
                                returned_buf.truncate(bytes_read);
                            }
                            Ok(returned_buf)
                        }.await;
                        let _ = response.send(result.map_err(Into::into));
                    },
                    IoUringCommand::SyncAll { response } => {
                        let result = file.sync_all().await.map_err(Into::into);
                        let _ = response.send(result);
                    },
                    IoUringCommand::FileSize { response } => {
                        let result = std::fs::metadata(&path)
                            .map(|metadata| metadata.len())
                            .map_err(Into::into);
                        let _ = response.send(result);
                    },
                    IoUringCommand::Shutdown => break,
                }
            }
        });
        
        Ok(Self { tx })
    }
}

#[cfg(all(target_os = "linux", feature = "io-uring"))]
#[async_trait]
impl AsyncWalFile for IoUringWalFile {
    async fn write_at(&self, data: Vec<u8>, offset: u64) -> Result<Vec<u8>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.tx.send(IoUringCommand::WriteAt {
            data,
            offset,
            response: response_tx,
        }).map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;
        
        response_rx.await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }
    
    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.tx.send(IoUringCommand::ReadAt {
            offset,
            len,
            response: response_tx,
        }).map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;
        
        response_rx.await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }
    
    async fn sync_all(&self) -> Result<()> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.tx.send(IoUringCommand::SyncAll {
            response: response_tx,
        }).map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;
        
        response_rx.await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }
    
    async fn file_size(&self) -> Result<u64> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();
        
        self.tx.send(IoUringCommand::FileSize {
            response: response_tx,
        }).map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;
        
        response_rx.await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }
    
    fn backend_type(&self) -> &'static str {
        "io-uring"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_platform_capabilities_detection() {
        let capabilities = PlatformCapabilities::detect();
        
        // Should always have some valid detection result
        println!("Platform capabilities: {:#?}", capabilities);
        
        // On Linux with io-uring feature, check if detection works
        #[cfg(all(target_os = "linux", feature = "io-uring"))]
        {
            // Detection should complete without panicking
            assert!(capabilities.kernel_version.is_some() || !capabilities.io_uring_available);
        }
        
        #[cfg(not(all(target_os = "linux", feature = "io-uring")))]
        {
            assert!(!capabilities.io_uring_available);
        }
    }

    #[tokio::test]
    async fn test_factory_creates_appropriate_backend() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.wal");
        
        let factory = AsyncWalFileFactory::new();
        let file = factory.create_file(&file_path).await.unwrap();
        
        // Should create some valid backend
        let backend_type = file.backend_type();
        assert!(backend_type == "tokio-fs" || backend_type.starts_with("io-uring"));
        
        println!("Created file with backend: {}", backend_type);
    }

    #[tokio::test]
    async fn test_basic_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.wal");
        
        let factory = AsyncWalFileFactory::new();
        let file = factory.create_file(&file_path).await.unwrap();
        
        // Test write operation
        let test_data = b"Hello, io_uring!".to_vec();
        let returned_buffer = file.write_at(test_data.clone(), 0).await.unwrap();
        assert_eq!(returned_buffer, test_data);
        
        // Test sync
        file.sync_all().await.unwrap();
        
        // Test read operation
        let read_data = file.read_at(0, test_data.len()).await.unwrap();
        assert_eq!(read_data, test_data);
        
        // Test file size
        let size = file.file_size().await.unwrap();
        assert_eq!(size as usize, test_data.len());
    }

    #[tokio::test]
    async fn test_tokio_backend_directly() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("tokio_test.wal");
        
        let file = TokioWalFile::new(&file_path).await.unwrap();
        assert_eq!(file.backend_type(), "tokio-fs");
        
        // Test basic operations
        let test_data = b"Tokio backend test".to_vec();
        let returned_buffer = file.write_at(test_data.clone(), 0).await.unwrap();
        assert_eq!(returned_buffer, test_data);
        
        file.sync_all().await.unwrap();
        
        let read_data = file.read_at(0, test_data.len()).await.unwrap();
        assert_eq!(read_data, test_data);
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    #[tokio::test]
    async fn test_io_uring_backend_directly() {
        // Only test if io_uring is actually available
        if !PlatformCapabilities::detect().io_uring_available {
            println!("Skipping io_uring test - not available on this system");
            return;
        }
        
        // We need to run this test within tokio_uring::start
        let result = tokio_uring::start(async {
            let temp_dir = TempDir::new().unwrap();
            let file_path = temp_dir.path().join("uring_test.wal");
            
            let file = IoUringWalFile::new(&file_path).await.unwrap();
            assert!(file.backend_type().starts_with("io-uring"));
            
            // Test basic operations
            let test_data = b"io_uring backend test".to_vec();
            let returned_buffer = file.write_at(test_data.clone(), 0).await.unwrap();
            assert_eq!(returned_buffer, test_data);
            
            file.sync_all().await.unwrap();
            
            let read_data = file.read_at(0, test_data.len()).await.unwrap();
            assert_eq!(read_data, test_data);
            
            // Return success
            true
        });
        
        assert!(result);
    }

    #[tokio::test]
    async fn test_multiple_writes_and_reads() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("multi_test.wal");
        
        let factory = AsyncWalFileFactory::new();
        let file = factory.create_file(&file_path).await.unwrap();
        
        // Write multiple chunks at different offsets
        let chunks = vec![
            (0, b"chunk1".to_vec()),
            (6, b"chunk2".to_vec()),
            (12, b"chunk3".to_vec()),
        ];
        
        for (offset, data) in &chunks {
            file.write_at(data.clone(), *offset).await.unwrap();
        }
        
        file.sync_all().await.unwrap();
        
        // Read back each chunk
        for (offset, expected_data) in &chunks {
            let read_data = file.read_at(*offset, expected_data.len()).await.unwrap();
            assert_eq!(&read_data, expected_data);
        }
        
        // Verify total file size
        let total_size = file.file_size().await.unwrap();
        assert_eq!(total_size, 18); // 12 + 6 bytes for "chunk3"
    }
}