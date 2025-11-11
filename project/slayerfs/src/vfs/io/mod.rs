use crate::chuck::BlockStore;
use crate::meta::MetaStore;
use crate::vfs::fs::ChunkIoFactory;
use crate::vfs::inode::Inode;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

mod reader;
mod writer;

pub use reader::FileReader;
pub use writer::FileWriter;

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

    // Protect by inode entry.
    pub fn ensure_init(&self, inode: Arc<Inode>, chunk_io: Arc<ChunkIoFactory<B, M>>) {
        let ino = inode.ino();

        // Only initialize writer/reader if not already present.
        let writer_arc = self
            .writers
            .entry(ino)
            .or_insert_with(|| {
                Arc::new(Mutex::new(FileWriter::new(inode.clone(), chunk_io.clone())))
            })
            .clone();
        self.readers
            .entry(ino)
            .or_insert_with(|| Arc::new(FileReader::new(inode, chunk_io, writer_arc.clone())));
    }

    pub fn inode(&self, ino: i64) -> Option<Arc<Inode>> {
        self.inode.get(&ino).map(|entry| Arc::clone(entry.value()))
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
