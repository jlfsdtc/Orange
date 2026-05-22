//! Custom containers using mimalloc as the global allocator.
//!
//! Since mimalloc is set as the global allocator in the binary crate,
//! all standard collections (Vec, HashMap, etc.) automatically use it.
//! This module re-exports type aliases for clarity and documentation.
