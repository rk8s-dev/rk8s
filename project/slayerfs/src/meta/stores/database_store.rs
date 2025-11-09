//! Database-based metadata store implementation
//!
//! Supports SQLite and PostgreSQL backends via SeaORM

use crate::meta::Permission;
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::*;
use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::Utc;
use log::info;
use sea_orm::*;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Database-based metadata store
pub struct DatabaseMetaStore {
    db: DatabaseConnection,
    _config: Config,
    next_inode: AtomicU64,
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
        let store = Self {
            db,
            _config,
            next_inode,
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
        let store = Self {
            db,
            _config,
            next_inode,
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
        ];

        for (i, stmt) in stmts.iter().enumerate() {
            let sql = builder.build(stmt);
            db.execute(sql).await.map_err(|e| {
                eprintln!("Failed to execute statement {}: {}", i + 1, e);
                MetaError::Database(e)
            })?;
        }

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
}

#[async_trait]
impl MetaStore for DatabaseMetaStore {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        if let Ok(Some(file_meta)) = self.get_file_meta(ino).await {
            let permission = file_meta.permission();
            return Ok(Some(FileAttr {
                ino: file_meta.inode,
                size: file_meta.size as u64,
                kind: FileType::File,
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
            .filter(content_meta::Column::EntryType.eq(EntryType::File))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or_else(|| {
                MetaError::Internal(format!("File '{}' not found in parent {}", name, parent))
            })?;

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

        let current_nlink = match &file_meta.nlink {
            Set(n) => *n,
            _ => 1,
        };

        if current_nlink > 1 {
            file_meta.nlink = Set(current_nlink - 1);
            file_meta.update(&txn).await.map_err(MetaError::Database)?;
        } else {
            file_meta.deleted = Set(true);
            file_meta.nlink = Set(0);
            file_meta.update(&txn).await.map_err(MetaError::Database)?;
        }

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
            file_meta.modify_time = Set(Utc::now().timestamp_nanos_opt().unwrap_or(0));
        }

        file_meta
            .update(&self.db)
            .await
            .map_err(|e| MetaError::Internal(e.to_string()))?;

        Ok(())
    }

    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError> {
        if ino == 1 {
            return Ok(None);
        }

        let entry = ContentMeta::find_by_id(ino)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entry.map(|e| e.parent_inode))
    }

    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == 1 {
            return Ok(Some("/".to_string()));
        }

        let entry = ContentMeta::find_by_id(ino)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entry.map(|e| e.entry_name))
    }

    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError> {
        if ino == 1 {
            return Ok(Some("/".to_string()));
        }

        let mut path_parts = Vec::new();
        let mut current_ino = ino;

        loop {
            let entry = content_meta::Entity::find_by_id(current_ino)
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
}
