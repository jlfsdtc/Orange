//! orange-utils: Core utility types and performance primitives.
//!
//! This crate provides:
//! - Global mimalloc allocator
//! - SIMD capability detection
//! - Performance counters
//! - File digest (xxHash)

pub mod cpu_info;
pub mod containers;
pub mod perf_counter;
pub mod file_digest;

/// SIMD capabilities detected at runtime.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimdCaps {
    pub sse2: bool,
    pub sse3: bool,
    pub ssse3: bool,
    pub sse41: bool,
    pub popcnt: bool,
    pub avx: bool,
    pub avx2: bool,
}
