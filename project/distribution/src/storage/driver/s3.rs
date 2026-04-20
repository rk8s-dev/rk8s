use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use bytes::Bytes;
use futures::TryStreamExt;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt, PutPayload};
use oci_spec::image::Digest;
use tokio::fs::{File, OpenOptions};
use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio_util::io::StreamReader;

use axum::body::BodyDataStream;

use crate::config::S3Config;
use crate::error::{AppError, InternalError, MapToAppError, OciError};
use crate::storage::paths::PathManager;
use crate::storage::{ObjectStream, Storage, StorageObject, verify_file_digest};

const MULTIPART_THRESHOLD: u64 = 8 * 1024 * 1024;
const MULTIPART_CHUNK_SIZE: usize = 8 * 1024 * 1024;

pub struct S3Storage {
    store: Arc<dyn ObjectStore>,
    path_manager: PathManager,
    temp_dir: String,
}

impl S3Storage {
    pub fn new(config: &S3Config) -> std::result::Result<Self, AppError> {
        let mut builder = AmazonS3Builder::new()
            .with_region(&config.region)
            .with_bucket_name(&config.bucket)
            .with_access_key_id(&config.access_key_id)
            .with_secret_access_key(&config.secret_access_key);

        if !config.endpoint.is_empty() {
            builder = builder.with_endpoint(&config.endpoint);
        }
        if config.allow_http {
            builder = builder.with_allow_http(true);
        }
        if config.path_style {
            builder = builder.with_virtual_hosted_style_request(false);
        }

        let store = builder
            .build()
            .map_err(|e| InternalError::Others(format!("Failed to build S3 client: {e}")))?;

        tracing::info!(
            "S3 storage initialized: bucket={}, region={}, endpoint={}",
            config.bucket,
            config.region,
            if config.endpoint.is_empty() {
                "default"
            } else {
                &config.endpoint
            }
        );

        Ok(S3Storage {
            store: Arc::new(store),
            path_manager: PathManager::new(""),
            temp_dir: std::env::temp_dir()
                .join("oci-uploads")
                .to_string_lossy()
                .to_string(),
        })
    }

    fn to_object_path(&self, key: &str) -> ObjectPath {
        let trimmed = key.trim_start_matches('/');
        ObjectPath::from(trimmed)
    }

    fn normalize_session_id(&self, session_id: &str) -> std::result::Result<String, AppError> {
        uuid::Uuid::parse_str(session_id)
            .map(|id| id.to_string())
            .map_err(|_| OciError::BlobUploadUnknown(session_id.to_string()).into())
    }

    fn upload_dir_path(&self, session_id: &str) -> std::path::PathBuf {
        Path::new(&self.temp_dir).join(session_id)
    }

    fn upload_temp_path(&self, session_id: &str) -> String {
        self.upload_dir_path(session_id)
            .join("data")
            .to_string_lossy()
            .to_string()
    }

    async fn cleanup_temp_files(&self, session_id: &str, temp_path: &str) {
        if let Err(e) = tokio::fs::remove_file(temp_path).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("failed to clean up temp file {temp_path}: {e}");
        }
        if let Err(e) = tokio::fs::remove_dir_all(self.upload_dir_path(session_id)).await
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!("failed to clean up upload dir for session {session_id}: {e}");
        }
    }

    async fn do_finalize_upload(&self, temp_path: &str, s3_path: &ObjectPath) -> Result<()> {
        let file_size = tokio::fs::metadata(temp_path)
            .await
            .map_to_internal()?
            .len();

        if file_size <= MULTIPART_THRESHOLD {
            let data = tokio::fs::read(temp_path).await.map_to_internal()?;
            self.store
                .put(s3_path, PutPayload::from_bytes(Bytes::from(data)))
                .await
                .map_err(|e| InternalError::Others(format!("S3 put error: {e}")))?;
        } else {
            let mut upload = self
                .store
                .put_multipart(s3_path)
                .await
                .map_err(|e| InternalError::Others(format!("S3 multipart init error: {e}")))?;

            if let Err(e) = self.upload_parts(&mut upload, temp_path).await {
                let _ = upload.abort().await;
                return Err(e);
            }

            if let Err(e) = upload.complete().await {
                let _ = upload.abort().await;
                return Err(
                    InternalError::Others(format!("S3 multipart complete error: {e}")).into(),
                );
            }
        }

        Ok(())
    }

    async fn upload_parts(
        &self,
        upload: &mut Box<dyn object_store::MultipartUpload>,
        temp_path: &str,
    ) -> Result<()> {
        let file = File::open(temp_path).await.map_to_internal()?;
        let mut reader = tokio::io::BufReader::new(file);
        let mut buf = vec![0u8; MULTIPART_CHUNK_SIZE];
        let mut filled = 0usize;

        loop {
            let n = tokio::io::AsyncReadExt::read(&mut reader, &mut buf[filled..])
                .await
                .map_to_internal()?;
            if n == 0 {
                if filled > 0 {
                    upload
                        .put_part(PutPayload::from_bytes(Bytes::copy_from_slice(
                            &buf[..filled],
                        )))
                        .await
                        .map_err(|e| InternalError::Others(format!("S3 upload part error: {e}")))?;
                }
                break;
            }
            filled += n;
            if filled >= MULTIPART_CHUNK_SIZE {
                upload
                    .put_part(PutPayload::from_bytes(Bytes::copy_from_slice(
                        &buf[..filled],
                    )))
                    .await
                    .map_err(|e| InternalError::Others(format!("S3 upload part error: {e}")))?;
                filled = 0;
            }
        }

        Ok(())
    }
}

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
impl Storage for S3Storage {
    async fn get_blob(&self, digest: &Digest) -> Result<StorageObject> {
        let key = self.path_manager.blob_data_path(digest);
        let path = self.to_object_path(&key);

        let result = self.store.get(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => {
                AppError::Oci(OciError::BlobUnknown(digest.to_string()))
            }
            other => AppError::Internal(InternalError::Others(format!("S3 get error: {other}"))),
        })?;

        let size = result.meta.size;
        let stream: ObjectStream = Box::pin(result.into_stream().map_err(std::io::Error::other));

        Ok(StorageObject { stream, size })
    }

    async fn blob_exists(&self, digest: &Digest) -> Result<bool> {
        let key = self.path_manager.blob_data_path(digest);
        let path = self.to_object_path(&key);

        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(InternalError::Others(format!("S3 head error: {e}")).into()),
        }
    }

    async fn blob_size(&self, digest: &Digest) -> Result<u64> {
        let key = self.path_manager.blob_data_path(digest);
        let path = self.to_object_path(&key);

        match self.store.head(&path).await {
            Ok(meta) => Ok(meta.size),
            Err(object_store::Error::NotFound { .. }) => {
                Err(OciError::BlobUnknown(digest.to_string()).into())
            }
            Err(e) => Err(InternalError::Others(format!("S3 head error: {e}")).into()),
        }
    }

    async fn resolve_tag(&self, name: &str, tag: &str) -> Result<Digest> {
        let key = self.path_manager.manifest_tag_link_path(name, tag);
        let path = self.to_object_path(&key);

        let result = self.store.get(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => {
                AppError::Oci(OciError::ManifestUnknown(format!("{name}:{tag}")))
            }
            other => {
                AppError::Internal(InternalError::Others(format!("S3 get tag error: {other}")))
            }
        })?;

        let data = result
            .bytes()
            .await
            .map_err(|e| InternalError::Others(format!("S3 read tag error: {e}")))?;
        let content = String::from_utf8_lossy(&data);

        Digest::from_str(content.trim())
            .map_err(|_| OciError::DigestInvalid(content.trim().to_string()).into())
    }

    async fn put_blob(&self, digest: &Digest, stream: BodyDataStream) -> Result<u64> {
        let body_with_io_error = stream.map_err(io::Error::other);
        let mut body_reader = StreamReader::new(body_with_io_error);

        let session_id = uuid::Uuid::new_v4().to_string();
        let temp_path = self.upload_temp_path(&session_id);
        let file_path = std::path::Path::new(&temp_path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_to_internal()?;
        }

        let file = File::create(&temp_path).await.map_to_internal()?;
        let mut file_writer = BufWriter::new(file);
        let size = match io::copy(&mut body_reader, &mut file_writer).await {
            Ok(size) => size,
            Err(error) => {
                drop(file_writer.into_inner());
                self.cleanup_temp_files(&session_id, &temp_path).await;
                return Err(InternalError::from(error).into());
            }
        };
        if let Err(error) = file_writer.shutdown().await {
            drop(file_writer.into_inner());
            self.cleanup_temp_files(&session_id, &temp_path).await;
            return Err(InternalError::from(error).into());
        }

        self.finalize_upload(&session_id, digest).await?;

        Ok(size)
    }

    async fn write_upload_chunk(&self, session_id: &str, stream: BodyDataStream) -> Result<u64> {
        let session_id = self.normalize_session_id(session_id)?;
        let body_with_io_error = stream.map_err(io::Error::other);
        let mut body_reader = StreamReader::new(body_with_io_error);

        let temp_path = self.upload_temp_path(&session_id);
        let file_path = std::path::Path::new(&temp_path);
        if let Some(parent) = file_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_to_internal()?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&temp_path)
            .await
            .map_to_internal()?;
        // Preserve chunks accepted before this PATCH; only roll back bytes
        // appended by the current request if its body stream fails.
        let original_len = file.metadata().await.map_to_internal()?.len();

        let mut file_writer = BufWriter::new(file);

        let size = match io::copy(&mut body_reader, &mut file_writer).await {
            Ok(size) => size,
            Err(error) => {
                let file = file_writer.into_inner();
                file.set_len(original_len).await.map_to_internal()?;
                return Err(InternalError::from(error).into());
            }
        };
        if let Err(error) = file_writer.shutdown().await {
            let file = file_writer.into_inner();
            file.set_len(original_len).await.map_to_internal()?;
            return Err(InternalError::from(error).into());
        }

        Ok(size)
    }

    async fn finalize_upload(&self, session_id: &str, digest: &Digest) -> Result<()> {
        let session_id = self.normalize_session_id(session_id)?;
        let temp_path = self.upload_temp_path(&session_id);

        if let Err(e) = verify_file_digest(&temp_path, digest).await {
            self.cleanup_temp_files(&session_id, &temp_path).await;
            return Err(e);
        }

        let key = self.path_manager.blob_data_path(digest);
        let s3_path = self.to_object_path(&key);

        let result = self.do_finalize_upload(&temp_path, &s3_path).await;
        self.cleanup_temp_files(&session_id, &temp_path).await;

        result
    }

    async fn put_tag(&self, name: &str, tag: &str, digest: &Digest) -> Result<()> {
        let key = self.path_manager.manifest_tag_link_path(name, tag);
        let path = self.to_object_path(&key);

        self.store
            .put(
                &path,
                PutPayload::from_bytes(Bytes::from(digest.to_string())),
            )
            .await
            .map_err(|e| InternalError::Others(format!("S3 put tag error: {e}")))?;

        Ok(())
    }

    async fn list_tags(&self, name: &str) -> Result<Vec<String>> {
        let key = self.path_manager.manifest_tags_path(name);
        let prefix = self.to_object_path(&format!("{key}/"));

        let list_result = self
            .store
            .list_with_delimiter(Some(&prefix))
            .await
            .map_err(|e| InternalError::Others(format!("S3 list error: {e}")))?;

        let mut tags: Vec<String> = list_result
            .common_prefixes
            .iter()
            .filter_map(|p| {
                p.as_ref()
                    .strip_prefix(prefix.as_ref())
                    .and_then(|s| s.strip_suffix('/'))
                    .map(|s| s.to_string())
            })
            .collect();

        tags.sort();
        Ok(tags)
    }

    async fn delete_tag(&self, name: &str, tag: &str) -> Result<()> {
        let key = self.path_manager.manifest_tag_link_path(name, tag);
        let path = self.to_object_path(&key);

        self.store.delete(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => {
                AppError::Oci(OciError::ManifestUnknown(tag.to_string()))
            }
            other => AppError::Internal(InternalError::Others(format!("S3 delete error: {other}"))),
        })?;

        Ok(())
    }

    async fn delete_blob(&self, digest: &Digest) -> Result<()> {
        let key = self.path_manager.blob_data_path(digest);
        let path = self.to_object_path(&key);

        self.store.delete(&path).await.map_err(|e| match e {
            object_store::Error::NotFound { .. } => {
                AppError::Oci(OciError::BlobUnknown(digest.to_string()))
            }
            other => AppError::Internal(InternalError::Others(format!("S3 delete error: {other}"))),
        })?;

        Ok(())
    }
}
