// Write pipeline (high-level):
// - FileWriter::write_at splits a file write into chunk spans and appends data into SliceState
//   (Writeable). Slices are append-only and live inside each ChunkState.
// - When a slice is frozen (Readonly), it becomes eligible for upload. auto_flush and explicit
//   flush() can freeze slices. spawn_flush_slice performs the upload:
//     Readonly -> Uploading -> Uploaded/Failed
// - commit_chunk runs per-chunk and waits for Uploaded slices. It appends metadata (SliceDesc)
//   to MetaStore and marks them Committed. Only Committed slices are visible to readers.
// - FileWriter::flush() freezes all slices and waits until commit threads drain the chunks.
//   While flushing, new writes are blocked via flush_waiting/write_waiting gates.

use super::reader::DataReader;
use crate::chuck::writer::DataUploader;
use crate::chuck::{BlockStore, SliceDesc};
use crate::meta::backoff::backoff;
use crate::meta::store::MetaError;
use crate::meta::{MetaStore, SLICE_ID_KEY};
use crate::vfs::backend::Backend;
use crate::vfs::cache::page::CacheSlice;
use crate::vfs::chunk_id_for;
use crate::vfs::config::WriteConfig;
use crate::vfs::extract_ino_and_chunk_index;
use crate::vfs::inode::Inode;
use crate::vfs::io::split_chunk_spans;
use bytes::Bytes;
use dashmap::DashMap;
use parking_lot::Mutex as ParkingMutex;
use rand::RngCore;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use tokio::time::{interval, timeout};
use tracing::warn;

const FLUSH_DURATION: Duration = Duration::from_secs(5);
const COMMIT_WAIT_SLICE: Duration = Duration::from_millis(100);
const FLUSH_WAIT: Duration = Duration::from_secs(3);
const FLUSH_DEADLINE: Duration = Duration::from_secs(300);
const UPLOAD_MAX_RETRIES: u64 = 5;

const MAX_UNFLUSHED_SLICES: usize = 3;
const MAX_SLICES_THRESHOLD: usize = 800;

#[derive(Default, Copy, Clone)]
pub(crate) enum SliceStatus {
    /// Writeable: append-only.
    #[default]
    Writeable,
    /// Readonly: frozen, no more writes allowed.
    Readonly,
    /// Uploading: data is being uploaded to object storage.
    Uploading,
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
    offset: u32,
    data: CacheSlice,
    /// Error occurred at background thread.
    err: Option<String>,
    notify: Arc<Notify>,
    started: Instant,
    last_mod: Instant,
}

impl SliceState {
    pub(crate) fn new(chunk_id: u64, offset: u32, config: Arc<WriteConfig>) -> Self {
        let now = Instant::now();
        Self {
            state: SliceStatus::Writeable,
            chunk_id,
            slice_id: None,
            offset,
            data: CacheSlice::new(config),
            err: None,
            notify: Arc::new(Notify::new()),
            started: now,
            last_mod: now,
        }
    }

    pub(crate) fn append(&mut self, buf: &[u8]) -> anyhow::Result<()> {
        self.data.append(buf)?;
        self.last_mod = Instant::now();
        Ok(())
    }

    pub(crate) fn can_append(&self, offset: u32) -> bool {
        if !matches!(self.state, SliceStatus::Writeable) || offset < self.offset {
            return false;
        }

        // For this function, the `offset` is relative to the chunk start,
        // whereas in `CacheSlice.append`, it is relative to the slice start.
        self.data.can_append(offset - self.offset)
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
    M: MetaStore,
{
    slice: &'a Arc<ParkingMutex<SliceState>>,
    shared: &'a Shared<B, M>,
}

impl<'a, B, M> SliceHandle<'a, B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    fn with_mut<T>(&self, f: impl FnOnce(&mut SliceState) -> T) -> T {
        let mut guard = self.slice.lock();
        f(&mut guard)
    }

    fn with_ref<T>(&self, f: impl FnOnce(&SliceState) -> T) -> T {
        let guard = self.slice.lock();
        f(&guard)
    }

    fn can_append(&self, offset: u32) -> bool {
        self.with_ref(|s| s.can_append(offset))
    }

    /// Check whether the slice is appendable and append buf atomically.
    /// The offset is relative to chunk start.
    fn try_append(&self, offset: u32, buf: &[u8]) -> anyhow::Result<bool> {
        self.with_mut(|s| {
            if !s.can_append(offset) {
                return Ok(false);
            }

            s.append(buf)?;
            Ok(true)
        })
    }

    fn freeze(&self) -> bool {
        self.with_mut(|s| {
            if matches!(s.state, SliceStatus::Writeable) {
                s.state = SliceStatus::Readonly;
                s.data.freeze();
                return true;
            }
            false
        })
    }

    fn should_freeze(&self) -> bool {
        self.with_ref(|s| {
            let end = s.offset as u64 + s.data.len() as u64;
            end >= self.shared.config.layout.chunk_size
        })
    }

    fn runtime_snapshot(&self) -> SliceRuntime {
        self.with_ref(|s| SliceRuntime {
            status: s.state,
            err: s.err.clone(),
            frozen: !matches!(s.state, SliceStatus::Writeable),
            started: s.started,
            notify: s.notify.clone(),
        })
    }

    // Transition Readonly/Failed -> Uploading. Returns false if already uploading/finished.
    fn start_uploading(&self) -> bool {
        self.with_mut(|s| {
            if matches!(s.state, SliceStatus::Readonly | SliceStatus::Failed) {
                s.state = SliceStatus::Uploading;
                s.err = None;
                return true;
            }
            false
        })
    }

    // Mark data upload success. Commit thread can now append metadata.
    fn mark_uploaded(&self) {
        self.with_mut(|s| {
            if matches!(s.state, SliceStatus::Uploading | SliceStatus::Readonly) {
                s.state = SliceStatus::Uploaded;
                s.err = None;
            }
            s.notify.notify_waiters();
        })
    }

    // Mark data upload failure and wake commit waiters.
    fn mark_failed(&self, err: anyhow::Error) {
        self.with_mut(|s| {
            s.state = SliceStatus::Failed;
            if s.err.is_none() {
                s.err = Some(err.to_string());
            }
            s.notify.notify_waiters();
        })
    }

    #[allow(clippy::type_complexity)]
    fn snapshot_for_flush(&self) -> anyhow::Result<Option<(u64, u32, Vec<Bytes>, Option<u64>)>> {
        self.with_ref(|s| {
            if s.data.len() == 0 {
                return Ok(None);
            }
            let data = s.data.collect_pages()?;
            Ok(Some((s.chunk_id, s.offset, data, s.slice_id)))
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
    M: MetaStore,
{
    chunk_id: u64,
    inner: &'a mut Inner,
    shared: &'a Shared<B, M>,
}

impl<'a, B, M> ChunkHandle<'a, B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    /// Find or create the next slice which can be written.
    /// A slice is append-only.
    fn find_slice_or_create(
        &mut self,
        offset: u32,
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
            offset as u64 + len as u64 <= self.shared.config.layout.chunk_size,
            "A write operation cannot exceed the chunk size"
        );

        let mut found: Option<Arc<ParkingMutex<SliceState>>> = None;
        let mut flush = Vec::new();
        for (idx, slice) in slices.iter().rev().enumerate() {
            let handle = SliceHandle {
                slice,
                shared: self.shared,
            };

            if handle.can_append(offset) {
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

    // Append data to a writable slice. If the slice reaches chunk end, freeze + flush it.
    fn write_at(&mut self, offset: u32, buf: &[u8]) -> anyhow::Result<WriteAction> {
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

            if handle.try_append(offset, buf)? {
                if handle.should_freeze() && handle.freeze() {
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
    M: MetaStore,
{
    pub(crate) fn new(
        inode: Arc<Inode>,
        config: Arc<WriteConfig>,
        backend: Arc<Backend<B, M>>,
        reader: Arc<DataReader<B, M>>,
    ) -> Self {
        Self {
            inode,
            config,
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
        M: MetaStore,
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
    M: MetaStore + Send + Sync + 'static,
{
    pub(crate) fn new(
        inode: Arc<Inode>,
        config: Arc<WriteConfig>,
        backend: Arc<Backend<B, M>>,
        reader: Arc<DataReader<B, M>>,
    ) -> Self {
        let shared = Arc::new(Shared::new(inode, config, backend, reader));
        let flush_shared = Arc::downgrade(&shared);
        tokio::spawn(async move { Self::auto_flush(flush_shared).await });
        Self { shared }
    }

    // Write path: split into chunk spans, append to per-chunk slices, and possibly
    // trigger background flush/commit. Updates in-memory inode size at the end.
    #[tracing::instrument(level = "trace", skip(self, buf), fields(offset, len = buf.len()))]
    pub(crate) async fn write_at(&self, offset: u64, buf: &[u8]) -> anyhow::Result<usize> {
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
            let cid = chunk_id_for(self.shared.inode.ino(), span.index);
            let ckey = guard.get_or_create_chunk(cid);

            let mut handle = guard.chunk_handle(&self.shared, ckey);

            // This is the last missing piece of the attempt to implement real zero-copy.
            // There is a copy operation when appending the user-provided buf to the page cache.
            // However, the buf is a byte slice, meaning that it is impossible to get the data with ownership
            // unless "clone" it. So this copy seems to be inevitable.
            // Alternatively, the API signature could be modified or added to request "Bytes" from users. However,
            // this would break POSIX compatibility and is not supported by FUSE.
            let action =
                handle.write_at(span.offset, &buf[position..position + span.len as usize])?;
            drop(guard);

            for slice in action.flush {
                Self::spawn_flush_slice(self.shared.clone(), slice);
            }

            if action.start_commit {
                let shared = self.shared.clone();
                tokio::spawn(async move { Self::commit_chunk(shared, ckey).await });
            }
            guard = self.shared.inner.lock().await;
            position += span.len as usize;
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
        let handle = SliceHandle {
            slice: &slice,
            shared: &shared,
        };
        if !handle.start_uploading() {
            return;
        }
        Self::spawn_upload_task(shared, slice);
    }

    fn spawn_upload_task(shared: Arc<Shared<B, M>>, slice: Arc<ParkingMutex<SliceState>>) {
        tokio::spawn(async move {
            let handle = SliceHandle {
                slice: &slice,
                shared: &shared,
            };
            let snapshot = match handle.snapshot_for_flush() {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => {
                    handle.mark_uploaded();
                    return;
                }
                Err(err) => {
                    handle.mark_failed(err);
                    return;
                }
            };

            let (chunk_id, offset, data, slice_id) = snapshot;
            let data_len: usize = data.iter().map(|b| b.len()).sum();

            let sid = match slice_id {
                Some(id) => id,
                None => {
                    let id = match shared.backend.meta().next_id(SLICE_ID_KEY).await {
                        Ok(id) => id as u64,
                        Err(err) => {
                            handle.mark_failed(anyhow::anyhow!(err));
                            return;
                        }
                    };
                    handle.set_slice_id(id);
                    id
                }
            };

            let uploader = DataUploader::new(shared.config.layout, chunk_id, &shared.backend);
            let result = backoff(UPLOAD_MAX_RETRIES, || async {
                match uploader.write_at_vectored(sid, offset, &data).await {
                    Ok(_) => Ok(()),
                    Err(err) => {
                        warn!(
                            chunk_id,
                            slice_id = sid,
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
                Ok(()) => handle.mark_uploaded(),
                Err(err) => {
                    warn!(
                        chunk_id,
                        slice_id = sid,
                        offset,
                        len = data_len,
                        error = ?err,
                        "upload failed after retries"
                    );
                    handle.mark_failed(anyhow::anyhow!(err))
                }
            }
        });
    }

    /// The background thread for committing a chunk.
    /// It waits for Uploaded slices, appends metadata, and marks them Committed.
    /// Each chunk will have a unique committing thread.
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
                tokio::time::sleep(COMMIT_WAIT_SLICE).await;
                continue;
            }

            if !runtime.upload_done() {
                if timeout(COMMIT_WAIT_SLICE, runtime.notify.notified())
                    .await
                    .is_ok()
                {
                    continue;
                }

                // If the slice is too old, it will be frozen and flushed.
                if !runtime.frozen && runtime.started.elapsed() > FLUSH_DURATION * 2 {
                    let froze = SliceHandle {
                        slice: &slice,
                        shared: &shared,
                    }
                    .freeze();
                    if froze {
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
                    let ok = shared
                        .backend
                        .meta()
                        .append_slice(desc.chunk_id, desc)
                        .await
                        .is_ok();
                    if ok {
                        SliceHandle {
                            slice: &slice,
                            shared: &shared,
                        }
                        .mark_committed();

                        let (ino, chunk_index) = extract_ino_and_chunk_index(desc.chunk_id);
                        let file_offset =
                            chunk_index * shared.config.layout.chunk_size + desc.offset as u64;
                        let _ = shared
                            .reader
                            .invalidate(ino as u64, file_offset, desc.length as usize)
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
                tokio::time::sleep(COMMIT_WAIT_SLICE).await;
                continue;
            }

            let mut guard = shared.inner.lock().await;
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
                                matches!(s.state, SliceStatus::Writeable),
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
}

impl<B, M> DataWriter<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
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
        self.files.remove(&ino);
    }

    #[cfg(test)]
    pub(crate) fn has_file(&self, ino: u64) -> bool {
        self.files.contains_key(&ino)
    }

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
    use crate::meta::factory::create_meta_store_from_url;
    use crate::vfs::config::ReadConfig;
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::{sleep, timeout};

    fn test_config(layout: ChunkLayout) -> Arc<WriteConfig> {
        Arc::new(WriteConfig::new(layout).page_size(4 * 1024))
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
            offset: u32,
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
            offset: u32,
            buf: &mut [u8],
        ) -> anyhow::Result<()> {
            self.inner.read_range(key, offset, buf).await
        }

        async fn delete_range(&self, key: BlockKey, len: usize) -> anyhow::Result<()> {
            self.inner.delete_range(key, len).await
        }
    }

    #[tokio::test]
    async fn test_file_writer_flush_commits_and_reads() {
        let layout = ChunkLayout::default();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let inode = Inode::new(11, 0);
        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let writer = FileWriter::new(inode.clone(), test_config(layout), backend.clone(), reader);

        let len = (layout.block_size / 2) as usize;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }

        writer.write_at(0, &data).await.unwrap();
        writer.flush().await.unwrap();

        assert!(inode.file_size() >= len as u64);

        let cid = chunk_id_for(inode.ino(), 0);
        let slices = meta.get_slices(cid).await.unwrap();
        assert_eq!(slices.len(), 1);

        let mut reader = DataFetcher::new(layout, cid, backend.as_ref());
        reader.prepare_slices().await.unwrap();
        let out = reader.read_at(0, len).await.unwrap();
        assert_eq!(out, data);
    }

    #[tokio::test]
    async fn test_file_writer_appends_slices_for_overwrite() {
        let layout = ChunkLayout::default();
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let inode = Inode::new(22, 0);
        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let writer = FileWriter::new(inode.clone(), test_config(layout), backend.clone(), reader);

        let len = (layout.block_size / 4) as usize;
        let first = vec![1u8; len];
        writer.write_at(0, &first).await.unwrap();

        let second = vec![2u8; len];
        writer.write_at(0, &second).await.unwrap();

        writer.flush().await.unwrap();

        let cid = chunk_id_for(inode.ino(), 0);
        let slices = meta.get_slices(cid).await.unwrap();
        assert_eq!(slices.len(), 2);

        let mut reader = DataFetcher::new(layout, cid, backend.as_ref());
        reader.prepare_slices().await.unwrap();
        let out = reader.read_at(0, len).await.unwrap();
        assert_eq!(out, second);
    }

    #[tokio::test]
    async fn test_file_writer_cross_chunks() {
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
        let inode = Inode::new(33, 0);

        let reader_cfg = Arc::new(ReadConfig::new(layout));
        let reader = Arc::new(DataReader::new(reader_cfg, backend.clone()));
        let writer = FileWriter::new(
            inode.clone(),
            test_config(layout),
            backend.clone(),
            reader.clone(),
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
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));
        let inode = Inode::new(44, 0);

        let reader = Arc::new(DataReader::new(
            Arc::new(ReadConfig::new(layout)),
            backend.clone(),
        ));
        let writer = Arc::new(FileWriter::new(
            inode,
            test_config(layout),
            backend.clone(),
            reader,
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
        let store = Arc::new(InMemoryBlockStore::new());
        let meta = create_meta_store_from_url("sqlite::memory:")
            .await
            .unwrap()
            .store();
        let backend = Arc::new(Backend::new(store.clone(), meta.clone()));

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

        let inode = Inode::new(55, 0);
        let writer = writer_pool.ensure_file(inode.clone());
        let data = vec![7u8; 1024];
        writer.write_at(0, &data).await.unwrap();

        let cid = chunk_id_for(inode.ino(), 0);
        timeout(Duration::from_secs(1), async {
            loop {
                if !meta.get_slices(cid).await.unwrap().is_empty() {
                    break;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("flush-all should commit");
    }
}
