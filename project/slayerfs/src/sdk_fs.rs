use async_trait::async_trait;
use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

use crate::vfs::fs::{DirEntry as VfsDirEntry, FileAttr as VfsFileAttr, FileType as VfsFileType};

#[async_trait]
pub trait SdkClient: Send + Sync + 'static {
    async fn mkdir_p(&self, path: &str) -> io::Result<()>;
    async fn create_file(&self, path: &str, create_new: bool) -> io::Result<()>;
    async fn write_at(&self, path: &str, offset: u64, data: &[u8]) -> io::Result<usize>;
    async fn read_at(&self, path: &str, offset: u64, len: usize) -> io::Result<Vec<u8>>;
    async fn readdir(&self, path: &str) -> io::Result<Vec<VfsDirEntry>>;
    async fn stat(&self, path: &str) -> io::Result<VfsFileAttr>;
    async fn unlink(&self, path: &str) -> io::Result<()>;
    async fn rmdir(&self, path: &str) -> io::Result<()>;
    async fn rename(&self, old: &str, new: &str) -> io::Result<()>;
    async fn truncate(&self, path: &str, size: u64) -> io::Result<()>;
}

pub type DynClient = Arc<dyn SdkClient>;

#[async_trait]
impl<S, M> SdkClient for crate::vfs::sdk::Client<S, M>
where
    S: crate::chuck::store::BlockStore + Send + Sync + 'static,
    M: crate::meta::MetaStore + Send + Sync + 'static,
{
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
}

fn path_to_str(path: impl AsRef<Path>) -> io::Result<String> {
    let s = path.as_ref().to_string_lossy().to_string();
    if s.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty path"));
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

#[derive(Debug, Clone)]
pub struct Metadata(VfsFileAttr);

impl Metadata {
    pub fn len(&self) -> u64 {
        self.0.size
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn file_type(&self) -> FileType {
        FileType(self.0.kind)
    }

    pub fn is_file(&self) -> bool {
        self.0.kind == VfsFileType::File
    }

    pub fn is_dir(&self) -> bool {
        self.0.kind == VfsFileType::Dir
    }

    pub fn is_symlink(&self) -> bool {
        self.0.kind == VfsFileType::Symlink
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
        if self.truncate && !(self.write || self.append) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "truncate requires write or append",
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

        if self.create_new {
            client.create_file(&path, true).await?;
        } else if self.create {
            client.create_file(&path, false).await?;
        } else {
            let _ = client.stat(&path).await?;
        }

        if self.truncate {
            client.truncate(&path, 0).await?;
        }

        let meta = client.stat(&path).await?;
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

fn append_locks() -> &'static dashmap::DashMap<String, Arc<Mutex<()>>> {
    static LOCKS: OnceLock<dashmap::DashMap<String, Arc<Mutex<()>>>> = OnceLock::new();
    LOCKS.get_or_init(dashmap::DashMap::new)
}

fn append_lock_for(path: &str) -> Arc<Mutex<()>> {
    let locks = append_locks();
    if let Some(lock) = locks.get(path) {
        Arc::clone(lock.value())
    } else {
        let lock = Arc::new(Mutex::new(()));
        locks.insert(path.to_string(), Arc::clone(&lock));
        lock
    }
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
}

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
}
