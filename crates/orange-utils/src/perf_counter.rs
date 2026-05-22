//! High-resolution performance counters for benchmarking.

use std::time::{Duration, Instant};

/// A simple named performance timer.
pub struct PerfCounter {
    name: String,
    start: Instant,
}

impl PerfCounter {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed().as_secs_f64() * 1000.0
    }

    pub fn report(&self) {
        tracing::debug!(counter = %self.name, elapsed_ms = self.elapsed_ms(), "perf counter");
    }
}

impl Drop for PerfCounter {
    fn drop(&mut self) {
        self.report();
    }
}
