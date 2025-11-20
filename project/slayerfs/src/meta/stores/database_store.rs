//! Database-based metadata store implementation
//!
//! Supports SQLite and PostgreSQL backends via SeaORM

use crate::chuck::SliceDesc;
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::session_meta::{self, Entity as SessionMeta};
use crate::meta::entities::slice_meta::{self, Entity as SliceMeta};
use crate::meta::entities::*;
use crate::meta::store::{
    DirEntry, FileAttr, MetaError, MetaStore, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::meta::{INODE_ID_KEY, Permission, SESSION_ID_KEY, SLICE_ID_KEY};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::Utc;
use log::info;
use sea_orm::prelude::Uuid;
use sea_orm::*;
use sea_query::Index;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Database-based metadata store
pub struct DatabaseMetaStore {
    db: DatabaseConnection,
    _config: Config,
    next_inode: AtomicU64,
    next_slice: AtomicU64,
    next_session: AtomicU64,
}

impl DatabaseMetaStore {
    /// Create or open a database metadata store
    #[allow(dead_code)]
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let _config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;

        info!("Initializing DatabaseMetaStore");
        info!("Backend path: {}", backend_path.display());
        info!("Database type: {}", _config.database.db_type_str());

        let db = Self::create_connection(&_config).await?;
        Self::init_schema(&db).await?;

        let next_inode = AtomicU64::new(Self::init_next_inode(&db).await?);
        let next_slice = AtomicU64::new(Self::init_next_slice(&db).await?);
        let next_session = AtomicU64::new(Self::init_next_session(&db).await?);
        let store = Self {
            db,
            _config,
            next_inode,
            next_slice,
            next_session,
        };
        store.init_root_directory().await?;

        info!("DatabaseMetaStore initialized successfully");
        Ok(store)
    }

    /// Create from existing config
    pub async fn from_config(_config: Config) -> Result<Self, MetaError> {
        info!("Initializing DatabaseMetaStore from config");
        info!("Database type: {}", _config.database.db_type_str());

        let db = Self::create_connection(&_config).await?;
        Self::init_schema(&db).await?;

        let next_inode = AtomicU64::new(Self::init_next_inode(&db).await?);
        let next_slice = AtomicU64::new(Self::init_next_slice(&db).await?);
        let next_session = AtomicU64::new(Self::init_next_session(&db).await?);
        let store = Self {
            db,
            _config,
            next_inode,
            next_slice,
            next_session,
        };
        store.init_root_directory().await?;

        info!("DatabaseMetaStore initialized successfully");
        Ok(store)
    }

    /// Initialize next inode counter from database
    async fn init_next_inode(db: &DatabaseConnection) -> Result<u64, MetaError> {
        let max_access = AccessMeta::find()
            .order_by_desc(access_meta::Column::Inode)
            .one(db)
            .await
            .map_err(MetaError::Database)?
            .map(|r| r.inode as u64)
            .unwrap_or(1);

        let max_file = FileMeta::find()
            .order_by_desc(file_meta::Column::Inode)
            .one(db)
            .await
            .map_err(MetaError::Database)?
            .map(|r| r.inode as u64)
            .unwrap_or(1);

        let next = max_access.max(max_file) + 1;
        info!("Initialized next inode counter to: {}", next);
        Ok(next)
    }

    async fn init_next_slice(db: &DatabaseConnection) -> Result<u64, MetaError> {
        let max_slice = SliceMeta::find()
            .order_by_desc(slice_meta::Column::SliceId)
            .one(db)
            .await
            .map_err(MetaError::Database)?
            .map(|r| r.slice_id as u64)
            .unwrap_or(0);

        Ok(max_slice + 1)
    }

    /// Initialize next session ID counter
    async fn init_next_session(db: &DatabaseConnection) -> Result<u64, MetaError> {
        let max_session = SessionMeta::find()
            .order_by_desc(session_meta::Column::Id)
            .one(db)
            .await
            .map_err(MetaError::Database)?
            .map(|r| r.id)
            .unwrap_or(0);

        // Start session IDs from 1 to avoid confusion with 0 as "no session"
        let next = if max_session == 0 { 1 } else { max_session + 1 };
        info!("Initialized next session counter to: {}", next);
        Ok(next)
    }

    /// Create database connection
    async fn create_connection(config: &Config) -> Result<DatabaseConnection, MetaError> {
        match &config.database.db_config {
            DatabaseType::Sqlite { url } => {
                info!("Connecting to SQLite: {}", url);
                let opts = ConnectOptions::new(url.clone());
                let db = Database::connect(opts).await?;
                Ok(db)
            }
            DatabaseType::Postgres { url } => {
                info!("Connecting to PostgreSQL: {}", url);
                let opts = ConnectOptions::new(url.clone());
                let db = Database::connect(opts).await?;
                Ok(db)
            }
            DatabaseType::Etcd { .. } => Err(MetaError::Config(
                "Etcd backend not supported by DatabaseMetaStore. Use EtcdMetaStore instead."
                    .to_string(),
            )),
            DatabaseType::Redis { .. } => Err(MetaError::Config(
                "Redis backend not supported by DatabaseMetaStore. Use RedisMetaStore instead."
                    .to_string(),
            )),
        }
    }

    /// Initialize database schema
    async fn init_schema(db: &DatabaseConnection) -> Result<(), MetaError> {
        let builder = db.get_database_backend();
        let schema = Schema::new(builder);

        let stmts = [
            schema
                .create_table_from_entity(AccessMeta)
                .if_not_exists()
                .to_owned(),
            schema
                .create_table_from_entity(ContentMeta)
                .if_not_exists()
                .to_owned(),
            schema
                .create_table_from_entity(FileMeta)
                .if_not_exists()
                .to_owned(),
            schema
                .create_table_from_entity(SessionMeta)
                .if_not_exists()
                .to_owned(),
            schema
                .create_table_from_entity(SliceMeta)
                .if_not_exists()
                .to_owned(),
        ];

        for (i, stmt) in stmts.iter().enumerate() {
            let sql = builder.build(stmt);
            db.execute(sql).await.map_err(|e| {
                eprintln!("Failed to execute statement {}: {}", i + 1, e);
                MetaError::Database(e)
            })?;
        }

        let index_stmt = Index::create()
            .if_not_exists()
            .name("idx_content_meta_inode")
            .table(ContentMeta)
            .col(content_meta::Column::Inode)
            .to_owned();

        let index_sql = builder.build(&index_stmt);
        db.execute(index_sql).await.map_err(|e| {
            eprintln!("Failed to create index idx_content_meta_inode: {}", e);
            MetaError::Database(e)
        })?;

        info!("Database schema initialized successfully");
        Ok(())
    }

    /// Initialize root directory
    async fn init_root_directory(&self) -> Result<(), MetaError> {
        // Check if root directory exists
        if (self.get_access_meta(1).await?).is_some() {
            return Ok(());
        }

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let root_permission = Permission::new(0o40755, 0, 0); // Directory bits: 0o40000 (dir flag) + 0o755 (mode)
        let root_dir = access_meta::ActiveModel {
            inode: Set(1),
            permission: Set(root_permission),
            access_time: Set(now),
            modify_time: Set(now),
            create_time: Set(now),
            nlink: Set(2),
        };

        root_dir
            .insert(&self.db)
            .await
            .map_err(MetaError::Database)?;
        info!("Root directory initialized");

        Ok(())
    }

    /// Get directory access metadata
    async fn get_access_meta(&self, inode: i64) -> Result<Option<AccessMetaModel>, MetaError> {
        AccessMeta::find_by_id(inode)
            .one(&self.db)
            .await
            .map_err(|e| MetaError::Internal(format!("Database error: {}", e)))
    }

    /// Get directory content metadata
    async fn get_content_meta(
        &self,
        parent_inode: i64,
    ) -> Result<Option<Vec<ContentMetaModel>>, MetaError> {
        let contents = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent_inode))
            .order_by_asc(content_meta::Column::EntryName) // Sort by name to match ls order
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        if contents.is_empty() {
            Ok(None)
        } else {
            Ok(Some(contents))
        }
    }

    /// Get file metadata
    async fn get_file_meta(&self, inode: i64) -> Result<Option<FileMetaModel>, MetaError> {
        FileMeta::find_by_id(inode)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)
    }

    /// Create a new directory
    async fn create_directory(&self, parent_inode: i64, name: String) -> Result<i64, MetaError> {
        // Start transaction
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        if AccessMeta::find_by_id(parent_inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .is_none()
        {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::ParentNotFound(parent_inode));
        }

        // Check if entry already exists
        let existing = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent_inode))
            .filter(content_meta::Column::EntryName.eq(&name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if existing.is_some() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::AlreadyExists {
                parent: parent_inode,
                name,
            });
        }

        let inode = self.generate_id();

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let dir_permission = Permission::new(0o40755, 0, 0);
        let access_meta = access_meta::ActiveModel {
            inode: Set(inode),
            permission: Set(dir_permission),
            access_time: Set(now),
            modify_time: Set(now),
            create_time: Set(now),
            nlink: Set(2),
        };

        access_meta
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        let content_meta = content_meta::ActiveModel {
            inode: Set(inode),
            parent_inode: Set(parent_inode),
            entry_name: Set(name),
            entry_type: Set(EntryType::Directory),
        };

        content_meta
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Update parent directory mtime
        let mut parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(parent_inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .unwrap()
            .into();
        parent_meta.modify_time = Set(now);
        parent_meta
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(inode)
    }

    /// Create a new file
    async fn create_file_internal(
        &self,
        parent_inode: i64,
        name: String,
    ) -> Result<i64, MetaError> {
        // Start transaction
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        if AccessMeta::find_by_id(parent_inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .is_none()
        {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::ParentNotFound(parent_inode));
        }

        // Check if entry already exists
        let existing = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent_inode))
            .filter(content_meta::Column::EntryName.eq(&name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if existing.is_some() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::AlreadyExists {
                parent: parent_inode,
                name,
            });
        }

        let inode = self.generate_id();

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let file_permission = Permission::new(0o644, 0, 0);
        let file_meta = file_meta::ActiveModel {
            inode: Set(inode),
            size: Set(0),
            permission: Set(file_permission),
            access_time: Set(now),
            modify_time: Set(now),
            create_time: Set(now),
            nlink: Set(1),
            deleted: Set(false),
            symlink_target: Set(None),
        };

        file_meta.insert(&txn).await.map_err(MetaError::Database)?;

        let content_meta = content_meta::ActiveModel {
            inode: Set(inode),
            parent_inode: Set(parent_inode),
            entry_name: Set(name),
            entry_type: Set(EntryType::File),
        };

        content_meta
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Update parent directory mtime
        let mut parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(parent_inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .unwrap()
            .into();
        parent_meta.modify_time = Set(now);
        parent_meta
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(inode)
    }

    /// Generate unique ID using atomic counter
    fn generate_id(&self) -> i64 {
        let id = self.next_inode.fetch_add(1, Ordering::SeqCst);
        id as i64
    }

    fn now_nanos() -> i64 {
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    }
}

#[async_trait]
impl MetaStore for DatabaseMetaStore {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        if let Ok(Some(file_meta)) = self.get_file_meta(ino).await {
            let permission = file_meta.permission();
            let kind = if file_meta.symlink_target.is_some() {
                FileType::Symlink
            } else {
                FileType::File
            };
            let size = if let Some(target) = &file_meta.symlink_target {
                target.len() as u64
            } else {
                file_meta.size as u64
            };
            return Ok(Some(FileAttr {
                ino: file_meta.inode,
                size,
                kind,
                mode: permission.mode,
                uid: permission.uid,
                gid: permission.gid,
                atime: file_meta.access_time,
                mtime: file_meta.modify_time,
                ctime: file_meta.create_time,
                nlink: file_meta.nlink as u32,
            }));
        }

        if let Ok(Some(access_meta)) = self.get_access_meta(ino).await {
            let permission = access_meta.permission();
            return Ok(Some(FileAttr {
                ino: access_meta.inode,
                size: 4096,
                kind: FileType::Dir,
                mode: permission.mode,
                uid: permission.uid,
                gid: permission.gid,
                atime: access_meta.access_time,
                mtime: access_meta.modify_time,
                ctime: access_meta.create_time,
                nlink: access_meta.nlink as u32,
            }));
        }

        Ok(None)
    }

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entry.map(|e| e.inode))
    }

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError> {
        if path == "/" {
            return Ok(Some((1, FileType::Dir)));
        }

        let parts: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();
        let mut current_inode = 1i64;

        for (index, part) in parts.iter().enumerate() {
            let entry = ContentMeta::find()
                .filter(content_meta::Column::ParentInode.eq(current_inode))
                .filter(content_meta::Column::EntryName.eq(*part))
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?;

            match entry {
                Some(entry) => match entry.entry_type {
                    EntryType::Directory => {
                        current_inode = entry.inode;
                    }
                    EntryType::File => {
                        if index == parts.len() - 1 {
                            return Ok(Some((entry.inode, FileType::File)));
                        } else {
                            return Ok(None);
                        }
                    }
                    EntryType::Symlink => {
                        if index == parts.len() - 1 {
                            return Ok(Some((entry.inode, FileType::Symlink)));
                        } else {
                            return Ok(None);
                        }
                    }
                },
                None => return Ok(None),
            }
        }

        Ok(Some((current_inode, FileType::Dir)))
    }

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError> {
        let access_meta = self
            .get_access_meta(ino)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        let permission = access_meta.permission();
        if !permission.is_directory() {
            return Err(MetaError::NotDirectory(ino));
        }

        let contents = match self.get_content_meta(ino).await? {
            Some(contents) => contents,
            None => return Ok(Vec::new()),
        };

        let mut entries = Vec::new();
        for content in contents {
            let kind = match content.entry_type {
                EntryType::File => FileType::File,
                EntryType::Directory => FileType::Dir,
                EntryType::Symlink => FileType::Symlink,
            };
            entries.push(DirEntry {
                name: content.entry_name,
                ino: content.inode,
                kind,
            });
        }

        Ok(entries)
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_directory(parent, name).await
    }

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let dir_entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .filter(content_meta::Column::EntryType.eq(EntryType::Directory))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(parent))?;

        let dir_id = dir_entry.inode;

        // Check if directory is empty
        let child_count = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(dir_id))
            .count(&txn)
            .await
            .map_err(MetaError::Database)?;

        if child_count > 0 {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::DirectoryNotEmpty(dir_id));
        }

        // Delete access meta
        AccessMeta::delete_by_id(dir_id)
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Delete content meta
        ContentMeta::delete_many()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Update parent directory mtime
        let mut parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::ParentNotFound(parent))?
            .into();
        parent_meta.modify_time = Set(Utc::now().timestamp_nanos_opt().unwrap_or(0));
        parent_meta
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(())
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_file_internal(parent, name).await
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let file_entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or_else(|| {
                MetaError::Internal(format!("File '{}' not found in parent {}", name, parent))
            })?;

        if file_entry.entry_type == EntryType::Directory {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotDirectory(file_entry.inode));
        }

        let file_id = file_entry.inode;

        let mut file_meta: file_meta::ActiveModel = FileMeta::find_by_id(file_id)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(file_id))?
            .into();

        // Delete content meta first
        ContentMeta::delete_many()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        let now = Self::now_nanos();
        let current_nlink = match &file_meta.nlink {
            Set(n) | Unchanged(n) => *n,
            _ => 1,
        };

        if current_nlink > 1 {
            file_meta.nlink = Set(current_nlink - 1);
            file_meta.deleted = Set(false);
        } else {
            file_meta.deleted = Set(true);
            file_meta.nlink = Set(0);
        }

        file_meta.modify_time = Set(now);
        file_meta.create_time = Set(now);
        file_meta.update(&txn).await.map_err(MetaError::Database)?;

        // Update parent directory mtime
        let mut parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::ParentNotFound(parent))?
            .into();
        parent_meta.modify_time = Set(Utc::now().timestamp_nanos_opt().unwrap_or(0));
        parent_meta
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(())
    }

    async fn link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        if ino == 1 {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotSupported(
                "cannot create hard links to the root inode".into(),
            ));
        }

        let Some(file) = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        else {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotFound(ino));
        };

        if file.deleted || file.nlink <= 0 {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotSupported(
                "cannot create hard link to deleted file".into(),
            ));
        }

        let parent_dir = AccessMeta::find_by_id(parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::ParentNotFound(parent))?;

        if !parent_dir.permission().is_directory() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotDirectory(parent));
        }

        let existing = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if existing.is_some() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::AlreadyExists {
                parent,
                name: name.to_string(),
            });
        }

        let now = Self::now_nanos();
        let entry_type = if file.symlink_target.is_some() {
            EntryType::Symlink
        } else {
            EntryType::File
        };

        let new_entry = content_meta::ActiveModel {
            inode: Set(ino),
            parent_inode: Set(parent),
            entry_name: Set(name.to_string()),
            entry_type: Set(entry_type),
        };
        new_entry.insert(&txn).await.map_err(MetaError::Database)?;

        let new_nlink = file.nlink.saturating_add(1);
        let mut file_active: file_meta::ActiveModel = file.into();
        file_active.nlink = Set(new_nlink);
        file_active.modify_time = Set(now);
        file_active.create_time = Set(now);
        file_active.deleted = Set(false);
        file_active
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        let mut parent_active: access_meta::ActiveModel = parent_dir.into();
        parent_active.modify_time = Set(now);
        parent_active
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        self.stat(ino).await?.ok_or(MetaError::NotFound(ino))
    }

    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let parent_dir = AccessMeta::find_by_id(parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::ParentNotFound(parent))?;

        if !parent_dir.permission().is_directory() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotDirectory(parent));
        }

        let existing = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if existing.is_some() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::AlreadyExists {
                parent,
                name: name.to_string(),
            });
        }

        let now = Self::now_nanos();
        let inode = self.generate_id();
        let owner_uid = parent_dir.permission().uid;
        let owner_gid = parent_dir.permission().gid;
        let perm = Permission::new(0o120777, owner_uid, owner_gid);

        let file_meta = file_meta::ActiveModel {
            inode: Set(inode),
            size: Set(target.len() as i64),
            permission: Set(perm),
            access_time: Set(now),
            modify_time: Set(now),
            create_time: Set(now),
            nlink: Set(1),
            deleted: Set(false),
            symlink_target: Set(Some(target.to_string())),
        };
        file_meta.insert(&txn).await.map_err(MetaError::Database)?;

        let content_meta = content_meta::ActiveModel {
            inode: Set(inode),
            parent_inode: Set(parent),
            entry_name: Set(name.to_string()),
            entry_type: Set(EntryType::Symlink),
        };
        content_meta
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        let mut parent_active: access_meta::ActiveModel = parent_dir.into();
        parent_active.modify_time = Set(now);
        parent_active
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;
        let attr = self.stat(inode).await?.ok_or(MetaError::NotFound(inode))?;
        Ok((inode, attr))
    }

    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let file = FileMeta::find_by_id(ino)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(ino))?;

        file.symlink_target
            .ok_or_else(|| MetaError::NotSupported(format!("inode {ino} is not a symbolic link")))
    }

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        // Verify new parent exists
        if AccessMeta::find_by_id(new_parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .is_none()
        {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::ParentNotFound(new_parent));
        }

        // Find the entry to rename
        let target_entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(old_parent))
            .filter(content_meta::Column::EntryName.eq(old_name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or_else(|| {
                MetaError::Internal(format!(
                    "Entry '{}' not found in parent {} for rename",
                    old_name, old_parent
                ))
            })?;

        // Check if target already exists in new location
        let existing = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(new_parent))
            .filter(content_meta::Column::EntryName.eq(&new_name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if existing.is_some() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::AlreadyExists {
                parent: new_parent,
                name: new_name,
            });
        }

        // Delete old content_meta entry
        ContentMeta::delete_many()
            .filter(content_meta::Column::ParentInode.eq(old_parent))
            .filter(content_meta::Column::EntryName.eq(old_name))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Insert new content_meta entry
        let new_content_meta = content_meta::ActiveModel {
            inode: Set(target_entry.inode),
            parent_inode: Set(new_parent),
            entry_name: Set(new_name),
            entry_type: Set(target_entry.entry_type),
        };

        new_content_meta
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Update old parent mtime
        let mut old_parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(old_parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::ParentNotFound(old_parent))?
            .into();
        old_parent_meta.modify_time = Set(now);
        old_parent_meta
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Update new parent mtime (if different)
        if old_parent != new_parent {
            let mut new_parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(new_parent)
                .one(&txn)
                .await
                .map_err(MetaError::Database)?
                .ok_or(MetaError::NotFound(new_parent))?
                .into();
            new_parent_meta.modify_time = Set(now);
            new_parent_meta
                .update(&txn)
                .await
                .map_err(MetaError::Database)?;
        }

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(())
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let mut file_meta: file_meta::ActiveModel = FileMeta::find_by_id(ino)
            .one(&self.db)
            .await
            .map_err(|e| MetaError::Internal(e.to_string()))?
            .ok_or(MetaError::NotFound(ino))?
            .into();

        // Only update mtime if size actually changed
        let old_size = match &file_meta.size {
            Set(s) | Unchanged(s) => *s as u64,
            _ => 0,
        };

        file_meta.size = Set(size as i64);

        // Only update mtime when size changes (not on every call)
        if old_size != size {
            file_meta.modify_time = Set(Self::now_nanos());
        }

        file_meta
            .update(&self.db)
            .await
            .map_err(|e| MetaError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        if let Some(mut file) = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            let mut permission = file.permission().clone();
            let mut ctime_update = false;
            let now = Self::now_nanos();

            if let Some(mode) = req.mode {
                permission.chmod(mode);
                ctime_update = true;
            }

            if let Some(uid) = req.uid {
                let gid = req.gid.unwrap_or(permission.gid);
                permission.chown(uid, gid);
                ctime_update = true;
            }

            if req.uid.is_none()
                && let Some(gid) = req.gid
            {
                permission.chown(permission.uid, gid);
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::CLEAR_SUID) {
                permission.mode &= !0o4000;
                ctime_update = true;
            }
            if flags.contains(SetAttrFlags::CLEAR_SGID) {
                permission.mode &= !0o2000;
                ctime_update = true;
            }

            file.permission = permission;

            if let Some(size) = req.size {
                let new_size = size as i64;
                if file.size != new_size {
                    file.size = new_size;
                    file.modify_time = now;
                }
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::SET_ATIME_NOW) {
                file.access_time = now;
            } else if let Some(atime) = req.atime {
                file.access_time = atime;
            }

            if flags.contains(SetAttrFlags::SET_MTIME_NOW) {
                file.modify_time = now;
                ctime_update = true;
            } else if let Some(mtime) = req.mtime {
                file.modify_time = mtime;
                ctime_update = true;
            }

            if let Some(ctime) = req.ctime {
                file.create_time = ctime;
            } else if ctime_update {
                file.create_time = now;
            }

            let active: file_meta::ActiveModel = file.into();
            active.update(&txn).await.map_err(MetaError::Database)?;

            txn.commit().await.map_err(MetaError::Database)?;
            return self.stat(ino).await?.ok_or(MetaError::NotFound(ino));
        }

        if let Some(mut dir) = AccessMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            let mut permission = dir.permission().clone();
            let mut ctime_update = false;
            let now = Self::now_nanos();

            if let Some(mode) = req.mode {
                permission.chmod(mode);
                ctime_update = true;
            }

            if let Some(uid) = req.uid {
                let gid = req.gid.unwrap_or(permission.gid);
                permission.chown(uid, gid);
                ctime_update = true;
            }

            if req.uid.is_none()
                && let Some(gid) = req.gid
            {
                permission.chown(permission.uid, gid);
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::CLEAR_SUID) {
                permission.mode &= !0o4000;
                ctime_update = true;
            }
            if flags.contains(SetAttrFlags::CLEAR_SGID) {
                permission.mode &= !0o2000;
                ctime_update = true;
            }

            dir.permission = permission;

            if flags.contains(SetAttrFlags::SET_ATIME_NOW) {
                dir.access_time = now;
            } else if let Some(atime) = req.atime {
                dir.access_time = atime;
            }

            if flags.contains(SetAttrFlags::SET_MTIME_NOW) {
                dir.modify_time = now;
                ctime_update = true;
            } else if let Some(mtime) = req.mtime {
                dir.modify_time = mtime;
                ctime_update = true;
            }

            if let Some(ctime) = req.ctime {
                dir.create_time = ctime;
            } else if ctime_update {
                dir.create_time = now;
            }

            let active: access_meta::ActiveModel = dir.into();
            active.update(&txn).await.map_err(MetaError::Database)?;

            txn.commit().await.map_err(MetaError::Database)?;
            return self.stat(ino).await?.ok_or(MetaError::NotFound(ino));
        }

        txn.rollback().await.map_err(MetaError::Database)?;
        Err(MetaError::NotFound(ino))
    }

    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError> {
        if ino == 1 {
            return Ok(None);
        }

        let entry = ContentMeta::find()
            .filter(content_meta::Column::Inode.eq(ino))
            .order_by_asc(content_meta::Column::ParentInode)
            .order_by_asc(content_meta::Column::EntryName)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entry.map(|e| e.parent_inode))
    }

    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == 1 {
            return Ok(Some("/".to_string()));
        }

        let entry = ContentMeta::find()
            .filter(content_meta::Column::Inode.eq(ino))
            .order_by_asc(content_meta::Column::ParentInode)
            .order_by_asc(content_meta::Column::EntryName)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entry.map(|e| e.entry_name))
    }

    async fn open(&self, ino: i64, flags: OpenFlags) -> Result<FileAttr, MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        if let Some(mut file) = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            if file.symlink_target.is_some() {
                txn.rollback().await.map_err(MetaError::Database)?;
                return Err(MetaError::NotSupported(
                    "opening symlink targets is not implemented".into(),
                ));
            }

            let now = Self::now_nanos();
            let truncate = flags.contains(OpenFlags::TRUNC);

            file.access_time = now;
            if truncate {
                file.size = 0;
                file.modify_time = now;
                file.create_time = now;
            }

            let active: file_meta::ActiveModel = file.into();
            active.update(&txn).await.map_err(MetaError::Database)?;

            txn.commit().await.map_err(MetaError::Database)?;
            return self.stat(ino).await?.ok_or(MetaError::NotFound(ino));
        }

        if flags.contains(OpenFlags::TRUNC) {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotSupported(
                "truncate flag only supported for regular files".into(),
            ));
        }

        if let Some(mut dir) = AccessMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            dir.access_time = Self::now_nanos();
            let active: access_meta::ActiveModel = dir.into();
            active.update(&txn).await.map_err(MetaError::Database)?;

            txn.commit().await.map_err(MetaError::Database)?;
            return self.stat(ino).await?.ok_or(MetaError::NotFound(ino));
        }

        txn.rollback().await.map_err(MetaError::Database)?;
        Err(MetaError::NotFound(ino))
    }

    async fn close(&self, ino: i64) -> Result<(), MetaError> {
        if self.stat(ino).await?.is_some() {
            Ok(())
        } else {
            Err(MetaError::NotFound(ino))
        }
    }

    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == 1 {
            return Ok(Some("/".to_string()));
        }

        let mut path_parts = Vec::new();
        let mut current_ino = ino;

        loop {
            let entry = ContentMeta::find()
                .filter(content_meta::Column::Inode.eq(current_ino))
                .order_by_asc(content_meta::Column::ParentInode)
                .order_by_asc(content_meta::Column::EntryName)
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?;

            let Some(entry) = entry else {
                return Ok(None);
            };

            path_parts.push(entry.entry_name);

            let parent = entry.parent_inode;
            if parent == 1 {
                break;
            }

            current_ino = parent;
        }

        path_parts.reverse();
        let path = format!("/{}", path_parts.join("/"));
        Ok(Some(path))
    }

    fn root_ino(&self) -> i64 {
        1
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        Ok(())
    }

    async fn stat_fs(&self) -> Result<StatFsSnapshot, MetaError> {
        let files = FileMeta::find()
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        let used_space: u64 = files.iter().map(|file| file.size.max(0) as u64).sum();

        let file_count = files.len() as u64;
        let dir_count = AccessMeta::find()
            .count(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(StatFsSnapshot {
            total_space: used_space,
            available_space: 0,
            used_inodes: file_count + dir_count,
            available_inodes: 0,
        })
    }

    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        let deleted_files = FileMeta::find()
            .filter(file_meta::Column::Deleted.eq(true))
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(deleted_files.into_iter().map(|f| f.inode).collect())
    }

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let file_meta = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(ino))?;

        // Note: In database design, files and directories are stored in different tables.
        // Files are stored in file_meta table, directories in access_meta table.
        // So if we found a record in file_meta table, it must be a file.

        // Check if the file is marked as deleted
        if !file_meta.deleted {
            return Err(MetaError::Internal(
                "File is not marked as deleted".to_string(),
            ));
        }

        // Delete the file metadata
        let file_meta_active: file_meta::ActiveModel = file_meta.into();
        file_meta_active
            .delete(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(())
    }

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let rows = SliceMeta::find()
            .filter(slice_meta::Column::ChunkId.eq(chunk_id as i64))
            .order_by_asc(slice_meta::Column::Id)
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let model = slice_meta::ActiveModel {
            chunk_id: Set(chunk_id as i64),
            slice_id: Set(slice.slice_id as i64),
            offset: Set(slice.offset as i32),
            length: Set(slice.length as i32),
            ..Default::default()
        };

        model.insert(&self.db).await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        match key {
            SLICE_ID_KEY => Ok(self.next_slice.fetch_add(1, Ordering::SeqCst) as i64),
            INODE_ID_KEY => Ok(self.next_inode.fetch_add(1, Ordering::SeqCst) as i64),
            SESSION_ID_KEY => Ok(self.next_session.fetch_add(1, Ordering::SeqCst) as i64),
            other => Err(MetaError::NotSupported(format!(
                "next_id not supported for key {other}"
            ))),
        }
    }

    // ---------- Session lifecycle implementation ----------

    async fn new_session(&self, payload: &[u8], update: bool) -> Result<(), MetaError> {
        // Parse the payload to extract client information
        let session_info = match serde_json::from_slice::<serde_json::Value>(payload) {
            Ok(info) => info,
            Err(_) => {
                // If parsing fails, create a minimal session info
                serde_json::json!({
                    "session_uuid": Uuid::new_v4().to_string(),
                    "version": "unknown",
                    "process_id": std::process::id(),
                })
            }
        };

        let session_uuid = session_info
            .get("session_uuid")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        let hostname = session_info
            .get("host_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let process_id = session_info
            .get("process_id")
            .and_then(|v| v.as_i64())
            .map(|i| i as i32);

        let mount_point = session_info
            .get("mount_point")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if update {
            // For update mode, try to find an existing session by UUID first
            let existing_session = SessionMeta::find()
                .filter(session_meta::Column::SessionUuid.eq(&session_uuid))
                .filter(session_meta::Column::Stale.eq(false))
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?;

            if let Some(session) = existing_session {
                // Update existing session
                let session_id = session.id;
                let mut active_model: session_meta::ActiveModel = session.into();
                active_model.payload = Set(payload.to_vec());
                active_model.updated_at = Set(Utc::now().timestamp_millis());
                active_model.stale = Set(false);
                if let Some(mp) = mount_point {
                    active_model.mount_point = Set(Some(mp));
                }

                active_model
                    .update(&self.db)
                    .await
                    .map_err(MetaError::Database)?;

                info!("Updated existing session: {}", session_id);
                return Ok(());
            }
        }

        // Create new session
        let session_id = self.next_id(SESSION_ID_KEY).await? as u64;
        let session_model = session_meta::ActiveModel {
            id: Set(session_id),
            session_uuid: Set(session_uuid),
            payload: Set(payload.to_vec()),
            created_at: Set(Utc::now().timestamp_millis()),
            updated_at: Set(Utc::now().timestamp_millis()),
            stale: Set(false),
            hostname: Set(hostname),
            process_id: Set(process_id),
            mount_point: Set(mount_point),
        };

        session_model
            .insert(&self.db)
            .await
            .map_err(MetaError::Database)?;

        info!("Created new session: {}", session_id);
        Ok(())
    }

    async fn refresh_session(&self) -> Result<(), MetaError> {
        // Default implementation: update the most recently updated session
        // This maintains backward compatibility
        let session = SessionMeta::find()
            .filter(session_meta::Column::Stale.eq(false))
            .order_by_desc(session_meta::Column::UpdatedAt)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        if let Some(session) = session {
            let session_id = session.id;
            let mut active_model: session_meta::ActiveModel = session.into();
            active_model.updated_at = Set(Utc::now().timestamp_millis());
            active_model.stale = Set(false);

            active_model
                .update(&self.db)
                .await
                .map_err(MetaError::Database)?;

            info!("Refreshed session: {}", session_id);
        } else {
            return Err(MetaError::NotFound(0));
        }

        Ok(())
    }

    async fn refresh_session_by_id(
        &self,
        session_id: &crate::meta::client::session::SessionId,
    ) -> Result<(), MetaError> {
        // For database, try to find the session by UUID first, fallback to hostname+pid for backward compatibility
        let session = SessionMeta::find()
            .filter(session_meta::Column::SessionUuid.eq(session_id.uuid.to_string()))
            .filter(session_meta::Column::Stale.eq(false))
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        let session = if let Some(s) = session {
            s
        } else {
            // Fallback to legacy hostname+pid lookup
            SessionMeta::find()
                .filter(session_meta::Column::Hostname.eq(&session_id.hostname))
                .filter(session_meta::Column::ProcessId.eq(session_id.process_id as i32))
                .filter(session_meta::Column::Stale.eq(false))
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?
                .ok_or_else(|| MetaError::NotFound(0))?
        };

        let session_id_num = session.id;
        let mut active_model: session_meta::ActiveModel = session.into();
        active_model.updated_at = Set(Utc::now().timestamp_millis());
        active_model.stale = Set(false);

        active_model
            .update(&self.db)
            .await
            .map_err(MetaError::Database)?;

        info!(
            "Refreshed database session: {} -> {}:{}",
            session_id_num, session_id.hostname, session_id.process_id
        );

        Ok(())
    }

    async fn find_stale_sessions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<crate::meta::store::SessionInfo>, MetaError> {
        use crate::meta::store::SessionInfo;
        use std::time::SystemTime;

        // Consider sessions stale if not updated for more than 5 minutes (300,000 ms)
        let stale_threshold = Utc::now().timestamp_millis() - 300_000;

        let mut query = SessionMeta::find()
            .filter(session_meta::Column::UpdatedAt.lt(stale_threshold))
            .filter(session_meta::Column::Stale.eq(false))
            .order_by_asc(session_meta::Column::UpdatedAt);

        if let Some(limit) = limit {
            query = query.limit(limit as u64);
        }

        let sessions = query.all(&self.db).await.map_err(MetaError::Database)?;

        let result: Vec<SessionInfo> = sessions
            .into_iter()
            .map(|s| SessionInfo {
                id: s.id,
                info: s.payload,
                updated_at: SystemTime::UNIX_EPOCH
                    + std::time::Duration::from_millis(s.updated_at as u64),
            })
            .collect();

        Ok(result)
    }

    async fn clean_stale_session(&self, session_id: u64) -> Result<(), MetaError> {
        let session = SessionMeta::find_by_id(session_id)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        if let Some(session) = session {
            // Mark as stale instead of deleting to allow for audit trail
            let mut active_model: session_meta::ActiveModel = session.into();
            active_model.stale = Set(true);

            active_model
                .update(&self.db)
                .await
                .map_err(MetaError::Database)?;

            info!("Marked session {} as stale", session_id);
        } else {
            return Err(MetaError::NotFound(session_id as i64));
        }

        Ok(())
    }

    async fn clean_session_by_id(
        &self,
        session_id: &crate::meta::client::session::SessionId,
    ) -> Result<(), MetaError> {
        // Find session by UUID first, fallback to hostname+pid for backward compatibility
        let session = SessionMeta::find()
            .filter(session_meta::Column::SessionUuid.eq(session_id.uuid.to_string()))
            .filter(session_meta::Column::Stale.eq(false))
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        let session = if let Some(s) = session {
            s
        } else {
            // Fallback to legacy hostname+pid lookup
            SessionMeta::find()
                .filter(session_meta::Column::Hostname.eq(&session_id.hostname))
                .filter(session_meta::Column::ProcessId.eq(session_id.process_id as i32))
                .filter(session_meta::Column::Stale.eq(false))
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?
                .ok_or_else(|| MetaError::NotFound(0))?
        };

        // Mark the session as stale
        let mut active_model: session_meta::ActiveModel = session.into();
        active_model.stale = Set(true);
        active_model.updated_at = Set(Utc::now().timestamp_millis());

        active_model
            .update(&self.db)
            .await
            .map_err(MetaError::Database)?;

        info!(
            "Marked session {}:{} as stale",
            session_id.hostname, session_id.process_id
        );

        Ok(())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
