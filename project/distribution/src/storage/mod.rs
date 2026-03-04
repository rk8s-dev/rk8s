use crate::error::AppError;
use axum::body::BodyDataStream;
use bytes::{Bytes, BytesMut};
use futures::stream::StreamExt;
use oci_spec::image::Digest;
use std::pin::Pin;

pub mod driver;
pub mod paths;

type Result<T> = std::result::Result<T, AppError>;

pub type ObjectStream =
    Pin<Box<dyn futures::Stream<Item = std::result::Result<Bytes, std::io::Error>> + Send>>;

pub struct StorageObject {
    pub stream: ObjectStream,
    pub size: u64,
}

impl StorageObject {
    pub async fn into_bytes(self) -> std::result::Result<Bytes, std::io::Error> {
        let mut buf = BytesMut::with_capacity(self.size as usize);
        let mut stream = self.stream;
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk?);
        }
        Ok(buf.freeze())
    }
}

#[allow(dead_code)]
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn get_blob(&self, digest: &Digest) -> Result<StorageObject>;

    async fn blob_exists(&self, digest: &Digest) -> Result<bool>;

    async fn blob_size(&self, digest: &Digest) -> Result<u64>;

    async fn resolve_tag(&self, name: &str, tag: &str) -> Result<Digest>;

    async fn put_blob(&self, digest: &Digest, stream: BodyDataStream) -> Result<u64>;

    async fn write_upload_chunk(&self, session_id: &str, stream: BodyDataStream) -> Result<u64>;

    async fn finalize_upload(&self, session_id: &str, digest: &Digest) -> Result<()>;

    async fn put_tag(&self, name: &str, tag: &str, digest: &Digest) -> Result<()>;

    async fn list_tags(&self, name: &str) -> Result<Vec<String>>;

    async fn delete_tag(&self, name: &str, tag: &str) -> Result<()>;

    async fn delete_blob(&self, digest: &Digest) -> Result<()>;
}
