//! Fast file digest using xxHash for change detection.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use xxhash_rust::xxh3::xxh3_64;

const DIGEST_BLOCK_SIZE: usize = 4096;

/// File digest computed from header and tail hashes.
/// Used to detect file changes without reading the entire file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileDigest {
    pub header_hash: u64,
    pub tail_hash: u64,
    pub file_size: u64,
}

impl FileDigest {
    /// Compute digest from a file path.
    pub fn compute(path: &Path) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len();

        // Read header
        let mut header_buf = vec![0u8; DIGEST_BLOCK_SIZE];
        let header_len = file.read(&mut header_buf)?;
        let header_hash = xxh3_64(&header_buf[..header_len]);

        // Read tail
        let tail_hash = if file_size > DIGEST_BLOCK_SIZE as u64 {
            let tail_start = file_size - DIGEST_BLOCK_SIZE as u64;
            file.seek(SeekFrom::Start(tail_start))?;
            let mut tail_buf = vec![0u8; DIGEST_BLOCK_SIZE];
            let tail_len = file.read(&mut tail_buf)?;
            xxh3_64(&tail_buf[..tail_len])
        } else {
            header_hash
        };

        Ok(Self { header_hash, tail_hash, file_size })
    }

    /// Check if this digest indicates the file has changed.
    pub fn has_changed(&self, other: &FileDigest) -> bool {
        self.header_hash != other.header_hash
            || self.tail_hash != other.tail_hash
            || self.file_size != other.file_size
    }
}
