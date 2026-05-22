//! Runtime CPU feature detection for SIMD capabilities.

use crate::SimdCaps;

/// Detect available SIMD instruction sets at runtime.
pub fn detect() -> SimdCaps {
    // `mut` is unused on non-x86_64 because the cfg block below is excluded;
    // silence the lint rather than duplicating the function per arch.
    #[allow(unused_mut)]
    let mut caps = SimdCaps::default();

    #[cfg(target_arch = "x86_64")]
    {
        caps.sse2 = std::is_x86_feature_detected!("sse2");
        caps.sse3 = std::is_x86_feature_detected!("sse3");
        caps.ssse3 = std::is_x86_feature_detected!("ssse3");
        caps.sse41 = std::is_x86_feature_detected!("sse4.1");
        caps.popcnt = std::is_x86_feature_detected!("popcnt");
        caps.avx = std::is_x86_feature_detected!("avx");
        caps.avx2 = std::is_x86_feature_detected!("avx2");
    }

    caps
}
