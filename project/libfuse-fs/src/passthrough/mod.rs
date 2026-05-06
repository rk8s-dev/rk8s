#![allow(clippy::useless_conversion)]
use config::{CachePolicy, Config};
#[cfg(target_os = "linux")]
use file_handle::{FileHandle, OpenableFileHandle};

#[cfg(target_os = "macos")]
use self::statx::statx_timestamp;
use futures::executor::block_on;
use inode_store::{InodeId, InodeStore};
#[cfg(target_os = "linux")]
use libc::{self, statx_timestamp};

use moka::future::Cache;
use rfuse3::{Errno, raw::reply::ReplyEntry};
use uuid::Uuid;

use crate::passthrough::mmap::{MmapCachedValue, MmapChunkKey};
use crate::util::convert_stat64_to_file_attr;
#[cfg(target_os = "linux")]
use mount_fd::MountFds;
use statx::StatExt;
use std::cmp;
use std::io::Result;
use std::ops::DerefMut;
#[cfg(target_os = "macos")]
use std::os::fd::FromRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::sync::Mutex as StdMutex;
use tracing::error;
use tracing::{debug, warn};

#[cfg(target_os = "macos")]
use std::num::NonZeroUsize;
#[cfg(target_os = "macos")]
use std::sync::Weak;
use std::sync::atomic::{AtomicBool, AtomicU32};
use std::{
    collections::{BTreeMap, btree_map},
    ffi::{CStr, CString, OsString},
    fs::File,
    io::{self, Error},
    marker::PhantomData,
    os::{
        fd::{AsFd, AsRawFd, BorrowedFd, RawFd},
        unix::ffi::OsStringExt,
    },
    path::PathBuf,
    sync::Arc,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use util::{
    UniqueInodeGenerator, ebadf, is_dir, openat, reopen_fd_through_proc, stat_fd,
    validate_path_component,
};

use vm_memory::bitmap::BitmapSlice;

use nix::sys::resource::{Resource, getrlimit, setrlimit};

pub mod async_io;
pub mod config;
#[cfg(target_os = "linux")]
mod file_handle;
mod inode_store;
mod mmap;
#[cfg(target_os = "linux")]
mod mount_fd;
mod os_compat;
mod statx;
pub mod util;

/// Current directory
pub const CURRENT_DIR_CSTR: &[u8] = b".\0";
/// Parent directory
pub const PARENT_DIR_CSTR: &[u8] = b"..\0";
pub const VFS_MAX_INO: u64 = 0xff_ffff_ffff_ffff;
/// Path to `/proc/self/mountinfo`, consumed by the Linux-only file-handle
/// path (`mount_fd::MountFds`). Linux-only — macOS lacks `/proc`.
#[cfg(target_os = "linux")]
const MOUNT_INFO_FILE: &str = "/proc/self/mountinfo";
pub const EMPTY_CSTR: &[u8] = b"\0";
#[cfg(target_os = "linux")]
pub const PROC_SELF_FD_CSTR: &[u8] = b"/proc/self/fd\0";
#[cfg(target_os = "macos")]
pub const PROC_SELF_FD_CSTR: &[u8] = b"/dev/fd\0";
pub const ROOT_ID: u64 = 1;
use tokio::sync::{Mutex, MutexGuard, RwLock};

const MIN_PASSTHROUGH_NOFILE_SOFT_LIMIT: u64 = 8192;
const RESERVED_FILE_DESCRIPTORS: u64 = 64;

#[cfg(target_os = "macos")]
fn recover_std_mutex<T>(mutex: &StdMutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone)]
pub struct PassthroughArgs<P, M>
where
    P: AsRef<Path>,
    M: AsRef<str>,
{
    pub root_dir: P,
    pub mapping: Option<M>,
}

pub async fn new_passthroughfs_layer<P: AsRef<Path>, M: AsRef<str>>(
    args: PassthroughArgs<P, M>,
) -> Result<PassthroughFs> {
    let mut config = Config {
        root_dir: args.root_dir.as_ref().to_path_buf(),
        // enable xattr
        xattr: true,
        do_import: true,
        ..Default::default()
    };
    #[cfg(target_os = "macos")]
    if !config.macos_lazy_inode_fd {
        // Eager-fd fallback: macOS has no `O_PATH`, so every lookup pins a
        // real fd. Force TTLs to zero so kernel cache invalidation releases
        // those references promptly and we don't exhaust the fd table.
        config.entry_timeout = Duration::ZERO;
        config.attr_timeout = Duration::ZERO;
        config.dir_entry_timeout = Some(Duration::ZERO);
        config.dir_attr_timeout = Some(Duration::ZERO);
    }
    if let Some(mapping) = args.mapping {
        config.mapping = mapping
            .as_ref()
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    }

    let fs = PassthroughFs::<()>::new(config)?;

    #[cfg(target_os = "linux")]
    if fs.cfg.do_import {
        fs.import().await?;
    }
    #[cfg(target_os = "macos")]
    {
        // On macOS, always import for now since we rely on the root node being set up?
        // Or respect the config.
        fs.import().await?;
    }

    Ok(fs)
}

type Inode = u64;
type Handle = u64;

fn desired_nofile_soft_limit(soft: u64, hard: u64, minimum: u64) -> Option<u64> {
    if soft >= minimum || hard <= soft {
        return None;
    }

    Some(cmp::min(minimum, hard))
}

fn raise_nofile_soft_limit(minimum: u64) -> u64 {
    let Ok((soft, hard)) = getrlimit(Resource::RLIMIT_NOFILE) else {
        return minimum;
    };

    if let Some(target) = desired_nofile_soft_limit(soft, hard, minimum) {
        match setrlimit(Resource::RLIMIT_NOFILE, target, hard) {
            Ok(()) => return target,
            Err(err) => {
                warn!(
                    "passthroughfs: failed to raise RLIMIT_NOFILE from {soft} to {target}: {err}"
                );
            }
        }
    }

    soft
}

/// Maximum host inode number supported by passthroughfs
const MAX_HOST_INO: u64 = 0x7fff_ffff_ffff;

/**
 * Represents the file associated with an inode (`InodeData`).
 *
 * When obtaining such a file, it may either be a new file (the `Owned` variant), in which case the
 * object's lifetime is static, or it may reference `InodeData.file` (the `Ref` variant), in which
 * case the object's lifetime is that of the respective `InodeData` object.
 */
#[derive(Debug)]
enum InodeFile<'a> {
    /// Freshly opened file, owned by this `InodeFile`. Linux constructs
    /// this from `OpenableFileHandle::open(O_PATH)`; macOS doesn't use it
    /// (lazy mode hands out `Arc(Arc<File>)`, eager mode hands out `Ref`).
    #[cfg(target_os = "linux")]
    Owned(File),
    Ref(&'a File),
    /// Shared reference into the lazy-fd cache (`InodeHandle::Reopenable`).
    /// Avoids the per-call `dup(2)` syscall that `Owned` would require — we
    /// just bump the `Arc` refcount and let the caller borrow the underlying
    /// fd. Lifetime is `'static` because the `Arc` itself owns the `File`.
    #[cfg(target_os = "macos")]
    Arc(Arc<File>),
}

impl AsRawFd for InodeFile<'_> {
    /// Return a file descriptor for this file
    /// Note: This fd is only valid as long as the `InodeFile` exists.
    fn as_raw_fd(&self) -> RawFd {
        match self {
            #[cfg(target_os = "linux")]
            Self::Owned(file) => file.as_raw_fd(),
            Self::Ref(file_ref) => file_ref.as_raw_fd(),
            #[cfg(target_os = "macos")]
            Self::Arc(arc) => arc.as_raw_fd(),
        }
    }
}

impl AsFd for InodeFile<'_> {
    fn as_fd(&self) -> BorrowedFd<'_> {
        match self {
            #[cfg(target_os = "linux")]
            Self::Owned(file) => file.as_fd(),
            Self::Ref(file_ref) => file_ref.as_fd(),
            #[cfg(target_os = "macos")]
            Self::Arc(arc) => arc.as_fd(),
        }
    }
}

#[derive(Debug)]
#[allow(dead_code)]
enum InodeHandle {
    // TODO: Remove this variant once we have a way to handle files that are not
    File(File),
    /// `name_to_handle_at`/`open_by_handle_at` based identity (Linux-only).
    /// macOS lacks the syscalls; the lazy-fd `Reopenable` variant is used
    /// instead.
    #[cfg(target_os = "linux")]
    Handle(Arc<OpenableFileHandle>),

    /// Lazy-fd inode reference (macOS only, gated by
    /// `Config::macos_lazy_inode_fd`).
    ///
    /// Stores an absolute host path plus an optional cached backing file,
    /// opened on first access via `InodeData::get_file()`. This lets entry/attr
    /// cache TTLs go above zero without pinning a real fd per kernel-cached
    /// inode — `O_PATH` is unavailable on macOS, so the only alternative was
    /// forcing TTL to 0.
    ///
    /// `state` is wrapped in `Arc` so `LazyFdLru` can hold a `Weak` reference
    /// and clear `cached` on eviction without touching the path or removing
    /// the inode from `InodeMap`.
    #[cfg(target_os = "macos")]
    Reopenable {
        state: Arc<StdMutex<ReopenableState>>,
    },
}

#[cfg(target_os = "macos")]
#[derive(Debug)]
struct ReopenableState {
    /// Absolute host path used to (re)open the backing file. Mutable so that a
    /// successful `rename(2)` can update the cached path; the cached fd is
    /// invalidated alongside the path so the next `get_file()` reopens.
    path: PathBuf,
    /// Lazily-opened backing file shared via `Arc` so `get_file()` returns
    /// `InodeFile::Arc(...)` without a `dup(2)` per call. Held while the inode
    /// is referenced so hot paths (`stat`/`read`/`xattr`) skip per-call
    /// `openat`. Reset by `InodeData::update_lazy_path` or by an LRU
    /// eviction.
    cached: Option<Arc<File>>,
    /// Backref into the global LRU so a successful lazy-open can register
    /// (or promote) this inode and bump the reopen counter. `None` when the
    /// LRU layer is disabled (e.g. `macos_lazy_inode_fd == false`).
    lazy_fd_lru: Option<Arc<LazyFdLru>>,
}

/// Bounded LRU of cached lazy-fd backing files (macOS only).
///
/// Each entry holds a `Weak<StdMutex<ReopenableState>>`. When the LRU exceeds
/// its capacity, the oldest entry is popped: if its `Weak` still upgrades, we
/// clear `ReopenableState::cached` so the underlying `Arc<File>` drops. The
/// path is left untouched, so subsequent `get_file()` calls simply reopen.
///
/// Insertions are `O(log n)` worst case (the underlying `lru::LruCache` uses
/// a `HashMap` + doubly-linked list; lookups are `O(1)` on average). The
/// guarding mutex is `parking_lot`-free `std::sync::Mutex` to keep the build
/// dependency footprint minimal — observed contention on the LRU mutex is
/// negligible because each operation is a single map mutation.
#[cfg(target_os = "macos")]
pub(crate) struct LazyFdLru {
    inner: StdMutex<lru::LruCache<Inode, Weak<StdMutex<ReopenableState>>>>,
    /// Number of times a cached fd had to be (re)opened. Includes both first
    /// opens and reopens triggered by LRU eviction. Exposed via
    /// `PassthroughFs::macos_lazy_fd_reopen_count` for tests / metrics.
    reopen_count: AtomicU64,
    /// Configured capacity (>= 1). Stored separately so the value is
    /// inspectable without locking the LruCache.
    cap: NonZeroUsize,
}

#[cfg(target_os = "macos")]
impl LazyFdLru {
    fn new(cap: NonZeroUsize) -> Self {
        LazyFdLru {
            inner: StdMutex::new(lru::LruCache::new(cap)),
            reopen_count: AtomicU64::new(0),
            cap,
        }
    }

    /// Register a fresh open for `inode`. Promotes if the entry already
    /// exists, otherwise inserts. If insertion pushes the cache over `cap`
    /// the oldest entry is evicted: its `Weak<ReopenableState>` is upgraded
    /// (best effort) and `cached` is cleared so the underlying `Arc<File>`
    /// drops.
    ///
    /// Uses `LruCache::push` (not `put`) because `push` returns the
    /// evicted (key, value) when the cache was full — `put` evicts
    /// silently and gives us no chance to drop the cached fd.
    fn touch(&self, inode: Inode, weak: Weak<StdMutex<ReopenableState>>) {
        let mut guard = recover_std_mutex(&self.inner);
        if let Some((_evicted_inode, evicted_weak)) = guard.push(inode, weak) {
            // Drop the lock before walking back to the InodeData mutex —
            // they're independent, but releasing eagerly keeps the LRU
            // critical section as small as possible.
            drop(guard);
            if let Some(state) = evicted_weak.upgrade() {
                let mut s = recover_std_mutex(&state);
                s.cached = None;
            }
        }
    }

    /// Drop the entry for `inode` (called from `forget_one` once refcount
    /// reaches zero). Idempotent — missing entries are a no-op. Does **not**
    /// touch `ReopenableState::cached`; the inode itself is going away.
    fn remove(&self, inode: Inode) {
        let mut guard = recover_std_mutex(&self.inner);
        let _ = guard.pop(&inode);
    }

    /// Snapshot of the reopen counter. Each lazy `open(2)` triggered by a
    /// missing/evicted cache entry bumps this by one.
    pub(crate) fn reopen_count(&self) -> u64 {
        self.reopen_count.load(Ordering::Relaxed)
    }

    /// Snapshot of the configured cap. Used by tests / metrics surfacing.
    pub(crate) fn cap(&self) -> usize {
        self.cap.get()
    }

    /// Snapshot of the current cache occupancy. Bounded by `cap()`.
    pub(crate) fn len(&self) -> usize {
        recover_std_mutex(&self.inner).len()
    }

    fn bump_reopen(&self) {
        self.reopen_count.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(target_os = "macos")]
impl std::fmt::Debug for LazyFdLru {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyFdLru")
            .field("cap", &self.cap.get())
            .field("reopen_count", &self.reopen_count())
            .finish()
    }
}

impl InodeHandle {
    #[cfg(target_os = "linux")]
    fn file_handle(&self) -> Option<&FileHandle> {
        match self {
            InodeHandle::File(_) => None,
            InodeHandle::Handle(h) => Some(h.file_handle()),
        }
    }

    /// Best-effort `InodeFile` accessor that **does not handle** `Reopenable`.
    ///
    /// `Reopenable` callers must go through [`InodeData::get_file`] so the
    /// lazy-open path can run. This is here for the existing call sites that
    /// only ever see `File`/`Handle` variants (e.g. internal helpers) — they
    /// still compile by going through `InodeData::get_file` which forwards
    /// here for non-Reopenable variants.
    fn get_file(&self) -> Result<InodeFile<'_>> {
        match self {
            InodeHandle::File(f) => Ok(InodeFile::Ref(f)),
            #[cfg(target_os = "linux")]
            InodeHandle::Handle(h) => {
                let f = h.open(libc::O_PATH)?;
                Ok(InodeFile::Owned(f))
            }
            #[cfg(target_os = "macos")]
            InodeHandle::Reopenable { .. } => {
                // Programmer error: every Reopenable access must go via
                // InodeData::get_file so we can drive the lazy-open path.
                #[cfg(debug_assertions)]
                panic!(
                    "InodeHandle::get_file called on Reopenable; \
                     use InodeData::get_file instead"
                );
                #[cfg(not(debug_assertions))]
                {
                    Err(io::Error::other(
                        "InodeHandle::get_file called on Reopenable; \
                         use InodeData::get_file instead",
                    ))
                }
            }
        }
    }

    fn open_file(&self, flags: libc::c_int, proc_self_fd: &File) -> Result<File> {
        match self {
            InodeHandle::File(f) => reopen_fd_through_proc(f, flags, proc_self_fd),
            #[cfg(target_os = "linux")]
            InodeHandle::Handle(h) => h.open(flags),
            #[cfg(target_os = "macos")]
            InodeHandle::Reopenable { state } => {
                // Open the backing path with the requested flags via the
                // symlink-aware helper. `state.path` is absolute, so the
                // dirfd is irrelevant. LRU bookkeeping happens one level up
                // in `InodeData::open_file`, which knows the inode number.
                let mut guard = recover_std_mutex(state);
                let path = CString::new(guard.path.as_os_str().as_bytes())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                let fd = lazy_open_path(&path, flags)?;
                let f = unsafe { File::from_raw_fd(fd) };
                // Populate the cache opportunistically — only for plain
                // read-only opens, so we don't stash fds with surprising flags
                // (write, O_TRUNC, ...). We still need a separate fd to give
                // back to the caller (it expects ownership), so dup once at
                // cache-population time. Subsequent `get_file()` calls reuse
                // the cached `Arc<File>` with no further syscalls.
                if guard.cached.is_none() && flags == libc::O_RDONLY {
                    guard.cached = Some(Arc::new(f.try_clone()?));
                }
                Ok(f)
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn stat(&self) -> Result<libc::stat64> {
        self.do_stat()
    }
    #[cfg(target_os = "macos")]
    fn stat(&self) -> Result<libc::stat> {
        // On macOS, stat_fd returns libc::stat, which is the correct type.
        // No explicit cast from stat64 is needed if stat_fd is correctly implemented
        // to return the platform-specific stat struct.
        self.do_stat()
    }

    #[cfg(target_os = "linux")]
    fn do_stat(&self) -> Result<libc::stat64> {
        match self {
            InodeHandle::File(f) => stat_fd(f, None),
            InodeHandle::Handle(_h) => {
                let file = self.get_file()?;
                stat_fd(&file, None)
            }
        }
    }

    #[cfg(target_os = "macos")]
    fn do_stat(&self) -> Result<libc::stat> {
        match self {
            InodeHandle::File(f) => stat_fd(f, None),
            InodeHandle::Reopenable { state } => {
                // Stat by path — no need to keep an fd around just for stat.
                let path = {
                    let guard = recover_std_mutex(state);
                    CString::new(guard.path.as_os_str().as_bytes())
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
                };
                // `guard.path` is absolute, so AT_FDCWD is only a required
                // placeholder. AT_SYMLINK_NOFOLLOW matches the rest of the
                // passthrough's stat behavior.
                let mut st = std::mem::MaybeUninit::<libc::stat>::zeroed();
                let res = unsafe {
                    libc::fstatat(
                        libc::AT_FDCWD,
                        path.as_ptr(),
                        st.as_mut_ptr(),
                        libc::AT_SYMLINK_NOFOLLOW,
                    )
                };
                if res != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(unsafe { st.assume_init() })
            }
        }
    }
}

/// Represents an inode in `PassthroughFs`.
#[derive(Debug)]
pub struct InodeData {
    inode: Inode,
    // Most of these aren't actually files but ¯\_(ツ)_/¯.
    handle: InodeHandle,
    id: InodeId,
    refcount: AtomicU64,
    // File type and mode
    mode: u32,
    /// Birth time used as part of the `(ino, btime)` cache key for the
    /// Linux-only `handle_cache`. macOS doesn't construct file handles, so
    /// the field is captured for consistency but never read.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    btime: statx_timestamp,
}

/// macOS lazy-fd open helper.
///
/// Opens an absolute path produced by the lazy-fd cache (`Reopenable`).
/// Two-step semantics:
/// 1. `O_NOFOLLOW` first — opens the entry as long as the trailing
///    component isn't a symlink.
/// 2. On `ELOOP`, retry with `O_SYMLINK` so the link node itself is
///    opened (for subsequent `readlink`/xattr).
///
/// **Why we don't use `O_NOFOLLOW_ANY`** (PR-9.3 finding): `/tmp` on
/// macOS is itself a symlink to `/private/tmp`, so any cached path that
/// went through `/tmp` would fail with `ELOOP` under
/// `O_NOFOLLOW_ANY`. Combining it with `O_NOFOLLOW` returns `EINVAL`
/// (the flags are mutually exclusive on Darwin). Hardening against
/// intermediate-symlink TOCTOU therefore needs a different design —
/// e.g. canonicalizing `cfg.root_dir` at startup so all stored paths
/// are realpath-resolved, then using `O_NOFOLLOW_ANY` standalone.
/// Tracked as future work in `macos-support-matrix.md`.
///
/// The `O_NOFOLLOW` -> `O_SYMLINK` retry is intentionally a compatibility
/// fallback and has a narrow race if the entry is swapped between the two
/// opens. `cfg.root_dir` is canonicalized before lazy mode is enabled, so the
/// race is confined to entries under that root; removing it completely needs
/// the future `O_NOFOLLOW_ANY` design above.
///
/// Always sets `O_CLOEXEC`. Returns the raw fd; the caller wraps it in `File`.
#[cfg(target_os = "macos")]
fn lazy_open_path(path: &CStr, flags: libc::c_int) -> io::Result<libc::c_int> {
    // Strip flags that don't make sense for this opener.
    let base = (flags & !libc::O_CREAT & !libc::O_DIRECTORY) | libc::O_CLOEXEC;
    let with_nofollow = base | libc::O_NOFOLLOW;
    let fd = unsafe { libc::open(path.as_ptr(), with_nofollow) };
    if fd >= 0 {
        return Ok(fd);
    }
    let err = io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::ELOOP) {
        let symlink_flags = (base & !libc::O_NOFOLLOW) | libc::O_SYMLINK;
        let fd = unsafe { libc::open(path.as_ptr(), symlink_flags) };
        if fd >= 0 {
            return Ok(fd);
        }
        return Err(io::Error::last_os_error());
    }
    Err(err)
}

impl InodeData {
    fn new(
        inode: Inode,
        f: InodeHandle,
        refcount: u64,
        id: InodeId,
        mode: u32,
        btime: statx_timestamp,
    ) -> Self {
        InodeData {
            inode,
            handle: f,
            id,
            refcount: AtomicU64::new(refcount),
            mode,
            btime,
        }
    }

    fn get_file(&self) -> Result<InodeFile<'_>> {
        #[cfg(target_os = "macos")]
        if let InodeHandle::Reopenable { state } = &self.handle {
            // Lazy-open path: lock state, ensure a cached `Arc<File>`, then
            // hand out a refcount bump as `InodeFile::Arc`. No `dup(2)` per
            // call — a single shared fd is reused while the inode is alive.
            let mut guard = recover_std_mutex(state);
            let mut touched_lru = None;
            if guard.cached.is_none() {
                let path = CString::new(guard.path.as_os_str().as_bytes())
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                // Use the symlink-aware helper so we don't follow links;
                // mirrors `open_file_restricted` semantics.
                let fd = lazy_open_path(&path, libc::O_RDONLY)?;
                guard.cached = Some(Arc::new(unsafe { File::from_raw_fd(fd) }));
                // Capture the LRU handle while we still hold the guard so
                // we can register/promote after dropping the lock. Bumping
                // `reopen_count` reflects this open (cache miss or post-
                // eviction reopen).
                touched_lru = guard.lazy_fd_lru.clone();
            }
            let arc = Arc::clone(guard.cached.as_ref().unwrap());
            drop(guard);
            if let Some(lru) = touched_lru {
                lru.bump_reopen();
                lru.touch(self.inode, Arc::downgrade(state));
            }
            return Ok(InodeFile::Arc(arc));
        }
        self.handle.get_file()
    }

    fn open_file(&self, flags: libc::c_int, proc_self_fd: &File) -> Result<File> {
        let f = self.handle.open_file(flags, proc_self_fd)?;
        // If `open_file` populated the lazy cache (RDONLY path), promote in
        // the LRU on the inode's actual key. The Reopenable handle owns its
        // own LRU backref, so we only need to consult it.
        #[cfg(target_os = "macos")]
        if let InodeHandle::Reopenable { state } = &self.handle {
            let (had_cache, lru_opt) = {
                let guard = recover_std_mutex(state);
                (guard.cached.is_some(), guard.lazy_fd_lru.clone())
            };
            if had_cache && let Some(lru) = lru_opt {
                // Open with non-RDONLY flags doesn't populate the cache,
                // so we only `bump_reopen` for the path that actually
                // refreshed cache. Touch is unconditional once we know
                // the cache is populated — promotes in LRU.
                if flags == libc::O_RDONLY {
                    lru.bump_reopen();
                }
                lru.touch(self.inode, Arc::downgrade(state));
            }
        }
        Ok(f)
    }

    /// macOS lazy-fd: replace the absolute path stored on a `Reopenable`
    /// inode and invalidate the cached fd so the next `get_file()` reopens
    /// at the new location. No-op for non-Reopenable handles.
    ///
    /// **Note**: an already-open `cached` fd survives `rename(2)` on POSIX,
    /// but the path is needed if we ever lose the cache (eviction,
    /// `update_lazy_path` itself, etc.). Keeping path and cache consistent
    /// avoids stale-path bugs once an LRU eviction layer is added.
    #[cfg(target_os = "macos")]
    fn update_lazy_path(&self, new_path: PathBuf) {
        if let InodeHandle::Reopenable { state } = &self.handle {
            let mut guard = recover_std_mutex(state);
            guard.path = new_path;
            guard.cached = None;
        }
    }

    /// Returns the absolute path of a `Reopenable` inode, if any. Used by
    /// `do_lookup` to compute child paths and by rename to rebuild paths.
    #[cfg(target_os = "macos")]
    fn lazy_path(&self) -> Option<PathBuf> {
        match &self.handle {
            InodeHandle::Reopenable { state } => Some(recover_std_mutex(state).path.clone()),
            _ => None,
        }
    }
}

/// Data structures to manage accessed inodes.
struct InodeMap {
    pub inodes: RwLock<InodeStore>,
}

impl InodeMap {
    fn new() -> Self {
        InodeMap {
            inodes: RwLock::new(Default::default()),
        }
    }

    async fn clear(&self) {
        // Do not expect poisoned lock here, so safe to unwrap().
        self.inodes.write().await.clear();
    }

    async fn get(&self, inode: Inode) -> Result<Arc<InodeData>> {
        // Do not expect poisoned lock here, so safe to unwrap().
        self.inodes
            .read()
            .await
            .get(&inode)
            .cloned()
            .ok_or_else(ebadf)
    }

    fn get_inode_locked(
        inodes: &InodeStore,
        #[cfg_attr(target_os = "macos", allow(unused_variables))] handle: &InodeHandle,
    ) -> Option<Inode> {
        #[cfg(target_os = "linux")]
        if let Some(h) = handle.file_handle() {
            return inodes.inode_by_handle(h).copied();
        }
        #[cfg(target_os = "macos")]
        let _ = inodes;
        None
    }

    async fn get_alt(&self, id: &InodeId, handle: &InodeHandle) -> Option<Arc<InodeData>> {
        // Do not expect poisoned lock here, so safe to unwrap().
        let inodes = self.inodes.read().await;

        Self::get_alt_locked(&inodes, id, handle)
    }

    fn get_alt_locked(
        inodes: &InodeStore,
        id: &InodeId,
        #[cfg_attr(target_os = "macos", allow(unused_variables))] handle: &InodeHandle,
    ) -> Option<Arc<InodeData>> {
        // Linux: try the by-handle lookup first to detect inode-ID reuse.
        // macOS: file handles don't exist, so by-id is the only key.
        #[cfg(target_os = "linux")]
        let by_handle = handle.file_handle().and_then(|h| inodes.get_by_handle(h));
        #[cfg(target_os = "macos")]
        let by_handle: Option<&Arc<InodeData>> = None;

        by_handle
            .or_else(|| {
                inodes.get_by_id(id).filter(|_data| {
                    // Linux only: when falling back to by-id, ensure we hit an
                    // entry that does not have a file handle. Entries *with*
                    // handles also have a handle alt key, so if we did not
                    // find it by that key we must have found an entry for a
                    // different file with a reused inode ID.
                    #[cfg(target_os = "linux")]
                    {
                        _data.handle.file_handle().is_none()
                    }
                    #[cfg(target_os = "macos")]
                    {
                        true
                    }
                })
            })
            .cloned()
    }

    async fn insert(&self, data: Arc<InodeData>) {
        let mut inodes = self.inodes.write().await;

        Self::insert_locked(&mut inodes, data)
    }

    fn insert_locked(inodes: &mut InodeStore, data: Arc<InodeData>) {
        inodes.insert(data);
    }
}

struct HandleData {
    inode: Inode,
    file: File,
    lock: Mutex<()>,
    open_flags: AtomicU32,
}

impl HandleData {
    fn new(inode: Inode, file: File, flags: u32) -> Self {
        HandleData {
            inode,
            file,
            lock: Mutex::new(()),
            open_flags: AtomicU32::new(flags),
        }
    }

    fn get_file(&self) -> &File {
        &self.file
    }

    async fn get_file_mut(&self) -> (MutexGuard<'_, ()>, &File) {
        (self.lock.lock().await, &self.file)
    }

    fn borrow_fd(&self) -> BorrowedFd<'_> {
        self.file.as_fd()
    }

    async fn get_flags(&self) -> u32 {
        self.open_flags.load(Ordering::Relaxed)
    }

    async fn set_flags(&self, flags: u32) {
        self.open_flags.store(flags, Ordering::Relaxed);
    }
}

struct HandleMap {
    handles: RwLock<BTreeMap<Handle, Arc<HandleData>>>,
}

impl HandleMap {
    fn new() -> Self {
        HandleMap {
            handles: RwLock::new(BTreeMap::new()),
        }
    }

    async fn clear(&self) {
        // Do not expect poisoned lock here, so safe to unwrap().
        self.handles.write().await.clear();
    }

    async fn insert(&self, handle: Handle, data: HandleData) {
        // Do not expect poisoned lock here, so safe to unwrap().
        self.handles.write().await.insert(handle, Arc::new(data));
    }

    async fn release(&self, handle: Handle, inode: Inode) -> Result<()> {
        // Do not expect poisoned lock here, so safe to unwrap().
        let mut handles = self.handles.write().await;

        if let btree_map::Entry::Occupied(e) = handles.entry(handle)
            && e.get().inode == inode
        {
            // We don't need to close the file here because that will happen automatically when
            // the last `Arc` is dropped.
            e.remove();

            return Ok(());
        }

        Err(ebadf())
    }

    async fn get(&self, handle: Handle, inode: Inode) -> Result<Arc<HandleData>> {
        // Do not expect poisoned lock here, so safe to unwrap().
        self.handles
            .read()
            .await
            .get(&handle)
            .filter(|hd| hd.inode == inode)
            .cloned()
            .ok_or_else(ebadf)
    }
}

/// Key into the per-fs `handle_cache` that maps `(host_inode, btime)` →
/// `Arc<FileHandle>`. Linux only; macOS doesn't construct file handles.
#[cfg(target_os = "linux")]
#[derive(Debug, Hash, Eq, PartialEq)]
struct FileUniqueKey(u64, statx_timestamp);

/// A file system that simply "passes through" all requests it receives to the underlying file
/// system.
///
/// To keep the implementation simple it servers the contents of its root directory. Users
/// that wish to serve only a specific directory should set up the environment so that that
/// directory ends up as the root of the file system process. One way to accomplish this is via a
/// combination of mount namespaces and the pivot_root system call.
pub struct PassthroughFs<S: BitmapSlice + Send + Sync = ()> {
    // File descriptors for various points in the file system tree. These fds are always opened with
    // the `O_PATH` option so they cannot be used for reading or writing any data. See the
    // documentation of the `O_PATH` flag in `open(2)` for more details on what one can and cannot
    // do with an fd opened with this flag.
    inode_map: InodeMap,
    next_inode: AtomicU64,

    // File descriptors for open files and directories. Unlike the fds in `inodes`, these _can_ be
    // used for reading and writing data.
    handle_map: HandleMap,
    next_handle: AtomicU64,

    // Use to generate unique inode
    ino_allocator: UniqueInodeGenerator,
    // Maps mount IDs to an open FD on the respective ID for the purpose of open_by_handle_at().
    #[cfg(target_os = "linux")]
    mount_fds: MountFds,

    // File descriptor pointing to the `/proc/self/fd` directory. This is used to convert an fd from
    // `inodes` into one that can go into `handles`. This is accomplished by reading the
    // `/proc/self/fd/{}` symlink. We keep an open fd here in case the file system tree that we are meant
    // to be serving doesn't have access to `/proc/self/fd`.
    proc_self_fd: File,

    // Whether writeback caching is enabled for this directory. This will only be true when
    // `cfg.writeback` is true and `init` was called with `FsOptions::WRITEBACK_CACHE`.
    writeback: AtomicBool,

    // Whether no_open is enabled.
    no_open: AtomicBool,

    // Whether no_opendir is enabled.
    no_opendir: AtomicBool,

    // Whether kill_priv_v2 is enabled.
    //killpriv_v2: AtomicBool,

    // Whether no_readdir is enabled.
    no_readdir: AtomicBool,

    // Whether seal_size is enabled.
    seal_size: AtomicBool,

    // Whether per-file DAX feature is enabled.
    // Init from guest kernel Init cmd of fuse fs.
    //perfile_dax: AtomicBool,
    dir_entry_timeout: Duration,
    dir_attr_timeout: Duration,

    cfg: Config,

    _uuid: Uuid,

    phantom: PhantomData<S>,

    #[cfg(target_os = "linux")]
    handle_cache: Cache<FileUniqueKey, Arc<FileHandle>>,

    mmap_chunks: Cache<MmapChunkKey, Arc<RwLock<mmap::MmapCachedValue>>>,

    /// LRU bounding the number of cached lazy-fd backing files. Some only
    /// when on macOS with `Config::macos_lazy_inode_fd == true`. Cloned
    /// into each `ReopenableState` so the lazy-open path can register
    /// without a backref to `self`.
    #[cfg(target_os = "macos")]
    lazy_fd_lru: Option<Arc<LazyFdLru>>,
}

impl<S: BitmapSlice + Send + Sync> PassthroughFs<S> {
    /// Create a Passthrough file system instance.
    pub fn new(mut cfg: Config) -> Result<PassthroughFs<S>> {
        if cfg.no_open && cfg.cache_policy != CachePolicy::Always {
            warn!("passthroughfs: no_open only work with cache=always, reset to open mode");
            cfg.no_open = false;
        }
        if cfg.writeback && cfg.cache_policy == CachePolicy::Never {
            warn!(
                "passthroughfs: writeback cache conflicts with cache=none, reset to no_writeback"
            );
            cfg.writeback = false;
        }
        #[cfg(target_os = "macos")]
        if cfg.macos_lazy_inode_fd {
            cfg.root_dir = std::fs::canonicalize(&cfg.root_dir)?;
        }

        // Safe because this is a constant value and a valid C string.
        let proc_self_fd_cstr = unsafe { CStr::from_bytes_with_nul_unchecked(PROC_SELF_FD_CSTR) };

        #[cfg(target_os = "linux")]
        let flags = libc::O_PATH | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        #[cfg(target_os = "macos")]
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC;

        let proc_self_fd = Self::open_file(&libc::AT_FDCWD, proc_self_fd_cstr, flags, 0)?;

        let (dir_entry_timeout, dir_attr_timeout) =
            match (cfg.dir_entry_timeout, cfg.dir_attr_timeout) {
                (Some(e), Some(a)) => (e, a),
                (Some(e), None) => (e, cfg.attr_timeout),
                (None, Some(a)) => (cfg.entry_timeout, a),
                (None, None) => (cfg.entry_timeout, cfg.attr_timeout),
            };

        #[cfg(target_os = "linux")]
        let mount_fds = MountFds::new(None)?;

        let fd_limit = raise_nofile_soft_limit(MIN_PASSTHROUGH_NOFILE_SOFT_LIMIT);

        // macOS lazy-fd LRU: cap defaults to half the soft RLIMIT_NOFILE,
        // floor of 1 (NonZeroUsize). `RESERVED_FILE_DESCRIPTORS` already
        // accounts for ancillary fds (proc_self_fd, mount_fds, …), so we
        // base the half-share on the post-reserve budget rather than the
        // raw rlimit.
        #[cfg(target_os = "macos")]
        let lazy_fd_lru: Option<Arc<LazyFdLru>> = if cfg.macos_lazy_inode_fd {
            let cap = match cfg.macos_lazy_fd_lru_max {
                Some(n) => n,
                None => {
                    let auto = fd_limit.saturating_sub(RESERVED_FILE_DESCRIPTORS).max(2) / 2;
                    NonZeroUsize::new(auto.try_into().unwrap_or(usize::MAX))
                        .unwrap_or(NonZeroUsize::new(1).unwrap())
                }
            };
            Some(Arc::new(LazyFdLru::new(cap)))
        } else {
            None
        };

        let max_mmap_size = if cfg.use_mmap { cfg.max_mmap_size } else { 0 };

        let mmap_cache_builder = Cache::builder()
            .max_capacity(max_mmap_size)
            .weigher(
                |_key: &MmapChunkKey, value: &Arc<RwLock<mmap::MmapCachedValue>>| -> u32 {
                    let guard = block_on(value.read());
                    match &*guard {
                        MmapCachedValue::Mmap(mmap) => mmap.len() as u32,
                        MmapCachedValue::MmapMut(mmap_mut) => mmap_mut.len() as u32,
                    }
                },
            )
            .time_to_idle(Duration::from_millis(60));

        Ok(PassthroughFs {
            inode_map: InodeMap::new(),
            next_inode: AtomicU64::new(ROOT_ID + 1),
            ino_allocator: UniqueInodeGenerator::new(),

            handle_map: HandleMap::new(),
            next_handle: AtomicU64::new(1),

            #[cfg(target_os = "linux")]
            mount_fds,
            proc_self_fd,

            writeback: AtomicBool::new(false),
            no_open: AtomicBool::new(false),
            no_opendir: AtomicBool::new(false),
            //killpriv_v2: AtomicBool::new(false),
            no_readdir: AtomicBool::new(cfg.no_readdir),
            seal_size: AtomicBool::new(cfg.seal_size),
            //perfile_dax: AtomicBool::new(false),
            dir_entry_timeout,
            dir_attr_timeout,
            cfg,

            _uuid: Uuid::new_v4(),

            phantom: PhantomData,

            #[cfg(target_os = "linux")]
            handle_cache: moka::future::Cache::new(
                fd_limit.saturating_sub(RESERVED_FILE_DESCRIPTORS).max(1),
            ),

            mmap_chunks: mmap_cache_builder.build(),

            #[cfg(target_os = "macos")]
            lazy_fd_lru,
        })
    }

    /// macOS only: snapshot of the lazy-fd reopen counter. Returns `None`
    /// when the lazy-fd path is disabled.
    #[cfg(target_os = "macos")]
    pub fn macos_lazy_fd_reopen_count(&self) -> Option<u64> {
        self.lazy_fd_lru.as_ref().map(|l| l.reopen_count())
    }

    /// macOS only: current number of cached lazy fds (bounded by the LRU
    /// cap). Returns `None` when the lazy-fd path is disabled.
    #[cfg(target_os = "macos")]
    pub fn macos_lazy_fd_cache_len(&self) -> Option<usize> {
        self.lazy_fd_lru.as_ref().map(|l| l.len())
    }

    /// macOS only: configured cap of the lazy-fd LRU. Returns `None` when
    /// the lazy-fd path is disabled.
    #[cfg(target_os = "macos")]
    pub fn macos_lazy_fd_cap(&self) -> Option<usize> {
        self.lazy_fd_lru.as_ref().map(|l| l.cap())
    }

    /// Resolve `inode` to the absolute host filesystem path that backs it.
    ///
    /// On macOS in lazy mode this returns the cached `Reopenable.path`
    /// directly (no syscall); otherwise it resolves via `F_GETPATH` on the
    /// inode's eager fd. On Linux it resolves via `readlink(/proc/self/fd/N)`
    /// using the same `fd_path_cstr` helper.
    ///
    /// Returns `None` if the inode is unknown or can't be resolved (e.g.
    /// already forgotten). Used by overlayfs/unionfs `copy_regfile_up` for
    /// the cross-layer APFS clone fast path; safe for any caller that
    /// needs a host-fs path for a tracked inode.
    pub async fn passthrough_host_path(&self, inode: Inode) -> Option<PathBuf> {
        let data = self.inode_map.get(inode).await.ok()?;
        #[cfg(target_os = "macos")]
        if let Some(p) = data.lazy_path() {
            return Some(p);
        }
        let file = data.get_file().ok()?;
        let cstr = util::fd_path_cstr(file.as_raw_fd()).ok()?;
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(cstr.to_bytes())))
    }

    /// Initialize the Passthrough file system.
    pub async fn import(&self) -> Result<()> {
        let root = CString::new(self.cfg.root_dir.as_os_str().as_bytes())
            .map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))?;

        let (handle, st) = Self::open_file_and_handle(
            self,
            &libc::AT_FDCWD,
            &root,
            #[cfg(target_os = "macos")]
            Some(self.cfg.root_dir.clone()),
        )
        .await
        .map_err(|e| {
            error!("fuse: import: failed to get file or handle: {e:?}");

            e
        })?;

        let id = InodeId::from_stat(&st);

        // Safe because this doesn't modify any memory and there is no need to check the return
        // value because this system call always succeeds. We need to clear the umask here because
        // we want the client to be able to set all the bits in the mode.
        unsafe { libc::umask(0o000) };

        // Not sure why the root inode gets a refcount of 2 but that's what libfuse does.
        self.inode_map
            .insert(Arc::new(InodeData::new(
                ROOT_ID,
                handle,
                2,
                id,
                st.st.st_mode.into(),
                st.btime
                    .ok_or_else(|| io::Error::other("birth time not available"))?,
            )))
            .await;

        Ok(())
    }

    /// Get the list of file descriptors which should be reserved across live upgrade.
    pub fn keep_fds(&self) -> Vec<RawFd> {
        vec![self.proc_self_fd.as_raw_fd()]
    }

    /// Read-only borrow of the configuration. Used by Layer impls and other
    /// integrations that need to consult fields like `whiteout_format`.
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    fn readlinkat(dfd: i32, pathname: &CStr) -> Result<PathBuf> {
        let mut buf = Vec::with_capacity(libc::PATH_MAX as usize);

        // Safe because the kernel will only write data to buf and we check the return value
        let buf_read = unsafe {
            libc::readlinkat(
                dfd,
                pathname.as_ptr(),
                buf.as_mut_ptr() as *mut libc::c_char,
                buf.capacity(),
            )
        };
        if buf_read < 0 {
            error!("fuse: readlinkat error");
            return Err(Error::last_os_error());
        }

        // Safe because we trust the value returned by kernel.
        unsafe { buf.set_len(buf_read as usize) };
        buf.shrink_to_fit();

        // Be careful:
        // - readlink() does not append a terminating null byte to buf
        // - OsString instances are not NUL terminated
        Ok(PathBuf::from(OsString::from_vec(buf)))
    }

    /// Get the file pathname corresponding to the Inode
    /// This function is used by Nydus blobfs
    pub async fn readlinkat_proc_file(&self, inode: Inode) -> Result<PathBuf> {
        let data = self.inode_map.get(inode).await?;
        let file = data.get_file()?;
        let pathname = CString::new(format!("{}", file.as_raw_fd()))
            .map_err(|e| Error::new(io::ErrorKind::InvalidData, e))?;

        Self::readlinkat(self.proc_self_fd.as_raw_fd(), &pathname)
    }

    fn create_file_excl(
        dir: &impl AsRawFd,
        pathname: &CStr,
        flags: i32,
        mode: u32,
    ) -> io::Result<Option<File>> {
        match openat(dir, pathname, flags | libc::O_CREAT | libc::O_EXCL, mode) {
            Ok(file) => Ok(Some(file)),
            Err(err) => {
                // Ignore the error if the file exists and O_EXCL is not present in `flags`.
                if err.kind() == io::ErrorKind::AlreadyExists {
                    if (flags & libc::O_EXCL) != 0 {
                        return Err(err);
                    }
                    return Ok(None);
                }
                Err(err)
            }
        }
    }

    fn open_file(dfd: &impl AsRawFd, pathname: &CStr, flags: i32, mode: u32) -> io::Result<File> {
        openat(dfd, pathname, flags, mode)
    }

    fn open_file_restricted(
        &self,
        dir: &impl AsRawFd,
        pathname: &CStr,
        flags: i32,
        mode: u32,
    ) -> io::Result<File> {
        let flags = libc::O_NOFOLLOW | libc::O_CLOEXEC | flags;

        // TODO
        //if self.os_facts.has_openat2 {
        //    oslib::do_open_relative_to(dir, pathname, flags, mode)
        //} else {
        #[cfg(target_os = "macos")]
        {
            match openat(dir, pathname, flags, mode) {
                Err(err) if err.raw_os_error() == Some(libc::ELOOP) => {
                    let symlink_flags = (flags & !libc::O_NOFOLLOW) | libc::O_SYMLINK;
                    openat(dir, pathname, symlink_flags, mode)
                }
                result => result,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            openat(dir, pathname, flags, mode)
        }
        //}
    }

    /// Create a File or File Handle for `name` under directory `dir_fd` to support `lookup()`.
    async fn open_file_and_handle(
        &self,
        dir: &impl AsRawFd,
        name: &CStr,
        #[cfg(target_os = "macos")] lazy_abs_path: Option<PathBuf>,
    ) -> io::Result<(InodeHandle, StatExt)> {
        // macOS lazy-fd path: stat by name (no fd held) and return a
        // `Reopenable` handle that opens lazily on first I/O. This is the
        // mechanism that lets entry/attr cache TTLs go above zero on macOS.
        #[cfg(target_os = "macos")]
        if self.cfg.macos_lazy_inode_fd
            && let Some(abs_path) = lazy_abs_path
        {
            let st = statx::statx(dir, Some(name))?;
            return Ok((
                InodeHandle::Reopenable {
                    state: Arc::new(StdMutex::new(ReopenableState {
                        path: abs_path,
                        cached: None,
                        lazy_fd_lru: self.lazy_fd_lru.clone(),
                    })),
                },
                st,
            ));
        }

        #[cfg(target_os = "linux")]
        {
            let path_file = self.open_file_restricted(dir, name, libc::O_PATH, 0)?;
            let st = statx::statx(&path_file, None)?;

            let btime_is_valid = match st.btime {
                Some(ts) => ts.tv_sec != 0 || ts.tv_nsec != 0,
                None => false,
            };

            if btime_is_valid {
                let key = FileUniqueKey(st.st.st_ino, st.btime.unwrap());
                let cache = self.handle_cache.clone();
                if let Some(h) = cache.get(&key).await {
                    let openable = self.to_openable_handle(h)?;
                    Ok((InodeHandle::Handle(openable), st))
                } else if let Some(handle_from_fd) = FileHandle::from_fd(&path_file)? {
                    let handle_arc = Arc::new(handle_from_fd);
                    cache.insert(key, Arc::clone(&handle_arc)).await;
                    let openable = self.to_openable_handle(handle_arc)?;
                    Ok((InodeHandle::Handle(openable), st))
                } else {
                    Ok((InodeHandle::File(path_file), st))
                }
            } else if let Some(handle_from_fd) = FileHandle::from_fd(&path_file)? {
                let handle_arc = Arc::new(handle_from_fd);
                let openable = self.to_openable_handle(handle_arc)?;
                Ok((InodeHandle::Handle(openable), st))
            } else {
                Ok((InodeHandle::File(path_file), st))
            }
        }
        #[cfg(target_os = "macos")]
        {
            // macOS without lazy mode: pin an `O_RDONLY` fd as
            // `InodeHandle::File`. file-handle path is unreachable since the
            // syscalls don't exist on Darwin.
            let path_file = self.open_file_restricted(dir, name, libc::O_RDONLY, 0)?;
            let st = statx::statx(&path_file, None)?;
            Ok((InodeHandle::File(path_file), st))
        }
    }

    #[cfg(target_os = "linux")]
    fn to_openable_handle(&self, fh: Arc<FileHandle>) -> io::Result<Arc<OpenableFileHandle>> {
        (*Arc::as_ref(&fh))
            .clone()
            .into_openable(&self.mount_fds, |fd, flags, _mode| {
                reopen_fd_through_proc(&fd, flags, &self.proc_self_fd)
            })
            .map(Arc::new)
            .map_err(|e| {
                if !e.silent() {
                    error!("{e}");
                }
                e.into_inner()
            })
    }

    async fn allocate_inode(
        &self,
        inodes: &InodeStore,
        id: &InodeId,
        handle: &InodeHandle,
    ) -> io::Result<Inode> {
        if !self.cfg.use_host_ino {
            // If the inode has already been assigned before, the new inode is not reassigned,
            // ensuring that the same file is always the same inode
            match InodeMap::get_inode_locked(inodes, handle) {
                Some(a) => Ok(a),
                None => Ok(self.next_inode.fetch_add(1, Ordering::Relaxed)),
            }
        } else {
            let inode = if id.ino > MAX_HOST_INO {
                // Prefer looking for previous mappings from memory
                match InodeMap::get_inode_locked(inodes, handle) {
                    Some(ino) => ino,
                    None => self.ino_allocator.get_unique_inode(id)?,
                }
            } else {
                self.ino_allocator.get_unique_inode(id)?
            };
            // trace!("fuse: allocate inode: {} for id: {:?}", inode, id);
            Ok(inode)
        }
    }

    async fn do_lookup(
        &self,
        parent: Inode,
        name: &CStr,
    ) -> std::result::Result<ReplyEntry, Errno> {
        let name = if parent == ROOT_ID && name.to_bytes_with_nul().starts_with(PARENT_DIR_CSTR) {
            // Safe as this is a constant value and a valid C string.
            CStr::from_bytes_with_nul(CURRENT_DIR_CSTR).unwrap()
        } else {
            name
        };

        let dir = self.inode_map.get(parent).await?;
        let dir_file = dir.get_file()?;

        // macOS lazy mode: child path = parent's path joined with `name`.
        // The parent must itself be Reopenable (lazy mode applies to the whole
        // FS), so `lazy_path()` returns Some.
        #[cfg(target_os = "macos")]
        let lazy_abs_path = if self.cfg.macos_lazy_inode_fd {
            dir.lazy_path().map(|parent_path| {
                let name_os = std::ffi::OsStr::from_bytes(name.to_bytes());
                parent_path.join(name_os)
            })
        } else {
            None
        };

        let (inode_handle, st) = self
            .open_file_and_handle(
                &dir_file,
                name,
                #[cfg(target_os = "macos")]
                lazy_abs_path,
            )
            .await?;
        let id = InodeId::from_stat(&st);
        debug!(
            "do_lookup: parent: {}, name: {}, handle: {:?}, id: {:?}",
            parent,
            name.to_string_lossy(),
            inode_handle,
            id
        );

        let mut found = None;
        'search: loop {
            match self.inode_map.get_alt(&id, &inode_handle).await {
                // No existing entry found
                None => break 'search,
                Some(data) => {
                    let curr = data.refcount.load(Ordering::Acquire);
                    // forgot_one() has just destroyed the entry, retry...
                    if curr == 0 {
                        continue 'search;
                    }

                    // Saturating add to avoid integer overflow, it's not realistic to saturate u64.
                    let new = curr.saturating_add(1);

                    // Synchronizes with the forgot_one()
                    if data
                        .refcount
                        .compare_exchange(curr, new, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                    {
                        found = Some(data.inode);
                        break;
                    }
                }
            }
        }

        let inode = if let Some(v) = found {
            v
        } else {
            // Write guard get_alt_locked() and insert_lock() to avoid race conditions.
            let mut inodes = self.inode_map.inodes.write().await;

            // Lookup inode_map again after acquiring the inode_map lock, as there might be another
            // racing thread already added an inode with the same id while we're not holding
            // the lock. If so just use the newly added inode, otherwise the inode will be replaced
            // and results in EBADF.
            // trace!("FS {} looking up inode for id: {:?} with handle: {:?}", self.uuid, id, handle);
            match InodeMap::get_alt_locked(&inodes, &id, &inode_handle) {
                Some(data) => {
                    // An inode was added concurrently while we did not hold a lock on
                    // `self.inodes_map`, so we use that instead. `handle` will be dropped.
                    // trace!("FS {} found existing inode: {}", self.uuid, data.inode);
                    data.refcount.fetch_add(1, Ordering::Relaxed);
                    data.inode
                }
                None => {
                    let inode = self.allocate_inode(&inodes, &id, &inode_handle).await?;
                    // trace!("FS {} allocated new inode: {} for id: {:?}", self.uuid, inode, id);

                    if inode > VFS_MAX_INO {
                        error!("fuse: max inode number reached: {VFS_MAX_INO}");
                        return Err(io::Error::other(format!(
                            "max inode number reached: {VFS_MAX_INO}"
                        ))
                        .into());
                    }

                    InodeMap::insert_locked(
                        inodes.deref_mut(),
                        Arc::new(InodeData::new(
                            inode,
                            inode_handle,
                            1,
                            id,
                            st.st.st_mode.into(),
                            st.btime
                                .ok_or_else(|| io::Error::other("birth time not available"))?,
                        )),
                    );

                    inode
                }
            }
        };

        let (entry_timeout, _) = if is_dir(st.st.st_mode.into()) {
            (self.dir_entry_timeout, self.dir_attr_timeout)
        } else {
            (self.cfg.entry_timeout, self.cfg.attr_timeout)
        };

        // // Whether to enable file DAX according to the value of dax_file_size
        // let mut attr_flags: u32 = 0;
        // if let Some(dax_file_size) = self.cfg.dax_file_size {
        //     // st.stat.st_size is i64
        //     if self.perfile_dax.load().await
        //         && st.st.st_size >= 0x0
        //         && st.st.st_size as u64 >= dax_file_size
        //     {
        //         attr_flags |= FUSE_ATTR_DAX;
        //     }
        // }
        let mut attr_temp = convert_stat64_to_file_attr(st.st);
        attr_temp.ino = inode;
        attr_temp.uid = self.cfg.mapping.find_mapping(attr_temp.uid, true, true);
        attr_temp.gid = self.cfg.mapping.find_mapping(attr_temp.gid, true, false);
        Ok(ReplyEntry {
            ttl: entry_timeout,
            attr: attr_temp,
            generation: 0,
        })
    }

    async fn forget_one(&self, inodes: &mut InodeStore, inode: Inode, count: u64) {
        // ROOT_ID should not be forgotten, or we're not able to access to files any more.
        if inode == ROOT_ID {
            return;
        }

        if let Some(data) = inodes.get(&inode) {
            // Acquiring the write lock on the inode map prevents new lookups from incrementing the
            // refcount but there is the possibility that a previous lookup already acquired a
            // reference to the inode data and is in the process of updating the refcount so we need
            // to loop here until we can decrement successfully.
            loop {
                let curr = data.refcount.load(Ordering::Acquire);

                // Saturating sub because it doesn't make sense for a refcount to go below zero and
                // we don't want misbehaving clients to cause integer overflow.
                let new = curr.saturating_sub(count);

                // Synchronizes with the acquire load in `do_lookup`.
                if data
                    .refcount
                    .compare_exchange(curr, new, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    if new == 0 {
                        #[cfg(target_os = "linux")]
                        if data.handle.file_handle().is_some()
                            && (data.btime.tv_sec != 0 || data.btime.tv_nsec != 0)
                        {
                            let key = FileUniqueKey(data.id.ino, data.btime);
                            let cache = self.handle_cache.clone();
                            cache.invalidate(&key).await;
                        }
                        // Drop any LRU entry tracking this inode's lazy fd
                        // so capacity stays accurate. The cached `Arc<File>`
                        // (if any) drops with the `InodeData` once we
                        // remove from the map below.
                        #[cfg(target_os = "macos")]
                        if let Some(lru) = self.lazy_fd_lru.as_ref() {
                            lru.remove(inode);
                        }
                        // We just removed the last refcount for this inode.
                        // The allocated inode number should be kept in the map when use_host_ino
                        // is false or host inode(don't use the virtual 56bit inode) is bigger than MAX_HOST_INO.
                        let keep_mapping = !self.cfg.use_host_ino || data.id.ino > MAX_HOST_INO;
                        inodes.remove(&inode, keep_mapping);
                    }
                    break;
                }
            }
        }
    }

    async fn do_release(&self, inode: Inode, handle: Handle) -> io::Result<()> {
        self.handle_map.release(handle, inode).await
    }

    // Validate a path component, same as the one in vfs layer, but only do the validation if this
    // passthroughfs is used without vfs layer, to avoid double validation.
    fn validate_path_component(&self, name: &CStr) -> io::Result<()> {
        // !self.cfg.do_import means we're under vfs, and vfs has already done the validation
        if !self.cfg.do_import {
            return Ok(());
        }
        validate_path_component(name)
    }

    //TODO: When seal_size is set, we don't allow operations that could change file size nor allocate
    // space beyond EOF
    // fn seal_size_check(
    //     &self,
    //     opcode: Opcode,
    //     file_size: u64,
    //     offset: u64,
    //     size: u64,
    //     mode: i32,
    // ) -> io::Result<()> {
    //     if offset.checked_add(size).is_none() {
    //         error!(
    //             "fuse: {:?}: invalid `offset` + `size` ({}+{}) overflows u64::MAX",
    //             opcode, offset, size
    //         );
    //         return Err(einval());
    //     }

    //     match opcode {
    //         // write should not exceed the file size.
    //         Opcode::Write => {
    //             if size + offset > file_size {
    //                 return Err(eperm());
    //             }
    //         }

    //         Opcode::Fallocate => {
    //             let op = mode & !(libc::FALLOC_FL_KEEP_SIZE | libc::FALLOC_FL_UNSHARE_RANGE);
    //             match op {
    //                 // Allocate, punch and zero, must not change file size.
    //                 0 | libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_ZERO_RANGE => {
    //                     if size + offset > file_size {
    //                         return Err(eperm());
    //                     }
    //                 }
    //                 // collapse and insert will change file size, forbid.
    //                 libc::FALLOC_FL_COLLAPSE_RANGE | libc::FALLOC_FL_INSERT_RANGE => {
    //                     return Err(eperm());
    //                 }
    //                 // Invalid operation
    //                 _ => return Err(einval()),
    //             }
    //         }

    //         // setattr operation should be handled in setattr handler.
    //         _ => return Err(enosys()),
    //     }

    //     Ok(())
    // }

    async fn get_writeback_open_flags(&self, flags: i32) -> i32 {
        let mut new_flags = flags;
        let writeback = self.writeback.load(Ordering::Relaxed);

        // When writeback caching is enabled, the kernel may send read requests even if the
        // userspace program opened the file write-only. So we need to ensure that we have opened
        // the file for reading as well as writing.
        if writeback && flags & libc::O_ACCMODE == libc::O_WRONLY {
            new_flags &= !libc::O_ACCMODE;
            new_flags |= libc::O_RDWR;
        }

        // When writeback caching is enabled the kernel is responsible for handling `O_APPEND`.
        // However, this breaks atomicity as the file may have changed on disk, invalidating the
        // cached copy of the data in the kernel and the offset that the kernel thinks is the end of
        // the file. Just allow this for now as it is the user's responsibility to enable writeback
        // caching only for directories that are not shared. It also means that we need to clear the
        // `O_APPEND` flag.
        if writeback && flags & libc::O_APPEND != 0 {
            new_flags &= !libc::O_APPEND;
        }

        new_flags
    }

    async fn get_mmap(
        &self,
        inode: Inode,
        offset: u64,
        file: &File,
    ) -> Option<(Arc<RwLock<mmap::MmapCachedValue>>, u64)> {
        let file_size = file.metadata().unwrap().len();
        let key = MmapChunkKey::new(inode, offset, file_size);
        let aligned_offset = key.aligned_offset;

        if let Some(cached) = self.mmap_chunks.get(&key).await {
            let guard = cached.read().await;
            let cache_len = match &*guard {
                MmapCachedValue::Mmap(mmap) => mmap.len() as u64,
                MmapCachedValue::MmapMut(mmap_mut) => mmap_mut.len() as u64,
            };
            if offset < key.aligned_offset + cache_len {
                return Some((cached.clone(), key.aligned_offset));
            }
        }

        let mmap = match mmap::create_mmap(offset, file).await {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to create mmap:{e}");
                return None;
            }
        };
        self.mmap_chunks.insert(key, mmap.clone()).await;
        Some((mmap, aligned_offset))
    }

    async fn read_from_mmap(
        &self,
        inode: Inode,
        offset: u64,
        size: u64,
        file: &File,
        buf: &mut [u8],
    ) -> Result<usize> {
        // check the buf size
        if buf.len() < size as usize {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("Buffer too small: {} < {}", buf.len(), size),
            ));
        }

        let file_size = file.metadata()?.len();

        // check the offset
        if offset >= file_size {
            return Ok(0); // offset exceeds file size, return 0 bytes read
        }

        // compute the maximum readable length
        let max_readable = file_size - offset;
        let actual_size = cmp::min(size, max_readable) as usize;

        let mut len = actual_size;
        let mut current_offset = offset;
        let mut buf_offset = 0;

        while len > 0 {
            let (chunk, chunk_start_offset) = match self.get_mmap(inode, current_offset, file).await
            {
                Some((chunk, aligned_offset)) => (chunk, aligned_offset),
                None => {
                    return Err(std::io::Error::other("Failed to get mmap chunk"));
                }
            };

            let chunk_guard = chunk.read().await;
            match &*chunk_guard {
                MmapCachedValue::Mmap(mmap) => {
                    let chunk_len = mmap.len();

                    // compute the start offset within the chunk using cached alignment
                    let copy_start = (current_offset - chunk_start_offset) as usize;

                    // ensure we don't read beyond the chunk boundary
                    let remaining_in_chunk = chunk_len - copy_start;
                    let copy_len = cmp::min(len, remaining_in_chunk);

                    // ensure we don't read beyond the buffer boundary
                    let copy_len = cmp::min(copy_len, buf.len() - buf_offset);

                    if copy_len == 0 {
                        break; // no more data to read
                    }

                    // execute data copy
                    buf[buf_offset..buf_offset + copy_len]
                        .copy_from_slice(&mmap[copy_start..copy_start + copy_len]);

                    buf_offset += copy_len;
                    len -= copy_len;
                    current_offset += copy_len as u64;
                }
                MmapCachedValue::MmapMut(mmap_mut) => {
                    let chunk_len = mmap_mut.len();

                    // compute the start offset within the chunk using cached alignment
                    let copy_start = (current_offset - chunk_start_offset) as usize;

                    // ensure we don't read beyond the chunk boundary
                    let remaining_in_chunk = chunk_len - copy_start;
                    let copy_len = cmp::min(len, remaining_in_chunk);

                    // ensure we don't read beyond the buffer boundary
                    let copy_len = cmp::min(copy_len, buf.len() - buf_offset);

                    if copy_len == 0 {
                        break; // no more data to read
                    }

                    // execute data copy
                    buf[buf_offset..buf_offset + copy_len]
                        .copy_from_slice(&mmap_mut[copy_start..copy_start + copy_len]);

                    buf_offset += copy_len;
                    len -= copy_len;
                    current_offset += copy_len as u64;
                }
            }
        }
        Ok(buf_offset)
    }

    async fn write_to_mmap(
        &self,
        inode: Inode,
        offset: u64,
        data: &[u8],
        file: &File,
    ) -> Result<usize> {
        let file_size = file.metadata()?.len();
        let len = data.len();

        // If the file needs to be extended, do so
        if offset + len as u64 > file_size {
            let raw_fd = file.as_raw_fd();
            let res = unsafe { libc::ftruncate(raw_fd, (offset + len as u64) as i64) };

            if res < 0 {
                return Err(std::io::Error::other("error to ftruncate"));
            }

            self.invalidate_mmap_cache(inode, file_size).await;
        }

        let mut remaining = len;
        let mut current_offset = offset;
        let mut data_offset = 0;

        while remaining > 0 {
            let (chunk, chunk_start_offset) = match self.get_mmap(inode, current_offset, file).await
            {
                Some((chunk, aligned_offset)) => (chunk, aligned_offset),
                None => {
                    return Err(std::io::Error::other("Failed to get mmap chunk"));
                }
            };

            let mut chunk_guard = chunk.write().await;
            match &mut *chunk_guard {
                MmapCachedValue::Mmap(_) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Cannot write to read-only mmap",
                    ));
                }
                MmapCachedValue::MmapMut(mmap_mut) => {
                    let chunk_len = mmap_mut.len();

                    // Calculate the start position of the current chunk using cached alignment
                    let copy_start = (current_offset - chunk_start_offset) as usize;

                    // Ensure we don't write beyond the chunk boundary
                    let remaining_in_chunk = chunk_len - copy_start;
                    let copy_len = cmp::min(remaining, remaining_in_chunk);

                    // Ensure we don't write beyond the data boundary
                    let copy_len = cmp::min(copy_len, data.len() - data_offset);

                    if copy_len == 0 {
                        break; // No more data to write
                    }

                    // Perform data copy
                    mmap_mut[copy_start..copy_start + copy_len]
                        .copy_from_slice(&data[data_offset..data_offset + copy_len]);

                    data_offset += copy_len;
                    remaining -= copy_len;
                    current_offset += copy_len as u64;
                    mmap_mut.flush_async_range(copy_start, copy_len)?;
                }
            }
        }
        Ok(data_offset)
    }

    async fn invalidate_mmap_cache(&self, inode: Inode, old_size: u64) {
        let keys_to_remove: Vec<_> = self
            .mmap_chunks
            .iter()
            .filter(|item| {
                let key = item.0.clone();
                key.inode == inode && key.aligned_offset + mmap::MAX_WINDOW_SIZE as u64 >= old_size
            })
            .collect();

        for item in keys_to_remove {
            self.mmap_chunks.invalidate(item.0.as_ref()).await;
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
#[allow(clippy::useless_conversion)]
mod tests {
    use crate::{
        passthrough::{PassthroughArgs, PassthroughFs, ROOT_ID, new_passthroughfs_layer},
        unwrap_or_skip_eperm, unwrap_or_skip_mount_error,
    };
    use std::ffi::{CStr, OsStr, OsString};

    use nix::unistd::{Gid, Uid, getgid, getuid};
    use rfuse3::{
        MountOptions,
        raw::{Filesystem, Request, Session},
    };

    macro_rules! pass {
        () => {
            ()
        };
        ($($tt:tt)*) => {
            ()
        };
    }

    #[test]
    fn nofile_limit_raise_is_capped_by_hard_limit() {
        assert_eq!(
            super::desired_nofile_soft_limit(256, 4096, 8192),
            Some(4096)
        );
        assert_eq!(
            super::desired_nofile_soft_limit(256, 16384, 8192),
            Some(8192)
        );
        assert_eq!(super::desired_nofile_soft_limit(8192, 16384, 8192), None);
    }

    #[cfg(target_os = "macos")]
    struct MacFuseMountCleanup {
        mount_dir: std::path::PathBuf,
    }

    #[cfg(target_os = "macos")]
    impl Drop for MacFuseMountCleanup {
        fn drop(&mut self) {
            let _ = std::process::Command::new("umount")
                .arg(&self.mount_dir)
                .status();
            let _ = std::process::Command::new("diskutil")
                .arg("unmount")
                .arg("force")
                .arg(&self.mount_dir)
                .status();
        }
    }

    /// This test attempts to mount a passthrough filesystem. It is explicitly
    /// gated because macFUSE availability depends on local kext approval and
    /// should not affect the default unit-test layer.
    #[tokio::test]
    async fn test_passthrough() {
        if std::env::var("RUN_MACFUSE_TESTS").ok().as_deref() != Some("1") {
            eprintln!("skip test_passthrough: RUN_MACFUSE_TESTS!=1");
            return;
        }

        let temp_dir = tempfile::tempdir().expect("tempdir");
        let source_dir = temp_dir.path().join("src");
        let mount_dir = temp_dir.path().join("mnt");
        std::fs::create_dir_all(&source_dir).expect("create source dir");
        std::fs::create_dir_all(&mount_dir).expect("create mount dir");
        #[cfg(target_os = "macos")]
        let _cleanup = MacFuseMountCleanup {
            mount_dir: mount_dir.clone(),
        };

        let args = PassthroughArgs {
            root_dir: source_dir.clone(),
            mapping: None::<&str>,
        };
        let fs = match super::new_passthroughfs_layer(args).await {
            Ok(fs) => fs,
            Err(e) => {
                eprintln!("skip test_passthrough: init failed: {e:?}");
                return;
            }
        };

        let uid = unsafe { libc::getuid() };
        let gid = unsafe { libc::getgid() };

        let mut mount_options = MountOptions::default();
        #[cfg(target_os = "linux")]
        mount_options.force_readdir_plus(true);
        mount_options.uid(uid).gid(gid);
        // Intentionally DO NOT call allow_other here to avoid requiring /etc/fuse.conf config.

        let mount_path = OsString::from(mount_dir.as_os_str());

        let session = Session::new(mount_options);
        let mount_handle = unwrap_or_skip_mount_error!(
            session.mount(fs, mount_path).await,
            "mount passthrough fs"
        );

        // Immediately unmount to verify we at least mounted successfully.
        let _ = mount_handle.unmount().await; // errors ignored
    }

    #[tokio::test]
    async fn lookup_rejects_nul_name_without_panicking() {
        use rfuse3::raw::{Filesystem, Request};
        use std::os::unix::ffi::OsStrExt;

        let temp_dir = tempfile::tempdir().unwrap();
        let fs = new_passthroughfs_layer(PassthroughArgs {
            root_dir: temp_dir.path(),
            mapping: None::<&str>,
        })
        .await
        .unwrap();

        let err = fs
            .lookup(Request::default(), ROOT_ID, OsStr::from_bytes(b"bad\0name"))
            .await
            .unwrap_err();
        let ioerr = std::io::Error::from(err);
        assert_eq!(ioerr.raw_os_error(), Some(libc::EINVAL));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_lazy_new_canonicalizes_root_dir() {
        use super::Config;
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        let real_root = temp_dir.path().join("real-root");
        let link_root = temp_dir.path().join("link-root");
        std::fs::create_dir(&real_root).unwrap();
        symlink(&real_root, &link_root).unwrap();

        let cfg = Config {
            root_dir: link_root.clone(),
            macos_lazy_inode_fd: true,
            ..Default::default()
        };
        let fs = PassthroughFs::<()>::new(cfg).expect("new fs");

        assert_eq!(fs.cfg.root_dir, real_root.canonicalize().unwrap());
        assert_ne!(fs.cfg.root_dir, link_root);
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_lookup_symlink_entry_does_not_return_eloop() {
        use std::os::unix::fs::symlink;

        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("target.txt"), "target").unwrap();
        symlink("target.txt", temp_dir.path().join("link.txt")).unwrap();

        let fs = new_passthroughfs_layer(PassthroughArgs {
            root_dir: temp_dir.path(),
            mapping: None::<&str>,
        })
        .await
        .unwrap();
        let name = c"link.txt";

        let entry = fs.do_lookup(ROOT_ID, name).await.unwrap();

        assert_eq!(entry.attr.kind, rfuse3::FileType::Symlink);
    }

    /// PR-9.3 finding: `O_NOFOLLOW_ANY` is *not* a drop-in upgrade for
    /// `O_NOFOLLOW` on macOS — it conflicts with combining and rejects
    /// `/tmp`-rooted paths because `/tmp` itself is a symlink. Instead
    /// of asserting a no-op behaviour, this regression guard documents
    /// the lazy-fd happy path: a file directly under tmpdir opens, and
    /// trailing-symlink retry still kicks in.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_lazy_open_path_two_step_works() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("file.txt"), b"PR93").unwrap();
        symlink("file.txt", temp.path().join("link.txt")).unwrap();

        // Regular file: O_NOFOLLOW path returns the fd.
        let file_c = CString::new(temp.path().join("file.txt").as_os_str().as_bytes()).unwrap();
        let fd = super::lazy_open_path(&file_c, libc::O_RDONLY).expect("regular open failed");
        assert!(fd >= 0);
        unsafe { libc::close(fd) };

        // Trailing-symlink: ELOOP on first try → O_SYMLINK retry returns
        // an fd to the link itself.
        let link_c = CString::new(temp.path().join("link.txt").as_os_str().as_bytes()).unwrap();
        let fd = super::lazy_open_path(&link_c, libc::O_RDONLY).expect("symlink retry path failed");
        assert!(fd >= 0);
        unsafe { libc::close(fd) };
    }

    /// Verifies that renaming a cached directory rewrites the cached
    /// `lazy_path` of every descendant, so post-eviction reopens land at
    /// the new location instead of `ENOENT` at the old one.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_lazy_dir_rename_rewrites_descendants() {
        use super::Config;
        use rfuse3::raw::Request;
        use std::ffi::OsStr;

        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp_dir.path().join("a")).unwrap();
        std::fs::create_dir(temp_dir.path().join("a/sub")).unwrap();
        std::fs::write(temp_dir.path().join("a/sub/file.txt"), b"hi").unwrap();

        let cfg = Config {
            root_dir: temp_dir.path().to_path_buf(),
            xattr: true,
            do_import: true,
            macos_lazy_inode_fd: true,
            ..Default::default()
        };
        let fs = PassthroughFs::<()>::new(cfg).expect("new fs");
        fs.import().await.unwrap();

        // Walk the tree to populate cached `lazy_path` on every node.
        let a_entry = fs.do_lookup(ROOT_ID, c"a").await.unwrap();
        let sub_entry = fs.do_lookup(a_entry.attr.ino, c"sub").await.unwrap();
        let file_entry = fs.do_lookup(sub_entry.attr.ino, c"file.txt").await.unwrap();

        // Drive the FUSE rename trait directly — it issues the underlying
        // `renameat(2)` and runs the lazy-path descendant walk.
        use rfuse3::raw::Filesystem;
        fs.rename(
            Request::default(),
            ROOT_ID,
            OsStr::new("a"),
            ROOT_ID,
            OsStr::new("b"),
        )
        .await
        .unwrap();

        // Every descendant must now resolve under "/…/b/sub/...".
        let new_root = temp_dir.path().canonicalize().unwrap();
        for ino in [a_entry.attr.ino, sub_entry.attr.ino, file_entry.attr.ino] {
            let data = fs.inode_map.get(ino).await.unwrap();
            let path = data.lazy_path().expect("Reopenable on macOS lazy mode");
            assert!(
                path.starts_with(new_root.join("b")),
                "inode {ino} path {path:?} should be under {:?} after rename",
                new_root.join("b"),
            );
        }
    }

    /// Verifies the lazy-fd LRU bounds the cache at the configured cap and
    /// that exceeding it evicts the LRU entry. Drives `do_lookup` directly
    /// (no real mount) so the cache populate path runs through `get_file()`.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_lazy_fd_lru_bounds_cache() {
        use super::Config;
        use std::num::NonZeroUsize;

        let temp_dir = tempfile::tempdir().unwrap();
        // 4 sibling files; cap=2 so the 3rd lookup must evict.
        for i in 0..4 {
            std::fs::write(temp_dir.path().join(format!("f{i}.txt")), b"x").unwrap();
        }

        let cfg = Config {
            root_dir: temp_dir.path().to_path_buf(),
            xattr: true,
            do_import: true,
            macos_lazy_inode_fd: true,
            macos_lazy_fd_lru_max: Some(NonZeroUsize::new(2).unwrap()),
            ..Default::default()
        };
        let fs = PassthroughFs::<()>::new(cfg).expect("new fs");
        fs.import().await.unwrap();

        assert_eq!(fs.macos_lazy_fd_cap(), Some(2));

        // Look up + force-open each child to populate the lazy cache.
        for i in 0..4 {
            let name = OsString::from(format!("f{i}.txt"));
            let bytes: Vec<u8> = name
                .as_os_str()
                .as_encoded_bytes()
                .iter()
                .copied()
                .chain(std::iter::once(0))
                .collect();
            let cname = CStr::from_bytes_with_nul(&bytes).unwrap();
            let entry = fs.do_lookup(ROOT_ID, cname).await.unwrap();
            // get_file() is what populates the lazy cache; emulate one
            // hot-path read so the LRU records this inode.
            let inode = entry.attr.ino;
            let data = fs.inode_map.get(inode).await.unwrap();
            let _ = data.get_file().unwrap();
        }

        let len = fs.macos_lazy_fd_cache_len().expect("lru enabled");
        let reopens = fs.macos_lazy_fd_reopen_count().expect("lru enabled");
        // 4 lookups all populated cache → 4 reopens. Cache length is bounded
        // by cap (2). Exact LRU order isn't asserted to keep the test
        // resilient against insertion-order quirks.
        assert!(
            len <= 2,
            "cache length {len} exceeded cap 2 — LRU eviction is broken",
        );
        assert!(
            reopens >= 4,
            "expected ≥4 reopens, saw {reopens} — counter not bumping",
        );
    }

    /// Stress: 200 files, cap=8, ensures the LRU caps both the in-memory
    /// cache **and** real OS fd usage, and that `forget_one()` releases
    /// LRU entries when refcount hits zero.
    ///
    /// Counts process fds via `/dev/fd` (macOS) — every entry under that
    /// directory is an open fd. After populating the cache we expect the
    /// per-inode fd cost to be capped at `cap`; after forget-all, fd usage
    /// should drop back near baseline.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_lazy_fd_pressure_caps_real_fds() {
        use super::Config;
        use std::num::NonZeroUsize;

        const FILES: usize = 200;
        const CAP: usize = 8;
        // Allow a small slack for ancillary fds (libfuse-fs itself opens
        // a handful: proc_self_fd, mount_fds, mmap chunks, allocator pools,
        // tokio runtime pipes, …). 32 covers observed steady-state churn.
        const FD_SLACK: usize = 32;

        let temp_dir = tempfile::tempdir().unwrap();
        for i in 0..FILES {
            std::fs::write(temp_dir.path().join(format!("f{i:04}.txt")), b"x").unwrap();
        }

        let cfg = Config {
            root_dir: temp_dir.path().to_path_buf(),
            xattr: true,
            do_import: true,
            macos_lazy_inode_fd: true,
            macos_lazy_fd_lru_max: Some(NonZeroUsize::new(CAP).unwrap()),
            ..Default::default()
        };
        let fs = PassthroughFs::<()>::new(cfg).expect("new fs");
        fs.import().await.unwrap();

        // Baseline fd count *after* fs construction so we factor out the
        // ancillary fds that PassthroughFs::new opens (proc_self_fd, etc.).
        let baseline_fds = count_open_fds();

        let mut inodes = Vec::with_capacity(FILES);
        for i in 0..FILES {
            let name = format!("f{i:04}.txt");
            let bytes: Vec<u8> = name
                .as_bytes()
                .iter()
                .copied()
                .chain(std::iter::once(0))
                .collect();
            let cname = CStr::from_bytes_with_nul(&bytes).unwrap();
            let entry = fs.do_lookup(ROOT_ID, cname).await.unwrap();
            let inode = entry.attr.ino;
            // Force the lazy-fd path to open + cache.
            let data = fs.inode_map.get(inode).await.unwrap();
            let _ = data.get_file().unwrap();
            inodes.push(inode);
        }

        let after_lookup_fds = count_open_fds();
        let cache_len = fs.macos_lazy_fd_cache_len().unwrap();
        let reopens = fs.macos_lazy_fd_reopen_count().unwrap();
        assert_eq!(
            cache_len, CAP,
            "cache should saturate at cap={CAP}, saw {cache_len}",
        );
        assert!(
            reopens as usize >= FILES,
            "expected ≥ {FILES} reopens (one per lookup), saw {reopens}",
        );
        assert!(
            after_lookup_fds <= baseline_fds + CAP + FD_SLACK,
            "fd usage exploded: baseline={baseline_fds}, after={after_lookup_fds}, \
             cap={CAP}, slack={FD_SLACK}",
        );

        // Forget every inode and confirm LRU drains and fd count returns
        // near baseline.
        let mut store = fs.inode_map.inodes.write().await;
        for inode in &inodes {
            fs.forget_one(&mut store, *inode, 1).await;
        }
        drop(store);

        let after_forget_fds = count_open_fds();
        let final_cache_len = fs.macos_lazy_fd_cache_len().unwrap();
        assert_eq!(
            final_cache_len, 0,
            "LRU should drain after forget-all, saw {final_cache_len}",
        );
        assert!(
            after_forget_fds <= baseline_fds + FD_SLACK,
            "fd usage didn't drop after forget-all: baseline={baseline_fds}, \
             after_forget={after_forget_fds}, slack={FD_SLACK}",
        );
    }

    /// Counts entries under `/dev/fd`, which on macOS exposes the calling
    /// process's open file descriptors. Used by the fd-pressure stress
    /// test to assert real (not just LRU-internal) fd accounting.
    #[cfg(target_os = "macos")]
    fn count_open_fds() -> usize {
        std::fs::read_dir("/dev/fd")
            .map(|d| d.filter_map(|e| e.ok()).count())
            .unwrap_or(0)
    }

    // ----- macOS-only opcodes (PR-7.2) ----------------------------------

    /// `setvolname` accepts and ignores. We only verify the trait dispatch
    /// resolves to a successful response (not the trait default's ENOSYS).
    /// A real Finder-level test would need a mounted volume + admin
    /// privileges.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_setvolname_accepts_and_returns_ok() {
        use rfuse3::raw::{Filesystem, Request};
        use std::ffi::OsStr;

        let temp_dir = tempfile::tempdir().unwrap();
        let fs = new_passthroughfs_layer(PassthroughArgs {
            root_dir: temp_dir.path(),
            mapping: None::<&str>,
        })
        .await
        .unwrap();
        let res = fs
            .setvolname(Request::default(), OsStr::new("MyVolume"))
            .await;
        assert!(
            res.is_ok(),
            "setvolname must not return ENOSYS, got {res:?}"
        );
    }

    /// `getxtimes` returns `st_birthtimespec` for both fields. Test creates
    /// a file, looks it up, then queries getxtimes and compares against
    /// the on-disk creation time.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_getxtimes_reports_creation_time() {
        use rfuse3::raw::{Filesystem, Request};

        let temp_dir = tempfile::tempdir().unwrap();
        let target = temp_dir.path().join("birthcheck.txt");
        std::fs::write(&target, b"hi").unwrap();

        let fs = new_passthroughfs_layer(PassthroughArgs {
            root_dir: temp_dir.path(),
            mapping: None::<&str>,
        })
        .await
        .unwrap();
        let cname = c"birthcheck.txt";
        let entry = fs.do_lookup(ROOT_ID, cname).await.unwrap();
        let times = fs
            .getxtimes(Request::default(), entry.attr.ino)
            .await
            .expect("getxtimes must not return ENOSYS");

        // crtime sec must be > 0 and equal across both fields. We don't
        // assert an exact value (filesystem time precision varies).
        assert_eq!(times.bkuptime, times.crtime);
        assert!(
            times.crtime.sec > 0,
            "crtime should be a real birthtime, got {:?}",
            times.crtime,
        );
    }

    /// `exchange` must atomically swap two siblings in place. After the
    /// swap, the inode bound to "a" should hold the contents that used
    /// to be at "b" and vice versa.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_exchange_swaps_two_siblings() {
        use rfuse3::raw::{Filesystem, Request};
        use std::ffi::OsStr;

        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("a.txt"), b"A_PAYLOAD").unwrap();
        std::fs::write(temp_dir.path().join("b.txt"), b"B_PAYLOAD").unwrap();

        let fs = new_passthroughfs_layer(PassthroughArgs {
            root_dir: temp_dir.path(),
            mapping: None::<&str>,
        })
        .await
        .unwrap();

        fs.exchange(
            Request::default(),
            ROOT_ID,
            OsStr::new("a.txt"),
            ROOT_ID,
            OsStr::new("b.txt"),
            0,
        )
        .await
        .expect("exchange must not return ENOSYS");

        // After RENAME_SWAP: a.txt now holds B_PAYLOAD; b.txt holds A_PAYLOAD.
        let after_a = std::fs::read(temp_dir.path().join("a.txt")).unwrap();
        let after_b = std::fs::read(temp_dir.path().join("b.txt")).unwrap();
        assert_eq!(
            after_a, b"B_PAYLOAD",
            "exchange did not move B's content to a.txt"
        );
        assert_eq!(
            after_b, b"A_PAYLOAD",
            "exchange did not move A's content to b.txt"
        );
    }

    /// Resource fork xattrs are the one macOS xattr namespace where the
    /// kernel-supplied position matters. Preserve it instead of flattening
    /// every write to offset 0.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn macos_resource_fork_xattr_honors_position() {
        use rfuse3::raw::{Filesystem, Request};
        use std::ffi::OsStr;

        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("forked.txt"), b"data").unwrap();

        let fs = new_passthroughfs_layer(PassthroughArgs {
            root_dir: temp_dir.path(),
            mapping: None::<&str>,
        })
        .await
        .unwrap();
        let entry = fs.do_lookup(ROOT_ID, c"forked.txt").await.unwrap();
        let attr = OsStr::new("com.apple.ResourceFork");

        fs.setxattr(Request::default(), entry.attr.ino, attr, b"abcd", 0, 0)
            .await
            .unwrap();
        fs.setxattr(Request::default(), entry.attr.ino, attr, b"EF", 0, 2)
            .await
            .unwrap();

        let data = fs
            .getxattr(Request::default(), entry.attr.ino, attr, 4)
            .await
            .unwrap();
        match data {
            rfuse3::raw::reply::ReplyXAttr::Data(bytes) => assert_eq!(&bytes[..], b"abEF"),
            other => panic!("expected resource-fork data, got {other:?}"),
        }
    }

    // // Test for uid/gid mapping
    // async fn setup(
    //     mapping: Option<&str>,
    // ) -> (PassthroughFs, tempfile::TempDir, Uid, Gid, Uid, Gid) {
    //     let tmp_dir = tempfile::tempdir().unwrap();
    //     let src_dir = tmp_dir.path();

    //     let cur_uid = getuid();
    //     let cur_gid = getgid();

    //     let container_uid = Uid::from_raw(1000);
    //     let container_gid = Gid::from_raw(1000);

    //     let args = PassthroughArgs {
    //         root_dir: src_dir.to_path_buf(),
    //         mapping: mapping,
    //     };
    //     let fs = new_passthroughfs_layer(args).await.unwrap();

    //     (fs, tmp_dir, cur_uid, cur_gid, container_uid, container_gid)
    // }

    /// Tests the reverse mapping (host -> container) for `lookup` and `getattr` operations.
    ///
    /// It sets up a mapping from the current host user to a container user (UID/GID 1000).
    /// Then, it creates a file owned by the host user and verifies that when FUSE looks up
    /// or gets attributes for this file, the returned UID/GID are correctly mapped to 1000.
    ///
    /// Unfortunately, this can not work because `do_lookup` calls `to_openable_handle` which
    /// requires CAP_DAC_READ_SEARCH capability, which is not available in unprivileged test environments.
    /// So this test is commented out for now.
    #[tokio::test]
    async fn test_lookup_and_getattr() {
        pass!()
    }
    // async fn test_lookup_and_getattr() {
    //     let cur_uid = getuid().as_raw();
    //     let cur_gid = getgid().as_raw();
    //     let mapping = format!("uidmapping={cur_uid}:1000:1,gidmapping={cur_gid}:1000:1");

    //     let (fs, tmp_dir, ..) = setup(Some(&mapping)).await;
    //     let src = tmp_dir.path();

    //     // Create a file in the source directory, owned by the current host user.
    //     let file_path = src.join("test_file.txt");
    //     std::fs::File::create(&file_path).unwrap();
    //     std::os::unix::fs::chown(&file_path, Some(cur_uid), Some(cur_gid)).unwrap();

    //     // Simulate a FUSE request from the container user (UID/GID 1000).
    //     let req = Request::default();
    //     // Perform a lookup, which should trigger attribute fetching.
    //     let reply = fs
    //         .do_lookup(
    //             ROOT_ID,
    //             CStr::from_bytes_with_nul(b"test_file.txt\0").unwrap(),
    //         )
    //         .await
    //         .unwrap();

    //     // Verify that the returned attributes are mapped to the container's perspective.
    //     assert_eq!(reply.attr.uid, 1000);
    //     assert_eq!(reply.attr.gid, 1000);

    //     // Explicitly call getattr and verify the same mapping logic.
    //     let getattr_reply = fs.getattr(req, reply.attr.ino, None, 0).await.unwrap();
    //     assert_eq!(getattr_reply.attr.uid, 1000);
    //     assert_eq!(getattr_reply.attr.gid, 1000);
    // }

    /// Tests the forward mapping (container -> host) for the `create` operation.
    ///
    /// It sets up a mapping from the current host user to a container user (UID/GID 1000).
    /// It then simulates a `create` request from the container user and verifies two things:
    /// 1. The newly created file on the host filesystem is owned by the mapped host user.
    /// 2. The attributes returned in the FUSE reply are correctly mapped back to the container user's ID.
    #[tokio::test]
    async fn test_create() {
        pass!()
    }
    // #[tokio::test]
    // async fn test_create() {
    //     let cur_uid = getuid().as_raw();
    //     let cur_gid = getgid().as_raw();
    //     let mapping = format!("uidmapping={cur_uid}:1000:1,gidmapping={cur_gid}:1000:1");

    //     let (fs, tmp_dir, host_uid, host_gid, container_uid, container_gid) =
    //         setup(Some(&mapping)).await;

    //     // Simulate a request coming from the container user (1000).
    //     let mut req = Request::default();
    //     req.uid = container_uid.as_raw();
    //     req.gid = container_gid.as_raw();

    //     let file_name = OsStr::new("new_file.txt");
    //     let mode = libc::S_IFREG | 0o644;

    //     // Perform the create operation.
    //     let created_reply = fs
    //         .create(req, ROOT_ID, file_name, mode, libc::O_CREAT as u32)
    //         .await
    //         .unwrap();

    //     let file_path = tmp_dir.path().join(file_name);
    //     let metadata = std::fs::metadata(file_path).unwrap();

    //     // Verify forward mapping: the file owner on the host should be the mapped host user.
    //     use std::os::unix::fs::MetadataExt;
    //     assert_eq!(Uid::from_raw(metadata.uid()), host_uid);
    //     assert_eq!(Gid::from_raw(metadata.gid()), host_gid);

    //     // Verify reverse mapping in the reply: the attributes sent back to the container
    //     // should reflect the container's user ID.
    //     assert_eq!(created_reply.attr.uid, container_uid.as_raw());
    //     assert_eq!(created_reply.attr.gid, container_gid.as_raw());
    // }
}
