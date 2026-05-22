//! SIMD-accelerated compressed line position storage.
//!
//! Uses streamvbyte delta encoding to compress end-of-line positions.
//! Positions are stored in blocks of 128 lines. Within each block, the
//! base position is stored as u64, and the delta offsets from the base
//! are encoded as u32 using streamvbyte's SIMD variable-byte encoding.

/// Size of a compression block (matches streamvbyte internal block size).
const BLOCK_SIZE: usize = 128;

/// Compressed storage for line end positions.
pub struct CompressedLineStorage {
    /// Compressed data blocks.
    blocks: Vec<CompressedBlock>,
    /// First line position of each block (for random access via binary search).
    block_first_positions: Vec<u64>,
    /// Total number of lines stored.
    total_lines: u64,
}

struct CompressedBlock {
    /// Compressed delta bytes (streamvbyte encoded).
    data: Vec<u8>,
    /// Number of lines in this block.
    line_count: usize,
    /// Base position for delta decoding.
    base_position: u64,
}

impl CompressedLineStorage {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            block_first_positions: Vec::new(),
            total_lines: 0,
        }
    }

    /// Append a batch of line end positions.
    /// Positions must be in ascending order.
    pub fn append(&mut self, positions: &[u64]) {
        if positions.is_empty() {
            return;
        }

        // Process in chunks of BLOCK_SIZE
        for chunk in positions.chunks(BLOCK_SIZE) {
            let base = chunk[0];

            // Encode: pass cumulative offsets from base with prev=0.
            // streamvbyte_delta_encode computes deltas internally.
            let offsets: Vec<u32> = chunk
                .iter()
                .map(|&pos| (pos - base) as u32)
                .collect();

            let max_bytes = streamvbyte_max_compressedbytes(offsets.len() as u32);
            let mut out_buf = vec![0u8; max_bytes];

            let bytes_written = unsafe {
                streamvbyte_delta_encode(
                    offsets.as_ptr(),
                    offsets.len() as u32,
                    out_buf.as_mut_ptr(),
                    0, // prev = 0
                )
            };

            out_buf.truncate(bytes_written);

            self.block_first_positions.push(base);
            self.blocks.push(CompressedBlock {
                data: out_buf,
                line_count: chunk.len(),
                base_position: base,
            });

            self.total_lines += chunk.len() as u64;
        }
    }

    /// Get the line end position for a given line number.
    /// Returns None if line_num is out of range.
    pub fn get_position(&self, line_num: u64) -> Option<u64> {
        if line_num >= self.total_lines {
            return None;
        }
        Some(self.find_position(line_num))
    }

    /// Find the position for a line number (internal, assumes valid range).
    fn find_position(&self, line_num: u64) -> u64 {
        let mut cumulative = 0u64;
        for (i, block) in self.blocks.iter().enumerate() {
            let block_end = cumulative + block.line_count as u64;
            if line_num < block_end {
                let offset_in_block = (line_num - cumulative) as usize;
                return self.decode_position_from_block(i, offset_in_block);
            }
            cumulative = block_end;
        }
        0
    }

    fn decode_position_from_block(&self, block_idx: usize, offset: usize) -> u64 {
        let block = &self.blocks[block_idx];
        let mut out = vec![0u32; block.line_count];
        unsafe {
            streamvbyte_delta_decode(
                block.data.as_ptr(),
                out.as_mut_ptr(),
                block.line_count as u32,
                0,
            );
        }
        // After delta_decode with prev=0, out[i] contains the cumulative offset
        // from base_position. So actual position = base_position + out[offset].
        block.base_position + out[offset] as u64
    }

    /// Get all positions in a range [start, end).
    /// More efficient than calling get_position in a loop.
    pub fn get_positions(&self, start: u64, end: u64) -> Vec<u64> {
        if start >= self.total_lines || start >= end {
            return Vec::new();
        }

        let end = end.min(self.total_lines);
        let mut result = Vec::with_capacity((end - start) as usize);
        let mut cumulative = 0u64;

        for block in &self.blocks {
            let block_end = cumulative + block.line_count as u64;

            // Check if this block overlaps with [start, end)
            if block_end > start && cumulative < end {
                // Decode the entire block
                let mut offsets = vec![0u32; block.line_count];
                unsafe {
                    streamvbyte_delta_decode(
                        block.data.as_ptr(),
                        offsets.as_mut_ptr(),
                        block.line_count as u32,
                        0,
                    );
                }

                // After delta_decode with prev=0, offsets[i] = cumulative offset from base
                let block_start_offset = if start > cumulative {
                    (start - cumulative) as usize
                } else {
                    0
                };
                let block_end_offset = ((end - cumulative) as usize).min(block.line_count);

                for &offset in &offsets[block_start_offset..block_end_offset] {
                    result.push(block.base_position + offset as u64);
                }
            }

            cumulative = block_end;
            if cumulative >= end {
                break;
            }
        }

        result
    }

    /// Total number of lines stored.
    pub fn len(&self) -> u64 {
        self.total_lines
    }

    pub fn is_empty(&self) -> bool {
        self.total_lines == 0
    }

    /// Number of compressed blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
}

// FFI declarations for streamvbyte C library
unsafe extern "C" {
    fn streamvbyte_delta_encode(in_: *const u32, length: u32, out: *mut u8, prev: u32) -> usize;
    fn streamvbyte_delta_decode(in_: *const u8, out: *mut u32, length: u32, prev: u32) -> usize;
}

/// Maximum compressed bytes for a given number of u32 values.
/// Mirrors the static inline function from streamvbyte.h.
fn streamvbyte_max_compressedbytes(length: u32) -> usize {
    let cb = ((length + 3) / 4) as usize; // control bytes
    let db = length as usize * 4;          // data bytes (worst case: 4 bytes per value)
    cb + db + 16                            // + STREAMVBYTE_PADDING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_storage() {
        let storage = CompressedLineStorage::new();
        assert_eq!(storage.len(), 0);
        assert!(storage.is_empty());
        assert_eq!(storage.get_position(0), None);
    }

    #[test]
    fn test_single_block() {
        let mut storage = CompressedLineStorage::new();
        let positions: Vec<u64> = (0..100).map(|i| i * 10).collect();
        storage.append(&positions);

        assert_eq!(storage.len(), 100);
        assert_eq!(storage.block_count(), 1);

        // Verify all positions
        for (i, &expected) in positions.iter().enumerate() {
            assert_eq!(
                storage.get_position(i as u64),
                Some(expected),
                "Mismatch at line {}",
                i
            );
        }
    }

    #[test]
    fn test_multiple_blocks() {
        let mut storage = CompressedLineStorage::new();
        // 300 lines = 2 full blocks + 1 partial
        let positions: Vec<u64> = (0..300).map(|i| i * 100).collect();
        storage.append(&positions);

        assert_eq!(storage.len(), 300);
        assert_eq!(storage.block_count(), 3);

        // Verify boundary positions
        assert_eq!(storage.get_position(0), Some(0));
        assert_eq!(storage.get_position(127), Some(12700));
        assert_eq!(storage.get_position(128), Some(12800));
        assert_eq!(storage.get_position(255), Some(25500));
        assert_eq!(storage.get_position(299), Some(29900));
    }

    #[test]
    fn test_out_of_range() {
        let mut storage = CompressedLineStorage::new();
        let positions: Vec<u64> = (0..10).map(|i| i).collect();
        storage.append(&positions);

        assert_eq!(storage.get_position(10), None);
        assert_eq!(storage.get_position(100), None);
    }

    #[test]
    fn test_large_positions() {
        let mut storage = CompressedLineStorage::new();
        // Simulate positions in a large file (>4GB)
        let base = 5_000_000_000u64; // 5GB
        let positions: Vec<u64> = (0..200).map(|i| base + i * 1000).collect();
        storage.append(&positions);

        assert_eq!(storage.get_position(0), Some(base));
        assert_eq!(storage.get_position(50), Some(base + 50000));
        assert_eq!(storage.get_position(199), Some(base + 199000));
    }

    #[test]
    fn test_incremental_append() {
        let mut storage = CompressedLineStorage::new();

        // Append in small batches
        for batch in 0..5u64 {
            let positions: Vec<u64> = (0..50).map(|i| (batch * 50 + i) * 10).collect();
            storage.append(&positions);
        }

        assert_eq!(storage.len(), 250);

        // Verify all positions
        for i in 0..250u64 {
            assert_eq!(storage.get_position(i), Some(i * 10));
        }
    }

    #[test]
    fn test_get_positions_range() {
        let mut storage = CompressedLineStorage::new();
        let positions: Vec<u64> = (0..300).map(|i| i * 10).collect();
        storage.append(&positions);

        let range = storage.get_positions(10, 20);
        assert_eq!(range.len(), 10);
        for (i, &pos) in range.iter().enumerate() {
            assert_eq!(pos, (10 + i as u64) * 10);
        }
    }
}
