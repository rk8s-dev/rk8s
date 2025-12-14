//! Database-based metadata store implementation
//!
//! Supports SQLite and PostgreSQL backends via SeaORM

use crate::chuck::SliceDesc;
use crate::meta::client::session::{Session, SessionInfo};
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::session_meta::{self, Entity as SessionMeta};
use crate::meta::entities::slice_meta::{self, Entity as SliceMeta};
use crate::meta::file_lock::{
    FileLockInfo, FileLockQuery, FileLockRange, FileLockType, PlockRecord,
};
use crate::meta::store::{
    DirEntry, FileAttr, LockName, MetaError, MetaStore, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::meta::{INODE_ID_KEY, Permission, SLICE_ID_KEY};
use crate::meta::{entities::*, file_lock};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use log::info;
use sea_orm::prelude::Uuid;
use sea_orm::*;
use sea_query::Index;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::Instrument;

#[derive(Eq, Hash, PartialEq)]
struct PlockHashMapKey {
    pub sid: Uuid,
    pub owner: i64,
}

/// Database-based metadata store
pub struct DatabaseMetaStore {
    db: DatabaseConnection,
    sid: OnceLock<Uuid>,
    _config: Config,
    next_inode: AtomicU64,
    next_slice: AtomicU64,
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
        let store = Self {
            db,
            sid: OnceLock::new(),
            _config,
            next_inode,
            next_slice,
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
        let store = Self {
            db,
            sid: OnceLock::new(),
            _config,
            next_inode,
            next_slice,
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
            schema
                .create_table_from_entity(LocksMeta)
                .if_not_exists()
                .to_owned(),
            schema
                .create_table_from_entity(PlockMeta)
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

    /// Check file is existing
    async fn file_is_existing(&self, inode: i64) -> Result<bool, MetaError> {
        let existing = FileMeta::find_by_id(inode)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;
        match existing {
            Some(_) => Ok(true),
            None => Ok(true),
        }
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

    async fn get_lock_internal(&self, lock_name: LockName) -> anyhow::Result<bool> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let lock_name_str = lock_name.to_string();
        let lock_ = LocksMeta::find()
            .filter(locks_meta::Column::LockName.eq(lock_name_str.clone()))
            .one(&txn)
            .await?;

        let current_time = Utc::now();
        let flag: bool;
        match lock_ {
            Some(lock) => {
                let mut lock = lock.into_active_model();

                let last_updated = match &lock.last_updated {
                    ActiveValue::Set(val) | ActiveValue::Unchanged(val) => *val,
                    ActiveValue::NotSet => {
                        return Err(anyhow::anyhow!("Lock last_updated field is not set"));
                    }
                };

                if last_updated < current_time - Duration::seconds(7) {
                    lock.last_updated = ActiveValue::Set(current_time);
                    lock.update(&txn).await?;
                    flag = true;
                } else {
                    flag = false;
                }
            }
            None => {
                let lock = locks_meta::ActiveModel {
                    lock_name: ActiveValue::Set(lock_name_str),
                    last_updated: ActiveValue::Set(current_time),
                };
                lock.insert(&txn).await?;
                flag = true;
            }
        };

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(flag)
    }

    async fn shutdown_session_internal<C: ConnectionTrait>(
        &self,
        session_id: Uuid,
        conn: &C,
    ) -> Result<(), MetaError> {
        let session = SessionMeta::find()
            .filter(session_meta::Column::SessionId.eq(session_id))
            .one(conn)
            .await?;
        let session = match session {
            Some(s) => s.into_active_model(),
            None => return Err(MetaError::SessionNotFound(session_id)),
        };
        session.delete(conn).await.map_err(MetaError::Database)?;
        Ok(())
    }
    async fn try_set_plock(
        &self,
        inode: i64,
        owner: i64,
        new_lock: &PlockRecord,
        lock_type: FileLockType,
        range: FileLockRange,
    ) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        // chech file is existing
        let exists = self.file_is_existing(inode).await?;
        if !exists {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotFound(inode));
        }

        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not seted".to_string()))?;

        match lock_type {
            FileLockType::UnLock => {
                // unlock file
                let row = PlockMeta::find()
                    .filter(plock_meta::Column::Inode.eq(inode))
                    .filter(plock_meta::Column::Owner.eq(owner))
                    .filter(plock_meta::Column::Sid.eq(*sid))
                    .one(&txn)
                    .await
                    .map_err(MetaError::Database)?;

                match row {
                    Some(plock) => {
                        let records: Vec<PlockRecord> =
                            serde_json::from_slice(&plock.records).unwrap_or_default();

                        if records.len() == 0 {
                            txn.commit().await.map_err(MetaError::Database)?;
                            return Ok(());
                        }

                        let new_records = PlockRecord::update_locks(records, new_lock.clone());
                        let new_records_bytes = serde_json::to_vec(&new_records).map_err(|e| {
                            MetaError::Internal(format!(
                                "error to serialization Vec<PlockRecord>: {e}"
                            ))
                        })?;

                        let mut active_model = plock_meta::ActiveModel {
                            inode: Set(inode),
                            sid: Set(*sid),
                            owner: Set(owner),
                            ..Default::default()
                        };

                        if new_records.len() == 0 {
                            let _ = PlockMeta::delete(active_model)
                                .exec(&txn)
                                .await
                                .map_err(MetaError::Database)?;
                        } else {
                            active_model.records = Set(new_records_bytes);
                            active_model
                                .insert(&txn)
                                .await
                                .map_err(MetaError::Database)?;
                        }
                    }
                    None => {
                        txn.commit().await.map_err(MetaError::Database)?;
                        return Ok(());
                    }
                }

                txn.commit().await.map_err(MetaError::Database)?;
                Ok(())
            }
            _ => {
                let ps = PlockMeta::find()
                    .filter(plock_meta::Column::Inode.eq(inode))
                    .all(&txn)
                    .await
                    .map_err(MetaError::Database)?;

                let mut locks = HashMap::new();
                for item in ps {
                    let key = PlockHashMapKey {
                        sid: item.sid,
                        owner: item.owner,
                    };
                    locks.insert(key, item.records);
                }

                let lkey = PlockHashMapKey { sid: *sid, owner };

                // check conflict
                let mut conflict_found = false;
                for (k, d) in &locks {
                    if *k == lkey {
                        continue;
                    }

                    let ls: Vec<PlockRecord> = serde_json::from_slice(&d).unwrap_or_default();
                    for l in ls {
                        if (lock_type == FileLockType::WriteLock
                            || l.lock_type == FileLockType::WriteLock)
                            && range.end >= l.lock_range.start
                            && range.start <= l.lock_range.end
                        {
                            conflict_found = true;
                            break;
                        }
                    }
                    if conflict_found {
                        break;
                    }
                }

                if conflict_found {
                    txn.rollback().await.map_err(MetaError::Database)?;
                    return Err(MetaError::LockConflict {
                        inode,
                        owner,
                        range,
                    });
                }

                let ls =
                    serde_json::from_slice(locks.get(&lkey).unwrap_or(&vec![])).unwrap_or_default();
                let ls = PlockRecord::update_locks(ls, new_lock.clone());

                let records = serde_json::to_vec(&ls).map_err(|e| {
                    MetaError::Internal(format!("error to serialization Vec<PlockRecord>: {e}"))
                })?;

                // lock records changed update
                if locks.get(&lkey).map(|r| r != &records).unwrap_or(true) {
                    let plock = plock_meta::ActiveModel {
                        sid: Set(*sid),
                        owner: Set(owner),
                        inode: Set(inode),
                        records: Set(records),
                    };
                    plock.save(&txn).await.map_err(MetaError::Database)?;
                }

                txn.commit().await.map_err(MetaError::Database)?;
                Ok(())
            }
        }
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
            other => Err(MetaError::NotSupported(format!(
                "next_id not supported for key {other}"
            ))),
        }
    }

    // ---------- Session lifecycle implementation ----------

    async fn new_session(&self, session_info: SessionInfo) -> Result<Session, MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let session_id = Uuid::now_v7();
        let expire = (Utc::now() + Duration::minutes(5)).timestamp_millis();
        let payload = serde_json::to_vec(&session_info).map_err(MetaError::Serialization)?;
        let session = session_meta::ActiveModel {
            session_id: Set(session_id),
            session_info: Set(payload),
            expire: Set(expire),
        };
        if let Err(e) = session.insert(&self.db).await {
            let _ = txn.rollback().await;
            return Err(MetaError::Database(e));
        }
        txn.commit().await.map_err(MetaError::Database)?;
        Ok(Session {
            session_id,
            expire,
            session_info,
        })
    }

    async fn refresh_session(&self, session_id: Uuid) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let expire = (Utc::now() + Duration::minutes(5)).timestamp_millis();
        let session = SessionMeta::find()
            .filter(session_meta::Column::SessionId.eq(session_id))
            .one(&txn)
            .await?;
        let mut session = match session {
            Some(s) => s.into_active_model(),
            None => return Err(MetaError::SessionNotFound(session_id)),
        };
        session.expire = Set(expire);
        session.update(&txn).await.map_err(MetaError::Database)?;
        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn shutdown_session(&self, session_id: Uuid) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        self.shutdown_session_internal(session_id, &txn).await?;
        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn cleanup_sessions(&self) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let sessions = SessionMeta::find()
            .filter(session_meta::Column::Expire.lt(Utc::now().timestamp_millis()))
            .all(&txn)
            .await?;

        for session in sessions {
            let session_id = session.session_id;
            self.shutdown_session_internal(session_id, &txn).await?;
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn get_lock(&self, lock_name: LockName) -> bool {
        self.get_lock_internal(lock_name).await.unwrap_or_default()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    // returns the current lock owner for a range on a file.
    async fn get_plock(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, MetaError> {
        let rows = PlockMeta::find()
            .filter(plock_meta::Column::Inode.eq(inode))
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        for row in rows {
            let locks: Vec<PlockRecord> = serde_json::from_slice(&row.records).unwrap_or_default();

            for lock in locks {
                if (lock.lock_type == FileLockType::WriteLock
                    || query.lock_type == FileLockType::WriteLock)
                    && lock.lock_range.overlaps(&query.range)
                {
                    let sid = self
                        .sid
                        .get()
                        .ok_or(MetaError::Internal("sid not seted".to_string()))?;

                    if *sid == row.sid {
                        return Ok(FileLockInfo {
                            lock_type: lock.lock_type,
                            range: lock.lock_range,
                            pid: lock.pid,
                        });
                    } else {
                        return Ok(FileLockInfo {
                            lock_type: lock.lock_type,
                            range: lock.lock_range,
                            pid: 0,
                        });
                    }
                }
            }
        }

        Ok(FileLockInfo {
            lock_type: FileLockType::UnLock,
            range: FileLockRange { start: 0, end: 0 },
            pid: 0,
        })
    }

    // sets a file range lock on given file.
    async fn set_plock(
        &self,
        inode: i64,
        owner: i64,
        block: bool,
        lock_type: FileLockType,
        range: FileLockRange,
        pid: u32,
    ) -> Result<(), MetaError> {
        let new_lock = PlockRecord::new(lock_type, pid, range.start, range.end);

        loop {
            let result = self
                .try_set_plock(inode, owner, &new_lock, lock_type, range)
                .await;

            match result {
                Ok(()) => return Ok(()),
                Err(MetaError::LockConflict { .. }) if block => {
                    if lock_type == FileLockType::WriteLock {
                        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                    } else {
                        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                    }
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn set_sid(&self, sid: Uuid) -> Result<(), MetaError> {
        self.sid
            .set(sid)
            .map_err(|_| MetaError::Internal("sid has been seted".to_string()))
    }
}
