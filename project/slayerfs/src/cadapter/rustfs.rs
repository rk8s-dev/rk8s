//! Minimal rustfs adapter placeholder: also uses a local directory to mock object storage.

use crate::cadapter::client::ObjectBackend;
use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};

#[allow(dead_code)]
#[derive(Clone)]
pub struct RustfsLikeBackend {
    root: PathBuf,
}

#[allow(dead_code)]
impl RustfsLikeBackend {
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
impl ObjectBackend for RustfsLikeBackend {
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
