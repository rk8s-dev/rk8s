use crate::chuck::{BlockStore, ChunkSpan, split_file_range_into_chunks};
use crate::meta::MetaStore;
use crate::vfs::chunk_id_for;
use crate::vfs::fs::ChunkIoFactory;
use crate::vfs::inode::Inode;
use std::sync::Arc;

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
