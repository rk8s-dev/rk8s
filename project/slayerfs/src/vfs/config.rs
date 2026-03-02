use crate::chuck::ChunkLayout;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PAGE_SIZE: u32 = 64 * 1024; // 64KB
pub const DEFAULT_MAX_AHEAD: u64 = 32 * 1024 * 1024; // 32MB
pub const DEFAULT_BUFFER_SIZE: u64 = 1024 * 1024 * 300; // 300MB
pub const DEFAULT_WRITE_BUFFER_SIZE: u64 = 1024 * 1024 * 300; // 300MB
pub const DEFAULT_FLUSH_ALL_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct ReadConfig {
    pub layout: ChunkLayout,
    /// Maximum buffer size for read operations (soft limit).
    /// When exceeded, reads will be throttled. Hard limit is 2x this value.
    /// Default: 300MB. Increase for high-throughput sequential reads.
    /// Decrease for memory-constrained environments.
    pub buffer_size: u64,

    /// Maximum readahead distance for sequential reads.
    /// Limits how far ahead the session will predict. Too large values
    /// can waste memory on random access patterns.
    /// Default: 32MB. Adjust based on typical sequential read sizes.
    pub max_ahead: u64,
}

impl Default for ReadConfig {
    fn default() -> Self {
        Self {
            layout: ChunkLayout::default(),
            buffer_size: DEFAULT_BUFFER_SIZE,
            max_ahead: DEFAULT_MAX_AHEAD,
        }
    }
}

#[allow(dead_code)]
impl ReadConfig {
    pub fn new(layout: ChunkLayout) -> Self {
        Self {
            layout,
            ..Default::default()
        }
    }

    pub fn buffer_size(self, buffer_size: u64) -> Self {
        Self {
            buffer_size,
            ..self
        }
    }

    pub fn max_ahead(self, max_ahead: u64) -> Self {
        Self { max_ahead, ..self }
    }
}

#[derive(Clone)]
pub struct WriteConfig {
    pub layout: ChunkLayout,
    pub page_size: u32,
    /// Maximum buffer size for write operations (soft limit).
    /// When exceeded, writes will be throttled. Hard limit is 2x this value.
    /// Default: 300MB. Set to 0 to disable throttling.
    pub buffer_size: u64,
    pub flush_all_interval: Duration,
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            layout: ChunkLayout::default(),
            page_size: DEFAULT_PAGE_SIZE,
            buffer_size: DEFAULT_WRITE_BUFFER_SIZE,
            flush_all_interval: DEFAULT_FLUSH_ALL_INTERVAL,
        }
    }
}

#[allow(dead_code)]
impl WriteConfig {
    pub fn new(layout: ChunkLayout) -> Self {
        Self {
            layout,
            ..Default::default()
        }
    }

    pub fn page_size(self, page_size: u32) -> Self {
        Self { page_size, ..self }
    }

    pub fn buffer_size(self, buffer_size: u64) -> Self {
        Self {
            buffer_size,
            ..self
        }
    }

    pub fn flush_all_interval(self, flush_all_interval: Duration) -> Self {
        Self {
            flush_all_interval,
            ..self
        }
    }
}

#[derive(Clone, Default)]
pub struct VFSConfig {
    pub read: Arc<ReadConfig>,
    pub write: Arc<WriteConfig>,
}

#[allow(dead_code)]
impl VFSConfig {
    pub fn read_config(self, read: ReadConfig) -> Self {
        Self {
            read: Arc::new(read),
            ..self
        }
    }

    pub fn write_config(self, write: WriteConfig) -> Self {
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

        let write = Arc::new(WriteConfig::new(layout).page_size(page_size));
        Self { read, write }
    }
}
