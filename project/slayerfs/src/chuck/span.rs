use std::marker::PhantomData;

/// The `SpanTag`, it is just a marker and has no method.
pub trait SpanTag {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChunkTag;
impl SpanTag for ChunkTag {}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockTag;
impl SpanTag for BlockTag {}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
/// Reserved for local cache.
pub struct PageTag;
impl SpanTag for PageTag {}

/// A generic Span structure.
/// `T` is used to distinguish between ChunkSpan, BlockSpan, etc. at compile time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span<T: SpanTag> {
    pub index: u64,
    pub offset: u64,
    pub len: u64,
    _marker: PhantomData<T>,
}

impl<T: SpanTag> Span<T> {
    pub fn new(index: u64, offset: u64, len: u64) -> Self {
        Self {
            index,
            offset,
            len,
            _marker: PhantomData,
        }
    }

    /// Splits the current Span into smaller-granularity Target Spans.
    ///
    /// # Arguments
    /// * `my_align`: The alignment size of the current level (e.g., ChunkSize).
    /// * `target_align`: The alignment size of the target level (e.g., BlockSize).
    /// * `relative`:
    ///     - `true`: Returns the sub-index relative to the current index (0, 1, 2...).
    ///     - `false`: Returns the global absolute index.
    pub fn split_into<Target: SpanTag>(
        &self,
        my_align: u64,
        target_align: u64,
        relative: bool,
    ) -> SpanIter<Target> {
        debug_assert!(
            my_align >= target_align,
            "Parent alignment must be >= child alignment"
        );
        debug_assert!(
            my_align.is_multiple_of(target_align),
            "Alignments must be divisible"
        );

        // Calculate the absolute linear start position
        let start_abs = self.index * my_align + self.offset;

        // If relative, the base is the start position of the parent; otherwise, the base is 0.
        let base_offset = if relative { self.index * my_align } else { 0 };

        SpanIter {
            cursor: start_abs,
            remaining: self.len,
            target_align,
            base_offset,
            _marker: PhantomData,
        }
    }
}

pub struct SpanIter<T> {
    /// Current absolute position
    cursor: u64,
    /// Remaining length to split
    remaining: u64,
    /// Target level alignment
    target_align: u64,
    /// Base offset for relative index calculation
    base_offset: u64,
    _marker: PhantomData<T>,
}

impl<T: SpanTag> Iterator for SpanIter<T> {
    type Item = Span<T>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        // Calculate the "effective" coordinate (absolute or relative)
        let effective_val = self.cursor - self.base_offset;

        let index = effective_val / self.target_align;
        let offset = effective_val % self.target_align;

        // Calculate how much we can take in the current target block
        let capacity = self.target_align - offset;
        let take = std::cmp::min(self.remaining, capacity);

        // Advance the state
        self.cursor += take;
        self.remaining -= take;

        Some(Span::new(index, offset, take))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Layout constants for testing convenience.
    // Simulation: Chunk = 64, Block = 16 (Small numbers for easy calculation)
    const CHUNK_SIZE: u64 = 64;
    const BLOCK_SIZE: u64 = 16;

    #[test]
    fn test_basic_split_spanning_multiple_blocks() {
        // Scenario: Chunk Index 0, Offset 10, Len 30
        // Global Range: [10..40)
        // Block Boundaries (Size 16): 0, 16, 32, 48
        //
        // Expected Splits:
        // 1. [10..16): Block 0, Offset 10, Len 6
        // 2. [16..32): Block 1, Offset 0,  Len 16 (Full)
        // 3. [32..40): Block 2, Offset 0,  Len 8

        let chunk_span = Span::<ChunkTag>::new(0, 10, 30);

        let blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, true)
            .collect();

        assert_eq!(blocks.len(), 3);

        assert_eq!(blocks[0], Span::new(0, 10, 6));
        assert_eq!(blocks[1], Span::new(1, 0, 16));
        assert_eq!(blocks[2], Span::new(2, 0, 8));
    }

    #[test]
    fn test_relative_vs_absolute_indexing() {
        // Scenario: Chunk Index 2 (Absolute start: 2 * 64 = 128)
        // Offset 5, Len 10. Absolute Range [133..143)
        // Block Size 16.
        // 133 falls into Global Block Index = 133 / 16 = 8. Remainder 5.

        let chunk_span = Span::<ChunkTag>::new(2, 5, 10);

        // Case A: Relative = true (Relative to Chunk 2)
        // The starting block of Chunk 2 corresponds to Global Block 8 (128/16).
        // Relative Index should be: (133 - 128) / 16 = 5 / 16 = 0.
        let rel_blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, true)
            .collect();

        assert_eq!(rel_blocks.len(), 1);
        assert_eq!(rel_blocks[0].index, 0, "Relative index should be 0");
        assert_eq!(rel_blocks[0].offset, 5);

        // Case B: Relative = false (Global Indexing)
        // Expected Index should be exactly 8.
        let abs_blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, false)
            .collect();

        assert_eq!(abs_blocks.len(), 1);
        assert_eq!(
            abs_blocks[0].index, 8,
            "Absolute index should be 8 (128+5)/16"
        );
        assert_eq!(abs_blocks[0].offset, 5);
    }

    #[test]
    fn test_tiny_span_within_block() {
        // Scenario: Entirely within Block 1
        // Chunk 0, Offset 20, Len 5. Range [20..25)
        // Block 0: 0-16, Block 1: 16-32.
        // Should generate only one Span: Block 1, Offset 4 (20-16), Len 5
        let chunk_span = Span::<ChunkTag>::new(0, 20, 5);

        let blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, true)
            .collect();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], Span::new(1, 4, 5));
    }

    #[test]
    fn test_exact_alignment() {
        // Scenario: Exactly fills one Block
        // Chunk 0, Offset 16, Len 16
        // Should correspond exactly to Block 1
        let chunk_span = Span::<ChunkTag>::new(0, 16, 16);

        let blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, true)
            .collect();

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0], Span::new(1, 0, 16));
    }

    #[test]
    fn test_zero_length() {
        let chunk_span = Span::<ChunkTag>::new(0, 10, 0);
        let blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, true)
            .collect();

        assert!(blocks.is_empty());
    }

    #[test]
    fn test_cross_chunk_boundary_logic() {
        // This is an edge case test.
        // Assume ChunkSize = 64.
        // Input Span: Index 0, Offset 60, Len 10. (Crosses boundary of Chunk 0 and Chunk 1)
        // Range: [60..70)
        // Block Size = 16.
        // 60..64: Block 3 (Offset 12, Len 4) -> The last block of Chunk 0
        // 64..70: Block 4 (Offset 0,  Len 6) -> The first block of Chunk 1

        let chunk_span = Span::<ChunkTag>::new(0, 60, 10);

        // Using Relative = true
        let blocks: Vec<Span<BlockTag>> = chunk_span
            .split_into(CHUNK_SIZE, BLOCK_SIZE, true)
            .collect();

        assert_eq!(blocks.len(), 2);

        // Part 1: Block 3 of Chunk 0 (Index 3)
        assert_eq!(blocks[0], Span::new(3, 12, 4));

        // Part 2:
        // Absolute Offset 64. Base Offset (Chunk 0 start) = 0.
        // Relative Index = (64 - 0) / 16 = 4.
        // Note: It returns 4 here.
        // Physically, Chunk 0 only has blocks 0, 1, 2, 3.
        // Index 4 actually implies "Block 0 of the next Chunk".
        // The caller needs to handle this (e.g., idx % blocks_per_chunk) if crossing boundaries is allowed.
        assert_eq!(blocks[1], Span::new(4, 0, 6));
    }

    #[test]
    #[should_panic(expected = "Parent alignment must be >= child alignment")]
    fn test_invalid_alignments_panic() {
        let s = Span::<ChunkTag>::new(0, 0, 1);
        // Parent size smaller than child size, should panic.
        let _ = s.split_into::<BlockTag>(10, 20, true);
    }
}
