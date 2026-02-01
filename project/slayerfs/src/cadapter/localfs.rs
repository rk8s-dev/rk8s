//! Local filesystem backend used to mock an object store (implements `ObjectBackend`).
//!

#[cfg(unix)]
use std::os::unix::fs::FileExt;

#[cfg(windows)]
use std::os::windows::fs::FileExt;

use crate::cadapter::client::ObjectBackend;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashSet;
use std::io::{IoSlice, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::{fs, io::AsyncWriteExt};
use tracing::field;

fn can_block_in_place() -> bool {
    tokio::runtime::Handle::try_current()
        .map(|handle| {
            matches!(
                handle.runtime_flavor(),
                tokio::runtime::RuntimeFlavor::MultiThread
            )
        })
        .unwrap_or(false)
}

#[derive(Clone)]
pub struct LocalFsBackend {
    root: PathBuf,
    created_dirs: Arc<DashSet<PathBuf>>,
}

impl LocalFsBackend {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            created_dirs: Arc::new(DashSet::new()),
        }
    }
    fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    async fn ensure_dir(&self, dir: &Path) -> Result<()> {
        if self.created_dirs.contains(dir) {
            return Ok(());
        }

        fs::create_dir_all(dir).await?;
        self.created_dirs.insert(dir.to_path_buf());
        Ok(())
    }
}

#[async_trait]
impl ObjectBackend for LocalFsBackend {
    #[tracing::instrument(
        name = "LocalFs.put_object_vectored",
        level = "trace",
        skip(self, chunks),
        fields(
            key,
            chunk_count = chunks.len(),
            total_bytes = field::Empty,
            queue_ms = field::Empty,
            open_ms = field::Empty,
            write_ms = field::Empty,
            flush_ms = field::Empty,
            write_calls = field::Empty,
            bytes_written = field::Empty
        )
    )]
    async fn put_object_vectored(&self, key: &str, chunks: Vec<Bytes>) -> Result<()> {
        let total_bytes = chunks.iter().map(|c| c.len()).sum::<usize>() as u64;
        tracing::Span::current().record("total_bytes", total_bytes);

        let path = self.path_for(key);

        if let Some(parent) = path.parent() {
            self.ensure_dir(parent).await?;
        }

        // `tokio::fs::File::write_vectored` is another option. However, according the implementation
        // of it, it performs an extra copy operation during writing. So using `std::fs::File::write_vectored`
        // + `spawn_blocking` is the best solution for current situation.
        #[derive(Debug)]
        struct WriteStats {
            queue_ms: u64,
            open_ms: u64,
            write_ms: u64,
            flush_ms: u64,
            write_calls: u64,
            bytes_written: u64,
        }

        let submit_at = Instant::now();
        let res = tokio::task::spawn_blocking(move || -> std::io::Result<WriteStats> {
            let start = Instant::now();
            let queue_ms = start.duration_since(submit_at).as_millis() as u64;

            let open_start = Instant::now();
            let mut f = std::fs::File::create(path)?;
            let open_ms = open_start.elapsed().as_millis() as u64;

            let write_start = Instant::now();
            let mut write_calls: u64 = 0;
            let mut bytes_written: u64 = 0;
            let mut slices = chunks
                .iter()
                .map(|e| IoSlice::new(e.as_ref()))
                .collect::<Vec<_>>();

            let mut slices_ref = slices.as_mut_slice();
            while !slices_ref.is_empty() {
                let n = f.write_vectored(slices_ref)?;
                if n == 0 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WriteZero,
                        "write zero",
                    ));
                }
                write_calls = write_calls.saturating_add(1);
                bytes_written = bytes_written.saturating_add(n as u64);
                IoSlice::advance_slices(&mut slices_ref, n);
            }
            let write_ms = write_start.elapsed().as_millis() as u64;

            let flush_start = Instant::now();
            f.flush()?;
            let flush_ms = flush_start.elapsed().as_millis() as u64;

            Ok(WriteStats {
                queue_ms,
                open_ms,
                write_ms,
                flush_ms,
                write_calls,
                bytes_written,
            })
        })
        .await
        .map_err(|e| anyhow::anyhow!("blocking write failed: {e}"))?;

        let stats = res?;
        let span = tracing::Span::current();
        span.record("queue_ms", stats.queue_ms);
        span.record("open_ms", stats.open_ms);
        span.record("write_ms", stats.write_ms);
        span.record("flush_ms", stats.flush_ms);
        span.record("write_calls", stats.write_calls);
        span.record("bytes_written", stats.bytes_written);
        Ok(())
    }

    async fn put_object(&self, key: &str, data: &[u8]) -> Result<()> {
        let path = self.path_for(key);

        if let Some(dir) = path.parent() {
            self.ensure_dir(dir).await?;
        }

        let mut f = fs::File::create(path).await?;
        f.write_all(data).await?;
        f.flush().await?;
        Ok(())
    }

    #[tracing::instrument(name = "LocalFsBackend.get_object", level = "trace", skip(self))]
    async fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(key);

        match fs::read(path).await {
            Ok(buf) => Ok(Some(buf)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    #[tracing::instrument(
        name = "LocalFsBackend.get_object_range",
        level = "trace",
        skip(self, buf),
        fields(key, offset, len = buf.len())
    )]
    async fn get_object_range(&self, key: &str, offset: u64, buf: &mut [u8]) -> Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let path = self.path_for(key);
        let len = buf.len();

        if !can_block_in_place() {
            let read = tokio::task::spawn_blocking(move || -> Result<Option<Vec<u8>>> {
                let file = match std::fs::File::open(&path) {
                    Ok(file) => file,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                    Err(e) => return Err(e.into()),
                };
                let mut local = vec![0u8; len];
                let n = file.read_at(&mut local, offset)?;
                local.truncate(n);
                Ok(Some(local))
            })
            .await
            .map_err(|e| anyhow::anyhow!("blocking get_object_range failed: {e}"))??;

            if let Some(data) = read {
                let n = data.len();
                buf[..n].copy_from_slice(&data);
                return Ok(n);
            }
            return Ok(0);
        }

        // Use block_in_place + read_at to fill caller's buffer directly (avoid alloc+copy).
        tokio::task::block_in_place(|| -> Result<usize> {
            let file = match std::fs::File::open(&path) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
                Err(e) => return Err(e.into()),
            };

            let mut read = 0usize;
            while read < len {
                let n = file.read_at(&mut buf[read..], offset + read as u64)?;
                if n == 0 {
                    break;
                }
                read += n;
            }
            Ok(read)
        })
    }

    async fn get_etag(&self, key: &str) -> Result<String> {
        let path = self.path_for(key);

        match fs::metadata(path).await {
            Ok(metadata) => {
                let modified = metadata.modified()?;
                Ok(format!("{modified:?}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("".to_string()),
            Err(e) => Err(e.into()),
        }
    }

    async fn delete_object(&self, key: &str) -> Result<()> {
        let path = self.path_for(key);

        match fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
