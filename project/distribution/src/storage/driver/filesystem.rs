use std::path::PathBuf;
use std::str::FromStr;

use crate::error::{AppError, InternalError, MapToAppError, OciError};
use crate::storage::paths::PathManager;
use crate::storage::{ObjectStream, Storage, StorageObject, verify_file_digest};
use axum::body::BodyDataStream;
use futures::TryStreamExt;
use oci_spec::image::Digest;
use tokio::fs::{File, OpenOptions, create_dir_all, read_dir, remove_dir_all};
use tokio::io::{self, AsyncWriteExt, BufWriter};
use tokio_util::io::{ReaderStream, StreamReader};

pub struct FilesystemStorage {
    path_manager: PathManager,
}

impl FilesystemStorage {
    pub fn new(root: &str) -> Self {
        FilesystemStorage {
            path_manager: PathManager::new(root),
        }
    }

    async fn ensure_parent_dir(&self, path: &str) -> Result<PathBuf> {
        let file_path = std::path::Path::new(path).to_path_buf();
        if let Some(parent) = file_path.parent() {
            create_dir_all(parent).await.map_to_internal()?;
        }
        Ok(file_path)
    }

    async fn cleanup_upload_dir(&self, session_id: &str) {
        let upload_path = self.path_manager.upload_path(session_id);
        if let Err(error) = remove_dir_all(&upload_path).await
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!("failed to clean up upload dir {upload_path}: {error}");
        }
    }
}

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
impl Storage for FilesystemStorage {
    async fn get_blob(&self, digest: &Digest) -> Result<StorageObject> {
        let path = self.path_manager.blob_data_path(digest);

        let file = match File::open(&path).await {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(OciError::BlobUnknown(digest.to_string()).into());
            }
            Err(e) => return Err(InternalError::from(e).into()),
        };

        let size = file.metadata().await.map_to_internal()?.len();
        let stream: ObjectStream = Box::pin(ReaderStream::new(file));

        Ok(StorageObject { stream, size })
    }

    async fn blob_exists(&self, digest: &Digest) -> Result<bool> {
        let path = self.path_manager.blob_data_path(digest);
        match tokio::fs::metadata(path).await {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn blob_size(&self, digest: &Digest) -> Result<u64> {
        let path = self.path_manager.blob_data_path(digest);
        match tokio::fs::metadata(&path).await {
            Ok(meta) => Ok(meta.len()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::BlobUnknown(digest.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn resolve_tag(&self, name: &str, tag: &str) -> Result<Digest> {
        let link_path = self.path_manager.manifest_tag_link_path(name, tag);

        let content = match tokio::fs::read_to_string(&link_path).await {
            Ok(c) => c,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Err(OciError::ManifestUnknown(format!("{name}:{tag}")).into());
            }
            Err(e) => return Err(InternalError::from(e).into()),
        };

        Digest::from_str(content.trim())
            .map_err(|_| OciError::DigestInvalid(content.trim().to_string()).into())
    }

    async fn put_blob(&self, digest: &Digest, stream: BodyDataStream) -> Result<u64> {
        let body_with_io_error = stream.map_err(io::Error::other);
        let mut body_reader = StreamReader::new(body_with_io_error);

        let session_id = uuid::Uuid::new_v4().to_string();
        let upload_data_path = self.path_manager.upload_data_path(&session_id);
        let file_path = self.ensure_parent_dir(&upload_data_path).await?;

        let file = File::create(file_path).await.map_to_internal()?;
        let mut file_writer = BufWriter::new(file);

        let size = match io::copy(&mut body_reader, &mut file_writer).await {
            Ok(size) => size,
            Err(error) => {
                drop(file_writer.into_inner());
                self.cleanup_upload_dir(&session_id).await;
                return Err(InternalError::from(error).into());
            }
        };
        if let Err(error) = file_writer.shutdown().await {
            drop(file_writer.into_inner());
            self.cleanup_upload_dir(&session_id).await;
            return Err(InternalError::from(error).into());
        }

        self.finalize_upload(&session_id, digest).await?;

        Ok(size)
    }

    async fn write_upload_chunk(&self, session_id: &str, stream: BodyDataStream) -> Result<u64> {
        let body_with_io_error = stream.map_err(io::Error::other);
        let mut body_reader = StreamReader::new(body_with_io_error);

        let file_path = self
            .ensure_parent_dir(&self.path_manager.upload_data_path(session_id))
            .await?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
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
        let upload_data_path = self.path_manager.upload_data_path(session_id);
        let blob_data_path = self.path_manager.blob_data_path(digest);

        let result = async {
            verify_file_digest(&upload_data_path, digest).await?;

            self.ensure_parent_dir(&blob_data_path).await?;

            const EXDEV: i32 = 18;
            if let Err(e) = tokio::fs::rename(&upload_data_path, &blob_data_path).await {
                if e.raw_os_error() == Some(EXDEV) {
                    tracing::warn!(
                        "rename across mount points (EXDEV), falling back to copy+delete: {} -> {}",
                        upload_data_path,
                        blob_data_path
                    );
                    tokio::fs::copy(&upload_data_path, &blob_data_path)
                        .await
                        .map_to_internal()?;
                    tokio::fs::remove_file(&upload_data_path).await.ok();
                } else {
                    return Err(InternalError::from(e).into());
                }
            }

            Ok(())
        }
        .await;

        self.cleanup_upload_dir(session_id).await;
        result
    }

    async fn put_tag(&self, name: &str, tag: &str, digest: &Digest) -> Result<()> {
        let link_path = self.path_manager.manifest_tag_link_path(name, tag);
        let file_path = self.ensure_parent_dir(&link_path).await?;

        tokio::fs::write(file_path, digest.to_string())
            .await
            .map_to_internal()?;

        Ok(())
    }

    async fn list_tags(&self, name: &str) -> Result<Vec<String>> {
        let mut entries = vec![];
        let path = self.path_manager.manifest_tags_path(name);

        let mut read_dir = match read_dir(path).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(Vec::new());
            }
            Err(e) => {
                return Err(InternalError::from(e).into());
            }
        };

        while let Some(entry) = read_dir.next_entry().await.map_to_internal()? {
            let path = entry.path();
            if let Some(file_name) = path.file_name()
                && let Some(file_name_str) = file_name.to_str()
            {
                entries.push(file_name_str.to_string());
            }
        }
        entries.sort();
        Ok(entries)
    }

    async fn delete_tag(&self, name: &str, tag: &str) -> Result<()> {
        let tag_path = self.path_manager.manifest_tag_path(name, tag);

        match remove_dir_all(tag_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::ManifestUnknown(tag.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn delete_blob(&self, digest: &Digest) -> Result<()> {
        let blob_path = self.path_manager.blob_path(digest);

        match remove_dir_all(blob_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::BlobUnknown(digest.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }
}
