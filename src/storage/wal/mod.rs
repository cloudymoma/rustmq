// WAL (Write-Ahead Log) module with multiple I/O backend support

pub mod aligned_buf; // Page-aligned buffers for O_DIRECT I/O
pub mod branchless_parser; // Branchless SIMD parser for batch operations
pub mod segmented; // Segmented WAL (partition-correct storage rebuild)

pub use aligned_buf::{AlignedBuf, BLOCK_SIZE};
pub use segmented::{SegmentedLog, SegmentedWal};

pub use branchless_parser::{
    BranchlessRecordBatchParser, BranchlessValidation, ParserStats, RecordHeader,
};

use std::time::Instant;

/// Metadata for a WAL segment/record
#[derive(Debug, Clone)]
pub struct WalSegmentMetadata {
    pub start_offset: u64,
    pub end_offset: u64,
    pub file_offset: u64,
    pub size_bytes: u64,
    pub created_at: Instant,
}
