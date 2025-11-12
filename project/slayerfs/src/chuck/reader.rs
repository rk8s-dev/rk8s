//! ChunkReader: fetch data from blocks according to offset/length, handling gaps with zeros.

use super::chunk::ChunkLayout;
use super::slice::{Read, SliceDesc, SliceIO};
use super::store::BlockStore;
use crate::meta::MetaStore;
use anyhow::{Result, ensure};
use futures_util::{StreamExt, TryStreamExt};
use std::cmp::{max, min};

pub struct ChunkReader<'a, B, S> {
    layout: ChunkLayout,
    chunk_id: u64,
    slices: Vec<SliceDesc>,
    prepared: bool,
    store: &'a B,
    meta: &'a S,
}

impl<'a, B: BlockStore, S: MetaStore> ChunkReader<'a, B, S> {
    pub fn new(layout: ChunkLayout, chunk_id: u64, store: &'a B, meta: &'a S) -> Self {
        Self {
            layout,
            chunk_id,
            slices: Vec::new(),
            prepared: false,
            store,
            meta,
        }
    }

    /// Load slice metadata for the current chunk. Must be called before `read`.
    pub async fn prepare_slices(&mut self) -> anyhow::Result<()> {
        self.slices = self.meta.get_slices(self.chunk_id).await?;
        self.prepared = true;
        Ok(())
    }

    pub async fn read(&mut self, offset: u32, len: usize) -> Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        ensure!(
            self.prepared,
            "ChunkReader::read requires prepare_slices() to run first"
        );

        let mut buf = vec![0; len];

        let mut intervals = Intervals::new(offset, offset + len as u32);
        let mut need_read = Vec::new();

        for slice in self.slices.iter().copied().rev() {
            let ranges = intervals.cut(slice.offset, slice.offset + slice.length);

            need_read.extend(ranges.into_iter().map(|(l, r)| SliceDesc {
                offset: l,
                length: r - l,
                ..slice
            }))
        }

        let results = futures::stream::iter(need_read.into_iter())
            .map(|desc| {
                let (layout, store) = (self.layout, self.store);
                async move {
                    let mut tmp = vec![0; desc.length as usize];
                    let slice = SliceIO::<Read, _>::new(desc, layout, store);
                    slice.read(&mut tmp).await?;
                    Ok::<_, anyhow::Error>((desc.offset, tmp))
                }
            })
            .buffer_unordered(16)
            .try_collect::<Vec<_>>()
            .await?;

        for (slice_offset, chunk) in results {
            let start = (slice_offset - offset) as usize;
            buf[start..start + chunk.len()].copy_from_slice(&chunk);
        }
        Ok(buf)
    }
}

pub struct Intervals<T: Copy + Ord>(Vec<(T, T)>);

impl<T: Copy + Ord> Intervals<T> {
    pub fn new(l: T, r: T) -> Self {
        debug_assert!(l <= r, "invalid interval: left must be <= right");
        Intervals(vec![(l, r)])
    }

    pub fn cut(&mut self, slice_l: T, slice_r: T) -> Vec<(T, T)> {
        if self.0.is_empty() {
            return Vec::new();
        }

        let mut remaining = Vec::new();
        let mut cut = Vec::new();
        let mut touched = false;

        for &(l, r) in &self.0 {
            if r <= slice_l || l >= slice_r {
                remaining.push((l, r));
                continue;
            }

            touched = true;
            let cut_l = max(l, slice_l);
            let cut_r = min(r, slice_r);
            if cut_l < cut_r {
                cut.push((cut_l, cut_r));
            }
            if l < cut_l {
                remaining.push((l, cut_l));
            }
            if cut_r < r {
                remaining.push((cut_r, r));
            }
        }

        if !touched {
            return Vec::new();
        }

        self.0 = remaining.into_iter().filter(|(l, r)| l < r).collect();
        cut
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::chuck::writer::ChunkWriter;
    use crate::meta::create_meta_store_from_url;

    fn assert_sorted_non_overlapping(ranges: &[(u64, u64)]) {
        for window in ranges.windows(2) {
            assert!(window[0].1 <= window[1].0, "overlap detected: {:?}", ranges);
        }
    }

    fn intervals_from_ranges(ranges: &[(u64, u64)]) -> Intervals<u64> {
        Intervals(ranges.to_vec())
    }

    fn intervals_as_vec(intervals: &Intervals<u64>) -> Vec<(u64, u64)> {
        intervals.0.clone()
    }

    fn naive_cut(intervals: &mut Vec<(u64, u64)>, slice_l: u64, slice_r: u64) -> Vec<(u64, u64)> {
        assert!(slice_l <= slice_r, "invalid slice");
        let mut result = Vec::new();
        let mut next = Vec::new();
        for &(l, r) in intervals.iter() {
            if r <= slice_l || l >= slice_r {
                next.push((l, r));
                continue;
            }
            let cut_l = l.max(slice_l);
            let cut_r = r.min(slice_r);
            if cut_l < cut_r {
                result.push((cut_l, cut_r));
            }
            if l < cut_l {
                next.push((l, cut_l));
            }
            if cut_r < r {
                next.push((cut_r, r));
            }
        }
        *intervals = next;
        result
    }

    #[test]
    fn test_intervals_cut_partial_left() {
        let mut intervals = Intervals::new(40, 140);
        let results = intervals.cut(10, 60);
        assert_eq!(results, [(40, 60)]);
        assert_eq!(intervals_as_vec(&intervals), vec![(60, 140)]);
    }

    #[test]
    fn test_intervals_cut_partial_right() {
        let mut intervals = Intervals::new(40, 140);
        let results = intervals.cut(100, 180);
        assert_eq!(results, [(100, 140)]);
        assert_eq!(intervals_as_vec(&intervals), vec![(40, 100)]);
    }

    #[test]
    fn test_intervals_cut_no_overlap() {
        let mut intervals = Intervals::new(40, 140);
        let results = intervals.cut(200, 240);
        assert!(results.is_empty());
        assert_eq!(intervals_as_vec(&intervals), vec![(40, 140)]);
    }

    #[test]
    fn test_intervals_cut_spans_multiple_ranges() {
        let mut intervals = intervals_from_ranges(&[(100, 250), (300, 500)]);
        let results = intervals.cut(150, 400);
        assert_eq!(results, vec![(150, 250), (300, 400)]);
        assert_eq!(intervals_as_vec(&intervals), vec![(100, 150), (400, 500)]);
    }

    #[test]
    fn test_intervals_multiple_sequential_cuts() {
        let mut intervals = Intervals::new(0, 1000);

        intervals.cut(200, 400);
        assert_eq!(intervals_as_vec(&intervals), vec![(0, 200), (400, 1000)]);

        intervals.cut(600, 800);
        assert_eq!(
            intervals_as_vec(&intervals),
            vec![(0, 200), (400, 600), (800, 1000)]
        );

        let results = intervals.cut(100, 900);
        assert_eq!(results, vec![(100, 200), (400, 600), (800, 900)]);
        assert_eq!(intervals_as_vec(&intervals), vec![(0, 100), (900, 1000)]);
    }

    #[test]
    fn test_intervals_cut_consumes_entire_ranges() {
        let mut intervals = intervals_from_ranges(&[(0, 100), (200, 300)]);
        let results = intervals.cut(0, 400);
        assert_eq!(results, vec![(0, 100), (200, 300)]);
        assert!(intervals_as_vec(&intervals).is_empty());
    }

    #[test]
    fn test_intervals_cut_with_touching_edges() {
        let mut intervals = intervals_from_ranges(&[(0, 50), (50, 100), (100, 150)]);
        let results = intervals.cut(25, 125);
        assert_eq!(results, vec![(25, 50), (50, 100), (100, 125)]);
        assert_eq!(intervals_as_vec(&intervals), vec![(0, 25), (125, 150)]);
    }

    #[test]
    fn test_intervals_long_sequence_matches_naive() {
        let mut intervals = Intervals::new(0, 10_000);
        let mut expected = vec![(0, 10_000)];
        let ops = vec![
            (100, 300),
            (50, 150),
            (400, 800),
            (200, 500),
            (0, 50),
            (950, 1_200),
            (850, 950),
            (600, 700),
            (1_000, 1_500),
            (100, 400),
            (10, 90),
            (0, 1_000),
            (2_000, 2_500),
            (7_500, 8_000),
            (9_500, 10_500),
        ];

        for &(l, r) in &ops {
            let got = intervals.cut(l, r);
            let expected_cut = naive_cut(&mut expected, l, r);
            assert_eq!(got, expected_cut, "cut mismatch for [{}, {})", l, r);
            assert_eq!(
                intervals_as_vec(&intervals),
                expected,
                "state mismatch after [{}, {})",
                l,
                r
            );
            assert_sorted_non_overlapping(&expected);
        }
    }

    #[test]
    fn test_intervals_long_sequence_multiple_ranges() {
        let mut intervals =
            intervals_from_ranges(&[(0, 200), (300, 600), (800, 1_200), (1_500, 1_800)]);
        let mut expected = vec![(0, 200), (300, 600), (800, 1_200), (1_500, 1_800)];
        let ops = vec![
            (50, 250),
            (100, 350),
            (275, 325),
            (500, 900),
            (0, 150),
            (700, 1_000),
            (1_100, 1_600),
            (1_550, 1_750),
            (350, 1_250),
            (0, 2_000),
        ];

        for &(l, r) in &ops {
            let got = intervals.cut(l, r);
            let expected_cut = naive_cut(&mut expected, l, r);
            assert_eq!(got, expected_cut, "cut mismatch for [{}, {})", l, r);
            assert_eq!(
                intervals_as_vec(&intervals),
                expected,
                "state mismatch after [{}, {})",
                l,
                r
            );
            assert_sorted_non_overlapping(&expected);
        }
    }

    #[tokio::test]
    async fn test_reader_zero_fills_holes() {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        // Only write the first half of the second block
        {
            let w = ChunkWriter::new(layout, 7, &store, &meta);
            let buf = vec![1u8; (layout.block_size / 2) as usize];
            w.write(layout.block_size, &buf).await.unwrap();
        }
        let mut r = ChunkReader::new(layout, 7, &store, &meta);
        r.prepare_slices().await.unwrap();
        // Read from the back half of block 0 to the front half of block 1 (one block total)
        let off = layout.block_size / 2;
        let res = r.read(off, layout.block_size as usize).await.unwrap();
        assert_eq!(res.len(), layout.block_size as usize);
        // The first half should be zero-filled and the second half should be ones
        assert!(
            res[..(layout.block_size / 2) as usize]
                .iter()
                .all(|&b| b == 0)
        );
        assert!(
            res[(layout.block_size / 2) as usize..]
                .iter()
                .all(|&b| b == 1)
        );
    }
}
