// Read pipeline (high-level):
// - FileReader::read_at splits a file read into chunk spans and prepares slices.
// - prepare_slices ensures SliceState records exist for target ranges and kicks off
//   background_fetch to pull data from object storage.
// - read_from_slice waits until the slice is Ready (or Invalid) and copies data into
//   the caller buffer. Overlapping slices obey "latest slice wins" ordering.
// - Writer commit will call DataReader::invalidate(...) to mark cached slices stale.

use crate::chuck::reader::DataFetcher;
use crate::chuck::{BlockStore, ChunkLayout};
use crate::meta::MetaStore;
use crate::utils::Intervals;
use crate::vfs::backend::Backend;
use crate::vfs::chunk_id_for;
use crate::vfs::config::ReadConfig;
use crate::vfs::inode::Inode;
use crate::vfs::io::split_chunk_spans;
use dashmap::DashMap;
use parking_lot::Mutex as ParkingMutex;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;

const MAX_WAIT: Duration = Duration::from_secs(30);

#[allow(clippy::type_complexity)]
pub(crate) struct DataReader<B, M> {
    config: Arc<ReadConfig>,
    buffer_usage: Arc<AtomicU64>,
    // per-handle readers, grouped by inode
    files: DashMap<u64, Vec<(u64, Arc<FileReader<B, M>>)>>, // ino -> (fh, reader)
    backend: Arc<Backend<B, M>>,
}

impl<B, M> DataReader<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    pub(crate) fn new(config: Arc<ReadConfig>, backend: Arc<Backend<B, M>>) -> Self {
        Self {
            config,
            buffer_usage: Arc::new(AtomicU64::new(0)),
            files: DashMap::new(),
            backend,
        }
    }

    pub(crate) fn open_for_handle(&self, ino: Arc<Inode>, fh: u64) -> Arc<FileReader<B, M>> {
        let ino_number = ino.ino();
        let reader = Arc::new(FileReader::new(
            self.config.clone(),
            self.buffer_usage.clone(),
            ino,
            self.backend.clone(),
        ));

        self.files
            .entry(ino_number as u64)
            .or_default()
            .push((fh, reader.clone()));
        reader
    }

    pub(crate) fn close_for_handle(&self, ino: u64, fh: u64) {
        if let Some(mut entry) = self.files.get_mut(&ino) {
            entry.retain(|(id, _)| *id != fh);
            let empty = entry.is_empty();
            drop(entry);
            if empty {
                self.files.remove(&ino);
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn reader_for_handle(&self, ino: u64, fh: u64) -> Option<Arc<FileReader<B, M>>> {
        self.files.get(&ino).and_then(|entry| {
            entry
                .iter()
                .find(|(id, _)| *id == fh)
                .map(|(_, reader)| reader.clone())
        })
    }

    fn collect_readers(&self, ino: u64) -> Vec<Arc<FileReader<B, M>>> {
        match self.files.get(&ino) {
            Some(entry) => entry.iter().map(|(_, reader)| reader.clone()).collect(),
            None => vec![],
        }
    }

    pub(crate) async fn invalidate(&self, ino: u64, offset: u64, len: usize) -> anyhow::Result<()> {
        for reader in self.collect_readers(ino) {
            reader.invalidate(offset, len).await;
        }
        Ok(())
    }

    pub(crate) async fn invalidate_all(&self, ino: u64) {
        for reader in self.collect_readers(ino) {
            reader.invalidate_all().await;
        }
    }
}

/// A Session tracks the read pattern of a specific handle to guide slice eviction.
///
/// There are 4 fields:
/// 1. `ahead`: possible readahead length.
/// 2. `last_off`: the offset of the last read operation.
/// 3. `total`: total read length of the session.
/// 4. `atime`: the last time.
///
/// According to the Principle of Locality, when a range is read, its adjacent ranges are
/// likely to be read soon. The session records the last read offset and predicts a readahead range.
/// It uses this pattern to evaluate slice utility:
/// slices outside the predicted range are treated as "useless" and will be cleaned to satisfy the buffer size limit.
///
/// A slice `[start, end]` is considered "useful" if it falls within the window:
/// `[last_off - backward_tolerance, last_off + forward_prediction]`
/// where:
/// - `backward_tolerance = max(ahead / 8, block_size)`
/// - `forward_prediction = 2 * ahead + 2 * block_size`
///
/// This windows reflects an aggressive forward readahead strategy while remaining tolerant of small backward seeks.
///
/// To adapt to larger sequential reads, the `ahead` length is doubled whenever the total read length reaches the current
/// `ahead` threshold, effectively expanding the predictive window. In contract, it reduces by half to adapt smaller reads.
///
/// A handle generally maintains two independent Sessions to support concurrent read patterns.
/// This is particularly beneficial for interleaved `pread` operations, as it allows the system to track
/// two separate read streams simultaneously without their predictive windows interfering with each other.
///
/// If these two sessions are both available, it selects the oldest (atime).
#[derive(Clone, Copy)]
struct Session {
    ahead: u64,
    last_off: u64,
    total: u64,
    atime: Instant,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            ahead: 0,
            last_off: 0,
            total: 0,
            atime: Instant::now(),
        }
    }
}

impl Session {
    fn reset(&mut self, off: u64, _len: u64) {
        self.last_off = off;
        self.total = 0;
        self.ahead = 0;
        self.atime = Instant::now();
    }

    fn update(&mut self, off: u64, len: u64) {
        let end = off + len;
        if end > self.last_off {
            self.total += end - self.last_off;
            self.last_off = end;
        }
        self.atime = Instant::now();
    }

    fn window(&self, block_size: u64) -> (u64, u64) {
        let back = (self.ahead / 8).max(block_size);

        let win_start = self.last_off.saturating_sub(back);
        let win_end = self
            .last_off
            .saturating_add(self.ahead.saturating_mul(2))
            .saturating_add(block_size.saturating_mul(2));
        (win_start, win_end)
    }

    fn update_ahead(&mut self, cfg: &ReadConfig) {
        if self.ahead == 0 {
            self.ahead = cfg.layout.block_size as u64;
        } else if self.total >= self.ahead && self.ahead < cfg.max_ahead {
            // Double the ahead to adapt larger read patterns.
            self.ahead *= 2;
        } else if self.total.saturating_mul(4) < self.ahead {
            // Only shrink when the current sequential progress is much smaller than
            // the prediction window.
            self.ahead /= 2;
        }
    }
}

#[derive(Copy, Clone)]
enum SliceStatus {
    /// Created and fetching has not yet begun.
    New = 0,
    /// Fetching data
    Busy,
    /// Data is ready
    Ready,
    /// Data is stale and may be recycled
    Invalid,
    /// Refreshing data
    Refresh,
}

struct SliceState {
    /// Chunk index it belongs to
    index: u64,
    /// Range it contains
    range: (u32, u32),
    page: Vec<u8>,
    state: SliceStatus,
    err: Option<String>,
    notify: Arc<Notify>,
    /// Generation count
    generation: u64,
    /// Reference count
    refs: u16,
}

impl SliceState {
    fn new(index: u64, range: (u32, u32), refs: u16) -> Self {
        Self {
            index,
            range,
            page: Vec::new(),
            state: SliceStatus::New,
            err: None,
            notify: Arc::new(Notify::new()),
            generation: 0,
            refs,
        }
    }

    fn in_flight(&self) -> bool {
        matches!(
            self.state,
            SliceStatus::Refresh | SliceStatus::New | SliceStatus::Busy
        )
    }

    fn overlaps(&self, offset: u32, len: u32) -> bool {
        let end = offset.saturating_add(len);
        self.range.0 < end && offset < self.range.1
    }

    fn background_fetch<B, M>(
        this: Arc<ParkingMutex<SliceState>>,
        ino: u64,
        usage: Arc<AtomicU64>,
        layout: ChunkLayout,
        backend: Arc<Backend<B, M>>,
    ) where
        B: BlockStore + Send + Sync + 'static,
        M: MetaStore + Send + Sync + 'static,
    {
        tokio::spawn(async move {
            let (index, (start, end), generation) = {
                let mut guard = this.lock();
                match guard.state {
                    SliceStatus::Busy | SliceStatus::Invalid => {
                        return;
                    }
                    _ => {
                        guard.state = SliceStatus::Busy;
                    }
                }
                (guard.index, guard.range, guard.generation)
            };

            let chunk_id = chunk_id_for(ino as i64, index);
            let f = || async {
                let mut fetcher = DataFetcher::new(layout, chunk_id, &backend);
                fetcher.prepare_slices().await?;

                let out = fetcher.read_at(start, (end - start) as usize).await?;
                Ok::<_, anyhow::Error>(out)
            };

            let result = f().await;
            let mut guard = this.lock();

            // Stale fetch and needs to drop.
            if guard.generation != generation {
                return;
            }

            match result {
                Ok(out) => {
                    let old_len = guard.page.len() as u64;
                    let new_len = out.len() as u64;

                    if new_len >= old_len {
                        usage.fetch_add(new_len - old_len, Ordering::Relaxed);
                    } else {
                        sub_usage(&usage, old_len - new_len);
                    }

                    guard.state = SliceStatus::Ready;
                    guard.page = out;
                    guard.err = None;
                }
                Err(e) => {
                    sub_usage(&usage, guard.page.len() as u64);
                    guard.state = SliceStatus::Invalid;
                    guard.page = Vec::new();
                    guard.err = Some(e.to_string());
                }
            }
            guard.notify.notify_waiters();
        });
    }
}

struct SlicePinGuard {
    slices: Vec<Arc<ParkingMutex<SliceState>>>,
}

impl SlicePinGuard {
    pub fn new() -> Self {
        Self { slices: Vec::new() }
    }

    pub fn add(&mut self, slice: Arc<ParkingMutex<SliceState>>) {
        self.slices.push(slice);
    }
}

impl Drop for SlicePinGuard {
    fn drop(&mut self) {
        for slice in self.slices.drain(..) {
            let mut guard = slice.lock();
            guard.refs = guard.refs.saturating_sub(1);
        }
    }
}

pub(crate) struct FileReader<B, M> {
    config: Arc<ReadConfig>,
    buffer_usage: Arc<AtomicU64>,
    inode: Arc<Inode>,
    slices: Mutex<VecDeque<Arc<ParkingMutex<SliceState>>>>,
    sessions: ParkingMutex<[Session; 2]>,
    backend: Arc<Backend<B, M>>,
}

impl<B, M> FileReader<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    pub(crate) fn new(
        config: Arc<ReadConfig>,
        buffer_usage: Arc<AtomicU64>,
        inode: Arc<Inode>,
        backend: Arc<Backend<B, M>>,
    ) -> Self {
        Self {
            config,
            inode,
            buffer_usage,
            slices: Mutex::new(VecDeque::new()),
            sessions: ParkingMutex::new([Session::default(); 2]),
            backend,
        }
    }

    pub(crate) async fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let mut buf = vec![0u8; len];
        let read = self.read_at(offset, &mut buf).await?;
        buf.truncate(read);
        Ok(buf)
    }

    fn select_forward_session_match(&self, sessions: &[Session; 2], offset: u64) -> Option<usize> {
        let sat = |s: &Session, offset: u64| {
            s.last_off <= offset
                && offset <= s.last_off + s.ahead + self.config.layout.block_size as u64
        };

        let max_off = if sessions[0].last_off > sessions[1].last_off {
            0
        } else {
            1
        };

        if sat(&sessions[max_off], offset) {
            return Some(max_off);
        }
        if sat(&sessions[1 - max_off], offset) {
            return Some(1 - max_off);
        }
        None
    }

    fn select_back_session_match(&self, sessions: &[Session; 2], offset: u64) -> Option<usize> {
        let sat = |s: &Session, offset: u64| {
            let back = (s.ahead / 8).max(self.config.layout.block_size as u64);
            offset < s.last_off && offset >= s.last_off.saturating_sub(back)
        };

        let min_off = if sessions[0].last_off < sessions[1].last_off {
            0
        } else {
            1
        };

        if sat(&sessions[min_off], offset) {
            return Some(min_off);
        }
        if sat(&sessions[1 - min_off], offset) {
            return Some(1 - min_off);
        }
        None
    }

    fn select_session_fallback(
        &self,
        sessions: &mut [Session; 2],
        offset: u64,
        len: usize,
    ) -> usize {
        if sessions[0].total == 0 {
            return 0;
        }
        if sessions[1].total == 0 {
            return 1;
        }

        let oldest_atime = if sessions[0].atime < sessions[1].atime {
            0
        } else {
            1
        };
        sessions[oldest_atime].reset(offset, len as u64);
        oldest_atime
    }

    fn check_session(&self, offset: u64, len: usize) {
        let mut session = self.sessions.lock();

        let selected = self
            .select_forward_session_match(&session, offset)
            .or_else(|| self.select_back_session_match(&session, offset))
            .unwrap_or(self.select_session_fallback(&mut session, offset, len));
        session[selected].update(offset, len as u64);
        session[selected].update_ahead(&self.config);
    }

    async fn clean_evictable_slices(&self, offset: u64, len: usize) {
        // Early exit if memory usage is acceptable
        if self.buffer_usage.load(Ordering::Relaxed) <= self.config.buffer_size {
            return;
        }

        let sessions = *self.sessions.lock();
        let windows = sessions
            .iter()
            .filter(|s| s.total > 0)
            .map(|s| s.window(self.config.layout.block_size as u64))
            .collect::<Vec<_>>();

        let cur_start = offset;
        let cur_end = offset + len as u64;

        let mut guard = self.slices.lock().await;
        guard.retain(|slice| {
            let slice = slice.lock();

            if slice.refs > 0 || slice.in_flight() {
                return true;
            }

            let base = slice.index * self.config.layout.chunk_size;
            let slice_start = base + slice.range.0 as u64;
            let slice_end = base + slice.range.1 as u64;

            let overlaps_current = slice_start < cur_end && cur_start < slice_end;
            let needed_by_session = windows
                .iter()
                .any(|(win_start, win_end)| slice_start < *win_end && *win_start < slice_end);
            let need_retain = overlaps_current || needed_by_session;

            if !need_retain {
                sub_usage(&self.buffer_usage, slice.page.len() as u64);
            }
            need_retain
        })
    }

    async fn back_pressure(&self) -> anyhow::Result<()> {
        tracing::trace!(
            "Memory usage: {}MiB",
            self.buffer_usage.load(Ordering::Relaxed) / 1024 / 1024
        );

        let mut total_wait = Duration::ZERO;
        let hard_limit = self.config.buffer_size * 2;
        // buffer size limit is just a soft limit. It is perfectly normal to see that the current memory usage exceed it.
        if self.buffer_usage.load(Ordering::Relaxed) > self.config.buffer_size {
            tokio::time::sleep(Duration::from_millis(10)).await;

            // `2 * buffer size limit` is a hard limit. A read operation idle until memory pressure is relieved.
            while self.buffer_usage.load(Ordering::Relaxed) > hard_limit {
                if total_wait >= MAX_WAIT {
                    return Err(anyhow::anyhow!(
                        "Timeout waiting for buffer space after {:?}. Current usage: {} bytes, limit: {} bytes",
                        total_wait,
                        self.buffer_usage.load(Ordering::Relaxed),
                        hard_limit,
                    ));
                }

                tracing::warn!("Reach buffer size hard limit: sleep for 100 millis");
                tokio::time::sleep(Duration::from_millis(100)).await;
                total_wait += Duration::from_millis(100);
            }
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self, buf), fields(offset, len = buf.len()))]
    pub(crate) async fn read_at(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let file_size = self.inode.file_size();
        if file_size <= offset {
            return Ok(0);
        }

        let actual_len = std::cmp::min(buf.len(), file_size as usize - offset as usize);
        if actual_len == 0 {
            return Ok(0);
        }

        self.back_pressure().await?;
        self.clean_evictable_slices(offset, actual_len).await;

        // Pre-fill with zeros so holes or missing slices read as zeros.
        buf[..actual_len].fill(0);

        let spans = split_chunk_spans(self.config.layout, offset, actual_len);

        let mut pin_guard = Vec::new();
        for span in spans.iter().copied() {
            // `prepare_slices` don't wait for all data is ready.
            pin_guard.push(
                self.prepare_slices(span.index, (span.offset, span.offset + span.len))
                    .await,
            );
        }

        self.check_session(offset, actual_len);

        let mut tail = buf;
        let result = async {
            for span in spans {
                let (seg, rest) = tail.split_at_mut(span.len as usize);
                tail = rest;
                self.read_from_slice(span.index, span.offset, seg).await?;
            }
            Ok::<_, anyhow::Error>(actual_len)
        }
        .await;

        drop(pin_guard);

        // Do a cleanup each read.
        self.cleanup_invalid().await;
        result
    }

    async fn wait_ready(slice: &Arc<ParkingMutex<SliceState>>) -> anyhow::Result<()> {
        loop {
            let notify = {
                let guard = slice.lock();

                match guard.state {
                    SliceStatus::Ready => return Ok(()),
                    SliceStatus::Invalid => {
                        let err = guard.err.as_deref().unwrap_or("slice invalid");
                        return Err(anyhow::anyhow!("Slice fetch failed: {err}"));
                    }
                    _ => guard.notify.clone(),
                }
            };
            notify.notified().await;
        }
    }

    // Read from cached slices for a chunk. This waits for slice readiness and copies
    // overlapping ranges into the provided buffer.
    async fn read_from_slice(&self, index: u64, offset: u32, buf: &mut [u8]) -> anyhow::Result<()> {
        let slices = {
            let guard = self.slices.lock().await;
            guard
                .iter()
                .filter(|s| s.lock().index == index)
                .cloned()
                .collect::<Vec<_>>()
        };

        for slice in slices {
            // There locks the slice twice, but it is still worth it
            // as the parking_lot::Mutex is lightweight.
            let (dst_start, dst_end) = {
                let guard = slice.lock();
                (
                    offset.max(guard.range.0),
                    guard.range.1.min(offset + buf.len() as u32),
                )
            };

            if dst_start >= dst_end {
                continue;
            }

            Self::wait_ready(&slice).await?;

            let guard = slice.lock();

            let dst_local_start = dst_start - offset;
            let dst_local_end = dst_end - offset;
            let src_start = dst_start - guard.range.0;
            let src_end = dst_end - guard.range.0;

            buf[dst_local_start as usize..dst_local_end as usize]
                .copy_from_slice(&guard.page[src_start as usize..src_end as usize]);
        }
        Ok(())
    }

    async fn prepare_slices(&self, index: u64, (start, end): (u32, u32)) -> SlicePinGuard {
        let mut pinned = SlicePinGuard::new();
        let mut cutter = Intervals::new(start, end);

        let mut guard = self.slices.lock().await;
        for slice in guard.iter() {
            let mut guard = slice.lock();

            if guard.index != index {
                continue;
            }

            if matches!(guard.state, SliceStatus::Invalid) {
                continue;
            }

            // The "reservation" needs to read this slice.
            if guard.overlaps(start, end.saturating_sub(start)) {
                guard.refs = guard.refs.saturating_add(1);
                pinned.add(slice.clone());
            }

            let (l, r) = guard.range;
            cutter.cut(l, r);
        }

        for range in cutter.collect() {
            let slice = Arc::new(ParkingMutex::new(SliceState::new(index, range, 1)));
            SliceState::background_fetch(
                slice.clone(),
                self.inode.ino() as u64,
                self.buffer_usage.clone(),
                self.config.layout,
                self.backend.clone(),
            );
            pinned.add(slice.clone());
            guard.push_back(slice);
        }

        pinned
    }

    async fn invalidate(&self, offset: u64, len: usize) {
        if len == 0 {
            return;
        }

        let spans = split_chunk_spans(self.config.layout, offset, len);

        let mut span_map = HashMap::new();
        for span in spans {
            span_map.insert(span.index, (span.offset, span.len));
        }

        let mut to_fetch = Vec::new();
        let mut new_slices = VecDeque::new();

        {
            let mut guard = self.slices.lock().await;
            let mut freed = 0u64;
            for slice in guard.drain(..) {
                let mut state = slice.lock();
                let Some((span_offset, span_len)) = span_map.get(&state.index) else {
                    new_slices.push_back(slice.clone());
                    continue;
                };
                if !state.overlaps(*span_offset, *span_len) {
                    new_slices.push_back(slice.clone());
                    continue;
                }

                state.generation += 1;

                match state.state {
                    SliceStatus::Ready => {
                        if state.refs > 0 {
                            state.state = SliceStatus::Refresh;
                            to_fetch.push(slice.clone());
                        } else {
                            state.state = SliceStatus::Invalid;
                        }
                    }
                    SliceStatus::Busy | SliceStatus::New | SliceStatus::Refresh => {
                        state.state = SliceStatus::Refresh;
                        to_fetch.push(slice.clone());
                    }
                    SliceStatus::Invalid => {}
                }
                state.notify.notify_waiters();

                if !matches!(state.state, SliceStatus::Invalid) || state.refs > 0 {
                    new_slices.push_back(slice.clone());
                } else {
                    freed += state.page.len() as u64;
                }
            }
            *guard = new_slices;
            sub_usage(&self.buffer_usage, freed);
        }

        // Invalidated slices must be re-fetched.
        for slice in to_fetch {
            SliceState::background_fetch(
                slice,
                self.inode.ino() as u64,
                self.buffer_usage.clone(),
                self.config.layout,
                self.backend.clone(),
            );
        }
    }

    async fn invalidate_all(&self) {
        let mut guard = self.slices.lock().await;
        let mut freed = 0u64;
        for slice in guard.drain(..) {
            let state = slice.lock();
            freed += state.page.len() as u64;
        }
        sub_usage(&self.buffer_usage, freed);
    }

    /// Clean all invalid and unused slices.
    async fn cleanup_invalid(&self) {
        let mut guard = self.slices.lock().await;
        guard.retain(|slice| {
            let state = slice.lock();

            let need_retain = !(matches!(state.state, SliceStatus::Invalid) && state.refs == 0);
            if !need_retain {
                sub_usage(&self.buffer_usage, state.page.len() as u64);
            }
            need_retain
        });
    }
}

fn sub_usage(usage: &AtomicU64, delta: u64) {
    let _ = usage.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |r| {
        Some(r.saturating_sub(delta))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::ChunkLayout;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::chuck::writer::DataUploader;
    use crate::meta::SLICE_ID_KEY;
    use crate::meta::factory::create_meta_store_from_url;
    use crate::vfs::config::{ReadConfig, WriteConfig};
    use crate::vfs::io::writer::FileWriter;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::{sleep, timeout};

    fn small_layout() -> ChunkLayout {
        ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        }
    }

    #[tokio::test]
    async fn test_file_reader_cross_chunks() {
        let layout = small_layout();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

        let ino: i64 = 11;
        let offset = layout.chunk_size - 512;
        let data = vec![9u8; 2048];
        let head = &data[..512];
        let tail = &data[512..];

        let slice_id1 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 0), backend.as_ref());
        let desc1 = uploader
            .write_at_vectored(
                slice_id1 as u64,
                offset as u32,
                &[bytes::Bytes::copy_from_slice(head)],
            )
            .await
            .unwrap();
        meta.append_slice(chunk_id_for(ino, 0), desc1)
            .await
            .unwrap();

        let slice_id2 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 1), backend.as_ref());
        let desc2 = uploader
            .write_at_vectored(slice_id2 as u64, 0, &[bytes::Bytes::copy_from_slice(tail)])
            .await
            .unwrap();
        meta.append_slice(chunk_id_for(ino, 1), desc2)
            .await
            .unwrap();

        let inode = Inode::new(ino, offset + data.len() as u64);
        let reader = DataReader::new(Arc::new(ReadConfig::new(layout)), backend.clone());
        let file_reader = reader.open_for_handle(inode, 1);
        let out = file_reader.read(offset, data.len()).await.unwrap();
        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_reader_invalidate_refresh() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

        let ino: i64 = 22;
        let data1 = vec![1u8; 2048];
        let data2 = vec![2u8; 2048];

        let slice_id1 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 0), backend.as_ref());
        let desc1 = uploader
            .write_at_vectored(
                slice_id1 as u64,
                0,
                &[bytes::Bytes::copy_from_slice(&data1)],
            )
            .await
            .unwrap();
        meta.append_slice(chunk_id_for(ino, 0), desc1)
            .await
            .unwrap();

        let inode = Inode::new(ino, data1.len() as u64);
        let reader = DataReader::new(Arc::new(ReadConfig::new(layout)), backend.clone());
        let file_reader = reader.open_for_handle(inode, 1);
        let out1 = file_reader.read(0, data1.len()).await.unwrap();
        assert_eq!(out1, data1);

        let slice_id2 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let desc2 = uploader
            .write_at_vectored(
                slice_id2 as u64,
                0,
                &[bytes::Bytes::copy_from_slice(&data2)],
            )
            .await
            .unwrap();
        meta.append_slice(chunk_id_for(ino, 0), desc2)
            .await
            .unwrap();

        reader.invalidate(ino as u64, 0, data2.len()).await.unwrap();
        let out2 = file_reader.read(0, data2.len()).await.unwrap();
        assert_eq!(out2, data2);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_read_while_write_eventually_sees_data() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

        let ino: i64 = 66;
        let data = vec![5u8; 2048];
        let inode = Inode::new(ino, 0);

        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let file_reader = reader.open_for_handle(inode.clone(), 1);

        let writer = Arc::new(FileWriter::new(
            inode,
            Arc::new(WriteConfig::new(layout).page_size(4 * 1024)),
            backend.clone(),
            reader,
        ));

        let write_task = {
            let w = writer.clone();
            let payload = data.clone();
            tokio::spawn(async move {
                w.write_at(0, &payload).await.unwrap();
                w.flush().await.unwrap();
            })
        };

        timeout(Duration::from_secs(1), async {
            loop {
                let out = file_reader.read(0, data.len()).await.unwrap();
                if out == data {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("reader should eventually see flushed data");

        write_task.await.unwrap();
    }
}
