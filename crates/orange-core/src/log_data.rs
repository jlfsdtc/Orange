//! Log file data model: indexing, reading, and line management.
//!
//! Files are indexed by scanning for newline characters in 5MB blocks.
//! Line end positions are stored in a CompressedLineStorage for efficient
//! random access with minimal memory usage.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::compressed_line_storage::CompressedLineStorage;
use crate::indexing_data::IndexingData;
use orange_utils::file_digest::FileDigest;

/// Size of a read block for indexing (5 MB, matching klogg).
const INDEX_BLOCK_SIZE: usize = 5 * 1024 * 1024;

/// Represents an indexed log file.
pub struct LogData {
    /// Path to the log file.
    path: PathBuf,
    /// Compressed line end positions.
    line_positions: CompressedLineStorage,
    /// Indexing state (shared with worker threads).
    indexing_data: Arc<IndexingData>,
    /// File size in bytes.
    file_size: u64,
    /// File digest for change detection.
    digest: FileDigest,
    /// Encoding of the file (detected or assumed UTF-8).
    encoding: &'static encoding_rs::Encoding,
    /// Length in bytes of the longest line (including its trailing newline).
    /// Used to size the horizontal scrollbar. This is a byte count, so it
    /// slightly over-estimates display width for multi-byte UTF-8 text — which
    /// is fine for scrollbar sizing.
    max_line_length: u64,
}

impl LogData {
    /// Open and index a log file.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let file_size = metadata.len();
        let digest = FileDigest::compute(path)?;

        let indexing_data = Arc::new(IndexingData::new(file_size));
        indexing_data.start();

        let mut line_positions = CompressedLineStorage::new();
        // Track the longest line as the largest gap between consecutive line
        // ends. Positions are absolute, so a single running cursor works across
        // block boundaries.
        let mut max_line_length = 0u64;
        let mut prev_line_end = 0u64;

        // Detect encoding from first block
        let mut file = File::open(path)?;
        let detect_size = (file_size as usize).min(8192);
        let mut detect_buf = vec![0u8; detect_size];
        let detect_len = file.read(&mut detect_buf)?;
        let encoding = crate::encoding::detect_encoding(&detect_buf[..detect_len]);

        // Index the file: scan for newlines in blocks
        file.seek(SeekFrom::Start(0))?;
        let mut reader = BufReader::with_capacity(INDEX_BLOCK_SIZE, file);
        let mut buffer = vec![0u8; INDEX_BLOCK_SIZE];
        let mut file_offset = 0u64;

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            // Find all newline positions in this block
            let block = &buffer[..bytes_read];
            let line_ends = find_line_ends(block, file_offset);

            for &end in &line_ends {
                max_line_length = max_line_length.max(end - prev_line_end);
                prev_line_end = end;
            }

            if !line_ends.is_empty() {
                line_positions.append(&line_ends);
            }

            file_offset += bytes_read as u64;
            indexing_data.add_bytes(bytes_read as u64);
            indexing_data.add_lines(line_ends.len() as u64);
        }

        // If the file doesn't end with a newline, add a final line end
        if file_size > 0 {
            let last_pos = line_positions.get_position(line_positions.len().saturating_sub(1));
            if last_pos != Some(file_size) {
                max_line_length = max_line_length.max(file_size - prev_line_end);
                line_positions.append(&[file_size]);
            }
        }

        indexing_data.finish();

        Ok(Self {
            path: path.to_path_buf(),
            line_positions,
            indexing_data,
            file_size,
            digest,
            encoding,
            max_line_length,
        })
    }

    /// Get the total number of lines.
    pub fn line_count(&self) -> u64 {
        self.line_positions.len()
    }

    /// Length in bytes of the longest line (including its trailing newline).
    /// Used to size the horizontal scrollbar.
    pub fn max_line_length(&self) -> u64 {
        self.max_line_length
    }

    /// Get the content of a specific line (0-indexed).
    pub fn get_line(&self, line_num: u64) -> Option<Vec<u8>> {
        if line_num >= self.line_positions.len() {
            return None;
        }

        let line_start = if line_num == 0 {
            0
        } else {
            self.line_positions.get_position(line_num - 1)?
        };
        let line_end = self.line_positions.get_position(line_num)?;

        // Read the line from the file
        let mut file = File::open(&self.path).ok()?;
        file.seek(SeekFrom::Start(line_start)).ok()?;

        let len = (line_end - line_start) as usize;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).ok()?;

        // Strip trailing newline
        if buf.last() == Some(&b'\n') {
            buf.pop();
            if buf.last() == Some(&b'\r') {
                buf.pop();
            }
        }

        Some(buf)
    }

    /// Get multiple lines in a range [start, end).
    /// More efficient than calling get_line in a loop due to sequential I/O.
    pub fn get_lines(&self, start: u64, count: usize) -> Vec<Vec<u8>> {
        if start >= self.line_positions.len() || count == 0 {
            return Vec::new();
        }

        let end = (start + count as u64).min(self.line_positions.len());
        let positions = self.line_positions.get_positions(start.saturating_sub(1), end);

        if positions.is_empty() {
            return Vec::new();
        }

        // Determine byte range to read
        let read_start = if start == 0 { 0 } else { positions[0] };
        let read_end = *positions.last().unwrap();

        let mut file = match File::open(&self.path) {
            Ok(f) => f,
            Err(_) => return Vec::new(),
        };
        file.seek(SeekFrom::Start(read_start)).ok();

        let read_len = (read_end - read_start) as usize;
        let mut data = vec![0u8; read_len];
        if file.read_exact(&mut data).is_err() {
            return Vec::new();
        }

        // Split into lines
        let mut result = Vec::with_capacity(count);
        let mut pos_in_data = 0usize;

        for (i, &abs_pos) in positions.iter().enumerate() {
            if i == 0 && start > 0 {
                // Skip the first position (it's the end of the previous line)
                continue;
            }
            let offset = (abs_pos - read_start) as usize;
            if offset > pos_in_data && offset <= data.len() {
                let mut line = data[pos_in_data..offset].to_vec();
                // Strip trailing newline
                if line.last() == Some(&b'\n') {
                    line.pop();
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                }
                result.push(line);
                pos_in_data = offset;
            }
        }

        result
    }

    /// Check if the file has changed on disk.
    pub fn has_changed(&self) -> bool {
        match FileDigest::compute(&self.path) {
            Ok(new_digest) => self.digest.has_changed(&new_digest),
            Err(_) => false,
        }
    }

    /// Re-index the file from the current end (for tail mode / live reload).
    pub fn reindex_from_current_end(&mut self) -> anyhow::Result<()> {
        let new_size = std::fs::metadata(&self.path)?.len();
        if new_size <= self.file_size {
            return Ok(());
        }

        let mut file = File::open(&self.path)?;
        file.seek(SeekFrom::Start(self.file_size))?;

        let mut reader = BufReader::with_capacity(INDEX_BLOCK_SIZE, file);
        let mut buffer = vec![0u8; INDEX_BLOCK_SIZE];
        let mut offset = self.file_size;
        // Continue measuring line lengths from the previous indexed end so the
        // first appended line is sized correctly.
        let mut prev_line_end = self.file_size;

        loop {
            let bytes_read = reader.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let block = &buffer[..bytes_read];
            let line_ends = find_line_ends(block, offset);

            for &end in &line_ends {
                self.max_line_length = self.max_line_length.max(end - prev_line_end);
                prev_line_end = end;
            }

            if !line_ends.is_empty() {
                self.line_positions.append(&line_ends);
                self.indexing_data.add_lines(line_ends.len() as u64);
            }

            offset += bytes_read as u64;
        }

        // Update final line end if needed
        if new_size > 0 {
            let last = self.line_positions.get_position(self.line_positions.len() - 1);
            if last != Some(new_size) {
                self.max_line_length = self.max_line_length.max(new_size - prev_line_end);
                self.line_positions.append(&[new_size]);
            }
        }

        self.file_size = new_size;
        self.digest = FileDigest::compute(&self.path)?;

        Ok(())
    }

    /// File path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// File size in bytes.
    pub fn file_size(&self) -> u64 {
        self.file_size
    }

    /// Detected encoding.
    pub fn encoding(&self) -> &'static encoding_rs::Encoding {
        self.encoding
    }

    /// Indexing progress (0.0 to 1.0).
    pub fn indexing_progress(&self) -> f64 {
        self.indexing_data.progress()
    }

    /// Whether indexing is still in progress.
    pub fn is_indexing(&self) -> bool {
        self.indexing_data.is_indexing()
    }
}

/// Find all newline byte positions in a block.
/// Returns absolute positions (block_start + byte offset).
fn find_line_ends(block: &[u8], block_start: u64) -> Vec<u64> {
    let mut positions = Vec::new();
    for (i, &byte) in block.iter().enumerate() {
        if byte == b'\n' {
            // Position points to the byte AFTER the newline (line end)
            positions.push(block_start + i as u64 + 1);
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_file(content: &[u8]) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn test_empty_file() {
        let file = create_test_file(b"");
        let data = LogData::open(file.path()).unwrap();
        // Empty file: 0 lines (or 1 line of 0 bytes depending on convention)
        assert!(data.line_count() <= 1);
    }

    #[test]
    fn test_single_line_no_newline() {
        let file = create_test_file(b"hello world");
        let data = LogData::open(file.path()).unwrap();
        assert_eq!(data.line_count(), 1);
        assert_eq!(data.get_line(0), Some(b"hello world".to_vec()));
    }

    #[test]
    fn test_single_line_with_newline() {
        let file = create_test_file(b"hello world\n");
        let data = LogData::open(file.path()).unwrap();
        assert_eq!(data.line_count(), 1);
        assert_eq!(data.get_line(0), Some(b"hello world".to_vec()));
    }

    #[test]
    fn test_multiple_lines() {
        let content = b"line1\nline2\nline3\n";
        let file = create_test_file(content);
        let data = LogData::open(file.path()).unwrap();

        assert_eq!(data.line_count(), 3);
        assert_eq!(data.get_line(0), Some(b"line1".to_vec()));
        assert_eq!(data.get_line(1), Some(b"line2".to_vec()));
        assert_eq!(data.get_line(2), Some(b"line3".to_vec()));
    }

    #[test]
    fn test_crlf_line_endings() {
        let content = b"line1\r\nline2\r\n";
        let file = create_test_file(content);
        let data = LogData::open(file.path()).unwrap();

        assert_eq!(data.line_count(), 2);
        assert_eq!(data.get_line(0), Some(b"line1".to_vec()));
        assert_eq!(data.get_line(1), Some(b"line2".to_vec()));
    }

    #[test]
    fn test_get_lines_range() {
        let content = b"a\nb\nc\nd\ne\n";
        let file = create_test_file(content);
        let data = LogData::open(file.path()).unwrap();

        let lines = data.get_lines(1, 3);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], b"b".to_vec());
        assert_eq!(lines[1], b"c".to_vec());
        assert_eq!(lines[2], b"d".to_vec());
    }

    #[test]
    fn test_get_line_out_of_range() {
        let file = create_test_file(b"hello\n");
        let data = LogData::open(file.path()).unwrap();

        assert_eq!(data.get_line(1), None);
        assert_eq!(data.get_line(100), None);
    }

    #[test]
    fn test_file_change_detection() {
        let mut file = create_test_file(b"original content\n");
        let data = LogData::open(file.path()).unwrap();

        // File hasn't changed
        assert!(!data.has_changed());

        // Modify the file
        file.write_all(b"added content\n").unwrap();
        file.flush().unwrap();

        // Now it should detect the change
        assert!(data.has_changed());
    }

    #[test]
    fn test_many_lines() {
        // Generate a file with 10000 lines
        let mut content = Vec::new();
        for i in 0..10000u32 {
            content.extend_from_slice(format!("line {}\n", i).as_bytes());
        }
        let file = create_test_file(&content);
        let data = LogData::open(file.path()).unwrap();

        assert_eq!(data.line_count(), 10000);
        assert_eq!(data.get_line(0), Some(b"line 0".to_vec()));
        assert_eq!(data.get_line(9999), Some(b"line 9999".to_vec()));
    }

    #[test]
    fn test_max_line_length() {
        // Longest line is "aaaaaaaa" (8 bytes) + '\n' = 9.
        let content = b"a\nbb\naaaaaaaa\ncc\n";
        let file = create_test_file(content);
        let data = LogData::open(file.path()).unwrap();
        assert_eq!(data.max_line_length(), 9);
    }

    #[test]
    fn test_max_line_length_no_trailing_newline() {
        // Last line has no newline: "longest_here" is 12 bytes.
        let content = b"short\nlongest_here";
        let file = create_test_file(content);
        let data = LogData::open(file.path()).unwrap();
        assert_eq!(data.max_line_length(), 12);
    }

    #[test]
    fn test_max_line_length_after_reindex() {
        let mut file = create_test_file(b"a\nbb\n");
        let mut data = LogData::open(file.path()).unwrap();
        assert_eq!(data.max_line_length(), 3); // "bb\n"

        file.write_all(b"cccccccc\n").unwrap();
        file.flush().unwrap();
        data.reindex_from_current_end().unwrap();
        assert_eq!(data.max_line_length(), 9); // "cccccccc\n"
    }

    #[test]
    fn test_reindex_from_current_end() {
        let mut file = create_test_file(b"line1\nline2\n");
        let mut data = LogData::open(file.path()).unwrap();
        assert_eq!(data.line_count(), 2);

        // Append more data
        file.write_all(b"line3\nline4\n").unwrap();
        file.flush().unwrap();

        data.reindex_from_current_end().unwrap();
        assert_eq!(data.line_count(), 4);
        assert_eq!(data.get_line(2), Some(b"line3".to_vec()));
        assert_eq!(data.get_line(3), Some(b"line4".to_vec()));
    }
}
