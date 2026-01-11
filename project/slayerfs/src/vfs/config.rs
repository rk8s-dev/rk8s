use crate::chuck::ChunkLayout;
use crate::vfs::fs::MetaClientConfig;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PAGE_SIZE: u32 = 64 * 1024;
pub const DEFAULT_FLUSH_ALL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default, Clone)]
pub struct ReadConfig {
    pub layout: ChunkLayout,
}

#[allow(dead_code)]
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

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            layout: ChunkLayout::default(),
            page_size: DEFAULT_PAGE_SIZE,
            flush_all_interval: DEFAULT_FLUSH_ALL_INTERVAL,
        }
    }
}

#[allow(dead_code)]
impl WriteConfig {
    pub fn new(layout: ChunkLayout, page_size: u32) -> Self {
        Self {
            layout,
            page_size,
            flush_all_interval: DEFAULT_FLUSH_ALL_INTERVAL,
        }
    }

    pub fn flush_all_interval(self, flush_all_interval: Duration) -> Self {
        Self {
            flush_all_interval,
            ..self
        }
    }

    pub fn layout(self, layout: ChunkLayout) -> Self {
        Self { layout, ..self }
    }

    pub fn page_size(self, page_size: u32) -> Self {
        Self { page_size, ..self }
    }
}

#[derive(Clone, Default)]
pub struct VFSConfig {
    pub read: Arc<ReadConfig>,
    pub write: Arc<WriteConfig>,
    pub(crate) meta: MetaClientConfig,
}

#[allow(dead_code)]
impl VFSConfig {
    pub fn with_read_config(self, read: ReadConfig) -> Self {
        Self {
            read: Arc::new(read),
            ..self
        }
    }

    pub fn with_write_config(self, write: WriteConfig) -> Self {
        Self {
            write: Arc::new(write),
            ..self
        }
    }

    pub fn new(layout: ChunkLayout) -> Self {
        let read = Arc::new(ReadConfig::new(layout));

        let page_size = if layout.block_size.is_multiple_of(DEFAULT_PAGE_SIZE) {
            DEFAULT_PAGE_SIZE
        } else {
            layout.block_size
        };

        let write = Arc::new(WriteConfig::new(layout, page_size));
        Self {
            read,
            write,
            meta: MetaClientConfig::default(),
        }
    }
}
