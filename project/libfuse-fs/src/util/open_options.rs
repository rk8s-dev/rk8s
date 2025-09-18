use bitflags::bitflags;

// Flags use by the OPEN request/reply.
/// Bypass page cache for this open file.
const FOPEN_DIRECT_IO: u32 = 1;

/// Don't invalidate the data cache on open.
const FOPEN_KEEP_CACHE: u32 = 2;

/// The file is not seekable.
const FOPEN_NONSEEKABLE: u32 = 4;

/// allow caching this directory
const FOPEN_CACHE_DIR: u32 = 8;

/// the file is stream-like (no file position at all)
const FOPEN_STREAM: u32 = 16;

/// Instructs the kernel not to send an implicit FLUSH request when the last file handle is closed.
/// This delegates the responsibility of persisting data entirely to the FUSE daemon.
const FOPEN_NOFLUSH: u32 = 32;

/// Indicates that the filesystem can handle parallel direct writes to the same file from multiple threads.
/// The kernel will not serialize writes, so the FUSE daemon itself MUST implement locking
const FOPEN_PARALLEL_DIRECT_WRITES: u32 = 64;

/// Enables passthrough I/O. After open, subsequent I/O calls (read, write) will be sent
/// directly to the underlying file by the kernel, bypassing the FUSE daemon for maximum performance.
/// The daemon will not receive further I/O requests for this file handle.
const FOPEN_PASSTHROUGH: u32 = 128;

bitflags! {
    /// Options controlling the behavior of files opened by the server in response
    /// to an open or create request.
    pub struct OpenOptions: u32 {
        /// Bypass page cache for this open file.
        const DIRECT_IO = FOPEN_DIRECT_IO;
        /// Don't invalidate the data cache on open.
        const KEEP_CACHE = FOPEN_KEEP_CACHE;
        /// The file is not seekable.
        const NONSEEKABLE = FOPEN_NONSEEKABLE;
        /// allow caching this directory
        const CACHE_DIR = FOPEN_CACHE_DIR;
        /// the file is stream-like (no file position at all)
        const STREAM = FOPEN_STREAM;
        /// Instructs the kernel not to send an implicit FLUSH request when the last file handle is closed.
        const NOFLUSH = FOPEN_NOFLUSH;
        /// Indicates that the filesystem can handle parallel direct writes to the same file from multiple threads.
        const PARALLEL_DIRECT_WRITES = FOPEN_PARALLEL_DIRECT_WRITES;
        /// Enables passthrough I/O. After open, subsequent I/O calls (read, write) will be sent
        const PASSTROUGH = FOPEN_PASSTHROUGH;
    }
}
