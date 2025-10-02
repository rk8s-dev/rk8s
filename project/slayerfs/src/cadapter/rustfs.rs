//! rustfs 适配器的最小占位实现：同样用本地目录模拟对象存储。

use crate::cadapter::client::ObjectBackend;
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::{fs, io::AsyncWriteExt};

#[allow(dead_code)]
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
    async fn put_object(
        &self,
        key: &str,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path_for(key);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).await?;
        }
        let mut f = fs::File::create(path).await?;
        f.write_all(data).await?;
        f.flush().await?;
        Ok(())
    }

    async fn get_object(
        &self,
        key: &str,
    ) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path_for(key);
        match fs::read(path).await {
            Ok(buf) => Ok(Some(buf)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(Box::new(e)),
        }
    }

    async fn get_etag(
        &self,
        key: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path_for(key);
        match fs::metadata(path).await {
            Ok(metadata) => {
                let modified = metadata.modified()?;
                Ok(format!("{modified:?}"))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("".to_string()),
            Err(e) => Err(Box::new(e)),
        }
    }

    async fn delete_object(
        &self,
        key: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let path = self.path_for(key);
        match fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Box::new(e)),
        }
    }
}
