//! Thread-safe indexing data structures.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Shared state between the indexing worker and the UI.
pub struct IndexingData {
    /// Whether indexing is in progress.
    is_indexing: AtomicBool,
    /// Number of lines indexed so far.
    indexed_lines: AtomicU64,
    /// Number of bytes indexed so far.
    indexed_bytes: AtomicU64,
    /// Total file size.
    total_bytes: u64,
}

impl IndexingData {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            is_indexing: AtomicBool::new(false),
            indexed_lines: AtomicU64::new(0),
            indexed_bytes: AtomicU64::new(0),
            total_bytes,
        }
    }

    pub fn is_indexing(&self) -> bool {
        self.is_indexing.load(Ordering::Relaxed)
    }

    pub fn indexed_lines(&self) -> u64 {
        self.indexed_lines.load(Ordering::Relaxed)
    }

    pub fn progress(&self) -> f64 {
        if self.total_bytes == 0 {
            return 1.0;
        }
        self.indexed_bytes.load(Ordering::Relaxed) as f64 / self.total_bytes as f64
    }

    pub fn start(&self) {
        self.is_indexing.store(true, Ordering::Relaxed);
    }

    pub fn finish(&self) {
        self.is_indexing.store(false, Ordering::Relaxed);
    }

    pub fn add_lines(&self, count: u64) {
        self.indexed_lines.fetch_add(count, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, count: u64) {
        self.indexed_bytes.fetch_add(count, Ordering::Relaxed);
    }
}
