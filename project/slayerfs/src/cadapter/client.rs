//! High-level object client wrapping backend put/get operations.

use anyhow::Result;
use async_trait::async_trait;
use bytes::Bytes;

#[async_trait]
pub trait ObjectBackend: Send + Sync {
    async fn put_object_vectored(&self, key: &str, chunks: Vec<Bytes>) -> Result<()> {
        let data = chunks
            .into_iter()
            .flat_map(|e| e.to_vec())
            .collect::<Vec<_>>();
        self.put_object(key, &data).await
    }

    async fn put_object(&self, key: &str, data: &[u8]) -> Result<()>;

    async fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>>;

    async fn get_object_range(&self, key: &str, offset: u64, buf: &mut [u8]) -> Result<usize>;

    #[allow(dead_code)]
    async fn get_etag(&self, key: &str) -> Result<String>;

    #[allow(dead_code)]
    async fn delete_object(&self, key: &str) -> Result<()>;
}

#[derive(Clone)]
pub struct ObjectClient<B: ObjectBackend> {
    backend: B,
}

impl<B: ObjectBackend> ObjectClient<B> {
    pub fn new(backend: B) -> Self {
        Self { backend }
    }

    pub async fn put_object(&self, key: &str, data: &[u8]) -> Result<()> {
        self.backend.put_object(key, data).await
    }

    pub async fn put_object_vectored(&self, key: &str, chunks: Vec<Bytes>) -> Result<()> {
        self.backend.put_object_vectored(key, chunks).await
    }

    pub async fn get_object(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.backend.get_object(key).await
    }

    pub async fn get_object_range(&self, key: &str, offset: u64, buf: &mut [u8]) -> Result<usize> {
        self.backend.get_object_range(key, offset, buf).await
    }

    #[allow(dead_code)]
    pub async fn get_etag(&self, key: &str) -> Result<String> {
        self.backend.get_etag(key).await
    }

    #[allow(dead_code)]
    pub async fn delete_object(&self, key: &str) -> Result<()> {
        self.backend.delete_object(key).await
    }
}
