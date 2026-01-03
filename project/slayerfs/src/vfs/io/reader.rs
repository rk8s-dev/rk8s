use super::FileWriter;
use crate::chuck::page::AccessKind;
use crate::chuck::{BlockStore, ChunkSpan, ChunkTag};
use crate::meta::MetaStore;
use crate::vfs::fs::ChunkIoFactory;
use crate::vfs::inode::Inode;
use std::convert::TryFrom;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct FileReader<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    inode: Arc<Inode>,
    chunk_io: Arc<ChunkIoFactory<B, M>>,
    writer: Arc<RwLock<FileWriter<B, M>>>,
}

impl<B, M> FileReader<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new(
        inode: Arc<Inode>,
        chunk_io: Arc<ChunkIoFactory<B, M>>,
        writer: Arc<RwLock<FileWriter<B, M>>>,
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

        // Check file size and adjust read length if necessary
        let file_size = self.inode.file_size();
        if offset >= file_size {
            // Reading beyond EOF returns empty data
            return Ok(Vec::new());
        }

        // Clamp the read length to not exceed file size
        let actual_len = std::cmp::min(len, (file_size - offset) as usize);
        if actual_len == 0 {
            return Ok(Vec::new());
        }

        // Lock the corresponding writer so a concurrent writer can't append a new slice while
        // we are sampling chunk metadata. Without this guard, the per-chunk readers could see
        // a stale slice set and end up reading the wrong data.
        let writer_guard = self.writer.read().await;

        let layout = self.chunk_io.layout();
        let chunk_span = ChunkSpan::new(
            layout.chunk_index_of(offset),
            u32::try_from(layout.within_chunk_offset(offset))
                .expect("chunk offset must fit within u32 for spans"),
            u32::try_from(actual_len).expect("read length must fit within u32 for spans"),
        );
        let spans: Vec<ChunkSpan> = chunk_span
            .split_into::<ChunkTag>(layout.chunk_size, layout.chunk_size, false)
            .collect();

        let mut out = Vec::with_capacity(actual_len);

        let cache_ctl = writer_guard.cache_ctl();
        let mut cache = cache_ctl.prepare(self.inode.clone());
        for span in spans {
            let mut guard = cache
                .probe(span.index, span.offset, span.len, AccessKind::Read)
                .await?;
            let mut buf = vec![0_u8; span.len as usize];
            guard.read_at(&mut buf, span.offset)?;
            out.extend(buf);
        }

        Ok(out)
    }
}
