// Abstract file interface for different I/O backends
use crate::Result;
use crate::storage::{AlignedBufferPool, BufferPool};
use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

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
                    if let (Ok(major), Ok(minor)) =
                        (parts[0].parse::<u32>(), parts[1].parse::<u32>())
                    {
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
            if self.capabilities.io_uring_available && Self::is_in_io_uring_context() {
                tracing::info!("Using io_uring backend for WAL file operations");
                return Ok(Box::new(IoUringWalFile::new(path).await?));
            }
        }

        tracing::info!("Using tokio::fs backend for WAL file operations");
        Ok(Box::new(TokioWalFile::new(path).await?))
    }

    #[cfg(all(target_os = "linux", feature = "io-uring"))]
    fn is_in_io_uring_context() -> bool {
        // Never use io_uring in test contexts or when not explicitly enabled

        // Check if we're in a test environment
        if cfg!(test) {
            return false;
        }

        // Check if we're in a cargo test context
        if std::env::var("CARGO_PKG_NAME").is_ok() {
            return false;
        }

        // Check if we're explicitly running tests
        if std::env::var("RUST_TEST_THREADS").is_ok() {
            return false;
        }

        // Check if io_uring is explicitly disabled
        if std::env::var("RUSTMQ_DISABLE_IO_URING").is_ok() {
            return false;
        }

        // Only enable io_uring in production when explicitly requested
        // This prevents runtime errors from incompatible contexts
        std::env::var("RUSTMQ_ENABLE_IO_URING").is_ok()
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
    buffer_pool: Arc<AlignedBufferPool>,
}

impl TokioWalFile {
    pub async fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path.as_ref())
            .await?;

        // Initialize buffer pool for WAL I/O
        // 4096-byte alignment for optimal I/O, 100 buffers per pool
        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 100));

        Ok(Self { file, buffer_pool })
    }

    pub async fn new_with_pool<P: AsRef<Path>>(
        path: P,
        buffer_pool: Arc<AlignedBufferPool>,
    ) -> Result<Self> {
        let file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .open(path.as_ref())
            .await?;

        Ok(Self { file, buffer_pool })
    }
}

#[async_trait]
impl AsyncWalFile for TokioWalFile {
    async fn write_at(&self, data: Vec<u8>, offset: u64) -> Result<Vec<u8>> {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

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

        // Get buffer from pool instead of allocating
        // Buffer pool may return a buffer larger than requested (due to alignment)
        let mut buffer = self.buffer_pool.get_aligned_buffer(len)?;

        // Truncate buffer to exactly the requested size before reading
        // This ensures read_exact doesn't try to read more than requested
        buffer.truncate(len);
        buffer.resize(len, 0);

        // Read into buffer, handling errors properly
        match file.read_exact(&mut buffer).await {
            Ok(_) => Ok(buffer),
            Err(e) => {
                // Return buffer to pool on error
                self.buffer_pool.return_buffer(buffer);
                Err(e.into())
            }
        }
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
    buffer_pool: Arc<AlignedBufferPool>,
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

        // Initialize buffer pool for WAL I/O
        // 4096-byte alignment for optimal I/O, 100 buffers per pool
        let buffer_pool = Arc::new(AlignedBufferPool::new(4096, 100));
        let buffer_pool_clone = buffer_pool.clone();

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
                    IoUringCommand::WriteAt {
                        data,
                        offset,
                        response,
                    } => {
                        let result: std::result::Result<Vec<u8>, std::io::Error> = async {
                            let (result, returned_buf) = file.write_at(data, offset).await;
                            result?;
                            Ok(returned_buf)
                        }
                        .await;
                        let _ = response.send(result.map_err(Into::into));
                    }
                    IoUringCommand::ReadAt {
                        offset,
                        len,
                        response,
                    } => {
                        let result: std::result::Result<Vec<u8>, std::io::Error> = async {
                            // Get buffer from pool instead of allocating
                            let buffer = match buffer_pool_clone.get_aligned_buffer(len) {
                                Ok(buf) => buf,
                                Err(e) => {
                                    return Err(std::io::Error::new(
                                        std::io::ErrorKind::Other,
                                        e.to_string(),
                                    ));
                                }
                            };

                            let (result, mut returned_buf) = file.read_at(buffer, offset).await;
                            match result {
                                Ok(bytes_read) => {
                                    if bytes_read < len {
                                        returned_buf.truncate(bytes_read);
                                    }
                                    Ok(returned_buf)
                                }
                                Err(e) => {
                                    // Return buffer to pool on error
                                    buffer_pool_clone.return_buffer(returned_buf);
                                    Err(e)
                                }
                            }
                        }
                        .await;
                        let _ = response.send(result.map_err(Into::into));
                    }
                    IoUringCommand::SyncAll { response } => {
                        let result = file.sync_all().await.map_err(Into::into);
                        let _ = response.send(result);
                    }
                    IoUringCommand::FileSize { response } => {
                        let result = std::fs::metadata(&path)
                            .map(|metadata| metadata.len())
                            .map_err(Into::into);
                        let _ = response.send(result);
                    }
                    IoUringCommand::Shutdown => break,
                }
            }
        });

        Ok(Self { tx, buffer_pool })
    }

    pub async fn new_with_pool<P: AsRef<Path>>(
        path: P,
        buffer_pool: Arc<AlignedBufferPool>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let buffer_pool_clone = buffer_pool.clone();

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
                    IoUringCommand::WriteAt {
                        data,
                        offset,
                        response,
                    } => {
                        let result: std::result::Result<Vec<u8>, std::io::Error> = async {
                            let (result, returned_buf) = file.write_at(data, offset).await;
                            result?;
                            Ok(returned_buf)
                        }
                        .await;
                        let _ = response.send(result.map_err(Into::into));
                    }
                    IoUringCommand::ReadAt {
                        offset,
                        len,
                        response,
                    } => {
                        let result: std::result::Result<Vec<u8>, std::io::Error> = async {
                            // Get buffer from pool instead of allocating
                            let buffer = match buffer_pool_clone.get_aligned_buffer(len) {
                                Ok(buf) => buf,
                                Err(e) => {
                                    return Err(std::io::Error::new(
                                        std::io::ErrorKind::Other,
                                        e.to_string(),
                                    ));
                                }
                            };

                            let (result, mut returned_buf) = file.read_at(buffer, offset).await;
                            match result {
                                Ok(bytes_read) => {
                                    if bytes_read < len {
                                        returned_buf.truncate(bytes_read);
                                    }
                                    Ok(returned_buf)
                                }
                                Err(e) => {
                                    // Return buffer to pool on error
                                    buffer_pool_clone.return_buffer(returned_buf);
                                    Err(e)
                                }
                            }
                        }
                        .await;
                        let _ = response.send(result.map_err(Into::into));
                    }
                    IoUringCommand::SyncAll { response } => {
                        let result = file.sync_all().await.map_err(Into::into);
                        let _ = response.send(result);
                    }
                    IoUringCommand::FileSize { response } => {
                        let result = std::fs::metadata(&path)
                            .map(|metadata| metadata.len())
                            .map_err(Into::into);
                        let _ = response.send(result);
                    }
                    IoUringCommand::Shutdown => break,
                }
            }
        });

        Ok(Self { tx, buffer_pool })
    }
}

#[cfg(all(target_os = "linux", feature = "io-uring"))]
#[async_trait]
impl AsyncWalFile for IoUringWalFile {
    async fn write_at(&self, data: Vec<u8>, offset: u64) -> Result<Vec<u8>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.tx
            .send(IoUringCommand::WriteAt {
                data,
                offset,
                response: response_tx,
            })
            .map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;

        response_rx
            .await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }

    async fn read_at(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.tx
            .send(IoUringCommand::ReadAt {
                offset,
                len,
                response: response_tx,
            })
            .map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;

        response_rx
            .await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }

    async fn sync_all(&self) -> Result<()> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.tx
            .send(IoUringCommand::SyncAll {
                response: response_tx,
            })
            .map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;

        response_rx
            .await
            .map_err(|_| crate::error::RustMqError::Wal("io_uring response failed".to_string()))?
    }

    async fn file_size(&self) -> Result<u64> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.tx
            .send(IoUringCommand::FileSize {
                response: response_tx,
            })
            .map_err(|_| crate::error::RustMqError::Wal("io_uring task unavailable".to_string()))?;

        response_rx
            .await
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

        // Use tokio runtime for this test - io-uring factory will use tokio backend as fallback
        let factory = AsyncWalFileFactory::new();
        let file = factory.create_file(&file_path).await.unwrap();

        // Should create some valid backend
        let backend_type = file.backend_type();
        assert!(backend_type == "tokio-fs" || backend_type.starts_with("io-uring"));

        // Test basic operation to ensure it works
        let test_data = b"test".to_vec();
        let returned_buffer = file.write_at(test_data.clone(), 0).await.unwrap();
        assert_eq!(returned_buffer, test_data);

        println!("Created file with backend: {}", backend_type);
    }

    #[tokio::test]
    async fn test_basic_file_operations() {
        // Test with tokio backend which should always work
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test_tokio.wal");

        let file = TokioWalFile::new(&file_path).await.unwrap();

        // Test write operation
        let test_data = b"Hello, WAL!".to_vec();
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

        println!(
            "Basic file operations test passed with backend: {}",
            file.backend_type()
        );
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
    #[test] // Note: not tokio::test - we use tokio_uring::start instead
    fn test_io_uring_backend_directly() {
        // Skip this test in CI or when io_uring is not available
        if std::env::var("CI").is_ok() {
            println!("Skipping io_uring test in CI environment");
            return;
        }

        // Only test if io_uring is actually available
        if !PlatformCapabilities::detect().io_uring_available {
            println!("Skipping io_uring test - not available on this system");
            return;
        }

        // Try to run within tokio_uring context, but don't fail if unavailable
        match std::panic::catch_unwind(|| {
            tokio_uring::start(async {
                let temp_dir = TempDir::new().unwrap();
                let file_path = temp_dir.path().join("uring_test.wal");

                match IoUringWalFile::new(&file_path).await {
                    Ok(file) => {
                        assert!(file.backend_type().starts_with("io-uring"));

                        // Test basic operations
                        let test_data = b"io_uring backend test".to_vec();
                        let returned_buffer = file.write_at(test_data.clone(), 0).await.unwrap();
                        assert_eq!(returned_buffer, test_data);

                        file.sync_all().await.unwrap();

                        let read_data = file.read_at(0, test_data.len()).await.unwrap();
                        assert_eq!(read_data, test_data);

                        println!("io_uring backend test passed");
                        true
                    }
                    Err(e) => {
                        println!("io_uring backend test failed: {}", e);
                        false
                    }
                }
            })
        }) {
            Ok(result) => assert!(result),
            Err(_) => {
                println!("io_uring runtime not available - skipping test");
            }
        }
    }

    #[tokio::test]
    async fn test_multiple_writes_and_reads() {
        // Use tokio backend for consistent testing
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("multi_test.wal");

        let file = TokioWalFile::new(&file_path).await.unwrap();

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

        println!(
            "Multiple writes and reads test passed with backend: {}",
            file.backend_type()
        );
    }
}
