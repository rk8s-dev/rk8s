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
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::{Mutex, Notify};

pub struct DataReader<B, M> {
    config: Arc<ReadConfig>,
    // todo: use per-handle level reader to improve concurrent degree
    files: DashMap<u64, Arc<FileReader<B, M>>>,
    backend: Arc<Backend<B, M>>,
}

impl<B, M> DataReader<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    pub fn new(config: Arc<ReadConfig>, backend: Arc<Backend<B, M>>) -> Self {
        Self {
            config,
            files: DashMap::new(),
            backend,
        }
    }

    pub fn ensure_file(&self, ino: Arc<Inode>) -> Arc<FileReader<B, M>> {
        self.files
            .entry(ino.ino() as u64)
            .or_insert_with(|| {
                Arc::new(FileReader::new(
                    self.config.clone(),
                    ino,
                    self.backend.clone(),
                ))
            })
            .clone()
    }

    pub async fn invalidate(&self, ino: u64, offset: u64, len: usize) -> anyhow::Result<()> {
        let reader = self
            .files
            .get(&ino)
            .ok_or_else(|| anyhow::anyhow!("File (ino: {ino}) reader not found"))?
            .clone();

        reader.invalidate(offset, len).await;
        Ok(())
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
    fn new(index: u64, range: (u32, u32)) -> Self {
        Self {
            index,
            range,
            page: Vec::new(),
            state: SliceStatus::New,
            err: None,
            notify: Arc::new(Notify::new()),
            generation: 0,
            refs: 0,
        }
    }

    fn overlaps(&self, offset: u32, len: u32) -> bool {
        let end = offset.saturating_add(len);
        self.range.0 < end && offset < self.range.1
    }

    fn background_fetch<B, M>(
        this: Arc<StdMutex<SliceState>>,
        ino: u64,
        layout: ChunkLayout,
        backend: Arc<Backend<B, M>>,
    ) where
        B: BlockStore + Send + Sync + 'static,
        M: MetaStore + Send + Sync + 'static,
    {
        tokio::spawn(async move {
            let (index, (start, end), generation) = {
                let mut guard = this.lock().unwrap();
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
            let mut guard = this.lock().unwrap();

            // Stale fetch and needs to drop.
            if guard.generation != generation {
                return;
            }

            match result {
                Ok(out) => {
                    guard.state = SliceStatus::Ready;
                    guard.page = out;
                    guard.err = None;
                }
                Err(e) => {
                    guard.state = SliceStatus::Invalid;
                    guard.err = Some(e.to_string());
                }
            }
            guard.notify.notify_waiters();
        });
    }
}

/// Slice Reference Guard
struct SliceRef {
    slice: Arc<StdMutex<SliceState>>,
}

impl SliceRef {
    fn new(slice: Arc<StdMutex<SliceState>>) -> Self {
        let mut guard = slice.lock().unwrap();
        guard.refs = guard.refs.saturating_add(1);
        drop(guard);
        Self { slice }
    }
}

impl Drop for SliceRef {
    fn drop(&mut self) {
        let mut guard = self.slice.lock().unwrap();
        guard.refs = guard.refs.saturating_sub(1);
    }
}

pub struct FileReader<B, M> {
    config: Arc<ReadConfig>,
    inode: Arc<Inode>,
    slices: Mutex<VecDeque<Arc<StdMutex<SliceState>>>>,
    backend: Arc<Backend<B, M>>,
}

impl<B, M> FileReader<B, M>
where
    B: BlockStore + Send + Sync + 'static,
    M: MetaStore + Send + Sync + 'static,
{
    pub fn new(config: Arc<ReadConfig>, inode: Arc<Inode>, backend: Arc<Backend<B, M>>) -> Self {
        Self {
            config,
            inode,
            slices: Mutex::new(VecDeque::new()),
            backend,
        }
    }

    pub async fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let mut buf = vec![0u8; len];
        let read = self.read_at(offset, &mut buf).await?;
        buf.truncate(read);
        Ok(buf)
    }

    pub async fn read_at(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<usize> {
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

        // Pre-fill with zeros so holes or missing slices read as zeros.
        buf[..actual_len].fill(0);

        let spans = split_chunk_spans(self.config.layout, offset, actual_len);

        for span in spans.iter().copied() {
            // `prepare_slices` don't wait for all data is ready.
            self.prepare_slices(span.index, (span.offset, span.offset + span.len))
                .await;
        }

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

        // Do a cleanup each read.
        self.cleanup_invalid().await;
        result
    }

    async fn wait_ready(slice: &Arc<StdMutex<SliceState>>) -> anyhow::Result<()> {
        loop {
            let notify = {
                let guard = slice.lock().unwrap();

                match guard.state {
                    SliceStatus::Ready => return Ok(()),
                    SliceStatus::Invalid => {
                        let err = guard.err.as_deref().unwrap_or("slice invalid").to_string();
                        return Err(anyhow::anyhow!(err));
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
                .filter(|s| s.lock().unwrap().index == index)
                .cloned()
                .collect::<Vec<_>>()
        };

        for slice in slices {
            let _slice_ref = SliceRef::new(slice.clone());
            Self::wait_ready(&slice).await?;

            let guard = slice.lock().unwrap();

            let dst_start = offset.max(guard.range.0);
            let dst_end = guard.range.1.min(offset + buf.len() as u32);

            if dst_start < dst_end {
                let dst_local_start = dst_start - offset;
                let dst_local_end = dst_end - offset;
                let src_start = dst_start - guard.range.0;
                let src_end = dst_end - guard.range.0;

                buf[dst_local_start as usize..dst_local_end as usize]
                    .copy_from_slice(&guard.page[src_start as usize..src_end as usize]);
            }
        }
        Ok(())
    }

    async fn prepare_slices(&self, index: u64, (start, end): (u32, u32)) {
        let mut cutter = Intervals::new(start, end);

        let mut guard = self.slices.lock().await;
        for slice in guard.iter() {
            let guard = slice.lock().unwrap();

            if guard.index != index {
                continue;
            }

            if matches!(guard.state, SliceStatus::Invalid) {
                continue;
            }

            let (l, r) = guard.range;
            cutter.cut(l, r);
        }

        for range in cutter.collect() {
            let slice = Arc::new(StdMutex::new(SliceState::new(index, range)));
            SliceState::background_fetch(
                slice.clone(),
                self.inode.ino() as u64,
                self.config.layout,
                self.backend.clone(),
            );
            guard.push_back(slice);
        }
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
                let mut state = slice.lock().unwrap();
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

    /// Clean all invalid and unused slices.
    async fn cleanup_invalid(&self) {
        let mut guard = self.slices.lock().await;
        guard.retain(|slice| {
            let state = slice.lock().unwrap();
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
            .write_at(slice_id1 as u64, offset as u32, head)
            .await
            .unwrap();
        meta.append_slice(chunk_id_for(ino, 0), desc1)
            .await
            .unwrap();

        let slice_id2 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let uploader = DataUploader::new(layout, chunk_id_for(ino, 1), backend.as_ref());
        let desc2 = uploader.write_at(slice_id2 as u64, 0, tail).await.unwrap();
        meta.append_slice(chunk_id_for(ino, 1), desc2)
            .await
            .unwrap();

        let inode = Inode::new(ino, offset + data.len() as u64);
        let reader = DataReader::new(Arc::new(ReadConfig::new(layout)), backend.clone());
        let file_reader = reader.ensure_file(inode);
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
            .write_at(slice_id1 as u64, 0, &data1)
            .await
            .unwrap();
        meta.append_slice(chunk_id_for(ino, 0), desc1)
            .await
            .unwrap();

        let inode = Inode::new(ino, data1.len() as u64);
        let reader = DataReader::new(Arc::new(ReadConfig::new(layout)), backend.clone());
        let file_reader = reader.ensure_file(inode);
        let out1 = file_reader.read(0, data1.len()).await.unwrap();
        assert_eq!(out1, data1);

        let slice_id2 = meta.next_id(SLICE_ID_KEY).await.unwrap();
        let desc2 = uploader
            .write_at(slice_id2 as u64, 0, &data2)
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
        let file_reader = reader.ensure_file(inode.clone());

        let writer = Arc::new(FileWriter::new(
            inode,
            Arc::new(WriteConfig::new(layout, 4 * 1024)),
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
