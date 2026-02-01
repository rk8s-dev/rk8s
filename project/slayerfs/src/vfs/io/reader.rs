// Read pipeline (high-level):
// - FileReader::read_at splits a file read into chunk spans and prepares slices.
// - prepare_slices ensures SliceState records exist for target ranges and kicks off
//   background_fetch to pull data from object storage.
// - read_from_slice waits until the slice is Ready (or Invalid) and copies data into
//   the caller buffer. Overlapping slices obey "latest slice wins" ordering.
// - Writer commit will call DataReader::invalidate(...) to mark cached slices stale.

use crate::chuck::reader::DataFetcher;
use crate::chuck::{BlockStore, ChunkLayout};
use crate::meta::MetaLayer;
use crate::utils::{Intervals, NumCastExt, UsageGuard};
use crate::vfs::Inode;
use crate::vfs::backend::Backend;
use crate::vfs::chunk_id_for;
use crate::vfs::config::ReadConfig;
use crate::vfs::io::split_chunk_spans;
use dashmap::{DashMap, Entry};
use parking_lot::Mutex as ParkingMutex;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::{Mutex, Notify};
use tokio::time::Instant;
use tracing::Instrument;

const MAX_WAIT: Duration = Duration::from_secs(30);
const DEFAULT_TOTAL_AHEAD_LIMIT: u64 = 256 * 1024 * 1024;
const READ_SESSIONS: usize = 2;

#[allow(clippy::type_complexity)]
pub(crate) struct DataReader<B, M> {
    config: Arc<ReadConfig>,
    buffer_usage: Arc<AtomicU64>,
    /// Per-handle readers, grouped by inode
    files: DashMap<u64, Vec<(u64, Arc<FileReader<B, M>>)>>, // ino -> (fh, reader)
    backend: Arc<Backend<B, M>>,
}

impl<B, M> DataReader<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
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
        if let Entry::Occupied(mut entry) = self.files.entry(ino) {
            let mut removed = Vec::new();
            let list = entry.get_mut();

            list.retain(|(id, reader)| {
                if *id == fh {
                    removed.push(reader.clone());
                    false
                } else {
                    true
                }
            });

            if list.is_empty() {
                entry.remove();
            }

            for reader in removed {
                tokio::spawn(async move {
                    reader.invalidate_all().await;
                });
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

    fn update_ahead(
        &mut self,
        block_size: u64,
        max_ahead: u64,
        total_ahead_limit: u64,
        usage: u64,
        offset: u64,
        len: u64,
    ) {
        let mut ahead = self.ahead;

        if ahead == 0 && block_size <= max_ahead && (offset == 0 || self.total > len) {
            ahead = block_size;
        } else if ahead < max_ahead
            && self.total >= ahead
            && total_ahead_limit > usage.saturating_add(ahead.saturating_mul(4))
        {
            ahead = ahead.saturating_mul(2);
        } else if ahead >= block_size
            && (total_ahead_limit < usage.saturating_add(ahead / 2) || self.total < ahead / 4)
        {
            ahead /= 2;
        }

        self.ahead = ahead;
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
    range: (u64, u64),
    page: Vec<u8>,
    usage: UsageGuard,
    state: SliceStatus,
    err: Option<String>,
    notify: Arc<Notify>,
    /// Generation count
    generation: u64,
    /// Reference count
    refs: u16,
    /// Queue delay (milliseconds) before the fetch task actually started.
    queue_delay_ms: Option<u64>,
    /// Fetch duration (milliseconds) for the last successful/failed attempt.
    fetch_ms: Option<u64>,
    /// Last access time for eviction decisions.
    last_access: Instant,
}

impl SliceState {
    fn new(index: u64, range: (u64, u64), refs: u16, usage: Arc<AtomicU64>) -> Self {
        Self {
            index,
            range,
            page: Vec::new(),
            usage: UsageGuard::new(usage),
            state: SliceStatus::New,
            err: None,
            notify: Arc::new(Notify::new()),
            generation: 0,
            refs,
            queue_delay_ms: None,
            fetch_ms: None,
            last_access: Instant::now(),
        }
    }

    fn in_flight(&self) -> bool {
        matches!(
            self.state,
            SliceStatus::Refresh | SliceStatus::New | SliceStatus::Busy
        )
    }

    fn range_to_file(&self, chunk_size: u64) -> (u64, u64) {
        let base = self.index * chunk_size;
        (base + self.range.0, base + self.range.1)
    }

    fn overlaps(&self, offset: u64, len: u64) -> bool {
        let end = offset.saturating_add(len);
        self.range.0 < end && offset < self.range.1
    }

    fn background_fetch<B, M>(
        this: Arc<ParkingMutex<SliceState>>,
        ino: u64,
        layout: ChunkLayout,
        backend: Arc<Backend<B, M>>,
    ) where
        B: BlockStore + Send + Sync + 'static,
        M: MetaLayer + Send + Sync + 'static,
    {
        let queued_at = Instant::now();

        tokio::spawn(async move {
            let start_at = Instant::now();
            let queue_delay_ms = start_at.duration_since(queued_at).as_millis() as u64;
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
                guard.queue_delay_ms = Some(queue_delay_ms);
                guard.fetch_ms = None;
                (guard.index, guard.range, guard.generation)
            };

            let chunk_id = match chunk_id_for(ino as i64, index) {
                Ok(id) => id,
                Err(err) => {
                    let mut guard = this.lock();
                    guard.state = SliceStatus::Invalid;
                    guard.err = Some(err.to_string());
                    guard.notify.notify_waiters();
                    return;
                }
            };
            let f = || async {
                let mut fetcher = DataFetcher::new(layout, chunk_id, &backend);
                fetcher.prepare_slices().await?;

                let out = fetcher.read_at(start, (end - start).as_usize()).await?;
                Ok::<_, anyhow::Error>(out)
            };

            let result = f().await;
            let fetch_ms = start_at.elapsed().as_millis() as u64;
            let mut guard = this.lock();

            // Stale fetch and needs to drop.
            if guard.generation != generation {
                return;
            }

            guard.fetch_ms = Some(fetch_ms);
            match result {
                Ok(out) => {
                    guard.state = SliceStatus::Ready;
                    guard.page = out;
                    let new_len = guard.page.len() as u64;
                    guard.usage.update_bytes(new_len);
                    guard.err = None;
                }
                Err(e) => {
                    guard.state = SliceStatus::Invalid;
                    guard.page = Vec::new();
                    guard.usage.update_bytes(0);
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
    sessions: ParkingMutex<[Session; READ_SESSIONS]>,
    backend: Arc<Backend<B, M>>,
}

impl<B, M> FileReader<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
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
            sessions: ParkingMutex::new([Session::default(); READ_SESSIONS]),
            backend,
        }
    }

    #[tracing::instrument(name = "FileReader.read", level = "trace", skip(self))]
    pub(crate) async fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut buf = vec![0u8; len];
        let read = self.read_at(offset, &mut buf).await?;
        buf.truncate(read);
        Ok(buf)
    }

    fn select_forward_session_match(
        &self,
        sessions: &[Session; READ_SESSIONS],
        offset: u64,
    ) -> Option<usize> {
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

    fn select_back_session_match(
        &self,
        sessions: &[Session; READ_SESSIONS],
        offset: u64,
    ) -> Option<usize> {
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
        sessions: &mut [Session; READ_SESSIONS],
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

    fn check_session(&self, offset: u64, len: usize) -> u64 {
        let mut session = self.sessions.lock();

        let selected = self
            .select_forward_session_match(&session, offset)
            .or_else(|| self.select_back_session_match(&session, offset))
            .unwrap_or(self.select_session_fallback(&mut session, offset, len));

        session[selected].update(offset, len as u64);
        session[selected].update_ahead(
            self.config.layout.block_size as u64,
            self.max_ahead(),
            self.total_ahead_limit(),
            self.buffer_usage(),
            offset,
            len as u64,
        );
        session[selected].ahead
    }

    fn buffer_usage(&self) -> u64 {
        self.buffer_usage.load(Ordering::Relaxed)
    }

    fn total_ahead_limit(&self) -> u64 {
        if self.config.buffer_size > 0 {
            self.config.buffer_size * 8 / 10
        } else {
            DEFAULT_TOTAL_AHEAD_LIMIT
        }
    }

    fn max_ahead(&self) -> u64 {
        self.config.max_ahead.min(self.total_ahead_limit())
    }

    fn max_slice_amount(&self) -> usize {
        // Allow each session to keep approximately `max_ahead / block_size` slices.
        self.max_ahead()
            .saturating_div(self.config.layout.block_size as u64)
            .saturating_mul(READ_SESSIONS as u64)
            .saturating_add(1) as usize
    }

    async fn clean_evictable_slices(&self, offset: u64, len: usize) {
        let sessions = *self.sessions.lock();
        let windows = sessions
            .iter()
            .filter(|s| s.total > 0)
            .map(|s| s.window(self.config.layout.block_size as u64))
            .collect::<Vec<_>>();

        let slice_limit = self.max_slice_amount();

        let cur_start = offset;
        let cur_end = offset + len as u64;
        let now = Instant::now();

        let mut guard = self.slices.lock().await;
        let mut cnt = 0_usize;

        guard.retain(|s| {
            let state = s.lock();

            let (slice_start, slice_end) = state.range_to_file(self.config.layout.chunk_size);

            let overlaps_current = slice_start < cur_end && cur_start < slice_end;
            let needed_by_session = windows
                .iter()
                .any(|(win_start, win_end)| slice_start < *win_end && *win_start < slice_end);
            let expired = now.duration_since(state.last_access) > Duration::from_secs(30);

            let mut keep = true;
            if (matches!(state.state, SliceStatus::Invalid) && state.refs == 0)
                || (!overlaps_current
                    && (expired || !needed_by_session)
                    && state.refs == 0
                    && !state.in_flight())
            {
                keep = false;
            }

            if keep && !overlaps_current {
                cnt = cnt.saturating_add(1);
            }

            keep
        });

        if cnt > slice_limit {
            guard.retain(|s| {
                let state = s.lock();

                let (slice_start, slice_end) = state.range_to_file(self.config.layout.chunk_size);
                let overlaps_current = slice_start < cur_end && cur_start < slice_end;

                if !overlaps_current && cnt > slice_limit && state.refs == 0 && !state.in_flight() {
                    cnt = cnt.saturating_sub(1);
                    return false;
                }
                true
            })
        }
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

    async fn prepare_ahead_slices(&self, offset: u64, ahead: u64, guards: &mut Vec<SlicePinGuard>) {
        let aligned = (offset + ahead).next_multiple_of(self.config.layout.block_size as u64);

        let spans = split_chunk_spans(self.config.layout, offset, (aligned - offset).as_usize());
        for span in spans.iter().copied() {
            guards.push(
                self.prepare_slices(span.index, (span.offset, span.offset + span.len))
                    .await,
            );
        }
    }

    #[tracing::instrument(name = "FileReader.read_at", level = "trace", skip(self, buf), fields(offset, len = buf.len()))]
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

        self.clean_evictable_slices(offset, actual_len)
            .instrument(tracing::trace_span!(
                "read_at.clean_evictable_slices",
                offset,
                len = actual_len
            ))
            .await;
        self.back_pressure()
            .instrument(tracing::trace_span!("read_at.back_pressure"))
            .await?;

        let spans = tracing::trace_span!("read_at.split_spans", offset, len = actual_len)
            .in_scope(|| split_chunk_spans(self.config.layout, offset, actual_len));

        let mut pin_guard = Vec::new();
        for span in spans.iter().copied() {
            // `prepare_slices` don't wait for all data is ready.
            pin_guard.push(
                self.prepare_slices(span.index, (span.offset, span.offset + span.len))
                    .instrument(tracing::trace_span!(
                        "read_at.prepare_slice",
                        index = span.index,
                        offset = span.offset,
                        len = span.len
                    ))
                    .await,
            );
        }

        let ahead = tracing::trace_span!("read_at.check_session", offset, len = actual_len)
            .in_scope(|| self.check_session(offset, actual_len));

        tracing::trace_span!("FileReader.read_at.prepare_ahead_slices", offset, ahead)
            .in_scope(|| self.prepare_ahead_slices(offset, ahead, &mut pin_guard))
            .await;

        let mut tail = buf;
        let result = async {
            for span in spans {
                let span_len = span.len.as_usize();
                let (seg, rest) = tail.split_at_mut(span_len);
                tail = rest;
                self.read_from_slice(span.index, span.offset, seg).await?;
            }
            Ok::<_, anyhow::Error>(actual_len)
        }
        .instrument(tracing::trace_span!("read_at.read_spans"))
        .await;

        drop(pin_guard);

        // Do a cleanup each read.
        self.cleanup_invalid()
            .instrument(tracing::trace_span!("read_at.cleanup_invalid"))
            .await;
        result
    }

    #[tracing::instrument(
        level = "trace",
        skip(slice),
        fields(
            total_wait_ms = tracing::field::Empty,
            waits = tracing::field::Empty,
            queue_ms = tracing::field::Empty,
            fetch_ms = tracing::field::Empty
        )
    )]
    async fn wait_ready(slice: &Arc<ParkingMutex<SliceState>>) -> anyhow::Result<()> {
        let mut total_wait = Duration::ZERO;
        let mut waits: u64 = 0;
        loop {
            let notify = {
                let guard = slice.lock();

                match guard.state {
                    SliceStatus::Ready => {
                        if let Some(queue_ms) = guard.queue_delay_ms {
                            tracing::Span::current().record("queue_ms", queue_ms);
                        }
                        if let Some(fetch_ms) = guard.fetch_ms {
                            tracing::Span::current().record("fetch_ms", fetch_ms);
                        }
                        tracing::Span::current()
                            .record("total_wait_ms", total_wait.as_millis() as u64);
                        tracing::Span::current().record("waits", waits);
                        return Ok(());
                    }
                    SliceStatus::Invalid => {
                        let err = guard.err.as_deref().unwrap_or("slice invalid");
                        if let Some(queue_ms) = guard.queue_delay_ms {
                            tracing::Span::current().record("queue_ms", queue_ms);
                        }
                        if let Some(fetch_ms) = guard.fetch_ms {
                            tracing::Span::current().record("fetch_ms", fetch_ms);
                        }
                        tracing::Span::current()
                            .record("total_wait_ms", total_wait.as_millis() as u64);
                        tracing::Span::current().record("waits", waits);
                        return Err(anyhow::anyhow!("Slice fetch failed: {err}"));
                    }
                    _ => guard.notify.clone(),
                }
            };
            let start = Instant::now();
            notify
                .notified()
                .instrument(tracing::trace_span!("wait_ready.wait"))
                .await;
            total_wait += start.elapsed();
            waits = waits.saturating_add(1);
            tracing::Span::current().record("total_wait_ms", total_wait.as_millis() as u64);
            tracing::Span::current().record("waits", waits);
        }
    }

    // Read from cached slices for a chunk. This waits for slice readiness and copies
    // overlapping ranges into the provided buffer.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(index, offset, len = buf.len()))]
    async fn read_from_slice(&self, index: u64, offset: u64, buf: &mut [u8]) -> anyhow::Result<()> {
        let slices = async {
            let guard = self.slices.lock().await;
            guard
                .iter()
                .filter(|s| s.lock().index == index)
                .cloned()
                .collect::<Vec<_>>()
        }
        .instrument(tracing::trace_span!(
            "read_from_slice.collect_slices",
            index,
            offset,
            len = buf.len()
        ))
        .await;

        for slice in slices {
            // There locks the slice twice, but it is still worth it
            // as the parking_lot::Mutex is lightweight.
            let (dst_start, dst_end) = {
                let guard = slice.lock();
                (
                    offset.max(guard.range.0),
                    guard.range.1.min(offset + buf.len() as u64),
                )
            };

            if dst_start >= dst_end {
                continue;
            }

            Self::wait_ready(&slice).await?;

            let mut guard = slice.lock();
            guard.last_access = Instant::now();

            let dst_local_start = dst_start - offset;
            let dst_local_end = dst_end - offset;
            let src_start = dst_start - guard.range.0;
            let src_end = dst_end - guard.range.0;

            tracing::trace_span!(
                "read_from_slice.copy_out",
                index,
                dst_start,
                dst_end,
                src_start,
                src_end,
                bytes = (dst_end - dst_start)
            )
            .in_scope(|| {
                buf[dst_local_start.as_usize()..dst_local_end.as_usize()]
                    .copy_from_slice(&guard.page[src_start.as_usize()..src_end.as_usize()]);
            });
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(index, start, end))]
    async fn prepare_slices(&self, index: u64, (start, end): (u64, u64)) -> SlicePinGuard {
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
                guard.last_access = Instant::now();
                pinned.add(slice.clone());
            }

            let (l, r) = guard.range;
            cutter.cut(l, r);
        }

        for range in cutter.collect() {
            let slice = Arc::new(ParkingMutex::new(SliceState::new(
                index,
                range,
                1,
                self.buffer_usage.clone(),
            )));

            SliceState::background_fetch(
                slice.clone(),
                self.inode.ino() as u64,
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
                }
            }
            *guard = new_slices;
        }

        // Invalidated slices must be re-fetched.
        for slice in to_fetch {
            SliceState::background_fetch(
                slice,
                self.inode.ino() as u64,
                self.config.layout,
                self.backend.clone(),
            );
        }
    }

    async fn invalidate_all(&self) {
        let mut guard = self.slices.lock().await;
        for slice in guard.drain(..) {
            let mut state = slice.lock();
            state.generation = state.generation.saturating_add(1);
            state.state = SliceStatus::Invalid;
            state.page = Vec::new();
            state.usage.update_bytes(0);
            state.notify.notify_waiters();
        }
    }

    /// Clean all invalid and unused slices.
    async fn cleanup_invalid(&self) {
        let mut guard = self.slices.lock().await;
        guard.retain(|slice| {
            let state = slice.lock();
            !(matches!(state.state, SliceStatus::Invalid) && state.refs == 0)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::ChunkLayout;
    use crate::chuck::store::InMemoryBlockStore;
    use crate::chuck::writer::DataUploader;
    use crate::meta::MetaLayer;
    use crate::meta::SLICE_ID_KEY;
    use crate::meta::factory::create_meta_store_from_url;
    use crate::meta::store::MetaStore;
    use crate::vfs::Inode;
    use crate::vfs::config::{ReadConfig, WriteConfig};
    use crate::vfs::io::writer::FileWriter;
    use bytes::Bytes;
    use std::sync::Arc;
    use std::sync::atomic::AtomicU64;
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
        let block_store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(block_store.clone(), meta.clone()));

        let ino: i64 = 11;
        let offset = layout.chunk_size - 512;
        let data = vec![9u8; 2048];
        let head = &data[..512];
        let tail = &data[512..];

        let slice_id1 = meta_store.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 0).unwrap(), backend.as_ref());
        let desc1 = uploader
            .write_at_vectored(
                slice_id1 as u64,
                offset,
                &[bytes::Bytes::copy_from_slice(head)],
            )
            .await
            .unwrap();
        meta_store
            .append_slice(chunk_id_for(ino, 0).unwrap(), desc1)
            .await
            .unwrap();

        let slice_id2 = meta_store.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 1).unwrap(), backend.as_ref());
        let desc2 = uploader
            .write_at_vectored(slice_id2 as u64, 0, &[Bytes::copy_from_slice(tail)])
            .await
            .unwrap();
        meta_store
            .append_slice(chunk_id_for(ino, 1).unwrap(), desc2)
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
        let block_store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(block_store.clone(), meta.clone()));

        let ino: i64 = 22;
        let data1 = vec![1u8; 2048];
        let data2 = vec![2u8; 2048];

        let slice_id1 = meta_store.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 0).unwrap(), backend.as_ref());
        let desc1 = uploader
            .write_at_vectored(slice_id1 as u64, 0, &[Bytes::copy_from_slice(&data1)])
            .await
            .unwrap();
        meta_store
            .append_slice(chunk_id_for(ino, 0).unwrap(), desc1)
            .await
            .unwrap();

        let inode = Inode::new(ino, data1.len() as u64);
        let reader = DataReader::new(Arc::new(ReadConfig::new(layout)), backend.clone());
        let file_reader = reader.open_for_handle(inode, 1);
        let out1 = file_reader.read(0, data1.len()).await.unwrap();
        assert_eq!(out1, data1);

        let slice_id2 = meta_store.next_id(SLICE_ID_KEY).await.unwrap();
        let desc2 = uploader
            .write_at_vectored(slice_id2 as u64, 0, &[Bytes::copy_from_slice(&data2)])
            .await
            .unwrap();
        meta_store
            .append_slice(chunk_id_for(ino, 0).unwrap(), desc2)
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
        let block_store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(block_store.clone(), meta.clone()));

        let ino = meta
            .create_file(1, "reader_write_eventual.txt".to_string())
            .await
            .unwrap();
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
            Arc::new(AtomicU64::new(0)),
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
