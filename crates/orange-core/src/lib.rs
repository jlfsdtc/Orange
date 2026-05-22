//! orange-core: Core data model for log file indexing, searching, and filtering.
//!
//! This crate implements the performance-critical data layer:
//! - File indexing with parallel block processing
//! - Compressed line position storage (SIMD-accelerated)
//! - Regex search with Hyperscan
//! - Roaring bitmap search result storage
//! - File change detection

pub mod log_data;
pub mod log_filtered_data;
pub mod compressed_line_storage;
pub mod indexing_data;
pub mod encoding;
pub mod session;
pub mod filewatch;

pub use log_data::LogData;
pub use log_filtered_data::LogFilteredData;
