use std::path::PathBuf;

use crate::storage::Storage;
use crate::storage::paths::PathManager;

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

#[async_trait::async_trait]
impl Storage for FilesystemStorage {
    async fn read_by_tag(&self, name: &str, tag: &str) -> io::Result<File> {
        File::open(self.path_manager.clone().manifest_tag_link_path(name, tag)).await
    }

    async fn read_by_digest(&self, digest: &Digest) -> io::Result<File> {
        File::open(self.path_manager.clone().blob_data_path(digest)).await
    }

    async fn read_by_uuid(&self, uuid: &str) -> io::Result<File> {
        File::open(self.path_manager.clone().upload_data_path(uuid)).await
    }

    async fn write_by_digest(
        &self,
        digest: &Digest,
        stream: BodyDataStream,
        append: bool,
    ) -> io::Result<()> {
        async {
            // Convert the stream into an `AsyncRead`.
            let body_with_io_error =
                stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
            let body_reader = StreamReader::new(body_with_io_error);
            futures::pin_mut!(body_reader);

            // Create the file. `File` implements `AsyncWrite`.
            let file_path = self
                .crate_path(&self.path_manager.clone().blob_data_path(digest))
                .await?;
            let mut file_writer = if append {
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(file_path)
                    .await?;
                BufWriter::new(file)
            } else {
                let file = File::create(file_path).await?;
                BufWriter::new(file)
            };

            // Copy the body into the file.
            tokio::io::copy(&mut body_reader, &mut file_writer).await?;

            Ok::<_, io::Error>(())
        }
        .await
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    async fn write_by_uuid(
        &self,
        uuid: &str,
        stream: BodyDataStream,
        append: bool,
    ) -> io::Result<()> {
        async {
            // Convert the stream into an `AsyncRead`.
            let body_with_io_error =
                stream.map_err(|err| io::Error::new(io::ErrorKind::Other, err));
            let body_reader = StreamReader::new(body_with_io_error);
            futures::pin_mut!(body_reader);

            // Create the file. `File` implements `AsyncWrite`.
            let file_path = self
                .crate_path(&self.path_manager.clone().upload_data_path(uuid))
                .await?;
            let mut file_writer = if append {
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(file_path)
                    .await?;
                BufWriter::new(file)
            } else {
                let file = File::create(file_path).await?;
                BufWriter::new(file)
            };

            // Copy the body into the file.
            tokio::io::copy(&mut body_reader, &mut file_writer).await?;

            Ok::<_, io::Error>(())
        }
        .await
        .map_err(|err| io::Error::new(io::ErrorKind::Other, err))
    }

    async fn move_to_digest(&self, session_id: &str, digest: &Digest) -> io::Result<()> {
        let upload_data_path = self
            .crate_path(&self.path_manager.clone().upload_data_path(session_id))
            .await?;
        let blob_data_path = self
            .crate_path(&self.path_manager.clone().blob_data_path(digest))
            .await?;
        rename(upload_data_path, blob_data_path).await?;
        Ok(())
    }

    async fn crate_path(&self, path: &str) -> io::Result<PathBuf> {
        let file_path = std::path::Path::new(&path);
        if let Some(parent) = file_path.parent() {
            create_dir_all(parent).await?;
        }
        Ok(file_path.to_path_buf())
    }

    async fn link_to_tag(&self, name: &str, tag: &str, digest: &Digest) -> io::Result<()> {
        let tag_path = self
            .crate_path(&self.path_manager.clone().manifest_tag_link_path(name, tag))
            .await?;
        let digest_path = self
            .crate_path(&self.path_manager.clone().blob_data_path(digest))
            .await?;

        // Remove the existing symlink if it exists
        if let Ok(metadata) = symlink_metadata(&tag_path).await {
            if metadata.file_type().is_symlink() {
                remove_file(&tag_path).await?;
            }
        }

        #[cfg(not(unix))]
        std::os::windows::fs::symlink_file(digest_path, tag_path)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(digest_path, tag_path)?;

        Ok(())
    }

    async fn walk_repo_dir(&self, name: &str) -> io::Result<Vec<String>> {
        let mut entries = vec![];
        let path = self.path_manager.clone().manifest_tags_path(name);
        let mut read_dir = read_dir(path).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if let Some(file_name) = path.file_name() {
                if let Some(file_name_str) = file_name.to_str() {
                    entries.push(file_name_str.to_string());
                }
            }
        }
        entries.sort();
        Ok(entries)
    }

    async fn delete_by_tag(&self, name: &str, tag: &str) -> io::Result<()> {
        let tag_path = self.path_manager.clone().manifest_tag_path(name, tag);
        remove_dir_all(tag_path).await?;
        Ok(())
    }

    async fn delete_by_digest(&self, digest: &Digest) -> io::Result<()> {
        let blob_path = self.path_manager.clone().blob_path(digest);
        remove_dir_all(blob_path).await?;
        Ok(())
    }
}
