// Write pipeline (high-level):
// - FileWriter::write_at splits a file write into chunk spans and appends data into SliceState
//   (Writeable). Slices are append-only and live inside each ChunkState.
// - When a slice is frozen (Readonly), it becomes eligible for upload. auto_flush and explicit
//   flush() can freeze slices. spawn_flush_slice performs the upload:
//     Readonly -> Uploading -> Uploaded/Failed
// - commit_chunk runs per-chunk and waits for Uploaded slices. It appends metadata (SliceDesc)
//   to the metadata layer and marks them Committed. Only Committed slices are visible to readers.
// - FileWriter::flush() freezes all slices and waits until commit threads drain the chunks.
//   While flushing, new writes are blocked via flush_waiting/write_waiting gates.

use super::reader::DataReader;
use crate::chuck::writer::DataUploader;
use crate::chuck::{BlockStore, SliceDesc};
use crate::meta::backoff::backoff;
use crate::meta::store::MetaError;
use crate::meta::{MetaLayer, SLICE_ID_KEY};
use crate::utils::{NumCastExt, UsageGuard};
use crate::vfs::Inode;
use crate::vfs::backend::Backend;
use crate::vfs::cache::page::CacheSlice;
use crate::vfs::cache::page::WriteAction as PageWriteAction;
use crate::vfs::chunk_id_for;
use crate::vfs::config::WriteConfig;
use crate::vfs::extract_ino_and_chunk_index;
use crate::vfs::io::split_chunk_spans;
use bytes::Bytes;
use dashmap::DashMap;
use parking_lot::Mutex as ParkingMutex;
use rand::RngCore;
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tokio::time::{interval, timeout};
use tracing::{Instrument, warn};

const FLUSH_DURATION: Duration = Duration::from_secs(5);
const COMMIT_WAIT_SLICE: Duration = Duration::from_millis(100);
const FLUSH_WAIT: Duration = Duration::from_secs(3);
const FLUSH_DEADLINE: Duration = Duration::from_secs(300);
const UPLOAD_MAX_RETRIES: u64 = 5;

const MAX_UNFLUSHED_SLICES: usize = 3;
const MAX_SLICES_THRESHOLD: usize = 800;
const WRITE_MAX_WAIT: Duration = Duration::from_secs(30);

struct UploadPlan {
    chunk_id: u64,
    data: Vec<(usize, Vec<Bytes>)>,
    slice_id: Option<u64>,
    uploaded: u64,
}

#[derive(Default, Copy, Clone, Debug)]
pub(crate) enum SliceStatus {
    /// Writable: slice is writable and there may be uploaded blocks.
    #[default]
    Writable,
    /// Readonly: frozen, no more writes allowed.
    Readonly,
    /// Uploaded: data uploaded successfully.
    Uploaded,
    /// Failed: data upload failed.
    Failed,
    /// Committed: metadata committed.
    Committed,
}

pub(crate) struct SliceState {
    state: SliceStatus,
    /// ID of the chunk it belongs to.
    chunk_id: u64,
    /// ID of this slice (assigned on flush).
    slice_id: Option<u64>,
    /// Offset relative to the chunk start.
    offset: u64,
    uploaded: u64,
    uploading: Option<(usize, usize)>,
    data: CacheSlice,
    usage: UsageGuard,
    /// Error occurred at background thread.
    err: Option<String>,
    notify: Arc<Notify>,
    started: Instant,
    last_mod: Instant,
}

impl SliceState {
    pub(crate) fn new(
        chunk_id: u64,
        offset: u64,
        config: Arc<WriteConfig>,
        usage: Arc<AtomicU64>,
    ) -> Self {
        let now = Instant::now();
        Self {
            state: SliceStatus::Writable,
            slice_id: None,
            chunk_id,
            offset,
            uploaded: 0,
            uploading: None,
            data: CacheSlice::new(config),
            usage: UsageGuard::new(usage),
            err: None,
            notify: Arc::new(Notify::new()),
            started: now,
            last_mod: now,
        }
    }

    pub(crate) fn can_write(&self, offset: u64, len: usize) -> Option<PageWriteAction> {
        if !matches!(self.state, SliceStatus::Writable) || offset < self.offset {
            return None;
        }

        let size = self.data.block_size();
        let pending_start = if let Some((_, end)) = self.uploading {
            end as u64 * size as u64
        } else {
            self.uploaded
        };

        let off_to_slice = offset - self.offset;

        // Uploaded blocks cannot be overlapped.
        if off_to_slice < pending_start.max(self.uploaded) {
            return None;
        }

        // For this function, the `offset` is relative to the chunk start,
        // whereas in `CacheSlice.append`, it is relative to the slice start.
        self.data.can_write(off_to_slice, len as u64)
    }

    #[tracing::instrument(level = "trace", skip(self, buf), fields(len = buf.len()))]
    pub(crate) fn write(
        &mut self,
        offset: u64,
        buf: &[u8],
        action: PageWriteAction,
    ) -> anyhow::Result<()> {
        self.data.write(offset - self.offset, buf, action)?;
        self.last_mod = Instant::now();
        Ok(())
    }

    pub fn has_idle_block(&self) -> bool {
        let size = self.data.block_size();
        let pending_end = self
            .uploading
            .map(|(_, end)| end as u64 * size as u64)
            .unwrap_or(self.uploaded)
            .max(self.uploaded);

        let remaining = self.data.len().saturating_sub(pending_end);

        if matches!(self.state, SliceStatus::Readonly | SliceStatus::Failed) {
            remaining > 0
        } else {
            remaining >= size as u64
        }
    }

    pub fn idx_need_upload(&self) -> (usize, usize) {
        let size = self.data.block_size() as u64;
        let start = (self.uploaded / size) as usize;
        let end = if matches!(self.state, SliceStatus::Readonly | SliceStatus::Failed) {
            if self.data.len() == 0 {
                0
            } else {
                self.data.len().div_ceil(size) as usize
            }
        } else {
            (self.data.len() / size) as usize
        };

        (start, end)
    }
}

pub(crate) struct ChunkState {
    /// ID of the chunk.
    chunk_id: u64,
    slices: VecDeque<Arc<ParkingMutex<SliceState>>>,
    commit_started: bool,
}

impl ChunkState {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            chunk_id: id,
            slices: VecDeque::new(),
            commit_started: false,
        }
    }
}

struct SliceHandle<'a, B, M>
where
    B: BlockStore,
    M: MetaLayer,
{
    slice: &'a Arc<ParkingMutex<SliceState>>,
    shared: &'a Shared<B, M>,
}

impl<'a, B, M> SliceHandle<'a, B, M>
where
    B: BlockStore,
    M: MetaLayer,
{
    fn with_mut<T>(&self, f: impl FnOnce(&mut SliceState) -> T) -> T {
        let mut guard = self.slice.lock();
        f(&mut guard)
    }

    fn with_ref<T>(&self, f: impl FnOnce(&SliceState) -> T) -> T {
        let guard = self.slice.lock();
        f(&guard)
    }

    fn can_write(&self, offset: u64, len: usize) -> Option<PageWriteAction> {
        self.with_ref(|s| s.can_write(offset, len))
    }

    fn try_write(&self, offset: u64, buf: &[u8]) -> anyhow::Result<bool> {
        let wrote = self.with_mut(|s| match s.can_write(offset, buf.len()) {
            Some(action) => {
                s.write(offset, buf, action)?;
                s.usage.update_bytes(s.data.alloc_bytes());
                Ok::<bool, anyhow::Error>(true)
            }
            None => Ok::<bool, anyhow::Error>(false),
        })?;

        Ok(wrote)
    }

    fn freeze(&self) -> bool {
        self.with_mut(|s| {
            if matches!(s.state, SliceStatus::Writable) {
                s.state = SliceStatus::Readonly;
                s.data.freeze();

                if s.uploading.is_none() && !s.has_idle_block() {
                    s.state = SliceStatus::Uploaded;
                    s.err = None;
                    s.notify.notify_waiters();
                }
                return true;
            }
            false
        })
    }

    fn advance_upload(&self, len: u64, need_release: Vec<usize>) {
        self.with_mut(|s| {
            s.uploading = None;
            s.uploaded += len;
            s.data.release_block(need_release);
            s.usage.update_bytes(s.data.alloc_bytes());

            if matches!(s.state, SliceStatus::Readonly | SliceStatus::Failed) && !s.has_idle_block()
            {
                s.state = SliceStatus::Uploaded;
                s.err = None;
            }
            s.notify.notify_waiters();
        })
    }

    fn should_freeze(&self) -> bool {
        self.with_ref(|s| {
            let end = s.offset + s.data.len();
            end >= self.shared.config.layout.chunk_size
        })
    }

    fn runtime_snapshot(&self) -> SliceRuntime {
        self.with_ref(|s| SliceRuntime {
            status: s.state,
            err: s.err.clone(),
            frozen: !matches!(s.state, SliceStatus::Writable),
            started: s.started,
            notify: s.notify.clone(),
        })
    }

    fn can_continue_upload(&self) -> bool {
        self.with_ref(|s| s.has_idle_block() && s.uploading.is_none())
    }

    // Mark data upload failure and wake commit waiters.
    fn mark_failed(&self, err: anyhow::Error) {
        self.with_mut(|s| {
            s.state = SliceStatus::Failed;
            s.uploading = None;

            if s.err.is_none() {
                s.err = Some(err.to_string());
            }

            s.notify.notify_waiters();
        })
    }

    fn prepare_upload(&self) -> anyhow::Result<Option<UploadPlan>> {
        self.with_mut(|s| {
            if s.uploading.is_some() || !s.has_idle_block() {
                return Ok(None);
            }

            let (start, end) = s.idx_need_upload();

            if end <= start {
                return Ok(None);
            }

            s.data.freeze_blocks(start, end);

            let data = s.data.collect_pages(start, end)?;
            s.uploading = Some((start, end));

            Ok(Some(UploadPlan {
                chunk_id: s.chunk_id,
                data,
                slice_id: s.slice_id,
                uploaded: s.uploaded,
            }))
        })
    }

    fn set_slice_id(&self, id: u64) {
        self.with_mut(|s| {
            if s.slice_id.is_none() {
                s.slice_id = Some(id);
            }
        })
    }

    fn desc_for_commit(&self) -> Option<SliceDesc> {
        self.with_ref(|s| {
            let length = s.data.len();
            let slice_id = match s.slice_id {
                Some(id) => id,
                None => return None,
            };
            if length == 0 {
                return None;
            }
            Some(SliceDesc {
                slice_id,
                chunk_id: s.chunk_id,
                offset: s.offset,
                length,
            })
        })
    }

    fn mark_committed(&self) {
        self.with_mut(|s| {
            s.state = SliceStatus::Committed;
        })
    }
}

/// A snapshot of a slice, allowing us to check slice status without lock.
struct SliceRuntime {
    status: SliceStatus,
    err: Option<String>,
    frozen: bool,
    started: Instant,
    notify: Arc<Notify>,
}

impl SliceRuntime {
    fn upload_done(&self) -> bool {
        matches!(
            self.status,
            SliceStatus::Uploaded | SliceStatus::Failed | SliceStatus::Committed
        )
    }

    fn can_commit(&self) -> bool {
        matches!(self.status, SliceStatus::Uploaded)
    }
}

struct WriteAction {
    start_commit: bool,
    flush: Vec<Arc<ParkingMutex<SliceState>>>,
}

struct ChunkHandle<'a, B, M>
where
    B: BlockStore,
    M: MetaLayer,
{
    chunk_id: u64,
    inner: &'a mut Inner,
    shared: &'a Shared<B, M>,
}

impl<'a, B, M> ChunkHandle<'a, B, M>
where
    B: BlockStore,
    M: MetaLayer,
{
    /// Find or create the next slice which can be written.
    /// A slice is append-only.
    fn find_slice_or_create(
        &mut self,
        offset: u64,
        len: usize,
    ) -> anyhow::Result<(Arc<ParkingMutex<SliceState>>, WriteAction)> {
        let (chunk_id, mut slices) = {
            let chunk = self
                .inner
                .chunks
                .get_mut(&self.chunk_id)
                .ok_or_else(|| anyhow::anyhow!("invalid chunk id"))?;
            let slices = std::mem::take(&mut chunk.slices);
            (chunk.chunk_id, slices)
        };

        anyhow::ensure!(
            offset + len as u64 <= self.shared.config.layout.chunk_size,
            "A write operation cannot exceed the chunk size"
        );

        let mut found: Option<Arc<ParkingMutex<SliceState>>> = None;
        let mut flush = Vec::new();
        for (idx, slice) in slices.iter().rev().enumerate() {
            let handle = SliceHandle {
                slice,
                shared: self.shared,
            };

            if handle.can_write(offset, len).is_some() {
                found = Some(slice.clone());
                break;
            }

            // Prevent slices from remaining unflushed for too long.
            if idx > MAX_UNFLUSHED_SLICES && handle.freeze() {
                flush.push(slice.clone());
            }
        }

        let slice = match found {
            Some(slice) => slice,
            None => {
                let slice = Arc::new(ParkingMutex::new(SliceState::new(
                    chunk_id,
                    offset,
                    self.shared.config.clone(),
                    self.shared.buffer_usage.clone(),
                )));
                slices.push_back(slice.clone());
                slice
            }
        };

        let chunk = self
            .inner
            .chunks
            .get_mut(&self.chunk_id)
            .ok_or_else(|| anyhow::anyhow!("invalid chunk id"))?;

        // This `slices` includes the newly created slice.
        chunk.slices = slices;
        let mut start_commit = false;

        // Enable the background commit thread if there is already a slice.
        if !chunk.commit_started && !chunk.slices.is_empty() {
            chunk.commit_started = true;
            start_commit = true;
        }

        Ok((
            slice,
            WriteAction {
                start_commit,
                flush,
            },
        ))
    }

    /// Append data to a writable slice. If the slice reaches chunk end, freeze + flush it.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(len = buf.len()))]
    fn write_at(&mut self, offset: u64, buf: &[u8]) -> anyhow::Result<WriteAction> {
        let mut start_commit = false;
        let mut flush = Vec::new();

        // There is a potential race condition in the time window between `find_slice_or_create` and `try_append`.
        // `find_slice_or_create` checks and returns a slice that can be appended, but after it selects the slice,
        // it releases the lock. `auto_flush` and `commit_chunk` can freeze a slice without holding the lock,
        // so when handle trying appending buf, the slice may have become readonly. This is highly unlikely to happen,
        // therefore, it is ok to retry until success.
        let mut failed_cnt = 0;

        loop {
            let (slice, action) = self.find_slice_or_create(offset, buf.len())?;
            start_commit |= action.start_commit;
            flush.extend(action.flush);

            let handle = SliceHandle {
                slice: &slice,
                shared: self.shared,
            };

            if handle.try_write(offset, buf)? {
                if handle.can_continue_upload() || handle.should_freeze() && handle.freeze() {
                    flush.push(slice);
                }

                return Ok(WriteAction {
                    start_commit,
                    flush,
                });
            }

            failed_cnt += 1;
            if failed_cnt >= 10 {
                warn!(
                    chunk_id = self.chunk_id,
                    offset,
                    len = buf.len(),
                    "write_at retried {failed_cnt} times due to concurrent slice freezing"
                );
            }
        }
    }
}

struct Shared<B, M> {
    inode: Arc<Inode>,
    config: Arc<WriteConfig>,
    buffer_usage: Arc<AtomicU64>,
    inner: Mutex<Inner>,
    /// Notify signal to wait write.
    write_notify: Notify,
    /// Notify signal to wait flush.
    flush_notify: Notify,
    backend: Arc<Backend<B, M>>,
    reader: Arc<DataReader<B, M>>,
}

impl<B, M> Shared<B, M>
where
    B: BlockStore,
    M: MetaLayer,
{
    pub(crate) fn new(
        inode: Arc<Inode>,
        config: Arc<WriteConfig>,
        backend: Arc<Backend<B, M>>,
        reader: Arc<DataReader<B, M>>,
        buffer_usage: Arc<AtomicU64>,
    ) -> Self {
        Self {
            inode,
            config,
            buffer_usage,
            inner: Mutex::new(Inner {
                flush_waiting: 0,
                write_waiting: 0,
                chunks: BTreeMap::default(),
            }),
            write_notify: Notify::new(),
            flush_notify: Notify::new(),
            backend,
            reader,
        }
    }
}

struct Inner {
    flush_waiting: u16,
    write_waiting: u16,
    chunks: BTreeMap<u64, ChunkState>,
}

impl Inner {
    fn chunk_handle<'a, B, M>(
        &'a mut self,
        shared: &'a Shared<B, M>,
        chunk_id: u64,
    ) -> ChunkHandle<'a, B, M>
    where
        B: BlockStore,
        M: MetaLayer,
    {
        ChunkHandle {
            chunk_id,
            inner: self,
            shared,
        }
    }

    fn get_or_create_chunk(&mut self, cid: u64) -> u64 {
        if self.chunks.contains_key(&cid) {
            return cid;
        }
        self.chunks.insert(cid, ChunkState::new(cid));
        cid
    }

    fn chunk_ids(&self) -> Vec<u64> {
        self.chunks.keys().copied().collect()
    }

    fn has_chunks(&self) -> bool {
        !self.chunks.is_empty()
    }
}

pub(crate) struct FileWriter<B, M> {
    shared: Arc<Shared<B, M>>,
}

impl<B, M> FileWriter<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    async fn back_pressure(&self) -> anyhow::Result<()> {
        let soft_limit = self.shared.config.buffer_size;
        if soft_limit == 0 {
            return Ok(());
        }

        let usage = self.shared.buffer_usage.load(Ordering::Relaxed);
        if usage <= soft_limit {
            return Ok(());
        }

        tokio::time::sleep(Duration::from_millis(10)).await;
        let hard_limit = soft_limit.saturating_mul(2);
        let mut total_wait = Duration::ZERO;

        while self.shared.buffer_usage.load(Ordering::Relaxed) > hard_limit {
            if total_wait >= WRITE_MAX_WAIT {
                return Err(anyhow::anyhow!(
                    "Timeout waiting for write buffer after {:?}. Current usage: {} bytes, limit: {} bytes",
                    total_wait,
                    self.shared.buffer_usage.load(Ordering::Relaxed),
                    hard_limit,
                ));
            }

            warn!("Reach write buffer hard limit: sleep for 100 millis");
            tokio::time::sleep(Duration::from_millis(100)).await;
            total_wait += Duration::from_millis(100);
        }
        Ok(())
    }

    pub(crate) fn new(
        inode: Arc<Inode>,
        config: Arc<WriteConfig>,
        backend: Arc<Backend<B, M>>,
        reader: Arc<DataReader<B, M>>,
        buffer_usage: Arc<AtomicU64>,
    ) -> Self {
        let shared = Arc::new(Shared::new(inode, config, backend, reader, buffer_usage));
        let flush_shared = Arc::downgrade(&shared);
        tokio::spawn(async move { Self::auto_flush(flush_shared).await });
        Self { shared }
    }

    // Write path: split into chunk spans, append to per-chunk slices, and possibly
    // trigger background flush/commit. Updates in-memory inode size at the end.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(offset, len = buf.len()))]
    pub(crate) async fn write_at(&self, offset: u64, buf: &[u8]) -> anyhow::Result<usize> {
        self.back_pressure().await?;
        let mut guard = self.shared.inner.lock().await;

        // Wait for any ongoing flush to finish. This serializes writes with flush().
        guard.write_waiting += 1;
        while guard.flush_waiting > 0 {
            drop(guard);
            self.shared.write_notify.notified().await;
            guard = self.shared.inner.lock().await;
        }
        guard.write_waiting -= 1;

        let mut position = 0;

        let spans = split_chunk_spans(self.shared.config.layout, offset, buf.len());
        for span in spans {
            let cid = chunk_id_for(self.shared.inode.ino(), span.index)?;
            let ckey = guard.get_or_create_chunk(cid);

            let mut handle = guard.chunk_handle(&self.shared, ckey);

            // This is the last missing piece of the attempt to implement real zero-copy.
            // There is a copy operation when appending the user-provided buf to the page cache.
            // However, the buf is a byte slice, meaning that it is impossible to get the data with ownership
            // unless "clone" it. So this copy seems to be inevitable.
            // Alternatively, the API signature could be modified or added to request "Bytes" from users. However,
            // this would break POSIX compatibility and is not supported by FUSE.
            let span_len = span.len.as_usize();
            let action = handle.write_at(span.offset, &buf[position..position + span_len])?;
            drop(guard);

            for slice in action.flush {
                Self::spawn_flush_slice(self.shared.clone(), slice);
            }

            if action.start_commit {
                let shared = self.shared.clone();
                tokio::spawn(async move { Self::commit_chunk(shared, ckey).await });
            }

            guard = self.shared.inner.lock().await;
            position += span_len;
        }

        drop(guard);
        let new_len = offset + buf.len() as u64;
        if new_len > self.shared.inode.file_size() {
            self.shared.inode.update_size(new_len);
        }
        Ok(buf.len())
    }

    // Flush: freeze all slices, upload them, and wait for commit threads to drain.
    // This blocks new writes until flushing completes (flush_waiting gate).
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) async fn flush(&self) -> anyhow::Result<()> {
        {
            let mut guard = self.shared.inner.lock().await;
            guard.flush_waiting += 1;
        }

        let start = Instant::now();
        let result = loop {
            let chunk_ids = {
                let guard = self.shared.inner.lock().await;
                guard.chunk_ids()
            };

            if chunk_ids.is_empty() {
                break Ok(());
            }

            let mut to_flush = Vec::new();
            {
                let guard = self.shared.inner.lock().await;
                let slices: Vec<Arc<ParkingMutex<SliceState>>> = guard
                    .chunks
                    .values()
                    .flat_map(|chunk| chunk.slices.iter().cloned())
                    .collect();

                drop(guard);

                for slice in slices {
                    let handle = SliceHandle {
                        slice: &slice,
                        shared: &self.shared,
                    };
                    if handle.freeze() {
                        to_flush.push(slice);
                    }
                }
            }

            for slice in to_flush {
                Self::spawn_flush_slice(self.shared.clone(), slice);
            }

            if timeout(FLUSH_WAIT, self.shared.flush_notify.notified())
                .await
                .is_err()
                && start.elapsed() > FLUSH_DEADLINE
            {
                break Err(anyhow::anyhow!("flush timeout after {:?}", FLUSH_DEADLINE));
            }
        };

        // Notify all write events.
        let mut guard = self.shared.inner.lock().await;
        if guard.flush_waiting > 0 {
            guard.flush_waiting -= 1;
        }
        if guard.flush_waiting == 0 && guard.write_waiting > 0 {
            self.shared.write_notify.notify_waiters();
        }

        result
    }

    pub(crate) async fn clear(&self) {
        let slices: Vec<Arc<ParkingMutex<SliceState>>> = {
            let guard = self.shared.inner.lock().await;
            guard
                .chunks
                .values()
                .flat_map(|chunk| chunk.slices.iter().cloned())
                .collect()
        };

        for slice in slices {
            let mut guard = slice.lock();
            guard.data.release_all();
            guard.usage.update_bytes(0);
        }

        let mut guard = self.shared.inner.lock().await;
        guard.chunks.clear();

        if guard.flush_waiting > 0 {
            self.shared.flush_notify.notify_waiters();
        }
    }

    pub(crate) async fn has_pending(&self) -> bool {
        let guard = self.shared.inner.lock().await;
        guard.has_chunks()
    }

    /// Spawn a background task to upload a frozen slice's data.
    /// Metadata commit is handled separately by commit_chunk.
    fn spawn_flush_slice(shared: Arc<Shared<B, M>>, slice: Arc<ParkingMutex<SliceState>>) {
        Self::spawn_upload_task(shared, slice);
    }

    fn spawn_upload_task(shared: Arc<Shared<B, M>>, slice: Arc<ParkingMutex<SliceState>>) {
        tokio::spawn(async move {
            loop {
                let handle = SliceHandle {
                    slice: &slice,
                    shared: &shared,
                };

                let plan = match handle.prepare_upload() {
                    Ok(Some(plan)) => plan,
                    Ok(None) => return,
                    Err(err) => {
                        warn!(error = ?err, "prepare_upload failed");
                        handle.mark_failed(err);
                        return;
                    }
                };

                let UploadPlan {
                    chunk_id,
                    data,
                    slice_id,
                    uploaded,
                } = plan;

                let mut all_chunks = Vec::new();
                let mut data_len = 0;
                let mut indices = Vec::new();

                for (index, chunks) in data {
                    indices.push(index);

                    for chunk in chunks {
                        data_len += chunk.len();
                        all_chunks.push(chunk);
                    }
                }

                let slice_id = match slice_id {
                    Some(slice_id) => slice_id,
                    None => match handle.shared.backend.meta().next_id(SLICE_ID_KEY).await {
                        Ok(id) => {
                            let id = id as u64;
                            handle.set_slice_id(id);
                            id
                        }
                        Err(e) => {
                            handle.mark_failed(anyhow::anyhow!("Failed to get slice id: {e}"));
                            return;
                        }
                    },
                };

                // The blocks to upload/write should be relative to the slice itself.
                // Otherwise, a previously uploaded block may be overwritten.
                let offset = uploaded;

                let uploader = DataUploader::new(shared.config.layout, &shared.backend);
                let result = backoff(UPLOAD_MAX_RETRIES, || async {
                    match uploader
                        .write_at_vectored(slice_id, offset.into(), &all_chunks)
                        .await
                    {
                        Ok(_) => Ok(()),
                        Err(err) => {
                            warn!(
                                chunk_id,
                                slice_id,
                                offset,
                                len = data_len,
                                error = ?err,
                                "upload failed, retrying"
                            );
                            Err(MetaError::ContinueRetry)
                        }
                    }
                })
                .await;

                match result {
                    Ok(()) => handle.advance_upload(data_len as u64, indices),
                    Err(err) => {
                        warn!(
                            chunk_id,
                            slice_id,
                            offset,
                            len = data_len,
                            error = ?err,
                            "upload failed after retries"
                        );
                        handle.mark_failed(anyhow::anyhow!(err));
                        return;
                    }
                }
            }
        });
    }

    /// The background thread for committing a chunk.
    /// It waits for Uploaded slices, appends metadata, and marks them Committed.
    /// Each chunk will have a unique committing thread.
    #[tracing::instrument(
        name = "FileWriter.commit_chunk",
        level = "trace",
        skip(shared),
        fields(chunk_id)
    )]
    async fn commit_chunk(shared: Arc<Shared<B, M>>, chunk_id: u64) {
        loop {
            let slice = {
                let guard = shared.inner.lock().await;
                let Some(chunk) = guard.chunks.get(&chunk_id) else {
                    return;
                };

                // Just flush one slice in each check.
                chunk.slices.front().cloned()
            };

            let Some(slice) = slice else {
                let mut guard = shared.inner.lock().await;
                guard.chunks.remove(&chunk_id);

                if !guard.has_chunks() && guard.flush_waiting > 0 {
                    shared.flush_notify.notify_waiters();
                }

                return;
            };

            // Get a snapshot of the current slice to check its status.
            // The `notification` in `Notify` is not queued (but the waiters are), this may result in a lost wake-up.
            // So it is needed to wait for an extra cycle to check timeout (COMMIT_WAIT_SLICE).
            let runtime = SliceHandle {
                slice: &slice,
                shared: &shared,
            }
            .runtime_snapshot();

            if matches!(runtime.status, SliceStatus::Failed) {
                Self::spawn_flush_slice(shared.clone(), slice.clone());
                tokio::time::sleep(COMMIT_WAIT_SLICE)
                    .instrument(tracing::trace_span!("commit_chunk.wait_retry"))
                    .await;
                continue;
            }

            if !runtime.upload_done() {
                if timeout(COMMIT_WAIT_SLICE, runtime.notify.notified())
                    .instrument(tracing::trace_span!("commit_chunk.wait_upload"))
                    .await
                    .is_ok()
                {
                    continue;
                }

                // If the slice is too old, it will be frozen and flushed.
                if !runtime.frozen && runtime.started.elapsed() > FLUSH_DURATION * 2 {
                    let _span = tracing::trace_span!("commit_chunk.freeze").entered();
                    let froze = SliceHandle {
                        slice: &slice,
                        shared: &shared,
                    }
                    .freeze();

                    if froze {
                        let _spawn_span =
                            tracing::trace_span!("commit_chunk.spawn_flush").entered();
                        Self::spawn_flush_slice(shared.clone(), slice.clone());
                    }
                }
                continue;
            }

            let mut should_pop = false;

            if runtime.can_commit() && runtime.err.is_none() {
                let desc = SliceHandle {
                    slice: &slice,
                    shared: &shared,
                }
                .desc_for_commit();

                if let Some(desc) = desc {
                    let (ino, chunk_index) = extract_ino_and_chunk_index(desc.chunk_id);
                    let file_offset = chunk_index * shared.config.layout.chunk_size + desc.offset;
                    let new_size = file_offset + desc.length;

                    let result = shared
                        .backend
                        .meta()
                        .write(ino, desc.chunk_id, desc, new_size)
                        .instrument(tracing::trace_span!(
                            "commit_chunk.meta_write",
                            ino,
                            chunk_id = desc.chunk_id,
                            slice_id = desc.slice_id,
                            offset = desc.offset,
                            len = desc.length,
                            new_size
                        ))
                        .await;

                    if let Err(err) = result {
                        warn!(
                            ino,
                            chunk_id = desc.chunk_id,
                            slice_id = desc.slice_id,
                            offset = desc.offset,
                            len = desc.length,
                            new_size,
                            error = ?err,
                            "commit_chunk meta write failed, retrying"
                        );
                    } else {
                        SliceHandle {
                            slice: &slice,
                            shared: &shared,
                        }
                        .mark_committed();

                        let _ = shared
                            .reader
                            .invalidate(ino as u64, file_offset, desc.length.as_usize())
                            .instrument(tracing::trace_span!(
                                "commit_chunk.invalidate",
                                ino,
                                offset = file_offset,
                                len = desc.length
                            ))
                            .await;
                        should_pop = true;
                    }
                } else {
                    should_pop = true;
                }
            } else if matches!(runtime.status, SliceStatus::Committed) {
                should_pop = true;
            }

            if !should_pop {
                tracing::trace!(
                    status = ?runtime.status,
                    err = ?runtime.err,
                    "commit_chunk retrying"
                );
                tokio::time::sleep(COMMIT_WAIT_SLICE)
                    .instrument(tracing::trace_span!("commit_chunk.wait_retry"))
                    .await;
                continue;
            }

            let mut guard = shared
                .inner
                .lock()
                .instrument(tracing::trace_span!("commit_chunk.pop_lock"))
                .await;
            if let Some(chunk) = guard.chunks.get_mut(&chunk_id) {
                let _ = chunk.slices.pop_front();
            }

            let empty = guard
                .chunks
                .get(&chunk_id)
                .map(|c| c.slices.is_empty())
                .unwrap_or(true);
            if empty {
                guard.chunks.remove(&chunk_id);
                if !guard.has_chunks() && guard.flush_waiting > 0 {
                    shared.flush_notify.notify_waiters();
                }
                return;
            }
        }
    }

    /// The automatic flush loop: periodically freezes older/idle slices to reduce memory
    /// usage and ensure progress. It does not commit metadata directly.
    /// Use `Weak` to stop it when the `FileWriter` was dropped.
    async fn auto_flush(shared: Weak<Shared<B, M>>) {
        let idle = Duration::from_secs(1);

        loop {
            let Some(shared) = shared.upgrade() else {
                return;
            };

            let mut to_flush = Vec::new();
            {
                let guard = shared.inner.lock().await;
                let now = Instant::now();
                let mut total_slices = 0usize;
                let mut chunk_slices = Vec::new();

                for chunk in guard.chunks.values() {
                    total_slices += chunk.slices.len();
                    chunk_slices.push(chunk.slices.iter().cloned().collect::<Vec<_>>());
                }
                drop(guard);

                // if there are too many slices, it should flush "a few more" to reduce memory usage.
                let too_many = total_slices > MAX_SLICES_THRESHOLD;

                // Randomly select a half of chunk to do extra flush to avoid jitter.
                let pick_bit = (rand::rng().next_u64() & 1) as usize;

                for (chunk_idx, slices) in chunk_slices.iter().enumerate() {
                    let half = slices.len() / 2;

                    for (idx, slice) in slices.iter().enumerate() {
                        let handle = SliceHandle {
                            slice,
                            shared: &shared,
                        };

                        let (age, idle_time, writeable) = handle.with_ref(|s| {
                            (
                                now.duration_since(s.started),
                                now.duration_since(s.last_mod),
                                matches!(s.state, SliceStatus::Writable),
                            )
                        });

                        if !writeable {
                            continue;
                        }

                        // age > FLUSH_DURATION means the slices are too old and (idle_time > idle && age > idle)
                        // represents it's been too long since last flushed.
                        let mut should = age > FLUSH_DURATION || (idle_time > idle && age > idle);
                        if !should && too_many {
                            // idx <= half represents older slices.
                            if chunk_idx % 2 == pick_bit && idx <= half {
                                should = true;
                            }
                        }

                        if should && handle.freeze() {
                            to_flush.push(slice.clone());
                        }
                    }
                }
            }

            for slice in to_flush {
                Self::spawn_flush_slice(shared.clone(), slice);
            }

            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

pub(crate) struct DataWriter<B, M> {
    config: Arc<WriteConfig>,
    backend: Arc<Backend<B, M>>,
    reader: Arc<DataReader<B, M>>,
    files: DashMap<u64, Arc<FileWriter<B, M>>>,
    buffer_usage: Arc<AtomicU64>,
}

impl<B, M> DataWriter<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaLayer + Send + Sync + 'static,
{
    pub(crate) fn new(
        config: Arc<WriteConfig>,
        backend: Arc<Backend<B, M>>,
        reader: Arc<DataReader<B, M>>,
    ) -> Self {
        Self {
            config,
            backend,
            reader,
            files: DashMap::new(),
            buffer_usage: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn ensure_file(&self, inode: Arc<Inode>) -> Arc<FileWriter<B, M>> {
        self.files
            .entry(inode.ino() as u64)
            .or_insert_with(|| {
                Arc::new(FileWriter::new(
                    inode.clone(),
                    self.config.clone(),
                    self.backend.clone(),
                    self.reader.clone(),
                    self.buffer_usage.clone(),
                ))
            })
            .clone()
    }

    pub(crate) fn start_flush_background(self: &Arc<Self>) {
        let flush_interval = self.config.flush_all_interval;
        let weak = Arc::downgrade(self);

        tokio::spawn(async move {
            let mut ticker = interval(flush_interval);
            loop {
                ticker.tick().await;
                let Some(writer) = weak.upgrade() else {
                    return;
                };
                writer.flush_once().await;
            }
        });
    }

    pub(crate) async fn flush_if_exists(&self, ino: u64) {
        let writer = self.files.get(&ino).map(|entry| entry.value().clone());
        if let Some(writer) = writer
            && writer.has_pending().await
        {
            let _ = writer.flush().await;
        }
    }

    pub(crate) async fn clear(&self, ino: u64) {
        let writer = self.files.get(&ino).map(|entry| entry.value().clone());
        if let Some(writer) = writer {
            writer.clear().await;
        }
    }

    pub(crate) fn release(&self, ino: u64) {
        if let Some((_, writer)) = self.files.remove(&ino) {
            tokio::spawn(async move {
                writer.clear().await;
            });
        }
    }

    #[cfg(test)]
    pub(crate) fn has_file(&self, ino: u64) -> bool {
        self.files.contains_key(&ino)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn flush_once(&self) {
        let writers: Vec<Arc<FileWriter<B, M>>> = self
            .files
            .iter()
            .map(|entry| entry.value().clone())
            .collect();

        for writer in writers {
            if writer.has_pending().await {
                let _ = writer.flush().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::ChunkLayout;
    use crate::chuck::reader::DataFetcher;
    use crate::chuck::store::{BlockKey, BlockStore, InMemoryBlockStore};
    use crate::meta::MetaLayer;
    use crate::meta::factory::create_meta_store_from_url;
    use crate::meta::store::MetaStore;
    use crate::vfs::Inode;
    use crate::vfs::config::ReadConfig;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use tokio::time::{sleep, timeout};

    fn test_config(layout: ChunkLayout) -> Arc<WriteConfig> {
        Arc::new(WriteConfig::new(layout).page_size(4 * 1024))
    }

    fn blocks_len(data: &[(usize, Vec<Bytes>)]) -> usize {
        data.iter()
            .map(|(_, pages)| pages.iter().map(|b| b.len()).sum::<usize>())
            .sum()
    }

    struct BlockingStore {
        inner: InMemoryBlockStore,
        blocked: AtomicBool,
        notify: Notify,
    }

    impl BlockingStore {
        fn new(blocked: bool) -> Self {
            Self {
                inner: InMemoryBlockStore::new(),
                blocked: AtomicBool::new(blocked),
                notify: Notify::new(),
            }
        }

        fn unblock(&self) {
            self.blocked.store(false, Ordering::Release);
            self.notify.notify_waiters();
        }
    }

    #[async_trait]
    impl BlockStore for BlockingStore {
        async fn write_range(
            &self,
            key: BlockKey,
            offset: u64,
            data: &[u8],
        ) -> anyhow::Result<u64> {
            while self.blocked.load(Ordering::Acquire) {
                self.notify.notified().await;
            }
            self.inner.write_range(key, offset, data).await
        }

        async fn read_range(
            &self,
            key: BlockKey,
            offset: u64,
            buf: &mut [u8],
        ) -> anyhow::Result<()> {
            self.inner.read_range(key, offset, buf).await
        }

        async fn delete_range(&self, key: BlockKey, len: u64) -> anyhow::Result<()> {
            self.inner.delete_range(key, len).await
        }
    }

    #[test]
    fn test_idx_need_upload_writable_only_full_blocks() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let mut slice = SliceState::new(1, 0, test_config(layout), Arc::new(AtomicU64::new(0)));
        let len = layout.block_size as usize + (layout.block_size as usize / 2);
        slice.data.append(&vec![1u8; len]).unwrap();

        let (start, end) = slice.idx_need_upload();
        assert_eq!((start, end), (0, 1));
    }

    #[test]
    fn test_idx_need_upload_readonly_includes_partial_block() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let mut slice = SliceState::new(1, 0, test_config(layout), Arc::new(AtomicU64::new(0)));
        let len = layout.block_size as usize + (layout.block_size as usize / 2);
        let data = vec![2u8; len];
        slice.data.append(&data).unwrap();
        slice.data.freeze();
        slice.state = SliceStatus::Readonly;

        let (start, end) = slice.idx_need_upload();
        assert_eq!((start, end), (0, 2));

        let blocks = slice.data.collect_pages(start, end).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks_len(&blocks), data.len());
    }

    #[test]
    fn test_uploaded_blocks_reject_overwrite() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let mut slice = SliceState::new(1, 0, test_config(layout), Arc::new(AtomicU64::new(0)));
        slice
            .data
            .append(&vec![0u8; layout.block_size as usize * 2])
            .unwrap();
        slice.uploaded = layout.block_size as u64;

        assert!(slice.can_write(0, 16).is_none());
        assert!(slice.can_write(layout.block_size as u64, 16).is_some());
    }

    #[tokio::test]
    async fn test_file_writer_flush_commits_and_reads() {
        let layout = ChunkLayout::default();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let ino = meta
            .create_file(1, "flush_reads.txt".to_string())
            .await
            .unwrap();
        let inode = Inode::new(ino, 0);
        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let writer = FileWriter::new(
            inode.clone(),
            test_config(layout),
            backend.clone(),
            reader,
            Arc::new(AtomicU64::new(0)),
        );

        let len = (layout.block_size / 2) as usize;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        writer.write_at(0, &data).await.unwrap();
        writer.flush().await.unwrap();

        assert!(inode.file_size() >= len as u64);

        let cid = chunk_id_for(inode.ino(), 0).unwrap();
        let slices = meta_store.get_slices(cid).await.unwrap();
        assert_eq!(slices.len(), 1);

        let mut reader = DataFetcher::new(layout, cid, backend.as_ref());
        reader.prepare_slices().await.unwrap();
        let out = reader.read_at(0u64.into(), len).await.unwrap();
        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_file_writer_appends_slices_for_overwrite() {
        let layout = ChunkLayout::default();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let ino = meta
            .create_file(1, "overwrite.txt".to_string())
            .await
            .unwrap();
        let inode = Inode::new(ino, 0);
        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let writer = FileWriter::new(
            inode.clone(),
            test_config(layout),
            backend.clone(),
            reader,
            Arc::new(AtomicU64::new(0)),
        );

        let len = (layout.block_size / 4) as usize;
        let first = vec![1u8; len];
        writer.write_at(0, &first).await.unwrap();

        let second = vec![2u8; len];
        writer.write_at(0, &second).await.unwrap();

        writer.flush().await.unwrap();

        let cid = chunk_id_for(inode.ino(), 0).unwrap();
        let slices = meta_store.get_slices(cid).await.unwrap();
        assert_eq!(slices.len(), 1);

        let mut reader = DataFetcher::new(layout, cid, backend.as_ref());
        reader.prepare_slices().await.unwrap();
        let out = reader.read_at(0u64.into(), len).await.unwrap();
        assert_eq!(out, second);
    }

    #[tokio::test]
    async fn test_file_writer_cross_chunks() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let _meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let ino = meta
            .create_file(1, "cross_chunks.txt".to_string())
            .await
            .unwrap();
        let inode = Inode::new(ino, 0);

        let reader_cfg = Arc::new(ReadConfig::new(layout));
        let reader = Arc::new(DataReader::new(reader_cfg, backend.clone()));
        let writer = FileWriter::new(
            inode.clone(),
            test_config(layout),
            backend.clone(),
            reader.clone(),
            Arc::new(AtomicU64::new(0)),
        );

        let len = layout.chunk_size as usize + 1024;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        writer.write_at(0, &data).await.unwrap();
        writer.flush().await.unwrap();

        let file_reader = reader.open_for_handle(inode, 1);
        let out = file_reader.read(0, len).await.unwrap();
        assert_eq!(out, data);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_flush_blocks_write_until_upload_done() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = Arc::new(BlockingStore::new(true));
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let _meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let ino = meta
            .create_file(1, "flush_blocking.txt".to_string())
            .await
            .unwrap();
        let inode = Inode::new(ino, 0);

        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let writer = Arc::new(FileWriter::new(
            inode,
            test_config(layout),
            backend.clone(),
            reader,
            Arc::new(AtomicU64::new(0)),
        ));

        let data = vec![3u8; 2048];
        writer.write_at(0, &data).await.unwrap();

        let flush_task = {
            let w = writer.clone();
            tokio::spawn(async move { w.flush().await })
        };
        sleep(Duration::from_millis(20)).await;
        assert!(!flush_task.is_finished());

        let write_task = {
            let w = writer.clone();
            let buf = vec![4u8; 512];
            tokio::spawn(async move { w.write_at(0, &buf).await })
        };
        sleep(Duration::from_millis(20)).await;
        assert!(!write_task.is_finished());

        store.unblock();

        timeout(Duration::from_secs(1), flush_task)
            .await
            .expect("flush should finish")
            .unwrap()
            .unwrap();
        timeout(Duration::from_secs(1), write_task)
            .await
            .expect("write should finish")
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn test_background_flush_all_commits() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let block_store = Arc::new(InMemoryBlockStore::new());
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let meta = meta_handle.layer();
        let backend = Arc::new(Backend::new(block_store.clone(), meta.clone()));

        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let write_cfg = Arc::new(
            WriteConfig::new(layout)
                .page_size(4 * 1024)
                .flush_all_interval(Duration::from_millis(50)),
        );
        let writer_pool = Arc::new(DataWriter::new(write_cfg, backend.clone(), reader));
        writer_pool.start_flush_background();

        let ino = meta
            .create_file(1, "background_flush.txt".to_string())
            .await
            .unwrap();
        let inode = Inode::new(ino, 0);
        let writer = writer_pool.ensure_file(inode.clone());
        let data = vec![7u8; 1024];
        writer.write_at(0, &data).await.unwrap();

        let cid = chunk_id_for(inode.ino(), 0).unwrap();
        timeout(Duration::from_secs(1), async {
            loop {
                if !meta_store.get_slices(cid).await.unwrap().is_empty() {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("flush-all should commit");
    }
}
