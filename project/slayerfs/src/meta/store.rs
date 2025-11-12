//! Metadata store abstract interface
//!
//! Defines unified interface for filesystem metadata operations
use crate::chuck::SliceDesc;
use crate::meta::entities::content_meta::EntryType;
use async_trait::async_trait;

/// File type enumeration
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileType {
    File,
    Dir,
}

impl From<EntryType> for FileType {
    fn from(entry_type: EntryType) -> Self {
        match entry_type {
            EntryType::File => FileType::File,
            EntryType::Directory => FileType::Dir,
        }
    }
}

/// File attributes
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileAttr {
    pub ino: i64,
    pub size: u64,
    pub kind: FileType,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub atime: i64,
    pub mtime: i64,
    pub ctime: i64,
    pub nlink: u32,
}

/// Directory entry
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub ino: i64,
    pub kind: FileType,
}

/// Metadata operation errors
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum MetaError {
    #[error("Entry not found: {0}")]
    NotFound(i64),

    #[error("Parent directory not found: {0}")]
    ParentNotFound(i64),

    #[error("Entry already exists: {name} in parent {parent}")]
    AlreadyExists { parent: i64, name: String },

    #[error("Not a directory: {0}")]
    NotDirectory(i64),

    #[error("Directory not empty: {0}")]
    DirectoryNotEmpty(i64),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error("Not implemented")]
    NotImplemented,

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("error: max retries exceeded")]
    MaxRetriesExceeded,

    #[error("Database error: {0}")]
    Database(#[from] sea_orm::DbErr),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Config error: {0}")]
    Config(String),

    #[error("error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

/// Metadata store abstract interface
#[async_trait]
#[auto_impl::auto_impl(&, Arc)]
#[allow(dead_code)]
pub trait MetaStore: Send + Sync {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError>;

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError>;

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError>;

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError>;

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError>;

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError>;

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError>;

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError>;

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError>;

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError>;

    /// get the node's parent inode
    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError>;

    /// get the node's name in its parent directory
    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError>;

    /// get the inode's full path (from the root directory)
    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError>;

    fn root_ino(&self) -> i64;

    async fn initialize(&self) -> Result<(), MetaError>;

    /// Returns all file inodes marked for deletion (for garbage collection)
    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError>;

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError>;

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError>;

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError>;

    async fn next_id(&self, key: &str) -> Result<i64, MetaError>;
    /// Allow downcasting to concrete types
    fn as_any(&self) -> &dyn std::any::Any;
}
