use crate::chuck::ChunkLayout;
use crate::chuck::page::PagedCacheConfig;
use crate::vfs::fs::MetaClientConfig;
use derive_builder::Builder;

#[derive(Builder, Clone)]
#[builder(pattern = "owned")]
pub struct VFSConfig {
    pub layout: ChunkLayout,
    pub cache_config: PagedCacheConfig,
    pub meta_config: MetaClientConfig,
}
