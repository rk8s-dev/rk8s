use axum::body::BodyDataStream;
use oci_spec::image::Digest;
use std::path::PathBuf;
use tokio::{fs::File, io};
use crate::error::AppError;

pub mod driver;
pub mod paths;
pub mod user_storage;
pub mod repo_storage;

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
pub trait Storage: Send + Sync {
    async fn read_by_tag(&self, name: &str, tag: &str) -> Result<File>;
    async fn read_by_digest(&self, digest: &Digest) -> Result<File>;
    async fn read_by_uuid(&self, uuid: &str) -> Result<File>;
    async fn write_by_digest(
        &self,
        digest: &Digest,
        stream: BodyDataStream,
        append: bool,
    ) -> Result<()>;
    async fn write_by_uuid(
        &self,
        uuid: &str,
        stream: BodyDataStream,
        append: bool,
    ) -> Result<()>;
    async fn move_to_digest(&self, session_id: &str, digest: &Digest) -> Result<()>;
    async fn create_path(&self, path: &str) -> Result<PathBuf>;
    async fn link_to_tag(&self, name: &str, tag: &str, digest: &Digest) -> Result<()>;
    async fn walk_repo_dir(&self, name: &str) -> Result<Vec<String>>;
    async fn delete_by_tag(&self, name: &str, tag: &str) -> Result<()>;
    async fn delete_by_digest(&self, digest: &Digest) -> Result<()>;
}
