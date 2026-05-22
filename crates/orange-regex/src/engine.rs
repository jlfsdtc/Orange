//! Unified regex engine with Hyperscan/Vectorscan acceleration.
//!
//! Provides block-mode scanning for single and multi-pattern matching.
//! All patterns are compiled with SOM_LEFTMOST to report accurate match start positions.

use crate::{RegexError, RegexFlags};
use hyperscan::prelude::*;

/// A match result from regex scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    /// Pattern ID that matched.
    pub id: u32,
    /// Start offset of the match (inclusive, byte index).
    pub from: usize,
    /// End offset of the match (exclusive, byte index).
    pub to: usize,
}

/// Apply user flags to a Hyperscan Pattern builder.
fn apply_flags(mut p: Pattern, flags: RegexFlags) -> Pattern {
    // SOM_LEFTMOST is always needed for accurate `from` position reporting.
    p = p.left_most();
    if flags.case_insensitive {
        p = p.caseless();
    }
    if flags.dot_matches_newline {
        p = p.dot_all();
    }
    p
}

/// A compiled single-pattern regex engine backed by Hyperscan.
pub struct RegexEngine {
    db: BlockDatabase,
    scratch: Scratch,
}

impl RegexEngine {
    /// Compile a single pattern with the given flags.
    pub fn compile(pattern: &str, flags: RegexFlags) -> Result<Self, RegexError> {
        let builder = Pattern::new(pattern)
            .map_err(|e| RegexError::CompileError(e.to_string()))?;
        let builder = apply_flags(builder, flags);

        let db: BlockDatabase = builder
            .build()
            .map_err(|e| RegexError::CompileError(e.to_string()))?;

        let scratch = db
            .alloc_scratch()
            .map_err(|e| RegexError::CompileError(e.to_string()))?;

        Ok(Self { db, scratch })
    }

    /// Scan data for all matches of the compiled pattern.
    pub fn scan(&self, data: &[u8]) -> Result<Vec<Match>, RegexError> {
        let mut matches = Vec::new();

        self.db
            .scan(data, &self.scratch, |id, from, to, _flags| {
                matches.push(Match {
                    id,
                    from: from as usize,
                    to: to as usize,
                });
                Matching::Continue
            })
            .map_err(|e| RegexError::ScanError(e.to_string()))?;

        Ok(matches)
    }

    /// Scan data and return only the first match (short-circuits on first hit).
    pub fn scan_first(&self, data: &[u8]) -> Result<Option<Match>, RegexError> {
        let mut found: Option<Match> = None;

        // ScanTerminated is the expected error when we terminate early.
        let result = self.db.scan(data, &self.scratch, |id, from, to, _flags| {
            found = Some(Match {
                id,
                from: from as usize,
                to: to as usize,
            });
            Matching::Terminate
        });

        match result {
            Ok(()) => {}
            Err(e) => {
                let msg = e.to_string();
                if !msg.contains("SCAN_TERMINATED") && !msg.contains("terminated") {
                    return Err(RegexError::ScanError(msg));
                }
            }
        }

        Ok(found)
    }
}

/// A compiled multi-pattern regex engine.
///
/// Each pattern is assigned an ID (0-based index if not specified).
/// Scanning reports all matches across all patterns.
pub struct MultiRegexEngine {
    db: BlockDatabase,
    scratch: Scratch,
    pattern_count: usize,
}

impl MultiRegexEngine {
    /// Compile multiple patterns with shared flags.
    pub fn compile(patterns: &[&str], flags: RegexFlags) -> Result<Self, RegexError> {
        if patterns.is_empty() {
            return Err(RegexError::CompileError("No patterns provided".into()));
        }

        let mut hs_patterns = Vec::with_capacity(patterns.len());
        for (i, pattern) in patterns.iter().enumerate() {
            let p = Pattern::new(*pattern)
                .map_err(|e| RegexError::CompileError(e.to_string()))?;
            let mut p = apply_flags(p, flags);
            p.id = Some(i);
            hs_patterns.push(p);
        }

        let patterns_vec = Patterns(hs_patterns);
        let db: BlockDatabase = patterns_vec
            .build()
            .map_err(|e| RegexError::CompileError(e.to_string()))?;

        let scratch = db
            .alloc_scratch()
            .map_err(|e| RegexError::CompileError(e.to_string()))?;

        Ok(Self {
            db,
            scratch,
            pattern_count: patterns.len(),
        })
    }

    /// Number of compiled patterns.
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    /// Scan data for all matches across all patterns.
    pub fn scan(&self, data: &[u8]) -> Result<Vec<Match>, RegexError> {
        let mut matches = Vec::new();

        self.db
            .scan(data, &self.scratch, |id, from, to, _flags| {
                matches.push(Match {
                    id,
                    from: from as usize,
                    to: to as usize,
                });
                Matching::Continue
            })
            .map_err(|e| RegexError::ScanError(e.to_string()))?;

        Ok(matches)
    }

    /// Scan and deduplicate: return at most one match per pattern ID.
    pub fn scan_unique(&self, data: &[u8]) -> Result<Vec<Match>, RegexError> {
        let mut matches = Vec::with_capacity(self.pattern_count);
        let mut seen = vec![false; self.pattern_count];

        self.db
            .scan(data, &self.scratch, |id, from, to, _flags| {
                let idx = id as usize;
                if idx < seen.len() && !seen[idx] {
                    seen[idx] = true;
                    matches.push(Match {
                        id,
                        from: from as usize,
                        to: to as usize,
                    });
                }
                Matching::Continue
            })
            .map_err(|e| RegexError::ScanError(e.to_string()))?;

        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pattern_compile_and_scan() {
        let engine = RegexEngine::compile("error", RegexFlags::default()).unwrap();
        let matches = engine.scan(b"this is an error message").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].from, 11);
        assert_eq!(matches[0].to, 16);
    }

    #[test]
    fn test_single_pattern_case_insensitive() {
        let flags = RegexFlags {
            case_insensitive: true,
            ..Default::default()
        };
        let engine = RegexEngine::compile("error", flags).unwrap();
        let matches = engine.scan(b"ERROR and Error").unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_single_pattern_no_match() {
        let engine = RegexEngine::compile("xyz", RegexFlags::default()).unwrap();
        let matches = engine.scan(b"hello world").unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn test_single_pattern_regex_syntax() {
        let engine = RegexEngine::compile(r"\d{3}-\d{4}", RegexFlags::default()).unwrap();
        let matches = engine.scan(b"call 555-1234 now").unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].from, 5);
        assert_eq!(matches[0].to, 13);
    }

    #[test]
    fn test_scan_first() {
        let engine = RegexEngine::compile("o", RegexFlags::default()).unwrap();
        let m = engine.scan_first(b"hello world").unwrap();
        assert!(m.is_some());
        let m = m.unwrap();
        assert_eq!(m.from, 4);
        assert_eq!(m.to, 5);
    }

    #[test]
    fn test_multi_pattern() {
        let engine =
            MultiRegexEngine::compile(&["error", "warn", "info"], RegexFlags::default()).unwrap();
        assert_eq!(engine.pattern_count(), 3);

        let matches = engine.scan(b"error: something warn: check info: ok").unwrap();
        assert_eq!(matches.len(), 3);

        let ids: Vec<u32> = matches.iter().map(|m| m.id).collect();
        assert!(ids.contains(&0));
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[test]
    fn test_multi_pattern_scan_unique() {
        let engine =
            MultiRegexEngine::compile(&["a", "b"], RegexFlags::default()).unwrap();
        let matches = engine.scan_unique(b"aabb").unwrap();
        // Should return exactly 2 matches (one per pattern), not 4
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_compile_invalid_pattern() {
        let result = RegexEngine::compile("[invalid", RegexFlags::default());
        assert!(result.is_err());
    }
}
