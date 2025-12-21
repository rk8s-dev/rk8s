//! Database-based metadata store implementation
//!
//! Supports SQLite and PostgreSQL backends via SeaORM

use crate::chuck::SliceDesc;
use crate::meta::client::session::{Session, SessionInfo};
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::session_meta::{self, Entity as SessionMeta};
use crate::meta::entities::slice_meta::{self, Entity as SliceMeta};
use crate::meta::entities::*;
use crate::meta::file_lock::{
    FileLockInfo, FileLockQuery, FileLockRange, FileLockType, PlockRecord,
};
use crate::meta::store::{
    DirEntry, FileAttr, LockName, MetaError, MetaStore, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::meta::{INODE_ID_KEY, Permission, SLICE_ID_KEY};
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use log::info;
use sea_orm::prelude::Uuid;
use sea_orm::*;
use sea_query::Index;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

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
                let mut opts = ConnectOptions::new(url.clone());
                opts.max_connections(1)
                    .min_connections(1)
                    .connect_timeout(Duration::from_secs(30))
                    .idle_timeout(Duration::from_secs(30));
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
            match db.execute(sql).await {
                Ok(_) => info!("Statement {} executed successfully", i + 1),
                Err(e) => {
                    if e.to_string().contains("duplicate key") {
                        info!(
                            "Table already exists for statement {}, skipping: {}",
                            i + 1,
                            e
                        );
                        continue;
                    }
                    return Err(MetaError::Database(e));
                }
            }
        }

        let index_stmt = Index::create()
            .if_not_exists()
            .name("idx_content_meta_inode")
            .table(ContentMeta)
            .col(content_meta::Column::Inode)
            .to_owned();

        let index_sql = builder.build(&index_stmt);
        match db.execute(index_sql).await {
            Ok(_) => info!("Index created successfully"),
            Err(e) => {
                if e.to_string().contains("already exists") {
                    info!("Index already exists, skipping: {}", e);
                } else {
                    return Err(MetaError::Database(e));
                }
            }
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

        let parent_meta = AccessMeta::find_by_id(parent_inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if parent_meta.is_none() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::ParentNotFound(parent_inode));
        }
        let parent_meta = parent_meta.unwrap();

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

        // Inherit gid from parent if parent has setgid bit set
        let parent_perm = parent_meta.permission();
        let parent_has_setgid = (parent_perm.mode & 0o2000) != 0;
        let gid = if parent_has_setgid {
            parent_perm.gid
        } else {
            0
        };

        // Directories inherit setgid bit from parent
        let mode = if parent_has_setgid {
            0o42755 // Directory with setgid bit
        } else {
            0o40755 // Regular directory
        };

        let dir_permission = Permission::new(mode, 0, gid);
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

        let parent_meta = AccessMeta::find_by_id(parent_inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        if parent_meta.is_none() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::ParentNotFound(parent_inode));
        }
        let parent_meta = parent_meta.unwrap();

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

        // Inherit gid from parent if parent has setgid bit set
        let parent_perm = parent_meta.permission();
        let parent_has_setgid = (parent_perm.mode & 0o2000) != 0;
        let gid = if parent_has_setgid {
            parent_perm.gid
        } else {
            0
        };

        // Per POSIX semantics: when a directory has the setgid bit set, newly created
        // entries inside inherit the directory's group (gid), but regular files
        // do NOT inherit the setgid bit itself. Only newly created directories
        // should carry the setgid bit. We therefore inherit `gid` from the parent
        // but intentionally do not set the setgid bit on the file mode.
        let file_permission = Permission::new(0o100644, 0, gid);
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

                if last_updated < current_time - ChronoDuration::seconds(7) {
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

        // check file is existing using the same transaction
        let exists = FileMeta::find_by_id(inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;
        if exists.is_none() {
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

                        if records.is_empty() {
                            // No locks to unlock, transaction is complete
                            txn.commit().await.map_err(MetaError::Database)?;
                            return Ok(());
                        }

                        let new_records = PlockRecord::update_locks(records.clone(), *new_lock);

                        if new_records.is_empty() {
                            // No more locks for this (inode, sid, owner) combination, delete the record
                            let delete_model = plock_meta::ActiveModel {
                                inode: Set(plock.inode),
                                sid: Set(plock.sid),
                                owner: Set(plock.owner),
                                ..Default::default()
                            };
                            let _ = delete_model
                                .delete(&txn)
                                .await
                                .map_err(MetaError::Database)?;
                        } else {
                            // Update the existing record with new lock list
                            let new_records_bytes =
                                serde_json::to_vec(&new_records).map_err(|e| {
                                    MetaError::Internal(format!(
                                        "error to serialization Vec<PlockRecord>: {e}"
                                    ))
                                })?;

                            let mut active_model: plock_meta::ActiveModel = plock.into();
                            active_model.records = Set(new_records_bytes);
                            active_model.save(&txn).await.map_err(MetaError::Database)?;
                        }
                    }
                    None => {
                        // No existing lock record found
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

                    let ls: Vec<PlockRecord> = serde_json::from_slice(d).unwrap_or_default();
                    conflict_found = PlockRecord::check_conflict(&lock_type, &range, &ls);
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
                let ls = PlockRecord::update_locks(ls, *new_lock);

                let records = serde_json::to_vec(&ls).map_err(|e| {
                    MetaError::Internal(format!("error to serialization Vec<PlockRecord>: {e}"))
                })?;

                // lock records changed update or insert
                if locks.get(&lkey).map(|r| r != &records).unwrap_or(true) {
                    let plock = plock_meta::ActiveModel {
                        sid: Set(*sid),
                        owner: Set(owner),
                        inode: Set(inode),
                        records: Set(records),
                    };

                    // Check if this is a new record or an update
                    if locks.contains_key(&lkey) {
                        plock.save(&txn).await.map_err(MetaError::Database)?;
                    } else {
                        plock.insert(&txn).await.map_err(MetaError::Database)?;
                    }
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

        // Update old parent mtime and ctime
        let mut old_parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(old_parent)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::ParentNotFound(old_parent))?
            .into();
        old_parent_meta.modify_time = Set(now);
        old_parent_meta.create_time = Set(now);
        old_parent_meta
            .update(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Update new parent mtime and ctime (if different)
        if old_parent != new_parent {
            let mut new_parent_meta: access_meta::ActiveModel = AccessMeta::find_by_id(new_parent)
                .one(&txn)
                .await
                .map_err(MetaError::Database)?
                .ok_or(MetaError::NotFound(new_parent))?
                .into();
            new_parent_meta.modify_time = Set(now);
            new_parent_meta.create_time = Set(now);
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

        if let Some(file) = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            let mut permission = file.permission().clone();
            let mut size = file.size;
            let mut access_time = file.access_time;
            let mut modify_time = file.modify_time;
            let mut create_time = file.create_time;
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

            if let Some(size_req) = req.size {
                let new_size = size_req as i64;
                if size != new_size {
                    size = new_size;
                    modify_time = now;
                }
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::SET_ATIME_NOW) {
                access_time = now;
                ctime_update = true;
            } else if let Some(atime) = req.atime {
                access_time = atime;
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::SET_MTIME_NOW) {
                modify_time = now;
                ctime_update = true;
            } else if let Some(mtime) = req.mtime {
                modify_time = mtime;
                ctime_update = true;
            }

            if let Some(ctime) = req.ctime {
                create_time = ctime;
            } else if ctime_update {
                create_time = now;
            }

            let kind = if file.symlink_target.is_some() {
                FileType::Symlink
            } else {
                FileType::File
            };
            let nlink = file.nlink;
            let symlink_len = file.symlink_target.as_ref().map(|t| t.len() as u64);

            let mut active: file_meta::ActiveModel = file.into();
            active.permission = Set(permission.clone());
            active.size = Set(size);
            active.access_time = Set(access_time);
            active.modify_time = Set(modify_time);
            active.create_time = Set(create_time);
            active.update(&txn).await.map_err(MetaError::Database)?;

            txn.commit().await.map_err(MetaError::Database)?;
            let out = FileAttr {
                ino,
                size: symlink_len.unwrap_or(size as u64),
                kind,
                mode: permission.mode,
                uid: permission.uid,
                gid: permission.gid,
                atime: access_time,
                mtime: modify_time,
                ctime: create_time,
                nlink: nlink as u32,
            };
            return Ok(out);
        }

        if let Some(dir) = AccessMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            let mut permission = dir.permission().clone();
            let mut ctime_update = false;
            let now = Self::now_nanos();
            let mut access_time = dir.access_time;
            let mut modify_time = dir.modify_time;
            let mut create_time = dir.create_time;

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

            if flags.contains(SetAttrFlags::SET_ATIME_NOW) {
                access_time = now;
                ctime_update = true;
            } else if let Some(atime) = req.atime {
                access_time = atime;
                ctime_update = true;
            }

            if flags.contains(SetAttrFlags::SET_MTIME_NOW) {
                modify_time = now;
                ctime_update = true;
            } else if let Some(mtime) = req.mtime {
                modify_time = mtime;
                ctime_update = true;
            }

            if let Some(ctime) = req.ctime {
                create_time = ctime;
            } else if ctime_update {
                create_time = now;
            }

            let mut active: access_meta::ActiveModel = dir.into();
            active.permission = Set(permission);
            active.access_time = Set(access_time);
            active.modify_time = Set(modify_time);
            active.create_time = Set(create_time);
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
            let out_perm = active.permission.clone().unwrap();
            let out_atime = active.access_time.clone().unwrap();
            let out_mtime = active.modify_time.clone().unwrap();
            let out_ctime = active.create_time.clone().unwrap();
            let out_nlink = active.nlink.clone().unwrap();
            active.update(&txn).await.map_err(MetaError::Database)?;

            txn.commit().await.map_err(MetaError::Database)?;
            let out = FileAttr {
                ino,
                size: 4096,
                kind: FileType::Dir,
                mode: out_perm.mode,
                uid: out_perm.uid,
                gid: out_perm.gid,
                atime: out_atime,
                mtime: out_mtime,
                ctime: out_ctime,
                nlink: out_nlink as u32,
            };
            return Ok(out);
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
        let expire = (Utc::now() + ChronoDuration::minutes(5)).timestamp_millis();
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
        let expire = (Utc::now() + ChronoDuration::minutes(5)).timestamp_millis();
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
        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not seted".to_string()))?;

        let rows = PlockMeta::find()
            .filter(plock_meta::Column::Inode.eq(inode))
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        for row in rows {
            let locks: Vec<PlockRecord> = serde_json::from_slice(&row.records).unwrap_or_default();

            if let Some(v) = PlockRecord::get_plock(&locks, query, sid, &row.sid) {
                return Ok(v);
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
                    if lock_type == FileLockType::Write {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::config::{CacheConfig, ClientOptions, DatabaseConfig};
    use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
    use tokio::time;

    fn test_config() -> Config {
        Config {
            database: DatabaseConfig {
                db_config: DatabaseType::Sqlite {
                    url: "sqlite:file::memory:".to_string(),
                },
            },
            cache: CacheConfig::default(),
            client: ClientOptions::default(),
        }
    }

    /// Configuration for shared database testing (multi-session)
    fn shared_db_config() -> Config {
        Config {
            database: DatabaseConfig {
                db_config: DatabaseType::Postgres {
                    url: "postgres://slayerfs:slayerfs@127.0.0.1:5432/database".to_string(),
                },
            },
            cache: CacheConfig::default(),
            client: ClientOptions::default(),
        }
    }

    async fn new_test_store() -> DatabaseMetaStore {
        DatabaseMetaStore::from_config(test_config())
            .await
            .expect("Failed to create test database store")
    }

    /// Create a new test store with pre-configured session ID
    async fn new_test_store_with_session(session_id: Uuid) -> DatabaseMetaStore {
        let store = new_test_store().await;
        store.set_sid(session_id).expect("Failed to set session ID");
        store
    }

    /// Helper struct to manage multiple test sessions
    struct TestSessionManager {
        stores: Vec<DatabaseMetaStore>,
    }

    use std::sync::LazyLock;
    use tokio::sync::Mutex;

    // 静态初始化，确保只执行一次
    static SHARED_DB_INIT: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    impl TestSessionManager {
        async fn new(session_count: usize) -> Self {
            // 获取锁，确保串行初始化
            let _guard = SHARED_DB_INIT.lock().await;

            use std::env;
            // Clean up existing shared test database
            let temp_dir = env::temp_dir();
            let db_path = temp_dir.join("slayerfs_shared_test.db");

            // 只在第一次初始化时清理
            static FIRST_INIT: std::sync::Once = std::sync::Once::new();
            FIRST_INIT.call_once(|| {
                let _ = std::fs::remove_file(&db_path);
            });

            let mut stores = Vec::with_capacity(session_count);
            let mut session_ids = Vec::with_capacity(session_count);

            // 创建第一个 store（会初始化数据库）
            let config = shared_db_config();
            let first_store = DatabaseMetaStore::from_config(config.clone())
                .await
                .expect("Failed to create shared test database store");

            let first_session_id = Uuid::now_v7();
            first_store
                .set_sid(first_session_id)
                .expect("Failed to set session ID");

            stores.push(first_store);
            session_ids.push(first_session_id);

            // 后续的 store 复用已初始化的数据库
            for _ in 1..session_count {
                let store = DatabaseMetaStore::from_config(config.clone())
                    .await
                    .expect("Failed to create shared test database store");

                let session_id = Uuid::now_v7();
                store.set_sid(session_id).expect("Failed to set session ID");

                stores.push(store);
                session_ids.push(session_id);

                time::sleep(time::Duration::from_millis(5)).await;
            }

            Self { stores }
        }

        fn get_store(&self, index: usize) -> &DatabaseMetaStore {
            &self.stores[index]
        }
    }

    #[tokio::test]
    async fn test_basic_read_lock() {
        let store = new_test_store().await;
        let session_id = Uuid::now_v7();
        let owner: u64 = 1001;

        // Set session
        store.set_sid(session_id).unwrap();

        // Create a file first
        let parent = store.root_ino();
        let file_ino = store
            .create_file(parent, "test_file.txt".to_string())
            .await
            .unwrap();

        // Acquire read lock
        store
            .set_plock(
                file_ino,
                owner as i64,
                false,
                FileLockType::Read,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Verify lock exists
        let query = FileLockQuery {
            owner: owner as i64,
            lock_type: FileLockType::Read,
            range: FileLockRange { start: 0, end: 100 },
        };

        let lock_info = store.get_plock(file_ino, &query).await.unwrap();
        assert_eq!(lock_info.lock_type, FileLockType::UnLock);
    }

    #[tokio::test]
    async fn test_multiple_read_locks() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: i64 = 1001;
        let owner2: i64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(parent, "test_multiple_read_locks_file.txt".to_string())
            .await
            .unwrap();

        // First session acquires read lock
        store1
            .set_plock(
                file_ino,
                owner1 as i64,
                false,
                FileLockType::Read,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Second session should be able to acquire read lock on same range
        let store2 = session_mgr.get_store(1);
        store2
            .set_plock(
                file_ino,
                owner2 as i64,
                false,
                FileLockType::Read,
                FileLockRange { start: 0, end: 100 },
                5678,
            )
            .await
            .unwrap();

        // Verify both locks exist by querying each session
        let query1 = FileLockQuery {
            owner: owner1,
            lock_type: FileLockType::Read,
            range: FileLockRange { start: 0, end: 100 },
        };

        let query2 = FileLockQuery {
            owner: owner2,
            lock_type: FileLockType::Write,
            range: FileLockRange { start: 0, end: 100 },
        };

        let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
        assert_eq!(lock_info1.lock_type, FileLockType::UnLock);

        let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
        assert_eq!(lock_info2.lock_type, FileLockType::Read);
        assert_eq!(lock_info2.range.start, 0);
        assert_eq!(lock_info2.range.end, 100);
        assert_eq!(lock_info2.pid, 5678);
    }

    #[tokio::test]
    async fn test_write_lock_conflict() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: u64 = 1001;
        let owner2: u64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(parent, "test_write_lock_conflict_file.txt".to_string())
            .await
            .unwrap();

        // First session acquires read lock
        store1
            .set_plock(
                file_ino,
                owner1 as i64,
                false,
                FileLockType::Read,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Second session should not be able to acquire write lock on overlapping range
        let store2 = session_mgr.get_store(1);
        let result = store2
            .set_plock(
                file_ino,
                owner2 as i64,
                false, // non-blocking
                FileLockType::Write,
                FileLockRange {
                    start: 50,
                    end: 150,
                }, // Overlapping range
                5678,
            )
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::LockConflict {
                inode: err_inode,
                owner: err_owner,
                range: err_range,
            } => {
                assert_eq!(err_inode, file_ino);
                assert_eq!(err_owner, owner2 as i64);
                assert_eq!(err_range.start, 50);
                assert_eq!(err_range.end, 150);
            }
            _ => panic!("Expected LockConflict error"),
        }
    }

    #[tokio::test]
    async fn test_lock_release() {
        let session_id = Uuid::now_v7();
        let owner = 1001;

        // Create a store with pre-configured session
        let store = new_test_store_with_session(session_id).await;

        // Create a file first
        let parent = store.root_ino();
        let file_ino = store
            .create_file(parent, "test_file.txt".to_string())
            .await
            .unwrap();

        // Acquire lock
        store
            .set_plock(
                file_ino,
                owner,
                false,
                FileLockType::Write,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Verify lock exists
        let query = FileLockQuery {
            owner: owner as i64,
            lock_type: FileLockType::Write,
            range: FileLockRange { start: 0, end: 100 },
        };

        let lock_info = store.get_plock(file_ino, &query).await.unwrap();
        assert_eq!(lock_info.lock_type, FileLockType::Write);

        // Release lock
        store
            .set_plock(
                file_ino,
                owner,
                false,
                FileLockType::UnLock,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Verify lock is released
        let lock_info = store.get_plock(file_ino, &query).await.unwrap();
        assert_eq!(lock_info.lock_type, FileLockType::UnLock);
    }

    #[tokio::test]
    async fn test_non_overlapping_locks() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: i64 = 1001;
        let owner2: i64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(parent, "test_none_overlapping_locks_file.txt".to_string())
            .await
            .unwrap();

        // First session acquires lock on range 0-100
        store1
            .set_plock(
                file_ino,
                owner1 as i64,
                false,
                FileLockType::Write,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Second session should be able to acquire lock on non-overlapping range 200-300
        let store2 = session_mgr.get_store(1);
        store2
            .set_plock(
                file_ino,
                owner2 as i64,
                false,
                FileLockType::Write,
                FileLockRange {
                    start: 200,
                    end: 300,
                },
                5678,
            )
            .await
            .unwrap();

        // Verify both locks exist
        let query1 = FileLockQuery {
            owner: owner1,
            lock_type: FileLockType::Write,
            range: FileLockRange { start: 0, end: 100 },
        };

        let query2 = FileLockQuery {
            owner: owner2,
            lock_type: FileLockType::Write,
            range: FileLockRange {
                start: 200,
                end: 300,
            },
        };

        let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
        assert_eq!(lock_info1.lock_type, FileLockType::Write);
        assert_eq!(lock_info1.range.start, 0);
        assert_eq!(lock_info1.range.end, 100);
        assert_eq!(lock_info1.pid, 1234);

        let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
        assert_eq!(lock_info2.lock_type, FileLockType::Write);
        assert_eq!(lock_info2.range.start, 200);
        assert_eq!(lock_info2.range.end, 300);
        assert_eq!(lock_info2.pid, 5678);
    }

    #[tokio::test]
    async fn test_concurrent_read_write_locks() {
        // Test multiple sessions acquiring different types of locks
        let session_mgr = TestSessionManager::new(3).await;

        // Create a file
        let store0 = session_mgr.get_store(0);
        let parent = store0.root_ino();
        let file_ino = store0
            .create_file(parent, "concurrent_test.txt".to_string())
            .await
            .unwrap();

        let owner1: i64 = 1001;
        let owner2: i64 = 1002;
        let owner3: i64 = 1003;

        // Session 1: Acquire write lock on range 0-100
        {
            let store1 = session_mgr.get_store(0);
            store1
                .set_plock(
                    file_ino,
                    owner1 as i64,
                    false,
                    FileLockType::Write,
                    FileLockRange { start: 0, end: 100 },
                    1111,
                )
                .await
                .expect("Failed to acquire write lock");
        }

        // Session 2: Acquire read lock on range 200-300 (should succeed)
        {
            let store2 = session_mgr.get_store(1);
            store2
                .set_plock(
                    file_ino,
                    owner2 as i64,
                    false,
                    FileLockType::Read,
                    FileLockRange {
                        start: 200,
                        end: 300,
                    },
                    2222,
                )
                .await
                .expect("Failed to acquire read lock");
        }

        // Session 3: Try to acquire write lock on overlapping range 50-150 (should fail)
        {
            let store3 = session_mgr.get_store(2);
            let result = store3
                .set_plock(
                    file_ino,
                    owner3 as i64,
                    false,
                    FileLockType::Write,
                    FileLockRange {
                        start: 50,
                        end: 150,
                    },
                    3333,
                )
                .await;

            // Verify it fails with LockConflict
            assert!(result.is_err());
            match result.unwrap_err() {
                MetaError::LockConflict { .. } => {}
                _ => panic!("Expected LockConflict error"),
            }
        }

        // Verify successful locks exist
        let query1 = FileLockQuery {
            owner: owner1,
            lock_type: FileLockType::Write,
            range: FileLockRange { start: 0, end: 100 },
        };

        let query2 = FileLockQuery {
            owner: owner2,
            lock_type: FileLockType::Read,
            range: FileLockRange {
                start: 200,
                end: 300,
            },
        };

        // Check locks from different sessions
        {
            let store1 = session_mgr.get_store(0);
            let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
            assert_eq!(lock_info1.lock_type, FileLockType::Write);
        }

        {
            let store2 = session_mgr.get_store(1);
            let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
            assert_eq!(lock_info2.lock_type, FileLockType::UnLock);
        }
    }

    #[tokio::test]
    async fn test_cross_session_lock_visibility() {
        // Test that locks set by one session are visible to another session
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: u64 = 1001;

        // Create a file
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(parent, "visibility_test.txt".to_string())
            .await
            .unwrap();

        // Session 1 acquires a write lock
        store1
            .set_plock(
                file_ino,
                owner1 as i64,
                false,
                FileLockType::Write,
                FileLockRange {
                    start: 0,
                    end: 1000,
                },
                4444,
            )
            .await
            .unwrap();

        // Session 2 should be able to see the lock (and respect it)
        let store2 = session_mgr.get_store(1);
        let conflict_result = store2
            .set_plock(
                file_ino,
                2002, // different owner
                false,
                FileLockType::Write,
                FileLockRange {
                    start: 500,
                    end: 600,
                }, // overlapping range
                5555,
            )
            .await;

        // Should fail due to lock conflict
        assert!(conflict_result.is_err());
        match conflict_result.unwrap_err() {
            MetaError::LockConflict { .. } => {}
            _ => panic!("Expected LockConflict error"),
        }

        // Session 1 releases the lock
        store1
            .set_plock(
                file_ino,
                owner1 as i64,
                false,
                FileLockType::UnLock,
                FileLockRange {
                    start: 0,
                    end: 1000,
                },
                4444,
            )
            .await
            .unwrap();

        // Now Session 2 should be able to acquire the lock
        store2
            .set_plock(
                file_ino,
                2002,
                false,
                FileLockType::Write,
                FileLockRange {
                    start: 500,
                    end: 600,
                },
                5555,
            )
            .await
            .unwrap();

        // Verify the lock exists
        let query = FileLockQuery {
            owner: 2002,
            lock_type: FileLockType::Write,
            range: FileLockRange {
                start: 500,
                end: 600,
            },
        };

        let lock_info = store2.get_plock(file_ino, &query).await.unwrap();
        assert_eq!(lock_info.lock_type, FileLockType::Write);
        assert_eq!(lock_info.pid, 5555);
    }
}
