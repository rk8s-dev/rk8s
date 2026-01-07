use crate::chuck::ChunkLayout;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PAGE_SIZE: u32 = 64 * 1024;
pub const DEFAULT_FLUSH_ALL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct ReadConfig {
    pub layout: ChunkLayout,
}

impl ReadConfig {
    pub fn new(layout: ChunkLayout) -> Self {
        Self { layout }
    }
}

#[derive(Clone)]
pub struct WriteConfig {
    pub layout: ChunkLayout,
    pub page_size: u32,
    pub flush_all_interval: Duration,
}

impl WriteConfig {
    pub fn new(layout: ChunkLayout, page_size: u32) -> Self {
        Self {
            layout,
            page_size,
            flush_all_interval: DEFAULT_FLUSH_ALL_INTERVAL,
        }
    }

    #[allow(dead_code)]
    pub fn with_flush_all_interval(self, flush_all_interval: Duration) -> Self {
        Self {
            flush_all_interval,
            ..self
        }
    }
}

#[derive(Clone)]
pub struct VFSConfig {
    pub read: Arc<ReadConfig>,
    pub write: Arc<WriteConfig>,
}

impl VFSConfig {
    pub fn new(layout: ChunkLayout) -> Self {
        let read = Arc::new(ReadConfig::new(layout));

        let page_size = if layout.block_size.is_multiple_of(DEFAULT_PAGE_SIZE) {
            DEFAULT_PAGE_SIZE
        } else {
            layout.block_size
        };

        let write = Arc::new(WriteConfig::new(layout, page_size));
        Self { read, write }
    }
}
