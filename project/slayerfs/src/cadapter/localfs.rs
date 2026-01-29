//! Local filesystem backend used to mock an object store (implements `ObjectBackend`).

use crate::cadapter::client::ObjectBackend;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::io::{IoSlice, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::{fs, io::AsyncWriteExt};
use tracing::{Instrument, field};

#[derive(Clone)]
pub struct LocalFsBackend {
    root: PathBuf,
}

impl LocalFsBackend {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
    fn path_for(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }
}

#[async_trait]
impl ObjectBackend for LocalFsBackend {
    #[tracing::instrument(
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
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .instrument(tracing::trace_span!("put_object_vectored.create_dir"))
                .await?;
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
            fs::create_dir_all(dir).await?;
        }
        let mut f = fs::File::create(path).await?;
        f.write_all(data).await?;
        f.flush().await?;
        Ok(())
    }

    async fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(key);
        match fs::read(path).await {
            Ok(buf) => Ok(Some(buf)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
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

    async fn sync_all(&self) -> Result<()> {
        let root = self.root.clone();

        tokio::task::spawn_blocking(move || -> Result<()> {
            fn sync_tree(path: &Path) -> Result<()> {
                if !path.exists() {
                    return Ok(());
                }
                if path.is_dir() {
                    for entry in std::fs::read_dir(path)? {
                        let entry = entry?;
                        sync_tree(&entry.path())?;
                    }
                    if let Ok(dir) = std::fs::File::open(path) {
                        let _ = dir.sync_all();
                    }
                } else {
                    let file = std::fs::File::open(path)?;
                    file.sync_all()?;
                }
                Ok(())
            }

            sync_tree(&root)
        })
        .await
        .map_err(|e| anyhow::anyhow!("sync_all task failed: {e}"))??;
        Ok(())
    }
}
