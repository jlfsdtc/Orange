//! orange-regex: Regular expression engine with Hyperscan/Vectorscan acceleration.
//!
//! Provides:
//! - Single pattern matching via Hyperscan
//! - Multi-pattern matching
//! - Boolean expression (AND/OR/NOT) evaluation
//! - Qt regex fallback (via `regex` crate)

pub mod engine;
pub mod boolean_expr;

pub use engine::RegexEngine;
pub use boolean_expr::BooleanExpr;

#[derive(Debug, thiserror::Error)]
pub enum RegexError {
    #[error("Compilation failed: {0}")]
    CompileError(String),

    #[error("Invalid boolean expression: {0}")]
    InvalidExpression(String),

    #[error("Scan error: {0}")]
    ScanError(String),
}

/// Flags for regex compilation.
#[derive(Debug, Clone, Copy, Default)]
pub struct RegexFlags {
    pub case_insensitive: bool,
    pub dot_matches_newline: bool,
}
