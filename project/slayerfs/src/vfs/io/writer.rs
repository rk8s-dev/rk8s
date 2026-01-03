use crate::chuck::page::AccessKind;
use crate::chuck::{BlockStore, ChunkSpan, ChunkTag};
use crate::meta::MetaStore;
use crate::vfs::inode::Inode;
use crate::vfs::io::cache::CacheCtl;
use std::convert::TryFrom;
use std::sync::Arc;

pub struct FileWriter<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    inode: Arc<Inode>,
    cache_ctl: Arc<CacheCtl<B, M>>,
}

impl<B, M> FileWriter<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new(inode: Arc<Inode>, cache_ctl: Arc<CacheCtl<B, M>>) -> Self {
        FileWriter { inode, cache_ctl }
    }

    pub fn cache_ctl(&self) -> &CacheCtl<B, M> {
        &self.cache_ctl
    }

    pub async fn write(&self, offset: u64, buf: &[u8]) -> anyhow::Result<usize> {
        let layout = self.cache_ctl.chunk_io.layout();
        let chunk_span = ChunkSpan::new(
            layout.chunk_index_of(offset),
            u32::try_from(layout.within_chunk_offset(offset))
                .expect("chunk offset must fit within u32 for spans"),
            u32::try_from(buf.len()).expect("write length must fit within u32 for spans"),
        );
        let spans: Vec<ChunkSpan> = chunk_span
            .split_into::<ChunkTag>(layout.chunk_size, layout.chunk_size, false)
            .collect();

        let mut cursor = 0;
        let mut cache = self.cache_ctl.prepare(self.inode.clone());
        for span in spans {
            let mut guard = cache
                .probe(span.index, span.offset, span.len, AccessKind::Write)
                .await?;
            guard.write_at(&buf[cursor..cursor + span.len as usize], span.offset)?;
            cursor += span.len as usize;
        }

        Ok(buf.len())
    }
}
