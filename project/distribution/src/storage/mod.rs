use crate::error::{AppError, MapToAppError, OciError};
use axum::body::BodyDataStream;
use bytes::{Bytes, BytesMut};
use futures::stream::StreamExt;
use oci_spec::image::Digest;
use sha2::Digest as _;
use std::pin::Pin;
use tokio::io::AsyncReadExt;

pub mod driver;
pub mod paths;

type Result<T> = std::result::Result<T, AppError>;

pub type ObjectStream =
    Pin<Box<dyn futures::Stream<Item = std::result::Result<Bytes, std::io::Error>> + Send>>;

pub struct StorageObject {
    pub stream: ObjectStream,
    pub size: u64,
}

const MAX_MANIFEST_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

impl StorageObject {
    pub async fn into_bytes(self) -> std::result::Result<Bytes, std::io::Error> {
        if self.size > MAX_MANIFEST_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "object too large to buffer: {} bytes (max {})",
                    self.size, MAX_MANIFEST_SIZE
                ),
            ));
        }
        let mut buf = BytesMut::with_capacity(self.size as usize);
        let mut stream = self.stream;
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk?);
            if buf.len() as u64 > MAX_MANIFEST_SIZE {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "stream exceeded maximum buffer size",
                ));
            }
        }
        Ok(buf.freeze())
    }
}

/// Compute SHA-256 of a local file and verify it matches the expected digest.
pub async fn verify_file_digest(path: &str, expected: &Digest) -> Result<()> {
    if expected.algorithm().as_ref() != "sha256" {
        return Err(OciError::DigestInvalid(format!(
            "unsupported algorithm: {}",
            expected.algorithm()
        ))
        .into());
    }

    let mut file = tokio::fs::File::open(path).await.map_to_internal()?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).await.map_to_internal()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual_hex = hex::encode(hasher.finalize());

    if actual_hex != expected.digest() {
        return Err(OciError::DigestInvalid(format!(
            "computed sha256:{actual_hex} does not match expected {expected}"
        ))
        .into());
    }
    Ok(())
}

#[allow(dead_code)]
#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn get_blob(&self, digest: &Digest) -> Result<StorageObject>;

    async fn get_blob_range(
        &self,
        digest: &Digest,
        range: std::ops::Range<u64>,
    ) -> Result<StorageObject>;

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
