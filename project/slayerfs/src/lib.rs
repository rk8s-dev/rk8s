// Library crate for SlayerFS: expose SDK APIs while keeping internals private.
#![allow(dead_code)]
#![allow(clippy::upper_case_acronyms)]

#[allow(unused_imports)]
pub(crate) mod cadapter;
pub(crate) mod chuck;
pub mod daemon;
pub(crate) mod fs;
pub mod fuse;
pub(crate) mod meta;
pub mod sdk_fs;
pub(crate) mod vfs;

pub(crate) mod utils;

// Public SDK surface for external users.
pub use crate::sdk_fs::{
    AccessMode, Client, ClientBackend, DirEntry as SdkDirEntry, File, FileType as SdkFileType,
    Metadata, OpenOptions, ReadDir,
};
pub use crate::vfs::sdk::{LocalClient, VfsClient};

// Re-export core types needed to construct SDK backends.
pub use crate::cadapter::client::{ObjectBackend, ObjectClient};
pub use crate::cadapter::localfs::LocalFsBackend;
pub use crate::cadapter::s3::{S3Backend, S3Config};
pub use crate::chuck::chunk::ChunkLayout;
pub use crate::chuck::store::{BlockKey, BlockStore, InMemoryBlockStore, ObjectBlockStore};
pub use crate::meta::client::MetaClient;
pub use crate::meta::config::{CacheConfig, ClientOptions, Config, DatabaseConfig, DatabaseType};
pub use crate::meta::{
    create_meta_store_from_url, create_redis_meta_store_from_url, MetaHandle, MetaStore,
};
pub use crate::meta::factory::MetaStoreFactory;
pub use crate::meta::stores::{DatabaseMetaStore, EtcdMetaStore, RedisMetaStore};
pub use crate::meta::file_lock::{FileLockInfo, FileLockQuery, FileLockRange, FileLockType};
pub use crate::meta::store::{
    DirEntry as VfsDirEntry, FileAttr as VfsFileAttr, FileType as VfsFileType, SetAttrFlags,
    SetAttrRequest, StatFsSnapshot,
};
pub use crate::vfs::fs::{RenameFlags, VFS};
