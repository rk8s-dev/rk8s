//! FileSystem layer: Path-based file system operations (JuiceFS pkg/fs/fs.go style).
//!
//! This module provides a higher-level API that wraps VFS:
//! - Path-based operations with symlink handling
//! - `File` wrapper backed by VFS handles
//! - `FileStat` for file metadata (similar to os.FileInfo)
//! - Optional access logging and permission checks
//!
//! The design mirrors JuiceFS's FileSystem layer while delegating core I/O
//! and handle lifecycle to VFS.

use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::BlockStore;
use crate::meta::MetaStore;
use crate::meta::client::MetaClient;
use crate::meta::config::MetaClientConfig;
use crate::meta::layer::MetaLayer;
use crate::meta::permission::Permission;
use crate::meta::store::{
    DirEntry, FileAttr, FileType, MetaError, SetAttrFlags, SetAttrRequest, StatFsSnapshot,
};
use crate::vfs::fs::VFS;
use libc::{getegid, geteuid, getgroups};
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tracing::info;

/// File system statistics similar to POSIX statvfs.
#[derive(Debug, Clone)]
pub struct StatFs {
    pub total_space: u64,
    pub avail_space: u64,
    pub used_space: u64,
    pub total_inodes: u64,
    pub avail_inodes: u64,
    pub used_inodes: u64,
}

impl From<StatFsSnapshot> for StatFs {
    fn from(s: StatFsSnapshot) -> Self {
        Self {
            total_space: s.total_space,
            avail_space: s.available_space,
            used_space: s.total_space.saturating_sub(s.available_space),
            total_inodes: s.used_inodes.saturating_add(s.available_inodes),
            avail_inodes: s.available_inodes,
            used_inodes: s.used_inodes,
        }
    }
}

/// File metadata similar to `os.FileInfo` in Go.
#[derive(Debug, Clone)]
pub struct FileStat {
    name: String,
    inode: i64,
    attr: FileAttr,
}

impl FileStat {
    pub fn new(name: String, inode: i64, attr: FileAttr) -> Self {
        Self { name, inode, attr }
    }

    pub fn inode(&self) -> i64 {
        self.inode
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn size(&self) -> u64 {
        self.attr.size
    }

    pub fn mode(&self) -> u32 {
        self.attr.mode
    }

    pub fn mod_time(&self) -> SystemTime {
        let nanos = self.attr.mtime as u64;
        UNIX_EPOCH + Duration::from_nanos(nanos)
    }

    pub fn access_time(&self) -> SystemTime {
        let nanos = self.attr.atime as u64;
        UNIX_EPOCH + Duration::from_nanos(nanos)
    }

    pub fn is_dir(&self) -> bool {
        self.attr.kind == FileType::Dir
    }

    pub fn is_file(&self) -> bool {
        self.attr.kind == FileType::File
    }

    pub fn is_symlink(&self) -> bool {
        self.attr.kind == FileType::Symlink
    }

    pub fn file_type(&self) -> FileType {
        self.attr.kind
    }

    pub fn uid(&self) -> u32 {
        self.attr.uid
    }

    pub fn gid(&self) -> u32 {
        self.attr.gid
    }

    pub fn nlink(&self) -> u32 {
        self.attr.nlink
    }

    pub fn attr(&self) -> &FileAttr {
        &self.attr
    }
}

/// Open file flags.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub append: bool,
    pub create: bool,
    pub truncate: bool,
    pub exclusive: bool,
}

impl OpenFlags {
    pub const fn read_only() -> Self {
        Self {
            read: true,
            write: false,
            append: false,
            create: false,
            truncate: false,
            exclusive: false,
        }
    }

    pub const fn write_only() -> Self {
        Self {
            read: false,
            write: true,
            append: false,
            create: false,
            truncate: false,
            exclusive: false,
        }
    }

    pub const fn read_write() -> Self {
        Self {
            read: true,
            write: true,
            append: false,
            create: false,
            truncate: false,
            exclusive: false,
        }
    }

    pub const fn create_write() -> Self {
        Self {
            read: false,
            write: true,
            append: false,
            create: true,
            truncate: false,
            exclusive: false,
        }
    }

    pub const fn create_new() -> Self {
        Self {
            read: false,
            write: true,
            append: false,
            create: true,
            truncate: false,
            exclusive: true,
        }
    }

    pub fn with_append(mut self) -> Self {
        self.append = true;
        self
    }

    pub fn with_truncate(mut self) -> Self {
        self.truncate = true;
        self
    }
}

/// Access log entry for tracking file operations.
#[derive(Debug, Clone)]
pub struct AccessLogEntry {
    pub timestamp: SystemTime,
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
    pub op: String,
    pub path: String,
    pub result: String,
    pub duration_us: u64,
}

impl std::fmt::Display for AccessLogEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ts = self
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        write!(
            f,
            "{:.6} [uid:{},gid:{},pid:{}] {} ({}) -> {} <{:.6}s>",
            ts,
            self.uid,
            self.gid,
            self.pid,
            self.op,
            self.path,
            self.result,
            self.duration_us as f64 / 1_000_000.0
        )
    }
}

/// Log context for tracking operation timing.
pub struct LogContext {
    pub uid: u32,
    pub gid: u32,
    pub pid: u32,
    start: std::time::Instant,
}

impl LogContext {
    pub fn new(uid: u32, gid: u32, pid: u32) -> Self {
        Self {
            uid,
            gid,
            pid,
            start: std::time::Instant::now(),
        }
    }

    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }
}

fn make_log_context() -> LogContext {
    // SAFETY: geteuid/getegid are thread-safe libc calls and do not dereference pointers.
    let uid = unsafe { geteuid() as u32 };
    let gid = unsafe { getegid() as u32 };
    let pid = std::process::id();
    LogContext::new(uid, gid, pid)
}

fn current_ids() -> (u32, u32) {
    // SAFETY: geteuid/getegid are thread-safe libc calls and do not dereference pointers.
    let uid = unsafe { geteuid() as u32 };
    let gid = unsafe { getegid() as u32 };
    (uid, gid)
}

#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub uid: u32,
    pub gid: u32,
    pub groups: Vec<u32>,
}

impl CallerIdentity {
    pub fn new(uid: u32, gid: u32, groups: Vec<u32>) -> Self {
        Self { uid, gid, groups }
    }

    pub fn current() -> Self {
        let (uid, gid) = current_ids();
        let groups = current_groups(gid);
        Self { uid, gid, groups }
    }

    pub fn root() -> Self {
        Self {
            uid: 0,
            gid: 0,
            groups: vec![0],
        }
    }
}

fn current_groups(primary_gid: u32) -> Vec<u32> {
    let mut groups = Vec::new();
    // SAFETY: getgroups follows POSIX semantics. We first query the count with a null pointer,
    // then allocate a buffer of that size and pass a valid pointer for the second call.
    unsafe {
        let count = getgroups(0, std::ptr::null_mut());
        if count > 0 {
            let mut buf = vec![0 as libc::gid_t; count as usize];
            let res = getgroups(count, buf.as_mut_ptr());
            if res >= 0 {
                groups.extend(buf.into_iter().take(res as usize));
            }
        }
    }
    if !groups.contains(&primary_gid) {
        groups.push(primary_gid);
    }
    groups
}

/// FileSystem configuration.
#[derive(Debug, Clone)]
pub struct FileSystemConfig {
    /// Enable access logging.
    pub access_log: bool,
    /// Access log buffer size.
    pub access_log_buffer_size: usize,
    /// Enforce POSIX-style permission checks.
    pub enforce_permissions: bool,
    /// Caller identity used for permission checks.
    pub caller: CallerIdentity,
}

impl Default for FileSystemConfig {
    fn default() -> Self {
        Self {
            access_log: false,
            access_log_buffer_size: 1024,
            enforce_permissions: true,
            caller: CallerIdentity::current(),
        }
    }
}

impl FileSystemConfig {
    pub fn with_caller(mut self, caller: CallerIdentity) -> Self {
        self.caller = caller;
        self
    }

    pub fn with_permissions(mut self, enforce: bool) -> Self {
        self.enforce_permissions = enforce;
        self
    }
}

fn access_log_sender(config: &FileSystemConfig) -> Option<mpsc::Sender<AccessLogEntry>> {
    if !config.access_log {
        return None;
    }

    let (tx, mut rx) = mpsc::channel::<AccessLogEntry>(config.access_log_buffer_size);
    // Spawn background task to process access logs
    tokio::spawn(async move {
        while let Some(entry) = rx.recv().await {
            // For now, just log to tracing; can be extended to write to file
            info!(target: "access_log", "{}", entry);
        }
    });
    Some(tx)
}

fn meta_error_to_io(path: &str, err: MetaError) -> io::Error {
    let kind = match err {
        MetaError::NotFound(_) | MetaError::ParentNotFound(_) => io::ErrorKind::NotFound,
        MetaError::AlreadyExists { .. } => io::ErrorKind::AlreadyExists,
        MetaError::NotDirectory(_) => io::ErrorKind::NotADirectory,
        MetaError::DirectoryNotEmpty(_) => io::ErrorKind::DirectoryNotEmpty,
        MetaError::InvalidPath(_) => io::ErrorKind::InvalidInput,
        MetaError::TooManySymlinks => io::ErrorKind::InvalidInput,
        MetaError::NotSupported(_) | MetaError::NotImplemented => io::ErrorKind::Unsupported,
        MetaError::InvalidHandle(_) => io::ErrorKind::InvalidInput,
        MetaError::LockConflict { .. } => io::ErrorKind::WouldBlock,
        MetaError::LockNotFound { .. } => io::ErrorKind::NotFound,
        MetaError::Io(ref e) => e.kind(),
        _ => io::ErrorKind::Other,
    };
    io::Error::new(kind, format!("{path}: {err}"))
}

/// High-level path-based file system API.
///
/// This struct owns a VFS instance for I/O and handle management while keeping
/// path resolution, permissions, and logging here (JuiceFS FileSystem style):
/// - Path-based operations (Open, Stat, Mkdir, etc.)
/// - `File` handle with lazy reader/writer
/// - Optional access logging
pub struct FileSystem<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    vfs: VFS<S, MetaClient<M>>,
    config: FileSystemConfig,
    access_log_tx: Option<mpsc::Sender<AccessLogEntry>>,
    next_file_id: AtomicU64,
}

bitflags::bitflags! {
    struct AccessMask: u8 {
        const READ = 0b001;
        const WRITE = 0b010;
        const EXEC = 0b100;
    }
}

impl<S, M> FileSystem<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    /// Create a new FileSystem with default meta/client configuration.
    pub async fn new(layout: ChunkLayout, store: S, meta: M) -> io::Result<Self> {
        Self::with_configs(
            layout,
            store,
            meta,
            MetaClientConfig::default(),
            FileSystemConfig::default(),
        )
        .await
    }

    /// Create a new FileSystem with FileSystem configuration.
    pub async fn with_config(
        layout: ChunkLayout,
        store: S,
        meta: M,
        config: FileSystemConfig,
    ) -> io::Result<Self> {
        Self::with_configs(layout, store, meta, MetaClientConfig::default(), config).await
    }

    /// Create a new FileSystem with MetaClient configuration.
    pub async fn with_meta_client_config(
        layout: ChunkLayout,
        store: S,
        meta: M,
        meta_config: MetaClientConfig,
    ) -> io::Result<Self> {
        Self::with_configs(
            layout,
            store,
            meta,
            meta_config,
            FileSystemConfig::default(),
        )
        .await
    }

    async fn with_configs(
        layout: ChunkLayout,
        store: S,
        meta: M,
        meta_config: MetaClientConfig,
        config: FileSystemConfig,
    ) -> io::Result<Self> {
        let store = Arc::new(store);
        let meta = Arc::new(meta);
        let meta_client = MetaClient::with_options(
            Arc::clone(&meta),
            meta_config.capacity.clone(),
            meta_config.effective_ttl(),
            meta_config.options.clone(),
        );
        meta_client
            .initialize()
            .await
            .map_err(|e| io::Error::other(format!("meta init failed: {e}")))?;
        Self::from_components(layout, store, meta, meta_client, config)
    }

    /// Create a new FileSystem from components and an existing meta layer.
    pub fn from_components(
        layout: ChunkLayout,
        store: Arc<S>,
        meta: Arc<M>,
        meta_layer: Arc<MetaClient<M>>,
        config: FileSystemConfig,
    ) -> io::Result<Self> {
        let access_log_tx = access_log_sender(&config);
        let _ = meta;
        let vfs = VFS::with_meta_layer(layout, Arc::clone(&store), Arc::clone(&meta_layer))
            .map_err(std::io::Error::from)?;
        Ok(Self {
            vfs,
            config,
            access_log_tx,
            next_file_id: AtomicU64::new(1),
        })
    }

    fn meta_layer(&self) -> &MetaClient<M> {
        self.vfs.meta_layer()
    }

    /// Log an access entry if access logging is enabled.
    fn log_access(&self, ctx: &LogContext, op: &str, path: &str, result: &str) {
        if let Some(ref tx) = self.access_log_tx {
            let entry = AccessLogEntry {
                timestamp: SystemTime::now(),
                uid: ctx.uid,
                gid: ctx.gid,
                pid: ctx.pid,
                op: op.to_string(),
                path: path.to_string(),
                result: result.to_string(),
                duration_us: ctx.elapsed_us(),
            };
            let _ = tx.try_send(entry);
        }
    }

    fn log_context(&self) -> Option<LogContext> {
        if self.access_log_tx.is_some() {
            Some(make_log_context())
        } else {
            None
        }
    }

    fn log_result<T>(
        &self,
        ctx: Option<&LogContext>,
        op: &str,
        path: &str,
        result: &io::Result<T>,
    ) {
        if let Some(ctx) = ctx {
            let outcome = match result {
                Ok(_) => "ok".to_string(),
                Err(err) => err.to_string(),
            };
            self.log_access(ctx, op, path, &outcome);
        }
    }

    /// Get file system statistics.
    pub async fn stat_fs(&self) -> io::Result<StatFs> {
        let snapshot = self
            .meta_layer()
            .stat_fs()
            .await
            .map_err(|e| meta_error_to_io("stat_fs", e))?;
        Ok(StatFs::from(snapshot))
    }

    /// Resolve a path to FileStat, following symlinks.
    pub async fn stat(&self, path: &str) -> io::Result<FileStat> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = self.resolve(&path, true).await;
        self.log_result(log_ctx.as_ref(), "stat", &path, &result);
        result
    }

    /// Resolve a path to FileStat without following the final symlink (lstat).
    pub async fn lstat(&self, path: &str) -> io::Result<FileStat> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = self.resolve(&path, false).await;
        self.log_result(log_ctx.as_ref(), "lstat", &path, &result);
        result
    }

    /// Internal path resolution with symlink handling.
    async fn resolve(&self, path: &str, follow_last: bool) -> io::Result<FileStat> {
        let ino = if follow_last {
            self.meta_layer()
                .resolve_path_follow(path)
                .await
                .map_err(|e| meta_error_to_io(path, e))?
        } else {
            self.meta_layer()
                .resolve_path(path)
                .await
                .map_err(|e| meta_error_to_io(path, e))?
        };

        let attr = self
            .meta_layer()
            .stat(ino)
            .await
            .map_err(|e| meta_error_to_io(path, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{path}: not found")))?;

        let name = path.rsplit('/').next().unwrap_or("");
        Ok(FileStat::new(name.to_string(), attr.ino, attr))
    }

    /// Normalize a path by removing redundant slashes.
    fn normalize_path(path: &str) -> String {
        if path.is_empty() {
            return "/".to_string();
        }
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", parts.join("/"))
        }
    }

    /// Split a normalized path into parent directory and basename.
    fn split_dir_file(path: &str) -> (String, String) {
        let n = path.rfind('/').unwrap_or(0);
        if n == 0 {
            ("/".into(), path[1..].into())
        } else {
            (path[..n].into(), path[n + 1..].into())
        }
    }

    fn permission_for(attr: &FileAttr) -> Permission {
        Permission {
            mode: attr.mode,
            uid: attr.uid,
            gid: attr.gid,
            acl: None,
        }
    }

    fn desired_gid(parent_attr: &FileAttr, current_gid: u32) -> u32 {
        if parent_attr.mode & 0o2000 != 0 {
            parent_attr.gid
        } else {
            current_gid
        }
    }

    fn check_access(&self, attr: &FileAttr, mask: AccessMask, path: &str) -> io::Result<()> {
        if !self.config.enforce_permissions {
            return Ok(());
        }
        let caller = &self.config.caller;
        let perm = Self::permission_for(attr);
        if mask.contains(AccessMask::READ) && !perm.can_read(caller.uid, &caller.groups) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{path}: read permission denied"),
            ));
        }
        if mask.contains(AccessMask::WRITE) && !perm.can_write(caller.uid, &caller.groups) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{path}: write permission denied"),
            ));
        }
        if mask.contains(AccessMask::EXEC) && !perm.can_execute(caller.uid, &caller.groups) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("{path}: execute permission denied"),
            ));
        }
        Ok(())
    }

    fn check_owner(&self, attr: &FileAttr, path: &str) -> io::Result<()> {
        if !self.config.enforce_permissions {
            return Ok(());
        }
        let caller = &self.config.caller;
        if caller.uid == 0 || caller.uid == attr.uid {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{path}: ownership required"),
        ))
    }

    async fn apply_owner(&self, ino: i64, parent_attr: &FileAttr, path: &str) -> io::Result<()> {
        let caller = &self.config.caller;
        let gid = Self::desired_gid(parent_attr, caller.gid);
        let req = SetAttrRequest {
            uid: Some(caller.uid),
            gid: Some(gid),
            ..Default::default()
        };
        self.meta_layer()
            .set_attr(ino, &req, SetAttrFlags::empty())
            .await
            .map_err(|e| meta_error_to_io(path, e))?;
        Ok(())
    }

    /// Open a file with the given flags.
    pub async fn open(&self, path: &str, flags: OpenFlags) -> io::Result<File<S, M>> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let mut resolved: Option<FileStat> = None;
            // Handle file creation
            if flags.create {
                match self.resolve(&path, true).await {
                    Ok(fi) => {
                        if flags.exclusive {
                            return Err(io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                "file already exists",
                            ));
                        }
                        if fi.is_dir() {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "cannot open directory as file",
                            ));
                        }
                        resolved = Some(fi);
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        // Create the file
                        self.create_file_in_existing_dir(&path, flags.exclusive)
                            .await?;
                    }
                    Err(e) => return Err(e),
                }
            }

            let fi = match resolved {
                Some(fi) => fi,
                None => self.resolve(&path, true).await?,
            };
            if fi.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot open directory as file",
                ));
            }
            let mut access = AccessMask::empty();
            if flags.read {
                access |= AccessMask::READ;
            }
            if flags.write {
                access |= AccessMask::WRITE;
            }
            if !access.is_empty() {
                self.check_access(fi.attr(), access, &path)?;
            }

            // Handle truncate
            if flags.truncate && flags.write {
                self.vfs
                    .truncate_inode(fi.inode(), 0)
                    .await
                    .map_err(io::Error::from)?;
            }

            let fh = self
                .vfs
                .open(fi.inode(), fi.attr().clone(), flags.read, flags.write)
                .await
                .map_err(io::Error::from)?;
            let file_id = self.next_file_id.fetch_add(1, Ordering::Relaxed);

            Ok(File {
                id: file_id,
                path: path.clone(),
                inode: fi.inode(),
                fh,
                info: fi,
                flags,
                offset: AtomicU64::new(0),
                vfs: self.vfs.clone(),
                access_log_tx: self.access_log_tx.clone(),
            })
        }
        .await;
        self.log_result(log_ctx.as_ref(), "open", &path, &result);
        result
    }

    /// Open a file without following symlinks.
    pub async fn lopen(&self, path: &str, flags: OpenFlags) -> io::Result<File<S, M>> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let fi = self.resolve(&path, false).await?;

            if fi.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot open directory as file",
                ));
            }
            let mut access = AccessMask::empty();
            if flags.read {
                access |= AccessMask::READ;
            }
            if flags.write {
                access |= AccessMask::WRITE;
            }
            if !access.is_empty() {
                self.check_access(fi.attr(), access, &path)?;
            }

            let fh = self
                .vfs
                .open(fi.inode(), fi.attr().clone(), flags.read, flags.write)
                .await
                .map_err(io::Error::from)?;
            let file_id = self.next_file_id.fetch_add(1, Ordering::Relaxed);

            Ok(File {
                id: file_id,
                path: path.clone(),
                inode: fi.inode(),
                fh,
                info: fi,
                flags,
                offset: AtomicU64::new(0),
                vfs: self.vfs.clone(),
                access_log_tx: self.access_log_tx.clone(),
            })
        }
        .await;
        self.log_result(log_ctx.as_ref(), "lopen", &path, &result);
        result
    }

    /// Create a new file (O_CREAT | O_EXCL).
    pub async fn create(&self, path: &str) -> io::Result<File<S, M>> {
        self.open(path, OpenFlags::create_new()).await
    }

    /// Create a regular file in an existing parent directory.
    pub async fn create_file(&self, path: &str) -> io::Result<i64> {
        let path = Self::normalize_path(path);
        self.create_file_in_existing_dir(&path, false).await
    }

    /// Check if a path exists.
    pub async fn exists(&self, path: &str) -> bool {
        let path = Self::normalize_path(path);
        self.vfs.exists(&path).await
    }

    /// Create a directory.
    pub async fn mkdir(&self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            if path == "/" {
                return Ok(());
            }
            let (dir, name) = Self::split_dir_file(&path);
            if name.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "empty directory name",
                ));
            }

            let parent_ino = if dir == "/" {
                self.meta_layer().root_ino()
            } else {
                let (ino, kind) = self
                    .meta_layer()
                    .lookup_path(&dir)
                    .await
                    .map_err(|e| meta_error_to_io(&dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
                if kind != FileType::Dir {
                    return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
                }
                ino
            };
            let parent_attr = self
                .meta_layer()
                .stat(parent_ino)
                .await
                .map_err(|e| meta_error_to_io(&dir, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
            if parent_attr.kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
            }
            self.check_access(&parent_attr, AccessMask::WRITE | AccessMask::EXEC, &dir)?;

            if let Some(ino) = self
                .meta_layer()
                .lookup(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
            {
                let attr = self
                    .meta_layer()
                    .stat(ino)
                    .await
                    .map_err(|e| meta_error_to_io(&path, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
                if attr.kind == FileType::Dir {
                    return Ok(());
                }
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, path.clone()));
            }

            let ino = self
                .meta_layer()
                .mkdir(parent_ino, name)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?;
            self.apply_owner(ino, &parent_attr, &path).await?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "mkdir", &path, &result);
        result
    }

    /// Create directories recursively (mkdir -p).
    pub async fn mkdir_all(&self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = self.mkdir_p(&path).await.map(|_| ());
        self.log_result(log_ctx.as_ref(), "mkdir_all", &path, &result);
        result
    }

    /// Remove a file (unlink).
    pub async fn unlink(&self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let (dir, name) = Self::split_dir_file(&path);
            let parent_ino = if dir == "/" {
                self.meta_layer().root_ino()
            } else {
                let (ino, kind) = self
                    .meta_layer()
                    .lookup_path(&dir)
                    .await
                    .map_err(|e| meta_error_to_io(&dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
                if kind != FileType::Dir {
                    return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
                }
                ino
            };

            let ino = self
                .meta_layer()
                .lookup(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
            let attr = self
                .meta_layer()
                .stat(ino)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
            if attr.kind == FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, path.clone()));
            }
            self.meta_layer()
                .unlink(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "unlink", &path, &result);
        result
    }

    /// Remove an empty directory.
    pub async fn rmdir(&self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            if path == "/" {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cannot remove root",
                ));
            }
            let (dir, name) = Self::split_dir_file(&path);
            let parent_ino = if dir == "/" {
                self.meta_layer().root_ino()
            } else {
                let (ino, kind) = self
                    .meta_layer()
                    .lookup_path(&dir)
                    .await
                    .map_err(|e| meta_error_to_io(&dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
                if kind != FileType::Dir {
                    return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
                }
                ino
            };

            let ino = self
                .meta_layer()
                .lookup(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
            let attr = self
                .meta_layer()
                .stat(ino)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
            if attr.kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, path.clone()));
            }
            let children = self
                .meta_layer()
                .readdir(ino)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?;
            if !children.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::DirectoryNotEmpty,
                    path.clone(),
                ));
            }
            self.meta_layer()
                .rmdir(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "rmdir", &path, &result);
        result
    }

    /// Remove a file or empty directory.
    pub async fn remove(&self, path: &str) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let fi = self.resolve(&path, false).await?;
        if fi.is_dir() {
            self.rmdir(&path).await
        } else {
            self.unlink(&path).await
        }
    }

    /// Rename a file or directory.
    pub async fn rename(&self, old_path: &str, new_path: &str) -> io::Result<()> {
        let old = Self::normalize_path(old_path);
        let new = Self::normalize_path(new_path);
        let log_ctx = self.log_context();
        let op_path = format!("{old} -> {new}");
        let result = async {
            let (old_dir, old_name) = Self::split_dir_file(&old);
            let (new_dir, new_name) = Self::split_dir_file(&new);

            let old_parent_ino = if &old_dir == "/" {
                self.meta_layer().root_ino()
            } else {
                self.meta_layer()
                    .lookup_path(&old_dir)
                    .await
                    .map_err(|e| meta_error_to_io(&old_dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, old_dir.clone()))?
                    .0
            };
            let old_parent_attr = self
                .meta_layer()
                .stat(old_parent_ino)
                .await
                .map_err(|e| meta_error_to_io(&old_dir, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, old_dir.clone()))?;
            if old_parent_attr.kind != FileType::Dir {
                return Err(io::Error::new(
                    io::ErrorKind::NotADirectory,
                    old_dir.clone(),
                ));
            }
            self.check_access(
                &old_parent_attr,
                AccessMask::WRITE | AccessMask::EXEC,
                &old_dir,
            )?;

            let src_ino = self
                .meta_layer()
                .lookup(old_parent_ino, &old_name)
                .await
                .map_err(|e| meta_error_to_io(&old, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, old.clone()))?;
            let src_attr = self
                .meta_layer()
                .stat(src_ino)
                .await
                .map_err(|e| meta_error_to_io(&old, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, old.clone()))?;

            if let Ok(Some((dest_ino, dest_kind))) = self.meta_layer().lookup_path(&new).await {
                let new_dir_ino = if &new_dir == "/" {
                    self.meta_layer().root_ino()
                } else {
                    self.meta_layer()
                        .lookup_path(&new_dir)
                        .await
                        .map_err(|e| meta_error_to_io(&new_dir, e))?
                        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, new_dir.clone()))?
                        .0
                };
                let new_parent_attr = self
                    .meta_layer()
                    .stat(new_dir_ino)
                    .await
                    .map_err(|e| meta_error_to_io(&new_dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, new_dir.clone()))?;
                if new_parent_attr.kind != FileType::Dir {
                    return Err(io::Error::new(
                        io::ErrorKind::NotADirectory,
                        new_dir.clone(),
                    ));
                }
                self.check_access(
                    &new_parent_attr,
                    AccessMask::WRITE | AccessMask::EXEC,
                    &new_dir,
                )?;

                if dest_kind == FileType::Dir {
                    if src_attr.kind != FileType::Dir {
                        return Err(io::Error::new(io::ErrorKind::NotADirectory, new.clone()));
                    }
                    let children = self
                        .meta_layer()
                        .readdir(dest_ino)
                        .await
                        .map_err(|e| meta_error_to_io(&new, e))?;
                    if !children.is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::DirectoryNotEmpty,
                            new.clone(),
                        ));
                    }
                    self.meta_layer()
                        .rmdir(new_dir_ino, &new_name)
                        .await
                        .map_err(|e| meta_error_to_io(&new, e))?;
                } else {
                    if src_attr.kind == FileType::Dir {
                        return Err(io::Error::new(io::ErrorKind::NotADirectory, new.clone()));
                    }
                    self.meta_layer()
                        .unlink(new_dir_ino, &new_name)
                        .await
                        .map_err(|e| meta_error_to_io(&new, e))?;
                }
            }

            let new_dir_ino = if &new_dir == "/" {
                self.meta_layer().root_ino()
            } else {
                let (ino, kind) = self
                    .meta_layer()
                    .lookup_path(&new_dir)
                    .await
                    .map_err(|e| meta_error_to_io(&new_dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, new_dir.clone()))?;
                if kind != FileType::Dir {
                    return Err(io::Error::new(io::ErrorKind::NotADirectory, new_dir));
                }
                ino
            };
            let new_parent_attr = self
                .meta_layer()
                .stat(new_dir_ino)
                .await
                .map_err(|e| meta_error_to_io(&new_dir, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, new_dir.clone()))?;
            if new_parent_attr.kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, new_dir));
            }
            self.check_access(
                &new_parent_attr,
                AccessMask::WRITE | AccessMask::EXEC,
                &new_dir,
            )?;
            self.meta_layer()
                .rename(old_parent_ino, &old_name, new_dir_ino, new_name)
                .await
                .map_err(|e| meta_error_to_io(&new, e))?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "rename", &op_path, &result);
        result
    }

    /// Create a hard link.
    pub async fn link(&self, existing: &str, link_path: &str) -> io::Result<()> {
        let existing = Self::normalize_path(existing);
        let link = Self::normalize_path(link_path);
        let log_ctx = self.log_context();
        let op_path = format!("{existing} -> {link}");
        let result = async {
            if existing == "/" {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, existing));
            }
            if link == "/" {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, link));
            }
            let (src_ino, src_kind) = self
                .meta_layer()
                .lookup_path(&existing)
                .await
                .map_err(|e| meta_error_to_io(&existing, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, existing.clone()))?;
            if src_kind == FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, existing));
            }

            let (parent_path, name) = Self::split_dir_file(&link);
            if name.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, link));
            }

            let parent_ino = if &parent_path == "/" {
                self.meta_layer().root_ino()
            } else {
                self.meta_layer()
                    .lookup_path(&parent_path)
                    .await
                    .map_err(|e| meta_error_to_io(&parent_path, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, parent_path.clone()))?
                    .0
            };

            let parent_attr = self
                .meta_layer()
                .stat(parent_ino)
                .await
                .map_err(|e| meta_error_to_io(&parent_path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, parent_path.clone()))?;
            if parent_attr.kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, parent_path));
            }
            self.check_access(
                &parent_attr,
                AccessMask::WRITE | AccessMask::EXEC,
                &parent_path,
            )?;

            if self
                .meta_layer()
                .lookup(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&link, e))?
                .is_some()
            {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, link));
            }

            self.meta_layer()
                .link(src_ino, parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&link, e))?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "link", &op_path, &result);
        result
    }

    /// Create a symbolic link.
    pub async fn symlink(&self, link_path: &str, target: &str) -> io::Result<()> {
        let link = Self::normalize_path(link_path);
        let log_ctx = self.log_context();
        let op_path = format!("{link} -> {target}");
        let result = async {
            if link == "/" {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, link));
            }
            let (dir, name) = Self::split_dir_file(&link);
            if name.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, link));
            }

            let parent_ino = if &dir == "/" {
                self.meta_layer().root_ino()
            } else {
                self.meta_layer()
                    .lookup_path(&dir)
                    .await
                    .map_err(|e| meta_error_to_io(&dir, e))?
                    .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?
                    .0
            };

            let parent_attr = self
                .meta_layer()
                .stat(parent_ino)
                .await
                .map_err(|e| meta_error_to_io(&dir, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
            if parent_attr.kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
            }

            if self
                .meta_layer()
                .lookup(parent_ino, &name)
                .await
                .map_err(|e| meta_error_to_io(&link, e))?
                .is_some()
            {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, link));
            }

            let (ino, _) = self
                .meta_layer()
                .symlink(parent_ino, &name, target)
                .await
                .map_err(|e| meta_error_to_io(&link, e))?;
            self.apply_owner(ino, &parent_attr, &link).await?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "symlink", &op_path, &result);
        result
    }

    /// Read the target of a symbolic link.
    pub async fn readlink(&self, path: &str) -> io::Result<String> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let (ino, kind) = self
                .meta_layer()
                .lookup_path(&path)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
            if kind != FileType::Symlink {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, path.clone()));
            }
            self.meta_layer()
                .read_symlink(ino)
                .await
                .map_err(|e| meta_error_to_io(&path, e))
        }
        .await;
        self.log_result(log_ctx.as_ref(), "readlink", &path, &result);
        result
    }

    /// Truncate a file to the given size.
    pub async fn truncate(&self, path: &str, size: u64) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let fi = self.resolve(&path, true).await?;
            if fi.is_dir() {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, path.clone()));
            }
            if fi.file_type() != FileType::File {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, path.clone()));
            }
            self.check_access(fi.attr(), AccessMask::WRITE, &path)?;
            self.vfs
                .truncate_inode(fi.inode(), size)
                .await
                .map_err(io::Error::from)?;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "truncate", &path, &result);
        result
    }

    /// Set file attributes.
    pub async fn set_attr(
        &self,
        path: &str,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> io::Result<FileAttr> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let fi = self.resolve(&path, true).await?;
            let attr = fi.attr();
            if req.mode.is_some()
                || req.uid.is_some()
                || req.gid.is_some()
                || flags.contains(SetAttrFlags::CLEAR_SUID)
                || flags.contains(SetAttrFlags::CLEAR_SGID)
            {
                self.check_owner(attr, &path)?;
            }
            if req.size.is_some() {
                self.check_access(attr, AccessMask::WRITE, &path)?;
            }
            if (req.atime.is_some()
                || req.mtime.is_some()
                || req.ctime.is_some()
                || flags.contains(SetAttrFlags::SET_ATIME_NOW)
                || flags.contains(SetAttrFlags::SET_MTIME_NOW))
                && self.check_owner(attr, &path).is_err()
            {
                self.check_access(attr, AccessMask::WRITE, &path)?;
            }
            self.vfs
                .set_attr(fi.inode(), req, flags)
                .await
                .map_err(io::Error::from)
        }
        .await;
        self.log_result(log_ctx.as_ref(), "set_attr", &path, &result);
        result
    }

    /// Read directory entries.
    pub async fn readdir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let (ino, attr) = self
                .meta_layer()
                .lookup_path_with_attr(&path)
                .await
                .map_err(|e| meta_error_to_io(&path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
            if attr.kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, path.clone()));
            }
            self.check_access(&attr, AccessMask::READ | AccessMask::EXEC, &path)?;
            self.meta_layer()
                .readdir(ino)
                .await
                .map_err(|e| meta_error_to_io(&path, e))
        }
        .await;
        self.log_result(log_ctx.as_ref(), "readdir", &path, &result);
        result
    }

    /// Read data at a specific offset (path-based).
    pub async fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let fi = self.resolve(&path, true).await?;
            self.check_access(fi.attr(), AccessMask::READ, &path)?;
            read_inode(&self.vfs, fi.inode(), fi.attr().clone(), offset, len, &path).await
        }
        .await;
        self.log_result(log_ctx.as_ref(), "read_at", &path, &result);
        result
    }

    /// Write data at a specific offset (path-based).
    pub async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> io::Result<usize> {
        let path = Self::normalize_path(path);
        let log_ctx = self.log_context();
        let result = async {
            let fi = self.resolve(&path, true).await?;
            self.check_access(fi.attr(), AccessMask::WRITE, &path)?;
            write_inode(
                &self.vfs,
                fi.inode(),
                fi.attr().clone(),
                offset,
                data,
                &path,
            )
            .await
        }
        .await;
        self.log_result(log_ctx.as_ref(), "write_at", &path, &result);
        result
    }

    /// Get file lock information for a given path and query.
    pub async fn get_plock(
        &self,
        path: &str,
        query: &crate::meta::file_lock::FileLockQuery,
    ) -> io::Result<crate::meta::file_lock::FileLockInfo> {
        let path = Self::normalize_path(path);
        let fi = self.resolve(&path, true).await?;
        self.meta_layer()
            .get_plock(fi.inode(), query)
            .await
            .map_err(|e| meta_error_to_io(&path, e))
    }

    /// Set file lock for a given path.
    pub async fn set_plock(
        &self,
        path: &str,
        owner: i64,
        block: bool,
        lock_type: crate::meta::file_lock::FileLockType,
        range: crate::meta::file_lock::FileLockRange,
        pid: u32,
    ) -> io::Result<()> {
        let path = Self::normalize_path(path);
        let fi = self.resolve(&path, true).await?;
        self.meta_layer()
            .set_plock(fi.inode(), owner, block, lock_type, range, pid)
            .await
            .map_err(|e| meta_error_to_io(&path, e))
    }

    async fn mkdir_p(&self, path: &str) -> io::Result<i64> {
        let path = Self::normalize_path(path);
        if path == "/" {
            return Ok(self.meta_layer().root_ino());
        }

        if let Some((ino, kind)) = self
            .meta_layer()
            .lookup_path(&path)
            .await
            .map_err(|e| meta_error_to_io(&path, e))?
        {
            if kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, path.clone()));
            }
            return Ok(ino);
        }

        let mut cur_ino = self.meta_layer().root_ino();
        let mut cur_path = String::new();
        let mut cur_attr = self
            .meta_layer()
            .stat(cur_ino)
            .await
            .map_err(|e| meta_error_to_io(&path, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.clone()))?;
        for part in path.trim_start_matches('/').split('/') {
            if part.is_empty() {
                continue;
            }
            let parent_path = if cur_path.is_empty() {
                "/".to_string()
            } else {
                cur_path.clone()
            };
            cur_path.push('/');
            cur_path.push_str(part);

            match self
                .meta_layer()
                .lookup(cur_ino, part)
                .await
                .map_err(|e| meta_error_to_io(&cur_path, e))?
            {
                Some(ino) => {
                    let attr = self
                        .meta_layer()
                        .stat(ino)
                        .await
                        .map_err(|e| meta_error_to_io(&cur_path, e))?
                        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, cur_path.clone()))?;
                    if attr.kind != FileType::Dir {
                        return Err(io::Error::new(io::ErrorKind::NotADirectory, cur_path));
                    }
                    self.check_access(&attr, AccessMask::EXEC, &cur_path)?;
                    cur_ino = ino;
                    cur_attr = attr;
                }
                None => {
                    self.check_access(
                        &cur_attr,
                        AccessMask::WRITE | AccessMask::EXEC,
                        &parent_path,
                    )?;
                    let ino = self
                        .meta_layer()
                        .mkdir(cur_ino, part.to_string())
                        .await
                        .map_err(|e| meta_error_to_io(&cur_path, e))?;
                    self.apply_owner(ino, &cur_attr, &cur_path).await?;
                    let attr = self
                        .meta_layer()
                        .stat(ino)
                        .await
                        .map_err(|e| meta_error_to_io(&cur_path, e))?
                        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, cur_path.clone()))?;
                    cur_ino = ino;
                    cur_attr = attr;
                }
            }
        }
        Ok(cur_ino)
    }

    async fn create_file_in_existing_dir(&self, path: &str, create_new: bool) -> io::Result<i64> {
        if path == "/" {
            return Err(io::Error::new(io::ErrorKind::IsADirectory, path));
        }

        let (dir, name) = Self::split_dir_file(path);
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty file name",
            ));
        }

        let parent_ino = if dir == "/" {
            self.meta_layer().root_ino()
        } else {
            let (ino, kind) = self
                .meta_layer()
                .lookup_path(&dir)
                .await
                .map_err(|e| meta_error_to_io(&dir, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
            if kind != FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
            }
            ino
        };
        let parent_attr = self
            .meta_layer()
            .stat(parent_ino)
            .await
            .map_err(|e| meta_error_to_io(&dir, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, dir.clone()))?;
        if parent_attr.kind != FileType::Dir {
            return Err(io::Error::new(io::ErrorKind::NotADirectory, dir));
        }
        self.check_access(&parent_attr, AccessMask::WRITE | AccessMask::EXEC, &dir)?;

        if let Some(existing) = self
            .meta_layer()
            .lookup(parent_ino, &name)
            .await
            .map_err(|e| meta_error_to_io(path, e))?
        {
            let attr = self
                .meta_layer()
                .stat(existing)
                .await
                .map_err(|e| meta_error_to_io(path, e))?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.to_string()))?;
            if attr.kind == FileType::Dir {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, path));
            }
            if create_new {
                return Err(io::Error::new(io::ErrorKind::AlreadyExists, path));
            }
            return Ok(existing);
        }

        let ino = self
            .meta_layer()
            .create_file(parent_ino, name)
            .await
            .map_err(|e| meta_error_to_io(path, e))?;
        self.apply_owner(ino, &parent_attr, path).await?;
        Ok(ino)
    }
}

async fn read_inode<S, M>(
    vfs: &VFS<S, MetaClient<M>>,
    ino: i64,
    attr: FileAttr,
    offset: u64,
    len: usize,
    path: &str,
) -> io::Result<Vec<u8>>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    if len == 0 {
        return Ok(Vec::new());
    }
    match attr.kind {
        FileType::File => {}
        FileType::Dir => return Err(io::Error::new(io::ErrorKind::IsADirectory, path)),
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, path)),
    }

    let guard = vfs
        .open_guard(ino, attr, true, false)
        .await
        .map_err(io::Error::from)?;
    let result = guard.read(offset, len).await.map_err(io::Error::from);
    let _ = guard.close().await;
    result
}

async fn write_inode<S, M>(
    vfs: &VFS<S, MetaClient<M>>,
    ino: i64,
    attr: FileAttr,
    offset: u64,
    data: &[u8],
    path: &str,
) -> io::Result<usize>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    match attr.kind {
        FileType::File => {}
        FileType::Dir => return Err(io::Error::new(io::ErrorKind::IsADirectory, path)),
        _ => return Err(io::Error::new(io::ErrorKind::InvalidInput, path)),
    }

    let guard = vfs
        .open_guard(ino, attr, false, true)
        .await
        .map_err(io::Error::from)?;
    let result = guard.write(offset, data).await.map_err(io::Error::from);
    let _ = guard.close().await;
    result
}

/// An open file handle.
///
/// Similar to JuiceFS's File struct in pkg/fs/fs.go.
/// Provides read/write operations with automatic offset tracking.
pub struct File<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    id: u64,
    path: String,
    inode: i64,
    fh: u64,
    info: FileStat,
    flags: OpenFlags,
    offset: AtomicU64,
    vfs: VFS<S, MetaClient<M>>,
    access_log_tx: Option<mpsc::Sender<AccessLogEntry>>,
}

impl<S, M> File<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    fn meta_layer(&self) -> &MetaClient<M> {
        self.vfs.meta_layer()
    }

    /// Get the file path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get the file inode.
    pub fn inode(&self) -> i64 {
        self.inode
    }

    /// Get file metadata.
    pub fn info(&self) -> &FileStat {
        &self.info
    }

    /// Get current offset.
    pub fn offset(&self) -> u64 {
        self.offset.load(Ordering::Relaxed)
    }

    /// Seek to a position.
    pub fn seek(&self, offset: u64) {
        self.offset.store(offset, Ordering::Relaxed);
    }

    fn log_access(&self, ctx: &LogContext, op: &str, result: &str) {
        if let Some(ref tx) = self.access_log_tx {
            let entry = AccessLogEntry {
                timestamp: SystemTime::now(),
                uid: ctx.uid,
                gid: ctx.gid,
                pid: ctx.pid,
                op: op.to_string(),
                path: self.path.clone(),
                result: result.to_string(),
                duration_us: ctx.elapsed_us(),
            };
            let _ = tx.try_send(entry);
        }
    }

    fn log_context(&self) -> Option<LogContext> {
        if self.access_log_tx.is_some() {
            Some(make_log_context())
        } else {
            None
        }
    }

    fn log_result<T>(&self, ctx: Option<&LogContext>, op: &str, result: &io::Result<T>) {
        if let Some(ctx) = ctx {
            let outcome = match result {
                Ok(_) => "ok".to_string(),
                Err(err) => err.to_string(),
            };
            self.log_access(ctx, op, &outcome);
        }
    }

    /// Read data at the current offset and advance offset.
    pub async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        let log_ctx = self.log_context();
        let result = async {
            if !self.flags.read {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file not opened for reading",
                ));
            }

            let offset = self.offset.load(Ordering::Relaxed);
            let data = self
                .vfs
                .read(self.fh, offset, buf.len())
                .await
                .map_err(io::Error::from)?;

            let n = data.len();
            buf[..n].copy_from_slice(&data);
            self.offset.fetch_add(n as u64, Ordering::Relaxed);

            Ok(n)
        }
        .await;
        self.log_result(log_ctx.as_ref(), "read", &result);
        result
    }

    /// Read data at a specific offset (pread).
    pub async fn read_at(&self, buf: &mut [u8], offset: u64) -> io::Result<usize> {
        let log_ctx = self.log_context();
        let result = async {
            if !self.flags.read {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file not opened for reading",
                ));
            }

            let data = self
                .vfs
                .read(self.fh, offset, buf.len())
                .await
                .map_err(io::Error::from)?;

            let n = data.len();
            buf[..n].copy_from_slice(&data);

            Ok(n)
        }
        .await;
        self.log_result(log_ctx.as_ref(), "read_at", &result);
        result
    }

    /// Write data at the current offset and advance offset.
    pub async fn write(&self, data: &[u8]) -> io::Result<usize> {
        let log_ctx = self.log_context();
        let result = async {
            if !self.flags.write {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file not opened for writing",
                ));
            }

            let offset = if self.flags.append {
                // For append mode, always write at end
                self.size().await?
            } else {
                self.offset.load(Ordering::Relaxed)
            };

            let n = self
                .vfs
                .write(self.fh, offset, data)
                .await
                .map_err(io::Error::from)?;

            if self.flags.append {
                self.offset.store(offset + n as u64, Ordering::Relaxed);
            } else {
                self.offset.fetch_add(n as u64, Ordering::Relaxed);
            }

            Ok(n)
        }
        .await;
        self.log_result(log_ctx.as_ref(), "write", &result);
        result
    }

    /// Write data at a specific offset (pwrite).
    pub async fn write_at(&self, data: &[u8], offset: u64) -> io::Result<usize> {
        let log_ctx = self.log_context();
        let result = async {
            if !self.flags.write {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file not opened for writing",
                ));
            }

            let n = self
                .vfs
                .write(self.fh, offset, data)
                .await
                .map_err(io::Error::from)?;
            Ok(n)
        }
        .await;
        self.log_result(log_ctx.as_ref(), "write_at", &result);
        result
    }

    /// Truncate the file to the given size.
    pub async fn truncate(&mut self, size: u64) -> io::Result<()> {
        let log_ctx = self.log_context();
        let result = async {
            if !self.flags.write {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "file not opened for writing",
                ));
            }

            self.vfs
                .truncate_inode(self.inode, size)
                .await
                .map_err(io::Error::from)?;
            self.info.attr.size = size;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "truncate", &result);
        result
    }

    /// Sync file data to storage.
    pub async fn sync(&mut self) -> io::Result<()> {
        let log_ctx = self.log_context();
        let result = async {
            if !self.flags.write {
                return Ok(());
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| io::Error::other(e.to_string()))?
                .as_nanos() as i64;
            let req = SetAttrRequest {
                mtime: Some(now),
                ctime: Some(now),
                ..Default::default()
            };
            self.vfs
                .set_attr(self.inode, &req, SetAttrFlags::empty())
                .await
                .map_err(io::Error::from)?;
            self.info.attr.mtime = now;
            self.info.attr.ctime = now;
            Ok(())
        }
        .await;
        self.log_result(log_ctx.as_ref(), "sync", &result);
        result
    }

    /// Get current file size.
    pub async fn size(&self) -> io::Result<u64> {
        self.vfs
            .inode_size(self.inode)
            .await
            .map_err(io::Error::from)
    }

    /// Refresh file metadata.
    pub async fn refresh_info(&mut self) -> io::Result<()> {
        let attr = self
            .meta_layer()
            .stat(self.inode)
            .await
            .map_err(|e| meta_error_to_io(&self.path, e))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "inode not found"))?;
        self.info = FileStat::new(self.info.name.clone(), self.inode, attr);
        Ok(())
    }
}

impl<S, M> Drop for File<S, M>
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    fn drop(&mut self) {
        close_handle_best_effort(self.vfs.clone(), self.fh);
    }
}

fn close_handle_best_effort<S, M>(vfs: VFS<S, MetaClient<M>>, fh: u64)
where
    S: BlockStore + Send + Sync + 'static,
    M: MetaStore + 'static,
{
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let _ = vfs.close(fh).await;
        });
        return;
    }

    let _ = std::thread::Builder::new()
        .name("slayerfs-file-close".to_string())
        .spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                let _ = rt.block_on(vfs.close(fh));
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::store::ObjectBlockStore;
    use crate::meta::factory::create_meta_store_from_url;
    use tempfile::tempdir;

    async fn create_test_fs() -> FileSystem<ObjectBlockStore<LocalFsBackend>, Arc<dyn MetaStore>> {
        let tmp = tempdir().unwrap();
        let layout = ChunkLayout::default();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let metadata: Arc<dyn MetaStore> = meta_handle.store();
        let store = ObjectBlockStore::new(client);
        let config = FileSystemConfig::default().with_caller(CallerIdentity::root());
        FileSystem::with_config(layout, store, metadata, config)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_filesystem_basic() {
        let fs = create_test_fs().await;

        // Create directory
        fs.mkdir_all("/test/subdir").await.unwrap();

        // Create and write file
        let file = fs.create("/test/hello.txt").await.unwrap();
        file.write(b"Hello, World!").await.unwrap();

        // Read file
        let file = fs
            .open("/test/hello.txt", OpenFlags::read_only())
            .await
            .unwrap();
        let mut buf = vec![0u8; 20];
        let n = file.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"Hello, World!");

        // Stat file
        let fi = fs.stat("/test/hello.txt").await.unwrap();
        assert!(fi.is_file());
        assert_eq!(fi.size(), 13);

        // List directory
        let entries = fs.readdir("/test").await.unwrap();
        assert!(entries.iter().any(|e| e.name == "hello.txt"));
        assert!(entries.iter().any(|e| e.name == "subdir"));
    }

    #[tokio::test]
    async fn test_file_read_write() {
        let fs = create_test_fs().await;
        fs.mkdir("/data").await.unwrap();

        // Create file with write access
        let file = fs
            .open("/data/test.bin", OpenFlags::create_write())
            .await
            .unwrap();

        // Write data
        let data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
        let n = file.write(&data).await.unwrap();
        assert_eq!(n, 1024);

        // Read it back
        let file = fs
            .open("/data/test.bin", OpenFlags::read_only())
            .await
            .unwrap();
        let mut buf = vec![0u8; 1024];
        let n = file.read(&mut buf).await.unwrap();
        assert_eq!(n, 1024);
        assert_eq!(buf, data);
    }

    #[tokio::test]
    async fn test_file_pread_pwrite() {
        let fs = create_test_fs().await;
        fs.mkdir("/ptest").await.unwrap();

        let file = fs
            .open(
                "/ptest/random.dat",
                OpenFlags::create_write().with_truncate(),
            )
            .await
            .unwrap();

        // Write at specific offsets
        file.write_at(b"HELLO", 100).await.unwrap();
        file.write_at(b"WORLD", 200).await.unwrap();

        // Read at specific offsets
        let file = fs
            .open("/ptest/random.dat", OpenFlags::read_only())
            .await
            .unwrap();

        let mut buf = vec![0u8; 5];
        let n = file.read_at(&mut buf, 100).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"HELLO");

        let n = file.read_at(&mut buf, 200).await.unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"WORLD");
    }

    #[tokio::test]
    async fn test_lstat_stat_symlink() {
        let fs = create_test_fs().await;
        fs.mkdir("/links").await.unwrap();

        let file = fs.create("/links/target.txt").await.unwrap();
        file.write(b"data").await.unwrap();

        fs.symlink("/links/link.symlink", "/links/target.txt")
            .await
            .unwrap();

        let lstat = fs.lstat("/links/link.symlink").await.unwrap();
        assert!(lstat.is_symlink());

        let stat = fs.stat("/links/link.symlink").await.unwrap();
        assert!(stat.is_file());
    }

    #[tokio::test]
    async fn test_append_writes() {
        let fs = create_test_fs().await;
        fs.mkdir("/append").await.unwrap();

        let file = fs
            .open("/append/log.txt", OpenFlags::create_write().with_append())
            .await
            .unwrap();
        file.write(b"one").await.unwrap();
        file.write(b"two").await.unwrap();

        let file = fs
            .open("/append/log.txt", OpenFlags::read_only())
            .await
            .unwrap();
        let mut buf = vec![0u8; 6];
        let n = file.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"onetwo");
    }
}
