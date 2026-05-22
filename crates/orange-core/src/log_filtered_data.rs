//! Filtered/search results over a LogData.
//!
//! Search results are stored in roaring bitmaps for compact storage
//! and fast set operations (union, intersection) needed for boolean queries.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use roaring::RoaringTreemap;

use crate::log_data::LogData;
use orange_regex::engine::RegexEngine;
use orange_regex::RegexFlags;

/// Represents search results (filtered view) over a log file.
pub struct LogFilteredData {
    /// Lines matching the search pattern (line numbers).
    matching_lines: RoaringTreemap,
    /// Marked/bookmarked lines.
    marked_lines: RoaringTreemap,
    /// Total number of matches.
    match_count: u64,
    /// Whether a search is currently running.
    is_searching: Arc<AtomicBool>,
    /// Search progress (lines scanned so far).
    search_progress: Arc<AtomicU64>,
}

impl LogFilteredData {
    /// Create a new empty filtered data view.
    pub fn new() -> Self {
        Self {
            matching_lines: RoaringTreemap::new(),
            marked_lines: RoaringTreemap::new(),
            match_count: 0,
            is_searching: Arc::new(AtomicBool::new(false)),
            search_progress: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Perform a synchronous search over the log data.
    /// Scans all lines and records matching line numbers.
    pub fn search(&mut self, log_data: &LogData, pattern: &str, flags: RegexFlags) -> anyhow::Result<()> {
        self.matching_lines.clear();
        self.match_count = 0;
        self.is_searching.store(true, Ordering::Relaxed);
        self.search_progress.store(0, Ordering::Relaxed);

        let engine = RegexEngine::compile(pattern, flags)
            .map_err(|e| anyhow::anyhow!("Regex compile error: {}", e))?;

        let total_lines = log_data.line_count();
        let batch_size = 1000u64;

        let mut line_num = 0u64;
        while line_num < total_lines {
            let end = (line_num + batch_size).min(total_lines);
            let lines = log_data.get_lines(line_num, (end - line_num) as usize);

            for (i, line) in lines.iter().enumerate() {
                let current_line = line_num + i as u64;
                if engine.scan_first(line)?.is_some() {
                    self.matching_lines.insert(current_line);
                    self.match_count += 1;
                }
            }

            line_num = end;
            self.search_progress.store(line_num, Ordering::Relaxed);
        }

        self.is_searching.store(false, Ordering::Relaxed);
        Ok(())
    }

    /// Get the number of matches.
    pub fn match_count(&self) -> u64 {
        self.match_count
    }

    /// Get the nth matching line number.
    pub fn get_matching_line(&self, index: u64) -> Option<u64> {
        self.matching_lines.select(index as u64)
    }

    /// Get all matching line numbers in a range.
    pub fn get_matching_lines(&self, start: u64, count: usize) -> Vec<u64> {
        self.matching_lines
            .iter()
            .skip(start as usize)
            .take(count)
            .map(|v| v as u64)
            .collect()
    }

    /// Check if a specific line is a match.
    pub fn is_match(&self, line_num: u64) -> bool {
        self.matching_lines.contains(line_num)
    }

    /// Toggle bookmark on a line.
    pub fn toggle_mark(&mut self, line_num: u64) {
        if self.marked_lines.contains(line_num) {
            self.marked_lines.remove(line_num);
        } else {
            self.marked_lines.insert(line_num);
        }
    }

    /// Check if a line is bookmarked.
    pub fn is_marked(&self, line_num: u64) -> bool {
        self.marked_lines.contains(line_num)
    }

    /// Get all bookmarked line numbers.
    pub fn marked_lines(&self) -> &RoaringTreemap {
        &self.marked_lines
    }

    /// Get the line number of the next match after `from`.
    pub fn next_match(&self, from: u64) -> Option<u64> {
        self.matching_lines
            .iter()
            .find(|&v| v > from)
    }

    /// Get the line number of the previous match before `from`.
    pub fn prev_match(&self, from: u64) -> Option<u64> {
        self.matching_lines
            .iter()
            .take_while(|&v| v < from)
            .last()
    }

    /// Whether a search is currently in progress.
    pub fn is_searching(&self) -> bool {
        self.is_searching.load(Ordering::Relaxed)
    }

    /// Search progress (lines scanned).
    pub fn search_progress(&self) -> u64 {
        self.search_progress.load(Ordering::Relaxed)
    }

    /// Clear all results.
    pub fn clear(&mut self) {
        self.matching_lines.clear();
        self.marked_lines.clear();
        self.match_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log_data::LogData;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_log(lines: &[&str]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(file, "{}", line).unwrap();
        }
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_search_basic() {
        let file = create_test_log(&[
            "INFO: starting up",
            "ERROR: something failed",
            "INFO: continuing",
            "ERROR: another failure",
            "INFO: done",
        ]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        filtered.search(&log_data, "ERROR", RegexFlags::default()).unwrap();

        assert_eq!(filtered.match_count(), 2);
        assert!(filtered.is_match(1));
        assert!(filtered.is_match(3));
        assert!(!filtered.is_match(0));
    }

    #[test]
    fn test_search_regex() {
        let file = create_test_log(&[
            "2024-01-15 INFO: ok",
            "2024-01-16 ERROR: bad",
            "2024-01-17 WARNING: hmm",
        ]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        filtered.search(&log_data, r"\d{4}-\d{2}-\d{2}", RegexFlags::default()).unwrap();

        assert_eq!(filtered.match_count(), 3);
    }

    #[test]
    fn test_search_case_insensitive() {
        let file = create_test_log(&["error", "ERROR", "Error"]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        let flags = RegexFlags { case_insensitive: true, ..Default::default() };
        filtered.search(&log_data, "error", flags).unwrap();

        assert_eq!(filtered.match_count(), 3);
    }

    #[test]
    fn test_search_no_matches() {
        let file = create_test_log(&["hello", "world"]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        filtered.search(&log_data, "xyz", RegexFlags::default()).unwrap();

        assert_eq!(filtered.match_count(), 0);
    }

    #[test]
    fn test_get_matching_line() {
        let file = create_test_log(&["a", "b", "c", "b", "e"]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        filtered.search(&log_data, "b", RegexFlags::default()).unwrap();

        assert_eq!(filtered.match_count(), 2);
        // First match at line 1, second at line 3
        let first = filtered.get_matching_line(0);
        let second = filtered.get_matching_line(1);
        assert!(first.is_some());
        assert!(second.is_some());
    }

    #[test]
    fn test_bookmarks() {
        let mut filtered = LogFilteredData::new();

        assert!(!filtered.is_marked(5));
        filtered.toggle_mark(5);
        assert!(filtered.is_marked(5));
        filtered.toggle_mark(5);
        assert!(!filtered.is_marked(5));
    }

    #[test]
    fn test_next_prev_match() {
        let file = create_test_log(&["a", "match", "c", "match", "e"]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        filtered.search(&log_data, "match", RegexFlags::default()).unwrap();

        assert_eq!(filtered.match_count(), 2);
        // next_match from before first match
        let next = filtered.next_match(0);
        assert!(next.is_some());
    }

    #[test]
    fn test_clear() {
        let file = create_test_log(&["match", "match"]);
        let log_data = LogData::open(file.path()).unwrap();
        let mut filtered = LogFilteredData::new();

        filtered.search(&log_data, "match", RegexFlags::default()).unwrap();
        assert_eq!(filtered.match_count(), 2);

        filtered.clear();
        assert_eq!(filtered.match_count(), 0);
    }
}
