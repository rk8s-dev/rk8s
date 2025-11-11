use crate::chuck::{BlockStore, ChunkSpan, split_file_range_into_chunks};
use crate::meta::MetaStore;
use crate::vfs::chunk_id_for;
use crate::vfs::fs::ChunkIoFactory;
use crate::vfs::inode::Inode;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct FileRegistry<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub inode: DashMap<i64, Arc<Inode>>,
    pub writers: DashMap<i64, Arc<Mutex<FileWriter<B, M>>>>,
    pub readers: DashMap<i64, Arc<FileReader<B, M>>>,
}

impl<B, M> Default for FileRegistry<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<B, M> FileRegistry<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new() -> Self {
        Self {
            inode: DashMap::new(),
            writers: DashMap::new(),
            readers: DashMap::new(),
        }
    }

    pub fn ensure_init(&self, inode: Arc<Inode>, chunk_io: Arc<ChunkIoFactory<B, M>>) {
        let ino = inode.ino();

        // The entry of `DashMap` will lock the corresponding bucket, so this is thread-safe.
        let inode_entry = self.inode.entry(ino);
        let writer_entry = self.writers.entry(ino);
        let reader_entry = self.readers.entry(ino);

        let writer = Arc::new(Mutex::new(FileWriter::new(inode.clone(), chunk_io.clone())));
        let reader = Arc::new(FileReader::new(inode.clone(), chunk_io, writer.clone()));

        inode_entry.or_insert(inode);
        writer_entry.or_insert(writer);
        reader_entry.or_insert(reader);
    }

    pub fn writer(&self, ino: i64) -> Option<Arc<Mutex<FileWriter<B, M>>>> {
        self.writers
            .get(&ino)
            .map(|entry| Arc::clone(entry.value()))
    }

    pub fn reader(&self, ino: i64) -> Option<Arc<FileReader<B, M>>> {
        self.readers
            .get(&ino)
            .map(|entry| Arc::clone(entry.value()))
    }
}

pub struct FileWriter<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    inode: Arc<Inode>,
    chunk_io: Arc<ChunkIoFactory<B, M>>,
}

impl<B, M> FileWriter<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new(inode: Arc<Inode>, chunk_io: Arc<ChunkIoFactory<B, M>>) -> Self {
        FileWriter { inode, chunk_io }
    }

    pub async fn write(&self, offset: u64, buf: &[u8]) -> anyhow::Result<usize> {
        let spans: Vec<ChunkSpan> =
            split_file_range_into_chunks(self.chunk_io.layout(), offset, buf.len());

        let mut cursor = 0usize;
        for sp in spans {
            let cid = chunk_id_for(self.inode.ino(), sp.index);
            let writer = self.chunk_io.writer(cid);
            let take = sp.len;
            let buf = &buf[cursor..cursor + take];
            writer.write(sp.offset as u32, buf).await?;
            cursor += take;
        }
        Ok(buf.len())
    }
}

pub struct FileReader<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    inode: Arc<Inode>,
    chunk_io: Arc<ChunkIoFactory<B, M>>,
    writer: Arc<Mutex<FileWriter<B, M>>>,
}

impl<B, M> FileReader<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new(
        inode: Arc<Inode>,
        chunk_io: Arc<ChunkIoFactory<B, M>>,
        writer: Arc<Mutex<FileWriter<B, M>>>,
    ) -> Self {
        FileReader {
            inode,
            chunk_io,
            writer,
        }
    }

    pub async fn read(&self, offset: u64, len: usize) -> anyhow::Result<Vec<u8>> {
        if len == 0 {
            return Ok(Vec::new());
        }

        // Lock the corresponding writer to prevent the slices are modified.
        let writer_guard = self.writer.lock().await;

        let spans: Vec<ChunkSpan> =
            split_file_range_into_chunks(self.chunk_io.layout(), offset, len);
        let mut readers = Vec::new();
        for span in spans.iter() {
            let cid = chunk_id_for(self.inode.ino(), span.index);
            let mut reader = self.chunk_io.reader(cid);
            reader.prepare_slices().await?;
            readers.push(reader);
        }

        // Drop the guard because the writer will write data into new slices and will not overwrite.
        drop(writer_guard);

        let mut out = Vec::new();
        for (span, mut reader) in spans.into_iter().zip(readers.into_iter()) {
            let part = reader.read(span.offset as u32, span.len).await?;
            out.extend(part);
        }
        Ok(out)
    }
}
