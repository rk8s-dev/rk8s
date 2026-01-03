use crate::chuck::BlockStore;
use crate::chuck::page::{AccessKind, Folio, PagedCache, PagedCacheConfig, PagedCacheGuard};
use crate::meta::MetaStore;
use crate::vfs::chunk_id_for;
use crate::vfs::fs::ChunkIoFactory;
use crate::vfs::inode::Inode;
use dashmap::DashMap;
use dashmap::mapref::one::RefMut;
use std::sync::Arc;
use std::time::Duration;

pub struct CacheCtl<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    caches: Arc<Folio>,
    inodes: DashMap<u64, Arc<Inode>>,
    pub chunk_io: Arc<ChunkIoFactory<B, M>>,
}

impl<B, M> CacheCtl<B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    pub fn new(config: PagedCacheConfig, chunk_io: Arc<ChunkIoFactory<B, M>>) -> Self {
        Self {
            caches: Arc::new(Folio::new(config)),
            inodes: DashMap::new(),
            chunk_io,
        }
    }

    pub fn prepare(&self, inode: Arc<Inode>) -> FileCacheGuard<'_, B, M> {
        let ino = inode.ino();
        let cache = self.caches.get_or_create(ino as u64);
        self.inodes.entry(ino as u64).or_insert(inode);

        FileCacheGuard {
            ino,
            cache,
            chunk_io: self.chunk_io.clone(),
        }
    }

    pub fn emit_flush_background(self: Arc<Self>)
    where
        B: Send + Sync + 'static,
        M: Send + Sync + 'static,
    {
        let ctl = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));

            loop {
                interval.tick().await;

                if let Err(e) = ctl.flush_all().await {
                    tracing::error!("Error occurred flushing cache: {e}");
                };
            }
        });
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn flush_all(&self) -> anyhow::Result<()> {
        for ino in self.caches.collect_all_ino() {
            self.flush_one(ino).await?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub async fn flush_one(&self, ino: u64) -> anyhow::Result<()> {
        let inode = {
            let inode_ref = self
                .inodes
                .get(&ino)
                .ok_or_else(|| anyhow::anyhow!("failed to flush: unknown inode {}", ino))?;
            inode_ref.value().clone()
        };

        // Lock the gate to block other write operations.
        let _guard = inode.gate.write().await;

        let mut chunks = self.caches.get_or_create(inode.ino() as u64);
        let dirty_results = chunks.collect_dirty_chunks();

        let mut need_clean = Vec::new();
        for (chunk_index, clean_info, write_op) in dirty_results {
            need_clean.push((chunk_index, clean_info));

            let chunk_id = chunk_id_for(inode.ino(), chunk_index);
            let writer = self.chunk_io.writer(chunk_id);
            for (offset, buf) in write_op {
                writer.write(offset, &buf).await?;
            }
        }

        for (chunk_index, clean_info) in need_clean {
            chunks.mark_clean(chunk_index, clean_info);
        }

        Ok(())
    }
}

pub struct FileCacheGuard<'a, B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    ino: i64,
    cache: RefMut<'a, u64, PagedCache>,
    chunk_io: Arc<ChunkIoFactory<B, M>>,
}

impl<'a, B, M> FileCacheGuard<'a, B, M>
where
    B: BlockStore,
    M: MetaStore,
{
    /// Probe cache and backfill missing pages from the backing store.
    pub async fn probe(
        &mut self,
        index: u64,
        offset: u32,
        len: u32,
        op: AccessKind,
    ) -> anyhow::Result<PagedCacheGuard<'_>> {
        let mut guard = self.cache.prepare(index);
        let info = guard.probe(offset, len, op);

        let mut position = 0;
        let mut buf = Vec::new();

        for current in info {
            if position <= current.offset as usize
                && position + buf.len() >= (current.offset + current.len) as usize
            {
                let start = current.offset as usize - position;
                let end = start + current.len as usize;
                let buf = &buf[start..end];
                guard.fill_back(current, buf);
                continue;
            }

            let chunk_id = chunk_id_for(self.ino, index);
            let mut reader = self.chunk_io.reader(chunk_id);

            reader.prepare_slices().await?;
            buf = reader.read(current.offset, current.len as usize).await?;

            guard.fill_back(current, &buf);
            position = current.offset as usize;
        }

        Ok(guard)
    }
}
