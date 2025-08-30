use std::path::PathBuf;

use crate::storage::Storage;
use crate::storage::paths::PathManager;

use crate::error::{AppError, InternalError, MapToAppError, OciError};
use axum::body::BodyDataStream;
use futures::TryStreamExt;
use oci_spec::image::Digest;
use tokio::{
    fs::{
        File, OpenOptions, create_dir_all, read_dir, remove_dir_all, remove_file, rename,
        symlink_metadata,
    },
    io::{self, BufWriter},
};
use tokio_util::io::StreamReader;

pub struct FilesystemStorage {
    path_manager: PathManager,
}

impl FilesystemStorage {
    pub fn new(root: &str) -> Self {
        FilesystemStorage {
            path_manager: PathManager::new(root),
        }
    }
}

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
impl Storage for FilesystemStorage {
    async fn read_by_tag(&self, name: &str, tag: &str) -> Result<File> {
        let path = self.path_manager.clone().manifest_tag_link_path(name, tag);

        match File::open(path).await {
            Ok(file) => Ok(file),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::ManifestUnknown(format!("{name}:{tag}")).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn read_by_digest(&self, digest: &Digest) -> Result<File> {
        let path = self.path_manager.clone().blob_data_path(digest);

        match File::open(path).await {
            Ok(file) => Ok(file),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::BlobUnknown(digest.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn read_by_uuid(&self, uuid: &str) -> Result<File> {
        let path = self.path_manager.clone().upload_data_path(uuid);

        match File::open(path).await {
            Ok(file) => Ok(file),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::BlobUploadUnknown(uuid.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn write_by_digest(
        &self,
        digest: &Digest,
        stream: BodyDataStream,
        append: bool,
    ) -> Result<u64> {
        let body_with_io_error = stream.map_err(io::Error::other);
        let mut body_reader = StreamReader::new(body_with_io_error);

        let file_path = self
            .create_path(&self.path_manager.clone().blob_data_path(digest))
            .await?;

        let file = if append {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .await
        } else {
            File::create(file_path).await
        }
        .map_to_internal()?;

        let mut file_writer = BufWriter::new(file);

        io::copy(&mut body_reader, &mut file_writer)
            .await
            .map_to_internal()
    }

    async fn write_by_uuid(&self, uuid: &str, stream: BodyDataStream, append: bool) -> Result<u64> {
        let body_with_io_error = stream.map_err(io::Error::other);
        let mut body_reader = StreamReader::new(body_with_io_error);

        let file_path = self
            .create_path(&self.path_manager.clone().upload_data_path(uuid))
            .await?;

        let file = if append {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
                .await
        } else {
            File::create(file_path).await
        }
        .map_to_internal()?;

        let mut file_writer = BufWriter::new(file);

        io::copy(&mut body_reader, &mut file_writer)
            .await
            .map_to_internal()
    }

    async fn move_to_digest(&self, session_id: &str, digest: &Digest) -> Result<()> {
        let upload_data_path = self.path_manager.clone().upload_data_path(session_id);
        let blob_data_path = self.path_manager.clone().blob_data_path(digest);

        self.create_path(&blob_data_path).await?;

        rename(upload_data_path, blob_data_path)
            .await
            .map_to_internal()?;
        Ok(())
    }

    async fn create_path(&self, path: &str) -> Result<PathBuf> {
        let file_path = std::path::Path::new(path).to_path_buf();
        if let Some(parent) = file_path.parent() {
            create_dir_all(parent).await.map_to_internal()?;
        }
        Ok(file_path)
    }

    async fn link_to_tag(&self, name: &str, tag: &str, digest: &Digest) -> Result<()> {
        let tag_path = self
            .create_path(&self.path_manager.clone().manifest_tag_link_path(name, tag))
            .await?;
        let digest_path_str = self.path_manager.clone().blob_data_path(digest);
        let digest_path = std::path::Path::new(&digest_path_str);

        if symlink_metadata(&tag_path).await.is_ok() {
            remove_file(&tag_path).await.map_to_internal()?;
        }

        #[cfg(unix)]
        let link_result = tokio::fs::symlink(digest_path, tag_path).await;
        #[cfg(windows)]
        let link_result = tokio::fs::symlink_file(digest_path, tag_path).await;

        link_result.map_to_internal()?;

        Ok(())
    }

    async fn walk_repo_dir(&self, name: &str) -> Result<Vec<String>> {
        let mut entries = vec![];
        let path = self.path_manager.clone().manifest_tags_path(name);

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
            if let Some(file_name) = entry.path().file_name().and_then(|s| s.to_str()) {
                entries.push(file_name.to_string());
            }
        }
        entries.sort();
        Ok(entries)
    }

    async fn delete_by_tag(&self, name: &str, tag: &str) -> Result<()> {
        let tag_path = self.path_manager.clone().manifest_tag_path(name, tag);

        match remove_dir_all(tag_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::ManifestUnknown(tag.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }

    async fn delete_by_digest(&self, digest: &Digest) -> Result<()> {
        let blob_path = self.path_manager.clone().blob_path(digest);

        match remove_dir_all(blob_path).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(OciError::BlobUnknown(digest.to_string()).into())
            }
            Err(e) => Err(InternalError::from(e).into()),
        }
    }
}
