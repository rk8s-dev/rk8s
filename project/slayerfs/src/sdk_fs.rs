use async_trait::async_trait;
use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::vfs::fs::{DirEntry as VfsDirEntry, FileAttr as VfsFileAttr, FileType as VfsFileType};

// Re-export useful types from meta store
pub use crate::meta::store::{SetAttrFlags, SetAttrRequest, StatFsSnapshot};

/// SDK client trait for filesystem operations.
///
/// This trait defines the interface for filesystem operations that can be
/// implemented by different backend storage systems.
#[async_trait]
pub trait SdkClient: Send + Sync + 'static {
    /// Create a single directory (non-recursive).
    async fn mkdir(&self, path: &str) -> io::Result<()>;

    /// Create directory recursively (like `mkdir -p`).
    async fn mkdir_p(&self, path: &str) -> io::Result<()>;

    /// Create a file. If `create_new` is true, fails if file already exists.
    async fn create_file(&self, path: &str, create_new: bool) -> io::Result<()>;

    /// Write data at the specified offset.
    async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> io::Result<usize>;

    /// Read data at the specified offset.
    async fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>>;

    /// Read directory entries.
    async fn readdir(&self, path: &str) -> io::Result<Vec<VfsDirEntry>>;

    /// Get file/directory attributes.
    async fn stat(&self, path: &str) -> io::Result<VfsFileAttr>;

    /// Remove a file.
    async fn unlink(&self, path: &str) -> io::Result<()>;

    /// Remove an empty directory.
    async fn rmdir(&self, path: &str) -> io::Result<()>;

    /// Rename a file or directory.
    async fn rename(&self, old: &str, new: &str) -> io::Result<()>;

    /// Truncate a file to the specified size.
    async fn truncate(&self, path: &str, size: u64) -> io::Result<()>;

    /// Check whether a path exists.
    async fn exists(&self, path: &str) -> bool;

    /// Set file/directory attributes (mode, uid, gid, times).
    async fn set_attr(
        &self,
        path: &str,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> io::Result<VfsFileAttr>;

    /// Get file attributes without following symlinks.
    async fn lstat(&self, path: &str) -> io::Result<VfsFileAttr>;

    /// Remove a directory and all its contents recursively.
    async fn remove_dir_all(&self, path: &str) -> io::Result<()>;

    /// Get file system statistics (total/available space and inodes).
    async fn stat_fs(&self) -> io::Result<StatFsSnapshot>;

    /// Create a hard link.
    async fn link(&self, existing: &str, link_path: &str) -> io::Result<VfsFileAttr>;

    /// Create a symbolic link.
    async fn symlink(&self, link_path: &str, target: &str) -> io::Result<VfsFileAttr>;

    /// Read the target of a symbolic link.
    async fn readlink(&self, path: &str) -> io::Result<String>;
}

pub type DynClient = Arc<dyn SdkClient>;

#[async_trait]
impl<S, M> SdkClient for crate::vfs::sdk::Client<S, M>
where
    S: crate::chuck::store::BlockStore + Send + Sync + 'static,
    M: crate::meta::MetaStore + Send + Sync + 'static,
{
    async fn mkdir(&self, path: &str) -> io::Result<()> {
        self.mkdir_io(path).await
    }

    async fn mkdir_p(&self, path: &str) -> io::Result<()> {
        self.mkdir_p_io(path).await
    }

    async fn create_file(&self, path: &str, create_new: bool) -> io::Result<()> {
        self.create_file_io(path, create_new).await
    }

    async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> io::Result<usize> {
        self.write_at_io(path, offset, data).await
    }

    async fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>> {
        self.read_at_io(path, offset, len).await
    }

    async fn readdir(&self, path: &str) -> io::Result<Vec<VfsDirEntry>> {
        self.readdir_io(path).await
    }

    async fn stat(&self, path: &str) -> io::Result<VfsFileAttr> {
        self.stat_io(path).await
    }

    async fn unlink(&self, path: &str) -> io::Result<()> {
        self.unlink_io(path).await
    }

    async fn rmdir(&self, path: &str) -> io::Result<()> {
        self.rmdir_io(path).await
    }

    async fn rename(&self, old: &str, new: &str) -> io::Result<()> {
        self.rename_io(old, new).await
    }

    async fn truncate(&self, path: &str, size: u64) -> io::Result<()> {
        self.truncate_io(path, size).await
    }

    async fn exists(&self, path: &str) -> bool {
        crate::vfs::sdk::Client::exists(self, path).await
    }

    async fn set_attr(
        &self,
        path: &str,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> io::Result<VfsFileAttr> {
        self.set_attr_io(path, req, flags).await
    }

    async fn lstat(&self, path: &str) -> io::Result<VfsFileAttr> {
        self.lstat_io(path).await
    }

    async fn remove_dir_all(&self, path: &str) -> io::Result<()> {
        self.remove_dir_all_io(path).await
    }

    async fn stat_fs(&self) -> io::Result<StatFsSnapshot> {
        self.stat_fs_io().await
    }

    async fn link(&self, existing: &str, link_path: &str) -> io::Result<VfsFileAttr> {
        self.link_io(existing, link_path).await
    }

    async fn symlink(&self, link_path: &str, target: &str) -> io::Result<VfsFileAttr> {
        self.symlink_io(link_path, target).await
    }

    async fn readlink(&self, path: &str) -> io::Result<String> {
        self.readlink_io(path).await
    }
}

fn path_to_str(path: impl AsRef<Path>) -> io::Result<String> {
    let s = path.as_ref().to_string_lossy().to_string();
    if s.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
    }
    for part in s.split('/').filter(|p| !p.is_empty()) {
        if part == "." || part == ".." {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path must not contain '.' or '..' components",
            ));
        }
    }
    Ok(s)
}

#[derive(Debug, Clone)]
pub struct FileType(VfsFileType);

impl FileType {
    pub fn is_file(&self) -> bool {
        self.0 == VfsFileType::File
    }

    pub fn is_dir(&self) -> bool {
        self.0 == VfsFileType::Dir
    }

    pub fn is_symlink(&self) -> bool {
        self.0 == VfsFileType::Symlink
    }
}

/// Metadata information about a file or directory.
#[derive(Debug, Clone)]
pub struct Metadata(VfsFileAttr);

impl Metadata {
    /// Returns the size of the file in bytes.
    pub fn len(&self) -> u64 {
        self.0.size
    }

    /// Returns true if the file size is zero.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the file type.
    pub fn file_type(&self) -> FileType {
        FileType(self.0.kind)
    }

    /// Returns true if this is a regular file.
    pub fn is_file(&self) -> bool {
        self.0.kind == VfsFileType::File
    }

    /// Returns true if this is a directory.
    pub fn is_dir(&self) -> bool {
        self.0.kind == VfsFileType::Dir
    }

    /// Returns true if this is a symbolic link.
    pub fn is_symlink(&self) -> bool {
        self.0.kind == VfsFileType::Symlink
    }

    /// Returns the file mode (permissions).
    pub fn mode(&self) -> u32 {
        self.0.mode
    }

    /// Returns the user ID of the owner.
    pub fn uid(&self) -> u32 {
        self.0.uid
    }

    /// Returns the group ID of the owner.
    pub fn gid(&self) -> u32 {
        self.0.gid
    }

    /// Returns the inode number.
    pub fn ino(&self) -> i64 {
        self.0.ino
    }

    /// Returns the number of hard links.
    pub fn nlink(&self) -> u32 {
        self.0.nlink
    }

    /// Returns the last access time as seconds since UNIX epoch.
    pub fn atime(&self) -> i64 {
        self.0.atime
    }

    /// Returns the last modification time as seconds since UNIX epoch.
    pub fn mtime(&self) -> i64 {
        self.0.mtime
    }

    /// Returns the last status change time as seconds since UNIX epoch.
    pub fn ctime(&self) -> i64 {
        self.0.ctime
    }

    /// Returns the last access time as SystemTime.
    pub fn accessed(&self) -> io::Result<SystemTime> {
        if self.0.atime >= 0 {
            Ok(UNIX_EPOCH + Duration::from_secs(self.0.atime as u64))
        } else {
            Err(io::Error::other("negative timestamp not supported"))
        }
    }

    /// Returns the last modification time as SystemTime.
    pub fn modified(&self) -> io::Result<SystemTime> {
        if self.0.mtime >= 0 {
            Ok(UNIX_EPOCH + Duration::from_secs(self.0.mtime as u64))
        } else {
            Err(io::Error::other("negative timestamp not supported"))
        }
    }

    /// Returns the creation/status change time as SystemTime.
    pub fn created(&self) -> io::Result<SystemTime> {
        if self.0.ctime >= 0 {
            Ok(UNIX_EPOCH + Duration::from_secs(self.0.ctime as u64))
        } else {
            Err(io::Error::other("negative timestamp not supported"))
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

impl OpenOptions {
    pub fn new() -> Self {
        Self {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
        }
    }

    pub fn read(&mut self, v: bool) -> &mut Self {
        self.read = v;
        self
    }

    pub fn write(&mut self, v: bool) -> &mut Self {
        self.write = v;
        self
    }

    pub fn append(&mut self, v: bool) -> &mut Self {
        self.append = v;
        self
    }

    pub fn truncate(&mut self, v: bool) -> &mut Self {
        self.truncate = v;
        self
    }

    pub fn create(&mut self, v: bool) -> &mut Self {
        self.create = v;
        self
    }

    pub fn create_new(&mut self, v: bool) -> &mut Self {
        self.create_new = v;
        self
    }

    fn validate(&self) -> io::Result<()> {
        if !self.read && !self.write && !self.append {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "must set at least one of read/write/append",
            ));
        }
        if self.append && self.truncate {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "append and truncate cannot be set together",
            ));
        }
        if self.truncate && !self.write {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate requires write",
            ));
        }
        if (self.create || self.create_new) && !(self.write || self.append) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "create/create_new requires write or append",
            ));
        }
        Ok(())
    }

    pub async fn open(&self, client: DynClient, path: impl AsRef<Path>) -> io::Result<File> {
        self.validate()?;
        let path = path_to_str(path)?;

        let mut pre_stat: Option<VfsFileAttr> = None;
        if self.create_new {
            client.create_file(&path, true).await?;
        } else if self.create {
            client.create_file(&path, false).await?;
        } else {
            pre_stat = Some(client.stat(&path).await?);
        }

        if self.truncate {
            if let Some(meta) = pre_stat.as_ref()
                && meta.kind == VfsFileType::Dir
            {
                return Err(io::Error::new(io::ErrorKind::IsADirectory, path));
            }
            client.truncate(&path, 0).await?;
        }

        let meta = match (pre_stat, self.truncate) {
            (Some(meta), false) => meta,
            _ => client.stat(&path).await?,
        };
        if meta.kind == VfsFileType::Dir {
            return Err(io::Error::new(io::ErrorKind::IsADirectory, path));
        }

        let length = meta.size;
        let offset = if self.append { length } else { 0 };

        Ok(File {
            client,
            path,
            opts: self.clone(),
            state: Mutex::new(FileState { offset, length }),
        })
    }
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct FileState {
    offset: u64,
    length: u64,
}

fn append_locks() -> &'static dashmap::DashMap<String, Weak<Mutex<()>>> {
    static LOCKS: OnceLock<dashmap::DashMap<String, Weak<Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(dashmap::DashMap::new)
}

fn append_lock_for(path: &str) -> Arc<Mutex<()>> {
    const APPEND_LOCK_CLEANUP_THRESHOLD: usize = 4096;

    let locks = append_locks();
    if let Some(entry) = locks.get(path)
        && let Some(lock) = entry.value().upgrade()
    {
        return lock;
    }

    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_string(), Arc::downgrade(&lock));

    if locks.len() > APPEND_LOCK_CLEANUP_THRESHOLD {
        locks.retain(|_, v| v.strong_count() > 0);
    }

    lock
}

pub struct File {
    client: DynClient,
    path: String,
    opts: OpenOptions,
    state: Mutex<FileState>,
}

impl File {
    pub async fn metadata(&self) -> io::Result<Metadata> {
        let attr = self.client.stat(&self.path).await?;
        Ok(Metadata(attr))
    }

    pub async fn seek(&self, pos: io::SeekFrom) -> io::Result<u64> {
        let mut state = self.state.lock().await;

        let end = match pos {
            io::SeekFrom::End(_) => {
                let meta = self.client.stat(&self.path).await?;
                state.length = meta.size;
                state.length
            }
            _ => state.length,
        };

        let base: i128 = match pos {
            io::SeekFrom::Start(_) => 0,
            io::SeekFrom::Current(_) => state.offset as i128,
            io::SeekFrom::End(_) => end as i128,
        };

        let delta: i128 = match pos {
            io::SeekFrom::Start(off) => off as i128,
            io::SeekFrom::Current(off) => off as i128,
            io::SeekFrom::End(off) => off as i128,
        };

        let next = base
            .checked_add(delta)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek overflow"))?;
        if next < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid seek to a negative position",
            ));
        }
        state.offset = next as u64;
        Ok(state.offset)
    }

    pub async fn stream_position(&self) -> io::Result<u64> {
        Ok(self.state.lock().await.offset)
    }

    pub async fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        if !self.opts.read {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file not opened for reading",
            ));
        }
        if buf.is_empty() {
            return Ok(0);
        }

        let mut state = self.state.lock().await;
        let data = self
            .client
            .read_at(&self.path, state.offset, buf.len())
            .await?;
        let n = data.len();
        buf[..n].copy_from_slice(&data);
        state.offset += n as u64;
        Ok(n)
    }

    pub async fn read_to_end(&self, out: &mut Vec<u8>) -> io::Result<usize> {
        let mut total = 0usize;
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = self.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
            total += n;
        }
        Ok(total)
    }

    pub async fn write(&self, data: &[u8]) -> io::Result<usize> {
        if !(self.opts.write || self.opts.append) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file not opened for writing",
            ));
        }
        if data.is_empty() {
            return Ok(0);
        }

        if self.opts.append {
            let lock = append_lock_for(&self.path);
            let _guard = lock.lock().await;

            let meta = self.client.stat(&self.path).await?;
            let mut state = self.state.lock().await;
            state.length = meta.size;
            state.offset = state.length;

            let written = self.client.write_at(&self.path, state.offset, data).await?;
            state.offset += written as u64;
            if state.offset > state.length {
                state.length = state.offset;
            }
            return Ok(written);
        }

        let mut state = self.state.lock().await;
        let written = self.client.write_at(&self.path, state.offset, data).await?;
        state.offset += written as u64;
        if state.offset > state.length {
            state.length = state.offset;
        }
        Ok(written)
    }

    pub async fn write_all(&self, mut data: &[u8]) -> io::Result<()> {
        while !data.is_empty() {
            let n = self.write(data).await?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write whole buffer",
                ));
            }
            data = &data[n..];
        }
        Ok(())
    }

    pub async fn set_len(&self, size: u64) -> io::Result<()> {
        if !(self.opts.write || self.opts.append) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "file not opened for writing",
            ));
        }
        self.client.truncate(&self.path, size).await?;
        let mut state = self.state.lock().await;
        state.length = size;
        if state.offset > size {
            state.offset = size;
        }
        Ok(())
    }

    /// Synchronize all file data and metadata to storage.
    ///
    /// In SlayerFS, writes are persisted immediately to object storage,
    /// so this is a no-op but provided for API compatibility.
    pub async fn sync_all(&self) -> io::Result<()> {
        // SlayerFS writes directly to object storage, so data is already persisted.
        // This method exists for API compatibility with std::fs::File.
        Ok(())
    }

    /// Synchronize file data to storage (without metadata).
    ///
    /// In SlayerFS, writes are persisted immediately to object storage,
    /// so this is a no-op but provided for API compatibility.
    pub async fn sync_data(&self) -> io::Result<()> {
        // SlayerFS writes directly to object storage, so data is already persisted.
        Ok(())
    }

    /// Flush internal buffers.
    ///
    /// In SlayerFS, there are no internal buffers to flush,
    /// so this is a no-op but provided for API compatibility.
    pub async fn flush(&self) -> io::Result<()> {
        Ok(())
    }
}

/// Create a single directory (non-recursive).
pub async fn create_dir(client: DynClient, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path_to_str(path)?;
    match client.stat(&path).await {
        Ok(_) => Err(io::Error::new(io::ErrorKind::AlreadyExists, path)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => client.mkdir(&path).await,
        Err(e) => Err(e),
    }
}

/// Create directory recursively (like `mkdir -p`).
pub async fn create_dir_all(client: DynClient, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path_to_str(path)?;
    client.mkdir_p(&path).await
}

pub async fn remove_file(client: DynClient, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path_to_str(path)?;
    client.unlink(&path).await
}

pub async fn remove_dir(client: DynClient, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path_to_str(path)?;
    client.rmdir(&path).await
}

pub async fn rename(
    client: DynClient,
    old: impl AsRef<Path>,
    new: impl AsRef<Path>,
) -> io::Result<()> {
    let old = path_to_str(old)?;
    let new = path_to_str(new)?;
    client.rename(&old, &new).await
}

pub async fn read(client: DynClient, path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let mut file = OpenOptions::new();
    file.read(true);
    let f = file.open(client, path).await?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).await?;
    Ok(out)
}

pub async fn read_to_string(client: DynClient, path: impl AsRef<Path>) -> io::Result<String> {
    let data = read(client, path).await?;
    String::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub async fn write(
    client: DynClient,
    path: impl AsRef<Path>,
    data: impl AsRef<[u8]>,
) -> io::Result<()> {
    let mut opts = OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    let f = opts.open(client, path).await?;
    f.write_all(data.as_ref()).await
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    parent: String,
    inner: VfsDirEntry,
}

impl DirEntry {
    pub fn file_name(&self) -> &str {
        &self.inner.name
    }

    pub fn file_type(&self) -> FileType {
        FileType(self.inner.kind)
    }

    pub fn path(&self) -> String {
        if self.parent == "/" {
            format!("/{}", self.inner.name)
        } else {
            format!("{}/{}", self.parent, self.inner.name)
        }
    }
}

pub struct ReadDir {
    client: DynClient,
    parent: String,
    entries: VecDeque<VfsDirEntry>,
}

impl ReadDir {
    pub async fn next_entry(&mut self) -> io::Result<Option<DirEntry>> {
        match self.entries.pop_front() {
            Some(inner) => Ok(Some(DirEntry {
                parent: self.parent.clone(),
                inner,
            })),
            None => Ok(None),
        }
    }

    pub fn client(&self) -> &DynClient {
        &self.client
    }
}

pub async fn read_dir(client: DynClient, path: impl AsRef<Path>) -> io::Result<ReadDir> {
    let path = path_to_str(path)?;
    let entries = client.readdir(&path).await?;
    Ok(ReadDir {
        client,
        parent: path,
        entries: entries.into(),
    })
}

pub async fn copy(
    client: DynClient,
    src: impl AsRef<Path>,
    dst: impl AsRef<Path>,
) -> io::Result<u64> {
    let src = path_to_str(src)?;
    let dst = path_to_str(dst)?;

    let mut src_opts = OpenOptions::new();
    src_opts.read(true);
    let src_f = src_opts.open(Arc::clone(&client), &src).await?;

    let mut dst_opts = OpenOptions::new();
    dst_opts.write(true).create(true).truncate(true);
    let dst_f = dst_opts.open(Arc::clone(&client), &dst).await?;

    let mut buf = vec![0u8; 128 * 1024];
    let mut copied = 0u64;
    loop {
        let n = src_f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        dst_f.write_all(&buf[..n]).await?;
        copied += n as u64;
    }
    Ok(copied)
}

/// Check whether a path exists.
pub async fn exists(client: DynClient, path: impl AsRef<Path>) -> bool {
    match path_to_str(path) {
        Ok(p) => client.exists(&p).await,
        Err(_) => false,
    }
}

/// Get metadata for a file or directory.
pub async fn metadata(client: DynClient, path: impl AsRef<Path>) -> io::Result<Metadata> {
    let path = path_to_str(path)?;
    let attr = client.stat(&path).await?;
    Ok(Metadata(attr))
}

/// Get metadata without following symlinks.
pub async fn symlink_metadata(client: DynClient, path: impl AsRef<Path>) -> io::Result<Metadata> {
    let path = path_to_str(path)?;
    let attr = client.lstat(&path).await?;
    Ok(Metadata(attr))
}

/// Remove a directory and all its contents recursively.
pub async fn remove_dir_all(client: DynClient, path: impl AsRef<Path>) -> io::Result<()> {
    let path = path_to_str(path)?;
    client.remove_dir_all(&path).await
}

/// Change the permission mode of a file or directory.
pub async fn set_permissions(
    client: DynClient,
    path: impl AsRef<Path>,
    mode: u32,
) -> io::Result<()> {
    let path = path_to_str(path)?;
    let req = SetAttrRequest {
        mode: Some(mode),
        ..Default::default()
    };
    client.set_attr(&path, &req, SetAttrFlags::empty()).await?;
    Ok(())
}

/// Change the owner and group of a file or directory.
pub async fn chown(
    client: DynClient,
    path: impl AsRef<Path>,
    uid: Option<u32>,
    gid: Option<u32>,
) -> io::Result<()> {
    let path = path_to_str(path)?;
    let req = SetAttrRequest {
        uid,
        gid,
        ..Default::default()
    };
    client.set_attr(&path, &req, SetAttrFlags::empty()).await?;
    Ok(())
}

/// Set the access and modification times of a file.
///
/// Times are specified as seconds since UNIX epoch.
/// Use `None` to set to the current time.
pub async fn set_times(
    client: DynClient,
    path: impl AsRef<Path>,
    atime: Option<i64>,
    mtime: Option<i64>,
) -> io::Result<()> {
    let path = path_to_str(path)?;
    let mut flags = SetAttrFlags::empty();
    let mut req = SetAttrRequest::default();

    if let Some(a) = atime {
        req.atime = Some(a);
    } else {
        flags |= SetAttrFlags::SET_ATIME_NOW;
    }

    if let Some(m) = mtime {
        req.mtime = Some(m);
    } else {
        flags |= SetAttrFlags::SET_MTIME_NOW;
    }

    client.set_attr(&path, &req, flags).await?;
    Ok(())
}

/// Get file system statistics.
pub async fn stat_fs(client: DynClient) -> io::Result<StatFsSnapshot> {
    client.stat_fs().await
}

/// Create a hard link.
pub async fn hard_link(
    client: DynClient,
    original: impl AsRef<Path>,
    link: impl AsRef<Path>,
) -> io::Result<()> {
    let original = path_to_str(original)?;
    let link = path_to_str(link)?;
    client.link(&original, &link).await?;
    Ok(())
}

/// Create a symbolic link.
pub async fn symlink(
    client: DynClient,
    original: impl AsRef<Path>,
    link: impl AsRef<Path>,
) -> io::Result<()> {
    let original = path_to_str(original)?;
    let link = path_to_str(link)?;
    client.symlink(&link, &original).await?;
    Ok(())
}

/// Read the target of a symbolic link.
pub async fn read_link(client: DynClient, path: impl AsRef<Path>) -> io::Result<String> {
    let path = path_to_str(path)?;
    client.readlink(&path).await
}

bitflags::bitflags! {
    /// Access mode flags for the `access` function.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct AccessMode: u32 {
        /// Test for existence of file.
        const F_OK = 0;
        /// Test for read permission.
        const R_OK = 4;
        /// Test for write permission.
        const W_OK = 2;
        /// Test for execute permission.
        const X_OK = 1;
    }
}

/// Check user's permissions for a file.
///
/// This is a simplified implementation that checks:
/// - F_OK: file exists
/// - R_OK/W_OK/X_OK: based on file mode bits
///
/// Note: This does not check against the actual calling process's UID/GID,
/// it simply verifies that the corresponding permission bits are set.
pub async fn access(client: DynClient, path: impl AsRef<Path>, mode: AccessMode) -> io::Result<()> {
    let path = path_to_str(path)?;
    let attr = client.stat(&path).await?;

    // F_OK (existence) is satisfied by successfully getting stat
    if mode == AccessMode::F_OK || mode.is_empty() {
        return Ok(());
    }

    let file_mode = attr.mode;

    // Check "other" permission bits (simplified - doesn't check uid/gid)
    // In a real implementation, we'd check against the process's uid/gid
    let other_r = (file_mode & 0o004) != 0;
    let other_w = (file_mode & 0o002) != 0;
    let other_x = (file_mode & 0o001) != 0;

    if mode.contains(AccessMode::R_OK) && !other_r {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "read permission denied",
        ));
    }
    if mode.contains(AccessMode::W_OK) && !other_w {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "write permission denied",
        ));
    }
    if mode.contains(AccessMode::X_OK) && !other_x {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "execute permission denied",
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chuck::chunk::ChunkLayout;
    use crate::vfs::sdk::LocalClient;
    use tempfile::tempdir;

    async fn local_client() -> (tempfile::TempDir, DynClient) {
        let tmp = tempdir().expect("tempdir");
        let layout = ChunkLayout::default();
        let cli = LocalClient::new_local(tmp.path(), layout)
            .await
            .expect("init LocalClient");
        (tmp, Arc::new(cli))
    }

    #[tokio::test]
    async fn open_create_new_fails_if_exists() {
        let (_tmp, client) = local_client().await;

        let mut a = OpenOptions::new();
        a.write(true).create(true);
        let _ = a.open(Arc::clone(&client), "/a.txt").await.unwrap();

        let mut b = OpenOptions::new();
        b.write(true).create_new(true);
        let err = match b.open(Arc::clone(&client), "/a.txt").await {
            Ok(_) => panic!("expected AlreadyExists error"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[tokio::test]
    async fn open_truncate_zeros_file() {
        let (_tmp, client) = local_client().await;

        let mut a = OpenOptions::new();
        a.write(true).create(true).truncate(true);
        let f = a.open(Arc::clone(&client), "/t.txt").await.unwrap();
        f.write_all(b"hello").await.unwrap();

        let mut b = OpenOptions::new();
        b.read(true).write(true).truncate(true);
        let _ = b.open(Arc::clone(&client), "/t.txt").await.unwrap();

        let meta = client.stat("/t.txt").await.unwrap();
        assert_eq!(meta.size, 0);
    }

    #[tokio::test]
    async fn append_writes_to_end() {
        let (_tmp, client) = local_client().await;

        let mut a = OpenOptions::new();
        a.write(true).create(true).truncate(true);
        let f = a.open(Arc::clone(&client), "/app.txt").await.unwrap();
        f.write_all(b"a").await.unwrap();

        let mut b = OpenOptions::new();
        b.append(true).create(true);
        let f2 = b.open(Arc::clone(&client), "/app.txt").await.unwrap();
        f2.write_all(b"b").await.unwrap();

        let s = read_to_string(Arc::clone(&client), "/app.txt")
            .await
            .unwrap();
        assert_eq!(s, "ab");
    }

    #[tokio::test]
    async fn seek_and_read_to_end() {
        let (_tmp, client) = local_client().await;

        let mut a = OpenOptions::new();
        a.write(true).create(true).truncate(true);
        let f = a.open(Arc::clone(&client), "/s.txt").await.unwrap();
        f.write_all(b"hello world").await.unwrap();

        let mut r = OpenOptions::new();
        r.read(true);
        let rf = r.open(Arc::clone(&client), "/s.txt").await.unwrap();
        rf.seek(io::SeekFrom::Start(6)).await.unwrap();
        let mut out = Vec::new();
        rf.read_to_end(&mut out).await.unwrap();
        assert_eq!(out, b"world");
    }

    #[tokio::test]
    async fn dir_ops_and_error_kinds() {
        let (_tmp, client) = local_client().await;

        create_dir_all(Arc::clone(&client), "/d/e").await.unwrap();
        write(Arc::clone(&client), "/d/e/f.txt", b"x")
            .await
            .unwrap();

        let err = remove_dir(Arc::clone(&client), "/d").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::DirectoryNotEmpty);

        let err = match read_dir(Arc::clone(&client), "/d/e/f.txt").await {
            Ok(_) => panic!("expected NotADirectory error"),
            Err(e) => e,
        };
        assert_eq!(err.kind(), io::ErrorKind::NotADirectory);

        let err = {
            let mut opts = OpenOptions::new();
            opts.read(true);
            match opts.open(Arc::clone(&client), "/d").await {
                Ok(_) => panic!("expected IsADirectory error"),
                Err(e) => e,
            }
        };
        assert_eq!(err.kind(), io::ErrorKind::IsADirectory);

        remove_file(Arc::clone(&client), "/d/e/f.txt")
            .await
            .unwrap();
        remove_dir(Arc::clone(&client), "/d/e").await.unwrap();
        remove_dir(Arc::clone(&client), "/d").await.unwrap();
    }

    #[tokio::test]
    async fn permission_denied_on_wrong_mode() {
        let (_tmp, client) = local_client().await;
        write(Arc::clone(&client), "/p.txt", b"hi").await.unwrap();

        let mut ro = OpenOptions::new();
        ro.read(true);
        let f = ro.open(Arc::clone(&client), "/p.txt").await.unwrap();
        let err = f.write_all(b"x").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }

    #[tokio::test]
    async fn test_exists() {
        let (_tmp, client) = local_client().await;

        assert!(!exists(Arc::clone(&client), "/noexist.txt").await);
        write(Arc::clone(&client), "/exist.txt", b"data")
            .await
            .unwrap();
        assert!(exists(Arc::clone(&client), "/exist.txt").await);
        create_dir_all(Arc::clone(&client), "/dir").await.unwrap();
        assert!(exists(Arc::clone(&client), "/dir").await);
    }

    #[tokio::test]
    async fn test_metadata_extended() {
        let (_tmp, client) = local_client().await;

        write(Arc::clone(&client), "/meta.txt", b"hello")
            .await
            .unwrap();
        let meta = metadata(Arc::clone(&client), "/meta.txt").await.unwrap();

        assert_eq!(meta.len(), 5);
        assert!(meta.is_file());
        assert!(!meta.is_dir());
        assert!(meta.mode() > 0);
        assert!(meta.mtime() > 0);
    }

    #[tokio::test]
    async fn test_remove_dir_all() {
        let (_tmp, client) = local_client().await;

        create_dir_all(Arc::clone(&client), "/rm/a/b")
            .await
            .unwrap();
        write(Arc::clone(&client), "/rm/a/b/f1.txt", b"1")
            .await
            .unwrap();
        write(Arc::clone(&client), "/rm/a/f2.txt", b"2")
            .await
            .unwrap();
        create_dir_all(Arc::clone(&client), "/rm/c").await.unwrap();
        write(Arc::clone(&client), "/rm/c/f3.txt", b"3")
            .await
            .unwrap();

        remove_dir_all(Arc::clone(&client), "/rm").await.unwrap();

        assert!(!exists(Arc::clone(&client), "/rm").await);
        assert!(!exists(Arc::clone(&client), "/rm/a").await);
        assert!(!exists(Arc::clone(&client), "/rm/a/b/f1.txt").await);
    }

    #[tokio::test]
    async fn test_remove_dir_all_refuses_root() {
        let (_tmp, client) = local_client().await;

        write(Arc::clone(&client), "/keep.txt", b"keep")
            .await
            .unwrap();
        create_dir_all(Arc::clone(&client), "/keepdir")
            .await
            .unwrap();
        write(Arc::clone(&client), "/keepdir/file.txt", b"x")
            .await
            .unwrap();

        let err = remove_dir_all(Arc::clone(&client), "/").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);

        assert!(exists(Arc::clone(&client), "/keep.txt").await);
        assert!(exists(Arc::clone(&client), "/keepdir").await);
        assert!(exists(Arc::clone(&client), "/keepdir/file.txt").await);
    }

    #[tokio::test]
    async fn test_create_dir_already_exists() {
        let (_tmp, client) = local_client().await;

        create_dir(Arc::clone(&client), "/dir").await.unwrap();
        let err = create_dir(Arc::clone(&client), "/dir").await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[tokio::test]
    async fn test_symlink_operations() {
        let (_tmp, client) = local_client().await;

        write(Arc::clone(&client), "/orig.txt", b"original")
            .await
            .unwrap();

        let result = symlink(Arc::clone(&client), "/orig.txt", "/link.txt").await;
        if result.is_ok() {
            let target = read_link(Arc::clone(&client), "/link.txt").await.unwrap();
            assert_eq!(target, "/orig.txt");

            let link_meta = symlink_metadata(Arc::clone(&client), "/link.txt")
                .await
                .unwrap();
            assert!(link_meta.is_symlink());
        }
    }

    #[tokio::test]
    async fn test_hard_link_operations() {
        let (_tmp, client) = local_client().await;

        write(Arc::clone(&client), "/source.txt", b"content")
            .await
            .unwrap();

        let result = hard_link(Arc::clone(&client), "/source.txt", "/hardlink.txt").await;
        if result.is_ok() {
            let orig_meta = metadata(Arc::clone(&client), "/source.txt").await.unwrap();
            let link_meta = metadata(Arc::clone(&client), "/hardlink.txt")
                .await
                .unwrap();

            assert_eq!(orig_meta.ino(), link_meta.ino());
            assert!(link_meta.nlink() >= 2);
        }
    }
}
