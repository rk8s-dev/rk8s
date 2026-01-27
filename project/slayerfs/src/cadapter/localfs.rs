//! Local filesystem backend used to mock an object store (implements `ObjectBackend`).

use crate::cadapter::client::ObjectBackend;
use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;
use std::io::{IoSlice, Write};
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};

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
    #[tracing::instrument(level = "trace", skip(self, chunks), fields(key, chunk_count = chunks.len()))]
    async fn put_object_vectored(&self, key: &str, chunks: Vec<Bytes>) -> Result<()> {
        let path = self.path_for(key);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).await?;
        }

        // `tokio::fs::File::write_vectored` is another option. However, according the implementation
        // of it, it performs an extra copy operation during writing. So using `std::fs::File::write_vectored`
        // + `spawn_blocking` is the best solution for current situation.
        let res = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            let mut f = std::fs::File::create(path)?;
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
                IoSlice::advance_slices(&mut slices_ref, n);
            }

            f.flush()?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("blocking write failed: {e}"))?;

        res?;
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
}
