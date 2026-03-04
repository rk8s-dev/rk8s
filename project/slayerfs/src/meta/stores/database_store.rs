//! Database-based metadata store implementation
//!
//! Supports SQLite and PostgreSQL backends via SeaORM

use super::{TrimAction, apply_truncate_plan, trim_action};
use crate::chuck::SliceDesc;
use crate::meta::client::session::{Session, SessionInfo};
use crate::meta::config::{Config, DatabaseType};
use crate::meta::entities::counter_meta;
use crate::meta::entities::link_parent_meta;
use crate::meta::entities::session_meta::{self, Entity as SessionMeta};
use crate::meta::entities::slice_meta::{self, Entity as SliceMeta};
use crate::meta::entities::xattr_meta;
use crate::meta::entities::*;
use crate::meta::file_lock::{
    FileLockInfo, FileLockQuery, FileLockRange, FileLockType, PlockRecord,
};
use crate::meta::store::{
    DirEntry, FileAttr, LockName, MetaError, MetaStore, OpenFlags, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::meta::{INODE_ID_KEY, Permission, SLICE_ID_KEY};
// Note: Intervals was used for merge_slices but no longer needed after
// changing implementation to not split slices (avoiding offset change issues)
use crate::utils::NumCastExt;
use crate::vfs::chunk_id_for;
use crate::vfs::fs::FileType;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, Utc};
use log::info;
use sea_orm::ActiveValue::{self, Set, Unchanged};
use sea_orm::prelude::Uuid;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectOptions, ConnectionTrait, Database, DatabaseConnection,
    DatabaseTransaction, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect, Schema, TransactionTrait, sea_query,
};
use sea_query::Index;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, warn};

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

        let store = Self {
            db,
            sid: OnceLock::new(),
            _config,
        };
        store.init_counters().await?;
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

        let store = Self {
            db,
            sid: OnceLock::new(),
            _config,
        };
        store.init_counters().await?;
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
            .unwrap_or(0); // Changed from 1 to 0 - root directory is inode 1

        let max_file = FileMeta::find()
            .order_by_desc(file_meta::Column::Inode)
            .one(db)
            .await
            .map_err(MetaError::Database)?
            .map(|r| r.inode as u64)
            .unwrap_or(0); // Changed from 1 to 0

        // Ensure next inode is at least 2 (root is 1)
        let next = max_access.max(max_file).max(1) + 1;
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

    async fn init_counters(&self) -> Result<(), MetaError> {
        let next_inode = i64::try_from(Self::init_next_inode(&self.db).await?)
            .map_err(|_| MetaError::Internal("inode counter overflow".to_string()))?;
        let next_slice = i64::try_from(Self::init_next_slice(&self.db).await?)
            .map_err(|_| MetaError::Internal("slice counter overflow".to_string()))?;

        Self::set_counter_floor(&self.db, INODE_ID_KEY, next_inode).await?;
        Self::set_counter_floor(&self.db, SLICE_ID_KEY, next_slice).await?;
        Ok(())
    }

    fn is_unique_violation(err: &sea_orm::DbErr) -> bool {
        let msg = err.to_string().to_lowercase();
        msg.contains("duplicate") || msg.contains("unique")
    }

    async fn set_counter_floor(
        db: &DatabaseConnection,
        key: &str,
        floor: i64,
    ) -> Result<(), MetaError> {
        loop {
            let existing = CounterMeta::find_by_id(key.to_string())
                .one(db)
                .await
                .map_err(MetaError::Database)?;

            match existing {
                Some(model) if model.value >= floor => return Ok(()),
                Some(_) => {
                    let updated = CounterMeta::update_many()
                        .col_expr(counter_meta::Column::Value, sea_query::Expr::value(floor))
                        .filter(counter_meta::Column::Name.eq(key))
                        .filter(counter_meta::Column::Value.lt(floor))
                        .exec(db)
                        .await
                        .map_err(MetaError::Database)?;
                    if updated.rows_affected > 0 {
                        return Ok(());
                    }
                }
                None => {
                    let row = counter_meta::ActiveModel {
                        name: Set(key.to_string()),
                        value: Set(floor),
                    };
                    match row.insert(db).await {
                        Ok(_) => return Ok(()),
                        Err(err) if Self::is_unique_violation(&err) => continue,
                        Err(err) => return Err(MetaError::Database(err)),
                    }
                }
            }
        }
    }

    async fn alloc_counter_id(&self, key: &str) -> Result<i64, MetaError> {
        const MAX_RETRIES: usize = 64;

        for _ in 0..MAX_RETRIES {
            let Some(row) = CounterMeta::find_by_id(key.to_string())
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?
            else {
                Self::set_counter_floor(&self.db, key, 1).await?;
                continue;
            };

            let next = row
                .value
                .checked_add(1)
                .ok_or_else(|| MetaError::Internal(format!("counter overflow for key {key}")))?;

            let updated = CounterMeta::update_many()
                .col_expr(counter_meta::Column::Value, sea_query::Expr::value(next))
                .filter(counter_meta::Column::Name.eq(key))
                .filter(counter_meta::Column::Value.eq(row.value))
                .exec(&self.db)
                .await
                .map_err(MetaError::Database)?;

            if updated.rows_affected == 1 {
                return Ok(row.value);
            }
        }

        Err(MetaError::Internal(format!(
            "failed to allocate counter value for key {key}: contention limit exceeded"
        )))
    }

    /// Create database connection
    async fn create_connection(config: &Config) -> Result<DatabaseConnection, MetaError> {
        match &config.database.db_config {
            DatabaseType::Sqlite { url } => {
                info!("Connecting to SQLite: {}", url);
                let mut opts = ConnectOptions::new(url.clone());
                // SQLite named shared memory (sqlite:file::memory:) needs single connection
                // SQLite anonymous in-memory (sqlite::memory:) can use multiple connections
                // Check for file::memory: first (more specific) before ::memory: (more general)
                if url.contains("file::memory:") {
                    // Named shared memory databases require exactly 1 connection
                    opts.max_connections(1).min_connections(1);
                } else if url.contains("::memory:") {
                    // Anonymous in-memory databases can use multiple connections for tests
                    opts.max_connections(5).min_connections(1);
                } else {
                    // File-based databases can use more connections
                    opts.max_connections(10).min_connections(1);
                }
                opts.connect_timeout(Duration::from_secs(30))
                    .idle_timeout(Duration::from_secs(30))
                    .acquire_timeout(Duration::from_secs(30));
                let db = Database::connect(opts).await?;
                Ok(db)
            }
            DatabaseType::Postgres { url } => {
                info!("Connecting to PostgreSQL: {}", url);
                let mut opts = ConnectOptions::new(url.clone());
                opts.max_connections(20)
                    .min_connections(2)
                    .connect_timeout(Duration::from_secs(30))
                    .idle_timeout(Duration::from_secs(30))
                    .acquire_timeout(Duration::from_secs(30));
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
                .create_table_from_entity(CounterMeta)
                .if_not_exists()
                .to_owned(),
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
                .create_table_from_entity(LinkParentMeta)
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
            schema
                .create_table_from_entity(XattrMeta)
                .if_not_exists()
                .to_owned(),
            schema
                .create_table_from_entity(DelayedSlice)
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
        if let Some(root) = self.get_access_meta(1).await? {
            info!(
                "Root directory already exists with inode 1, nlink={}",
                root.nlink
            );
            return Ok(());
        }

        info!("Creating root directory with inode 1...");
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
        info!("Root directory created successfully with inode 1");

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
        let inode = self.alloc_counter_id(INODE_ID_KEY).await?;

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
        let inode = self.alloc_counter_id(INODE_ID_KEY).await?;

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
            parent: Set(parent_inode),
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

    fn now_nanos() -> i64 {
        Utc::now().timestamp_nanos_opt().unwrap_or(0)
    }

    async fn prune_slices_for_truncate<C>(
        &self,
        conn: &C,
        ino: i64,
        new_size: u64,
        old_size: u64,
        chunk_size: u64,
    ) -> Result<(), MetaError>
    where
        C: ConnectionTrait,
    {
        apply_truncate_plan(
            new_size,
            old_size,
            chunk_size,
            |cutoff_chunk, cutoff_offset| async move {
                let chunk_id = i64::try_from(chunk_id_for(ino, cutoff_chunk)?)
                    .map_err(|_| MetaError::Internal("chunk_id overflow".to_string()))?;
                let rows = SliceMeta::find()
                    .filter(slice_meta::Column::ChunkId.eq(chunk_id))
                    .order_by_asc(slice_meta::Column::Id)
                    .all(conn)
                    .await
                    .map_err(MetaError::Database)?;

                for row in rows {
                    debug_assert!(row.offset >= 0);
                    debug_assert!(row.length >= 0);
                    let offset = row.offset as u64;
                    let length = row.length as u64;

                    match trim_action(offset, length, cutoff_offset) {
                        TrimAction::Keep => {}
                        TrimAction::Drop => {
                            let active: slice_meta::ActiveModel = row.into();
                            active.delete(conn).await.map_err(MetaError::Database)?;
                        }
                        TrimAction::Truncate(new_len) => {
                            let mut active: slice_meta::ActiveModel = row.into();
                            active.length = Set(new_len as i64);
                            active.update(conn).await.map_err(MetaError::Database)?;
                        }
                    }
                }
                Ok(())
            },
            |start, end| async move {
                let start_chunk_id = i64::try_from(chunk_id_for(ino, start)?)
                    .map_err(|_| MetaError::Internal("chunk_id overflow".to_string()))?;
                let end_chunk_id = i64::try_from(chunk_id_for(ino, end)?)
                    .map_err(|_| MetaError::Internal("chunk_id overflow".to_string()))?;
                SliceMeta::delete_many()
                    .filter(slice_meta::Column::ChunkId.gte(start_chunk_id))
                    .filter(slice_meta::Column::ChunkId.lt(end_chunk_id))
                    .exec(conn)
                    .await
                    .map_err(MetaError::Database)?;
                Ok(())
            },
        )
        .await
    }

    /// Convert FileMeta to FileAttr
    fn file_meta_to_attr(file_meta: &FileMetaModel) -> FileAttr {
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

        FileAttr {
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
        }
    }

    /// Convert AccessMeta to FileAttr
    fn access_meta_to_attr(access_meta: &AccessMetaModel) -> FileAttr {
        let permission = access_meta.permission();
        FileAttr {
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
        }
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

    async fn shutdown_session_by_id<C: ConnectionTrait>(
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

        PlockMeta::delete_many()
            .filter(plock_meta::Column::Sid.eq(session_id))
            .exec(conn)
            .await?;
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
            .ok_or_else(|| MetaError::Internal("sid not set".to_string()))?;

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

    fn set_sid(&self, session_id: Uuid) -> Result<(), MetaError> {
        self.sid
            .set(session_id)
            .map_err(|_| MetaError::Internal("sid has been set".to_string()))?;
        Ok(())
    }

    fn get_sid(&self) -> Result<&Uuid, MetaError> {
        self.sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid has not been set".to_string()))
    }

    async fn refresh_session(session_id: Uuid, conn: &DatabaseConnection) -> Result<(), MetaError> {
        let txn = conn.begin().await.map_err(MetaError::Database)?;
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

    async fn life_cycle(token: CancellationToken, session_id: Uuid, conn: DatabaseConnection) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            select! {
                _ = interval.tick() => {
                    // refresh session
                    match Self::refresh_session(session_id, &conn).await {
                        Ok(_) => {}
                        Err(err) => {
                            error!("Failed to refresh session: {}", err);
                        }
                    }

                }
                _ = token.cancelled() => {
                    break;
                }
            }
        }
    }

    /// Delete all slices for the specified chunk (internal method that accepts a transaction parameter)
    async fn delete_all_slices(
        &self,
        txn: &impl ConnectionTrait,
        chunk_id: u64,
    ) -> Result<(), MetaError> {
        SliceMeta::delete_many()
            .filter(slice_meta::Column::ChunkId.eq(chunk_id as i64))
            .exec(txn)
            .await
            .map_err(MetaError::Database)?;
        Ok(())
    }

    /// Replace all slices of the specified chunk (internal method, accepting transaction parameters)
    async fn replace_slices(
        &self,
        txn: &impl ConnectionTrait,
        chunk_id: u64,
        slices: &[SliceDesc],
    ) -> Result<(), MetaError> {
        self.delete_all_slices(txn, chunk_id).await?;
        for slice in slices {
            let model = slice_meta::ActiveModel {
                chunk_id: Set(chunk_id as i64),
                slice_id: Set(slice.slice_id as i64),
                offset: Set(slice.offset.as_i64()),
                length: Set(slice.length.as_i64()),
                ..Default::default()
            };
            model.insert(txn).await.map_err(MetaError::Database)?;
        }
        Ok(())
    }

    async fn cleanup_delayed_slices(
        &self,
        chunk_id: u64,
        delayed: &[u8],
        txn: &DatabaseTransaction,
    ) -> Result<(), MetaError> {
        if delayed.is_empty() {
            return Ok(());
        }

        // Format: slice_id (u64) + offset (u64) + size (u32) = 20 bytes per slice
        if !delayed.len().is_multiple_of(20) {
            return Err(MetaError::Internal(
                "Invalid delayed data length".to_string(),
            ));
        }

        let now = Utc::now().timestamp();

        for chunk in delayed.chunks(20) {
            let slice_id = u64::from_le_bytes([
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
            ]);
            let offset = u64::from_le_bytes([
                chunk[8], chunk[9], chunk[10], chunk[11], chunk[12], chunk[13], chunk[14],
                chunk[15],
            ]);
            let size = u32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]);

            let delayed_model = delayed_slice::ActiveModel {
                slice_id: Set(slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(offset as i64),
                size: Set(size as i64),
                created_at: Set(now),
                reason: Set("compact".to_string()),
                ..Default::default()
            };

            delayed_model
                .insert(txn)
                .await
                .map_err(MetaError::Database)?;
        }

        Ok(())
    }

    async fn process_delayed_slices(
        &self,
        batch_size: usize,
        max_age_secs: i64,
    ) -> Result<Vec<(u64, u64, u64)>, MetaError> {
        // get delayed slices that are old enough
        let cutoff_time = Utc::now().timestamp() - max_age_secs;

        let delayed_slices: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::CreatedAt.lt(cutoff_time))
            .limit(batch_size as u64)
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        if delayed_slices.is_empty() {
            return Ok(vec![]);
        }

        let mut deleted_slices = Vec::new();
        let mut succeeded_ids = Vec::new();
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        for delayed in &delayed_slices {
            // delete from slice_meta table by slice_id column (not primary key)
            // note: slice_meta primary key is 'id', but we need to delete by 'slice_id' column
            let result = SliceMeta::delete_many()
                .filter(slice_meta::Column::SliceId.eq(delayed.slice_id))
                .exec(&txn)
                .await;

            match result {
                Ok(deleted) => {
                    if deleted.rows_affected > 0 {
                        debug!(
                            slice_id = delayed.slice_id,
                            chunk_id = delayed.chunk_id,
                            "Deleted old slice after verification"
                        );
                    } else {
                        // slice already deleted, this is ok
                        debug!(
                            slice_id = delayed.slice_id,
                            "Slice already deleted, cleaning up delayed record"
                        );
                    }
                    // Mark for removal from delayed_slice table and record for block store cleanup
                    // Returns (slice_id, offset, size) for proper block range calculation
                    succeeded_ids.push(delayed.id);
                    deleted_slices.push((
                        delayed.slice_id as u64,
                        delayed.offset as u64,
                        delayed.size as u64,
                    ));
                }
                Err(e) => {
                    warn!(
                        slice_id = delayed.slice_id,
                        error = ?e,
                        "Failed to delete old slice from slice_meta, will retry later"
                    );
                    continue;
                }
            }
        }

        for delayed_id in &succeeded_ids {
            DelayedSlice::delete_by_id(*delayed_id)
                .exec(&txn)
                .await
                .map_err(|e| {
                    warn!(
                        delayed_id = *delayed_id,
                        error = ?e,
                        "Failed to remove from delayed_slice table, transaction will rollback"
                    );
                    MetaError::Database(e)
                })?;
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(deleted_slices)
    }

    /// Get compaction statistics for a chunk
    /// Returns (slice_count, total_size, fragmentation_ratio)
    async fn get_chunk_compact_stats(&self, chunk_id: u64) -> Result<(usize, u64, f64), MetaError> {
        // Get all slices for this chunk
        let slices = self.get_slices(chunk_id).await?;
        let slice_count = slices.len();

        if slice_count == 0 {
            return Ok((0, 0, 0.0));
        }

        // Calculate total size
        let total_size: u64 = slices.iter().map(|s| s.length).sum();

        // Calculate fragmentation ratio
        // Fragmentation = (total_slice_size - merged_slice_size) / total_slice_size
        let merged = self.merge_slices(&slices).await?;
        let merged_size: u64 = merged.iter().map(|s| s.length).sum();

        let fragmentation_ratio = if total_size > 0 {
            (total_size - merged_size) as f64 / total_size as f64
        } else {
            0.0
        };

        Ok((slice_count, total_size, fragmentation_ratio))
    }

    /// check if a chunk needs compaction based on configured thresholds
    /// returns (should_compact, is_sync) - is_sync indicates if sync compaction is needed
    async fn should_compact_chunk(&self, chunk_id: u64) -> Result<(bool, bool), MetaError> {
        let config = &self._config.compact;

        // get chunk statistics
        let (slice_count, _total_size, fragment_ratio) =
            self.get_chunk_compact_stats(chunk_id).await?;

        // check minimum slice count threshold (JuiceFS: 5)
        if slice_count < config.min_slice_count {
            return Ok((false, false));
        }

        // determine if compaction is needed based on slice count
        // 5-99: async compact
        // 100-349: async compact
        // 350+: sync compact
        let (should_compact, is_sync) = if slice_count >= config.sync_threshold {
            (true, true)
        } else if slice_count >= config.async_threshold {
            (true, false)
        } else {
            // slice_count >= 5 but < 100
            (true, false)
        };

        // If we should compact, check fragmentation ratio to avoid unnecessary work
        // only compact if there's actual fragmentation
        if should_compact && fragment_ratio < config.min_fragment_ratio {
            debug!(
                chunk_id = chunk_id,
                slice_count = slice_count,
                fragment_ratio = fragment_ratio,
                "Chunk has enough slices but low fragmentation, skipping compact"
            );
            return Ok((false, false));
        }
        if should_compact {
            debug!(
                chunk_id = chunk_id,
                slice_count = slice_count,
                fragment_ratio = fragment_ratio,
                is_sync = is_sync,
                "Chunk meets compaction thresholds"
            );
        }
        Ok((should_compact, is_sync))
    }

    async fn run_compact_by_threshold(&self) -> Result<usize, MetaError> {
        let _max_concurrent_tasks = self._config.compact.max_concurrent_tasks; // TODO: implement concurrent compaction
        let max_chunks_per_run = self._config.compact.max_chunks_per_run;

        let chunk_ids_result: Vec<(i64,)> = SliceMeta::find()
            .select_only()
            .column(slice_meta::Column::ChunkId)
            .distinct()
            .into_tuple()
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        // Process chunks sequentially for now (concurrency can be added with Arc<Self>)
        let mut compacted_count = 0;

        for (chunk_id_i64,) in chunk_ids_result.into_iter().take(max_chunks_per_run) {
            let chunk_id = chunk_id_i64 as u64;

            // check if this chunk needs compaction
            match self.should_compact_chunk(chunk_id).await {
                Ok((true, is_sync)) => {
                    if is_sync {
                        info!("Sync compacting chunk {}", chunk_id);
                    } else {
                        info!("Async compacting chunk {}", chunk_id);
                    }

                    match self.compact_chunk_with_delay(chunk_id).await {
                        Ok(_) => {
                            compacted_count += 1;
                            info!("Chunk {} compacted successfully", chunk_id);
                        }
                        Err(e) => {
                            warn!("Failed to compact chunk {}: {}", chunk_id, e);
                            // continue with other chunks even if one fails
                        }
                    }
                }
                Ok((false, _)) => {
                    // chunk doesn't need compaction, skip
                }
                Err(e) => {
                    warn!("Error checking chunk {} compaction status: {}", chunk_id, e);
                    // continue with other chunks
                }
            }
        }

        info!(
            "Compaction run completed, compacted {} chunks",
            compacted_count
        );

        Ok(compacted_count)
    }

    async fn compact_chunk_with_delay(&self, chunk_id: u64) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let rows = SliceMeta::find()
            .filter(slice_meta::Column::ChunkId.eq(chunk_id as i64))
            .order_by_asc(slice_meta::Column::Id)
            .all(&txn)
            .await
            .map_err(MetaError::Database)?;

        let slices: Vec<SliceDesc> = rows.into_iter().map(Into::into).collect();

        let merged_slices = self.merge_slices(&slices).await?;
        let replaced_slice_ids = Self::find_replaced_slice_ids(&slices, &merged_slices);

        if replaced_slice_ids.is_empty() {
            // No slices can be merged, commit empty transaction and return
            txn.commit().await.map_err(MetaError::Database)?;
            return Ok(());
        }

        let delayed_data = Self::prepare_delayed_data(&slices, &replaced_slice_ids);

        self.replace_slices(&txn, chunk_id, &merged_slices).await?;
        self.cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await?;
        txn.commit().await.map_err(MetaError::Database)?;

        Ok(())
    }

    fn find_replaced_slice_ids(original: &[SliceDesc], merged: &[SliceDesc]) -> Vec<u64> {
        let merged_ids: std::collections::HashSet<u64> =
            merged.iter().map(|s| s.slice_id).collect();
        let original_ids: std::collections::HashSet<u64> =
            original.iter().map(|s| s.slice_id).collect();

        original_ids.difference(&merged_ids).copied().collect()
    }

    fn prepare_delayed_data(slices: &[SliceDesc], replaced_ids: &[u64]) -> Vec<u8> {
        let replaced_set: std::collections::HashSet<u64> = replaced_ids.iter().copied().collect();
        // Format: slice_id (u64) + offset (u64) + size (u32) = 20 bytes per slice
        let mut delayed = Vec::with_capacity(replaced_ids.len() * 20);

        for slice in slices {
            if replaced_set.contains(&slice.slice_id) {
                delayed.extend_from_slice(&slice.slice_id.to_le_bytes());
                delayed.extend_from_slice(&slice.offset.to_le_bytes());
                let size = slice.length.min(u32::MAX as u64) as u32;
                delayed.extend_from_slice(&size.to_le_bytes());
            }
        }

        delayed
    }
}

#[async_trait]
impl MetaStore for DatabaseMetaStore {
    fn name(&self) -> &'static str {
        "database"
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        if let Ok(Some(file_meta)) = self.get_file_meta(ino).await {
            return Ok(Some(Self::file_meta_to_attr(&file_meta)));
        }

        if let Ok(Some(access_meta)) = self.get_access_meta(ino).await {
            return Ok(Some(Self::access_meta_to_attr(&access_meta)));
        }

        Ok(None)
    }

    /// Batch stat implementation using SQL WHERE IN clause for optimal performance
    #[tracing::instrument(
        level = "trace",
        skip(self, inodes),
        fields(inode_count = inodes.len())
    )]
    async fn batch_stat(&self, inodes: &[i64]) -> Result<Vec<Option<FileAttr>>, MetaError> {
        if inodes.is_empty() {
            return Ok(Vec::new());
        }

        // Use concurrent queries for both tables - simpler and potentially faster
        let file_query = FileMeta::find()
            .filter(file_meta::Column::Inode.is_in(inodes.iter().copied()))
            .all(&self.db);

        let dir_query = AccessMeta::find()
            .filter(access_meta::Column::Inode.is_in(inodes.iter().copied()))
            .all(&self.db);

        let (file_metas, access_metas) =
            tokio::try_join!(file_query, dir_query).map_err(MetaError::Database)?;

        // Build result map
        let mut result_map: HashMap<i64, FileAttr> = HashMap::with_capacity(inodes.len());

        // Process file_meta results
        for file_meta in file_metas {
            result_map.insert(file_meta.inode, Self::file_meta_to_attr(&file_meta));
        }

        // Process access_meta results (directories)
        for access_meta in access_metas {
            result_map.insert(access_meta.inode, Self::access_meta_to_attr(&access_meta));
        }

        // Preserve input order
        Ok(inodes
            .iter()
            .map(|ino| result_map.get(ino).cloned())
            .collect())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(parent))
            .filter(content_meta::Column::EntryName.eq(name))
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entry.map(|e| e.inode))
    }

    #[tracing::instrument(level = "trace", skip(self), fields(path))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_directory(parent, name).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
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

        XattrMeta::delete_many()
            .filter(xattr_meta::Column::Inode.eq(dir_id))
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

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_file_internal(parent, name).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
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
            // Delete the LinkParent entry for this specific (parent, name)
            LinkParentMeta::delete_many()
                .filter(link_parent_meta::Column::Inode.eq(file_id))
                .filter(link_parent_meta::Column::ParentInode.eq(parent))
                .filter(link_parent_meta::Column::EntryName.eq(name))
                .exec(&txn)
                .await
                .map_err(MetaError::Database)?;

            file_meta.nlink = Set(current_nlink - 1);
            file_meta.deleted = Set(false);

            // 2->1 transition: Restore parent field and remove all LinkParent
            if current_nlink == 2 {
                // Find the remaining ContentMeta entry
                let remaining_entry = ContentMeta::find()
                    .filter(content_meta::Column::Inode.eq(file_id))
                    .one(&txn)
                    .await
                    .map_err(MetaError::Database)?
                    .ok_or(MetaError::Internal(format!(
                        "No remaining ContentMeta found for inode {}",
                        file_id
                    )))?;

                // Restore parent field from remaining entry
                file_meta.parent = Set(remaining_entry.parent_inode);

                // Delete all LinkParent entries
                LinkParentMeta::delete_many()
                    .filter(link_parent_meta::Column::Inode.eq(file_id))
                    .exec(&txn)
                    .await
                    .map_err(MetaError::Database)?;
            }
        } else {
            // 1->0 transition: Mark as deleted
            file_meta.deleted = Set(true);
            file_meta.nlink = Set(0);
            file_meta.parent = Set(0);
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino, parent, name))]
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

        if file.symlink_target.is_some() {
            txn.rollback().await.map_err(MetaError::Database)?;
            return Err(MetaError::NotSupported(
                "cannot create hard links to symbolic links".into(),
            ));
        }

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

        let old_nlink = file.nlink;
        let new_nlink = file.nlink.saturating_add(1);

        // Query original entry BEFORE inserting new entry to avoid conflicts
        //
        // Why query ContentMeta instead of using file.parent directly?
        // - file.parent only stores the parent inode, not the entry name
        // - We need both parent_inode AND entry_name to create LinkParent entries
        // - ContentMeta stores the complete directory entry (parent_inode + entry_name)
        //
        // Why query before insert?
        // - After inserting the new entry, there will be 2 ContentMeta rows with the same inode
        // - Using one() on multiple rows may return either entry non-deterministically
        // - We must capture the original entry's name before creating the new link
        let original_entry = if old_nlink == 1 {
            Some(
                ContentMeta::find()
                    .filter(content_meta::Column::Inode.eq(ino))
                    .one(&txn)
                    .await
                    .map_err(MetaError::Database)?
                    .ok_or_else(|| {
                        MetaError::Internal(format!(
                            "ContentMeta entry not found for inode {}",
                            ino
                        ))
                    })?,
            )
        } else {
            None
        };

        let new_entry = content_meta::ActiveModel {
            inode: Set(ino),
            parent_inode: Set(parent),
            entry_name: Set(name.to_string()),
            entry_type: Set(entry_type),
        };
        new_entry.insert(&txn).await.map_err(MetaError::Database)?;

        let mut file_active: file_meta::ActiveModel = file.clone().into();
        file_active.nlink = Set(new_nlink);
        file_active.modify_time = Set(now);
        file_active.create_time = Set(now);
        file_active.deleted = Set(false);

        if old_nlink == 1 {
            let orig = original_entry.unwrap();
            let old_parent = file.parent;
            let old_entry_name = orig.entry_name;

            // Use link_parent instead of parent
            file_active.parent = Set(0);
            let link_parent_old = link_parent_meta::ActiveModel {
                inode: Set(ino),
                parent_inode: Set(old_parent),
                entry_name: Set(old_entry_name),
            };
            link_parent_old
                .insert(&txn)
                .await
                .map_err(MetaError::Database)?;

            // New link
            let link_parent_new = link_parent_meta::ActiveModel {
                inode: Set(ino),
                parent_inode: Set(parent),
                entry_name: Set(name.to_string()),
            };
            link_parent_new
                .insert(&txn)
                .await
                .map_err(MetaError::Database)?;
        } else if old_nlink > 1 {
            let link_parent_new = link_parent_meta::ActiveModel {
                inode: Set(ino),
                parent_inode: Set(parent),
                entry_name: Set(name.to_string()),
            };
            link_parent_new
                .insert(&txn)
                .await
                .map_err(MetaError::Database)?;
        }

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

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name, target))]
    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError> {
        let inode = self.alloc_counter_id(INODE_ID_KEY).await?;
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
            parent: Set(parent),
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let file = FileMeta::find_by_id(ino)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(ino))?;

        file.symlink_target
            .ok_or_else(|| MetaError::NotSupported(format!("inode {ino} is not a symbolic link")))
    }

    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(old_parent, old_name, new_parent, new_name)
    )]
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

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Get metadata to check nlink and type
        // Try FileMeta first (for files), then AccessMeta (for directories)
        let file_meta_opt = FileMeta::find_by_id(target_entry.inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        let access_meta_opt = AccessMeta::find_by_id(target_entry.inode)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Determine nlink based on entry type
        let nlink = if let Some(ref file_meta) = file_meta_opt {
            file_meta.nlink
        } else if let Some(ref access_meta) = access_meta_opt {
            access_meta.nlink
        } else {
            return Err(MetaError::NotFound(target_entry.inode));
        };

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
            entry_name: Set(new_name.clone()),
            entry_type: Set(target_entry.entry_type),
        };

        new_content_meta
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Handle LinkParentMeta updates for hardlinked files
        // Note: Directories are stored in AccessMeta only, not FileMeta
        if nlink > 1 {
            // For hardlinked files (nlink > 1), update LinkParentMeta
            LinkParentMeta::delete_many()
                .filter(link_parent_meta::Column::Inode.eq(target_entry.inode))
                .filter(link_parent_meta::Column::ParentInode.eq(old_parent))
                .filter(link_parent_meta::Column::EntryName.eq(old_name))
                .exec(&txn)
                .await
                .map_err(MetaError::Database)?;

            let new_link_parent = link_parent_meta::ActiveModel {
                inode: Set(target_entry.inode),
                parent_inode: Set(new_parent),
                entry_name: Set(new_name.clone()),
            };
            new_link_parent
                .insert(&txn)
                .await
                .map_err(MetaError::Database)?;
        } else if nlink == 1 && file_meta_opt.is_some() {
            // For regular files with single link, update file_meta.parent directly
            let file_meta = file_meta_opt.unwrap();
            let mut file_active: file_meta::ActiveModel = file_meta.into();
            file_active.parent = Set(new_parent);
            file_active.modify_time = Set(now);
            // Note: create_time should not be updated during rename
            file_active
                .update(&txn)
                .await
                .map_err(MetaError::Database)?;
        }
        // For directories (nlink >= 2, no FileMeta), no additional updates needed
        // The ContentMeta update above is sufficient

        // Update old parent mtime (not ctime, which should only change on metadata changes)
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

    async fn rename_exchange(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        // Find both entries to exchange
        let old_entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(old_parent))
            .filter(content_meta::Column::EntryName.eq(old_name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or_else(|| {
                MetaError::Internal(format!(
                    "Entry '{}' not found in parent {} for exchange",
                    old_name, old_parent
                ))
            })?;

        let new_entry = ContentMeta::find()
            .filter(content_meta::Column::ParentInode.eq(new_parent))
            .filter(content_meta::Column::EntryName.eq(new_name))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or_else(|| {
                MetaError::Internal(format!(
                    "Entry '{}' not found in parent {} for exchange",
                    new_name, new_parent
                ))
            })?;

        let old_ino = old_entry.inode;
        let new_ino = new_entry.inode;

        // Get file metadata for both files
        let old_file_meta = FileMeta::find_by_id(old_ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(old_ino))?;

        let new_file_meta = FileMeta::find_by_id(new_ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(new_ino))?;

        let now = Utc::now().timestamp_nanos_opt().unwrap_or(0);

        // Delete both content_meta entries
        ContentMeta::delete_many()
            .filter(content_meta::Column::ParentInode.eq(old_parent))
            .filter(content_meta::Column::EntryName.eq(old_name))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        ContentMeta::delete_many()
            .filter(content_meta::Column::ParentInode.eq(new_parent))
            .filter(content_meta::Column::EntryName.eq(new_name))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Insert swapped content_meta entries
        let swapped_old_content = content_meta::ActiveModel {
            inode: Set(new_ino),
            parent_inode: Set(old_parent),
            entry_name: Set(old_name.to_string()),
            entry_type: Set(new_entry.entry_type),
        };
        swapped_old_content
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        let swapped_new_content = content_meta::ActiveModel {
            inode: Set(old_ino),
            parent_inode: Set(new_parent),
            entry_name: Set(new_name.to_string()),
            entry_type: Set(old_entry.entry_type),
        };
        swapped_new_content
            .insert(&txn)
            .await
            .map_err(MetaError::Database)?;

        // Handle LinkParentMeta updates for hardlinked files
        // Update old file (now at new location)
        if old_file_meta.nlink > 1 {
            LinkParentMeta::delete_many()
                .filter(link_parent_meta::Column::Inode.eq(old_ino))
                .filter(link_parent_meta::Column::ParentInode.eq(old_parent))
                .filter(link_parent_meta::Column::EntryName.eq(old_name))
                .exec(&txn)
                .await
                .map_err(MetaError::Database)?;

            let new_link_parent = link_parent_meta::ActiveModel {
                inode: Set(old_ino),
                parent_inode: Set(new_parent),
                entry_name: Set(new_name.to_string()),
            };
            new_link_parent
                .insert(&txn)
                .await
                .map_err(MetaError::Database)?;
        } else if old_file_meta.nlink == 1 {
            let mut file_active: file_meta::ActiveModel = old_file_meta.into();
            file_active.parent = Set(new_parent);
            file_active.modify_time = Set(now);
            file_active
                .update(&txn)
                .await
                .map_err(MetaError::Database)?;
        }

        // Update new file (now at old location)
        if new_file_meta.nlink > 1 {
            LinkParentMeta::delete_many()
                .filter(link_parent_meta::Column::Inode.eq(new_ino))
                .filter(link_parent_meta::Column::ParentInode.eq(new_parent))
                .filter(link_parent_meta::Column::EntryName.eq(new_name))
                .exec(&txn)
                .await
                .map_err(MetaError::Database)?;

            let old_link_parent = link_parent_meta::ActiveModel {
                inode: Set(new_ino),
                parent_inode: Set(old_parent),
                entry_name: Set(old_name.to_string()),
            };
            old_link_parent
                .insert(&txn)
                .await
                .map_err(MetaError::Database)?;
        } else if new_file_meta.nlink == 1 {
            let mut file_active: file_meta::ActiveModel = new_file_meta.into();
            file_active.parent = Set(old_parent);
            file_active.modify_time = Set(now);
            file_active
                .update(&txn)
                .await
                .map_err(MetaError::Database)?;
        }

        // Update parent directories' mtime
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
    async fn extend_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let now = Self::now_nanos();
        let result = file_meta::Entity::update_many()
            .col_expr(
                file_meta::Column::Size,
                sea_query::Expr::val(size as i64).into(),
            )
            .col_expr(
                file_meta::Column::ModifyTime,
                sea_query::Expr::val(now).into(),
            )
            .filter(file_meta::Column::Inode.eq(ino))
            .filter(file_meta::Column::Size.lt(size as i64))
            .exec(&self.db)
            .await
            .map_err(MetaError::Database)?;

        if result.rows_affected == 0 {
            let exists = FileMeta::find_by_id(ino)
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?;
            if exists.is_none() {
                return Err(MetaError::NotFound(ino));
            }
        }

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size, chunk_size))]
    async fn truncate(&self, ino: i64, size: u64, chunk_size: u64) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let mut file_meta: file_meta::ActiveModel = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(ino))?
            .into();

        let old_size = match &file_meta.size {
            Set(s) | Unchanged(s) => *s as u64,
            _ => 0,
        };

        file_meta.size = Set(size as i64);
        if old_size != size {
            file_meta.modify_time = Set(Self::now_nanos());
        }

        file_meta.update(&txn).await.map_err(MetaError::Database)?;
        self.prune_slices_for_truncate(&txn, ino, size, old_size, chunk_size)
            .await?;

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, req),
        fields(ino, size = req.size, flags = ?flags)
    )]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn get_names(&self, ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError> {
        if ino == 1 {
            return Ok(vec![(None, "/".to_string())]);
        }

        if AccessMeta::find_by_id(ino)
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?
            .is_some()
        {
            let entry = ContentMeta::find()
                .filter(content_meta::Column::Inode.eq(ino))
                .one(&self.db)
                .await
                .map_err(MetaError::Database)?;

            return Ok(entry
                .map(|e| vec![(Some(e.parent_inode), e.entry_name)])
                .unwrap_or_default());
        }

        let entries = ContentMeta::find()
            .filter(content_meta::Column::Inode.eq(ino))
            .order_by_asc(content_meta::Column::ParentInode)
            .order_by_asc(content_meta::Column::EntryName)
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(entries
            .into_iter()
            .map(|e| (Some(e.parent_inode), e.entry_name))
            .collect())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, flags = ?flags))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn close(&self, ino: i64) -> Result<(), MetaError> {
        if self.stat(ino).await?.is_some() {
            Ok(())
        } else {
            Err(MetaError::NotFound(ino))
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn get_paths(&self, ino: i64) -> Result<Vec<String>, MetaError> {
        if ino == 1 {
            return Ok(vec!["/".to_string()]);
        }

        let names = self.get_names(ino).await?;
        let mut out = Vec::with_capacity(names.len());

        for (parent_opt, name) in names {
            let Some(parent) = parent_opt else {
                continue;
            };

            let mut path_parts = vec![name];
            let mut current_ino = parent;

            while current_ino != 1 {
                let entry = ContentMeta::find()
                    .filter(content_meta::Column::Inode.eq(current_ino))
                    .order_by_asc(content_meta::Column::ParentInode)
                    .order_by_asc(content_meta::Column::EntryName)
                    .one(&self.db)
                    .await
                    .map_err(MetaError::Database)?;

                let Some(entry) = entry else {
                    path_parts.clear();
                    break;
                };

                path_parts.push(entry.entry_name);
                current_ino = entry.parent_inode;
            }

            if path_parts.is_empty() {
                continue;
            }

            path_parts.reverse();
            out.push(format!("/{}", path_parts.join("/")));
        }

        out.sort();
        out.dedup();
        Ok(out)
    }

    fn root_ino(&self) -> i64 {
        1
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn initialize(&self) -> Result<(), MetaError> {
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
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

    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        let deleted_files = FileMeta::find()
            .filter(file_meta::Column::Deleted.eq(true))
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;

        Ok(deleted_files.into_iter().map(|f| f.inode).collect())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let file_meta = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
            .ok_or(MetaError::NotFound(ino))?;

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

        XattrMeta::delete_many()
            .filter(xattr_meta::Column::Inode.eq(ino))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        txn.commit().await.map_err(MetaError::Database)?;

        Ok(())
    }

    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(chunk_id, slice_count = tracing::field::Empty)
    )]
    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let rows = SliceMeta::find()
            .filter(slice_meta::Column::ChunkId.eq(chunk_id as i64))
            .order_by_asc(slice_meta::Column::Id)
            .all(&self.db)
            .instrument(tracing::trace_span!("get_slices.query", chunk_id))
            .await
            .map_err(MetaError::Database)?;

        let slices: Vec<SliceDesc> = rows.into_iter().map(Into::into).collect();
        tracing::Span::current().record("slice_count", slices.len());
        Ok(slices)
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, slice),
        fields(chunk_id, slice_id = slice.slice_id, offset = slice.offset, len = slice.length)
    )]
    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let model = slice_meta::ActiveModel {
            chunk_id: Set(chunk_id as i64),
            slice_id: Set(slice.slice_id as i64),
            offset: Set(slice.offset.as_i64()),
            length: Set(slice.length.as_i64()),
            ..Default::default()
        };
        model.insert(&txn).await.map_err(MetaError::Database)?;
        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, slice),
        fields(ino, chunk_id, slice_id = slice.slice_id, offset = slice.offset, len = slice.length, new_size)
    )]
    async fn write(
        &self,
        ino: i64,
        chunk_id: u64,
        slice: SliceDesc,
        new_size: u64,
    ) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let model = slice_meta::ActiveModel {
            chunk_id: Set(chunk_id as i64),
            slice_id: Set(slice.slice_id as i64),
            offset: Set(slice.offset.as_i64()),
            length: Set(slice.length.as_i64()),
            ..Default::default()
        };

        if let Err(err) = model.insert(&txn).await {
            let _ = txn.rollback().await;
            return Err(MetaError::Database(err));
        }

        let now = Self::now_nanos();

        // First, try to update size if needed
        let result = file_meta::Entity::update_many()
            .col_expr(
                file_meta::Column::Size,
                sea_query::Expr::val(new_size as i64).into(),
            )
            .col_expr(
                file_meta::Column::ModifyTime,
                sea_query::Expr::val(now).into(),
            )
            .filter(file_meta::Column::Inode.eq(ino))
            .filter(file_meta::Column::Size.lt(new_size as i64))
            .exec(&txn)
            .await
            .map_err(MetaError::Database)?;

        if result.rows_affected == 0 {
            let exists = FileMeta::find_by_id(ino)
                .one(&txn)
                .await
                .map_err(MetaError::Database)?;
            if exists.is_none() {
                let _ = txn.rollback().await;
                return Err(MetaError::NotFound(ino));
            }
        }

        // POSIX: clear setuid/setgid bits on write (security: prevent privilege escalation)
        // Need to fetch-modify-update because Permission is a JSON field
        if let Some(file) = FileMeta::find_by_id(ino)
            .one(&txn)
            .await
            .map_err(MetaError::Database)?
        {
            let mut perm = file.permission.clone();
            perm.mode &= !0o6000; // Clear setuid (04000) and setgid (02000) bits

            let mut active: file_meta::ActiveModel = file.into();
            active.permission = Set(perm);
            active.update(&txn).await.map_err(MetaError::Database)?;
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(key))]
    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        match key {
            SLICE_ID_KEY | INODE_ID_KEY => self.alloc_counter_id(key).await,
            other => Err(MetaError::NotSupported(format!(
                "next_id not supported for key {other}"
            ))),
        }
    }

    // ---------- Session lifecycle implementation ----------

    #[tracing::instrument(level = "trace", skip(self), fields(pid = session_info.process_id))]
    async fn start_session(
        &self,
        session_info: SessionInfo,
        token: CancellationToken,
    ) -> Result<Session, MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let session_id = Uuid::now_v7();
        let expire = (Utc::now() + ChronoDuration::minutes(5)).timestamp_millis();
        let payload = serde_json::to_vec(&session_info)
            .map_err(|e| MetaError::Serialization(e.to_string()))?;
        let session = session_meta::ActiveModel {
            session_id: Set(session_id),
            session_info: Set(payload),
            expire: Set(expire),
        };
        if let Err(e) = session.insert(&self.db).await {
            let _ = txn.rollback().await;
            return Err(MetaError::Database(e));
        }
        self.set_sid(session_id)?;
        txn.commit().await.map_err(MetaError::Database)?;

        tokio::spawn(Self::life_cycle(token.clone(), session_id, self.db.clone()));

        Ok(Session {
            session_id,
            expire,
            session_info,
        })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn shutdown_session(&self) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let session_id = self.get_sid()?;
        self.shutdown_session_by_id(*session_id, &txn).await?;
        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn cleanup_sessions(&self) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let sessions = SessionMeta::find()
            .filter(session_meta::Column::Expire.lt(Utc::now().timestamp_millis()))
            .all(&txn)
            .await?;
        for session in sessions {
            let session_id = session.session_id;
            self.shutdown_session_by_id(session_id, &txn).await?;
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(lock_name = ?lock_name))]
    async fn get_global_lock(&self, lock_name: LockName) -> bool {
        self.get_lock_internal(lock_name).await.unwrap_or_default()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    // returns the current lock owner for a range on a file.
    #[tracing::instrument(level = "trace", skip(self, query), fields(inode, owner = query.owner))]
    async fn get_plock(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, MetaError> {
        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not set".to_string()))?;

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
    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(inode, owner, block, lock_type = ?lock_type, pid)
    )]
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

    async fn set_xattr(
        &self,
        inode: i64,
        name: &str,
        value: &[u8],
        flags: u32,
    ) -> Result<(), MetaError> {
        if self.stat(inode).await?.is_none() {
            return Err(MetaError::NotFound(inode));
        }
        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        let existing = XattrMeta::find_by_id((inode, name.to_string()))
            .one(&txn)
            .await
            .map_err(MetaError::Database)?;
        let create_only = flags & (libc::XATTR_CREATE as u32) != 0;
        let replace_only = flags & (libc::XATTR_REPLACE as u32) != 0;

        match existing {
            Some(entry) => {
                if create_only {
                    txn.rollback().await.map_err(MetaError::Database)?;
                    return Err(MetaError::AlreadyExists {
                        parent: inode,
                        name: name.to_string(),
                    });
                }
                let mut active: xattr_meta::ActiveModel = entry.into();
                active.value = Set(value.to_vec());
                active.update(&txn).await.map_err(MetaError::Database)?;
            }
            None => {
                if replace_only {
                    txn.rollback().await.map_err(MetaError::Database)?;
                    return Err(MetaError::NotFound(inode));
                }
                let active = xattr_meta::ActiveModel {
                    inode: Set(inode),
                    name: Set(name.to_string()),
                    value: Set(value.to_vec()),
                };
                active.insert(&txn).await.map_err(MetaError::Database)?;
            }
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn get_xattr(&self, inode: i64, name: &str) -> Result<Option<Vec<u8>>, MetaError> {
        if self.stat(inode).await?.is_none() {
            return Err(MetaError::NotFound(inode));
        }
        let entry = XattrMeta::find_by_id((inode, name.to_string()))
            .one(&self.db)
            .await
            .map_err(MetaError::Database)?;
        Ok(entry.map(|e| e.value))
    }

    async fn list_xattr(&self, inode: i64) -> Result<Vec<String>, MetaError> {
        if self.stat(inode).await?.is_none() {
            return Err(MetaError::NotFound(inode));
        }
        let entries = XattrMeta::find()
            .filter(xattr_meta::Column::Inode.eq(inode))
            .all(&self.db)
            .await
            .map_err(MetaError::Database)?;
        Ok(entries.into_iter().map(|e| e.name).collect())
    }

    async fn remove_xattr(&self, inode: i64, name: &str) -> Result<(), MetaError> {
        if self.stat(inode).await?.is_none() {
            return Err(MetaError::NotFound(inode));
        }
        let result = XattrMeta::delete_by_id((inode, name.to_string()))
            .exec(&self.db)
            .await
            .map_err(MetaError::Database)?;
        if result.rows_affected == 0 {
            return Err(MetaError::NotFound(inode));
        }
        Ok(())
    }
    /// Merge overlapping slices within a chunk, removing only fully covered slices.
    ///
    /// algorithm:
    /// 1. sort slices by slice_id descending (newest first)
    /// 2. for each slice, check if it's fully covered by newer slices
    /// 3. only remove slices that are fully covered (keep partially covered ones)
    ///
    /// note: we cannot split slices because:
    /// - block data is stored with slice-relative offsets
    /// - changing slice.offset without rewriting data breaks read calculations
    /// - (read_offset - slice.offset) would be incorrect
    ///
    /// example:
    /// - slice A: slice_id=1, offset=0, length=100 (old)
    /// - slice B: slice_id=2, offset=50, length=100 (newer)
    /// - A's [50-100] is covered by B, but [0-50] is still needed
    /// - since we can't split A without rewriting, we keep all of A
    /// - result: [Slice(1, 0, 100), Slice(2, 50, 100)]
    ///
    /// - slice A: slice_id=1, offset=0, length=100 (old)
    /// - slice B: slice_id=2, offset=0, length=150 (newer, fully covers A)
    /// - result: [Slice(2, 0, 150)] (A is completely removed)
    async fn merge_slices(&self, slices: &[SliceDesc]) -> Result<Vec<SliceDesc>, MetaError> {
        if slices.is_empty() {
            return Ok(vec![]);
        }

        let chunk_id = slices[0].chunk_id;
        for slice in slices {
            if slice.chunk_id != chunk_id {
                return Err(MetaError::Internal(
                    "All slices must belong to the same chunk".to_string(),
                ));
            }
        }

        let mut covered_ranges: Vec<(u64, u64)> = Vec::new();
        let mut result_slices: Vec<SliceDesc> = Vec::new();

        // Process in reverse: newest first (last in the vector), oldest last
        for slice in slices.iter().rev() {
            let slice_start = slice.offset;
            let slice_end = slice.offset + slice.length;

            // Check if this slice is fully covered by newer slices
            let mut is_fully_covered = false;
            for &(covered_start, covered_end) in &covered_ranges {
                if slice_start >= covered_start && slice_end <= covered_end {
                    is_fully_covered = true;
                    break;
                }
            }

            if !is_fully_covered {
                // Keep the entire slice (can't split without rewriting data)
                result_slices.push(*slice);
            }

            // mark this slice's full range as covered for older slices
            covered_ranges.push((slice_start, slice_end));
        }

        // sort result by offset for consistent ordering
        result_slices.sort_by_key(|s| s.offset);
        Ok(result_slices)
    }

    async fn merge_overlapping_slices(&self, chunk_id: u64) -> Result<(), MetaError> {
        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        // Get all slices for the chunk using the transaction
        // ordered by id (time order: smaller id = older)
        let rows = SliceMeta::find()
            .filter(slice_meta::Column::ChunkId.eq(chunk_id as i64))
            .order_by_asc(slice_meta::Column::Id)
            .all(&txn)
            .await
            .map_err(MetaError::Database)?;

        let slices: Vec<SliceDesc> = rows.into_iter().map(Into::into).collect();

        let merged_slices = self.merge_slices(&slices).await?;

        // Find slices that were removed (not in merged result)
        let merged_ids: std::collections::HashSet<u64> =
            merged_slices.iter().map(|s| s.slice_id).collect();
        let removed_slices: Vec<&SliceDesc> = slices
            .iter()
            .filter(|s| !merged_ids.contains(&s.slice_id))
            .collect();

        // Create delayed data for soft deletion
        let mut delayed_data = Vec::with_capacity(removed_slices.len() * 20);
        for slice in &removed_slices {
            delayed_data.extend_from_slice(&slice.slice_id.to_le_bytes());
            delayed_data.extend_from_slice(&slice.offset.to_le_bytes());
            let size = slice.length.min(u32::MAX as u64) as u32;
            delayed_data.extend_from_slice(&size.to_le_bytes());
        }

        self.replace_slices(&txn, chunk_id, &merged_slices).await?;

        // Create delayed records for removed slices (soft delete)
        if !delayed_data.is_empty() {
            self.cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
                .await?;
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn compact_chunk(
        &self,
        inode: i64,
        _index: u32,
        _origin: &[u8],
        slices: &[SliceDesc],
        _skipped: i32,
        pos: u32,
        chunk_id: u64,
        size: u32,
        delayed: &[u8],
    ) -> Result<(), MetaError> {
        // basic parameter validation
        if chunk_id == 0 {
            warn!(
                inode = inode,
                chunk_id = chunk_id,
                "compact_chunk failed: chunk_id is 0 (invalid)"
            );
            return Err(MetaError::Internal("Invalid chunk_id: 0".to_string()));
        }

        // need at least 2 slices to perform compaction
        if slices.len() < 2 {
            info!("compact_chunk: less than 2 slices, no need to compact");
            return Ok(());
        }

        // size must be greater than 0, otherwise compaction is meaningless
        if size == 0 {
            warn!(
                inode = inode,
                chunk_id = chunk_id,
                "compact_chunk failed: size is 0"
            );
            return Err(MetaError::Internal("Compact size is 0".to_string()));
        }

        // ensure pos + size does not exceed u32::MAX
        if pos as u64 + size as u64 > u32::MAX as u64 {
            warn!(
                inode = inode,
                chunk_id = chunk_id,
                pos = pos,
                size = size,
                "compact_chunk failed: chunk offset + size out of range"
            );
            return Err(MetaError::Internal("Invalid chunk range".to_string()));
        }

        // origin can be empty, but if not empty, it should contain valid slice data
        debug!(
            inode = inode,
            chunk_id = chunk_id,
            "compact_chunk: starting with {} slices",
            slices.len()
        );

        // delayed slice encoding: 8 bytes slice_id + 8 bytes offset + 4 bytes size = 20 bytes per slice
        if !delayed.is_empty() && !delayed.len().is_multiple_of(20) {
            warn!(
                inode = inode,
                delayed_len = delayed.len(),
                "compact_chunk failed: delayed data length is invalid"
            );
            return Err(MetaError::Internal(
                "Invalid delayed data length".to_string(),
            ));
        }

        // Validate: all slices must belong to the specified chunk_id
        for slice in slices {
            if slice.chunk_id != chunk_id {
                warn!(
                    inode = inode,
                    chunk_id = chunk_id,
                    slice_id = slice.slice_id,
                    slice_chunk_id = slice.chunk_id,
                    "compact_chunk failed: slice chunk_id mismatch"
                );
                return Err(MetaError::Internal("Slice chunk_id mismatch".to_string()));
            }
        }

        let valid_slices: Vec<SliceDesc> = slices
            .iter()
            .filter(|s| {
                let slice_end = s.offset + s.length;
                let chunk_end = pos as u64 + size as u64;
                slice_end <= chunk_end
            })
            .cloned()
            .collect();
        if valid_slices.is_empty() {
            let txn = self.db.begin().await.map_err(MetaError::Database)?;
            self.cleanup_delayed_slices(chunk_id, delayed, &txn).await?;
            txn.commit().await.map_err(MetaError::Database)?;
            return Ok(());
        }
        let compacted_slices = self.merge_slices(&valid_slices).await?;

        // check if any slices were removed by compaction
        if compacted_slices.len() >= valid_slices.len() {
            debug!(
                "compact_chunk: no fully-covered slices found, chunk_id={}, slice_count={}",
                chunk_id,
                valid_slices.len()
            );
            // no slices were removed, nothing to do
            return Ok(());
        }

        // Calculate which slices were actually removed
        let compacted_ids: std::collections::HashSet<u64> =
            compacted_slices.iter().map(|s| s.slice_id).collect();
        let removed_ids: std::collections::HashSet<u64> = valid_slices
            .iter()
            .map(|s| s.slice_id)
            .filter(|id| !compacted_ids.contains(id))
            .collect();

        // Validate: delayed data must match removed slices (if provided)
        if !delayed.is_empty() {
            let delayed_ids: std::collections::HashSet<u64> = delayed
                .chunks(20)
                .map(|chunk| {
                    u64::from_le_bytes([
                        chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                        chunk[7],
                    ])
                })
                .collect();

            if delayed_ids != removed_ids {
                warn!(
                    inode = inode,
                    chunk_id = chunk_id,
                    delayed_count = delayed_ids.len(),
                    removed_count = removed_ids.len(),
                    "compact_chunk failed: delayed data does not match removed slices"
                );
                return Err(MetaError::Internal(
                    "Delayed data mismatch with removed slices".to_string(),
                ));
            }
        }

        info!(
            "compact_chunk: removed fully-covered slices, chunk_id={}, before={}, after={}, removed={}",
            chunk_id,
            valid_slices.len(),
            compacted_slices.len(),
            valid_slices.len() - compacted_slices.len()
        );

        let txn = self.db.begin().await.map_err(MetaError::Database)?;
        self.replace_slices(&txn, chunk_id, &compacted_slices)
            .await?;
        self.cleanup_delayed_slices(chunk_id, delayed, &txn).await?;
        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }

    async fn should_compact_chunk(&self, chunk_id: u64) -> Result<(bool, bool), MetaError> {
        DatabaseMetaStore::should_compact_chunk(self, chunk_id).await
    }

    async fn get_chunk_compact_stats(&self, chunk_id: u64) -> Result<(usize, u64, f64), MetaError> {
        DatabaseMetaStore::get_chunk_compact_stats(self, chunk_id).await
    }

    async fn run_compact_by_threshold(&self) -> Result<usize, MetaError> {
        DatabaseMetaStore::run_compact_by_threshold(self).await
    }

    async fn process_delayed_slices(
        &self,
        batch_size: usize,
        max_age_secs: i64,
    ) -> Result<Vec<(u64, u64, u64)>, MetaError> {
        DatabaseMetaStore::process_delayed_slices(self, batch_size, max_age_secs).await
    }

    /// !!! This function only deletes slices specified in old_slices_to_delay and inserts new_slices,
    /// !!! leaving other existing slices intact. It does NOT replace all slices in the chunk.
    /// !!! In real usage, new_slices should have newly allocated IDs to avoid conflicts.
    async fn replace_slices_for_compact(
        &self,
        chunk_id: u64,
        new_slices: &[SliceDesc],
        old_slices_to_delay: &[u8],
    ) -> Result<(), MetaError> {
        if !old_slices_to_delay.is_empty() && !old_slices_to_delay.len().is_multiple_of(20) {
            warn!(
                chunk_id = chunk_id,
                delayed_len = old_slices_to_delay.len(),
                "replace_slices_for_compact: invalid delayed data length"
            );
            return Err(MetaError::Internal(
                "Invalid delayed data length".to_string(),
            ));
        }

        let txn = self.db.begin().await.map_err(MetaError::Database)?;

        let mut slice_ids_to_delete = Vec::new();
        if !old_slices_to_delay.is_empty() {
            for chunk in old_slices_to_delay.chunks(20) {
                let slice_id = u64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
                ]);
                slice_ids_to_delete.push(slice_id as i64);
            }
        }

        if !slice_ids_to_delete.is_empty() {
            SliceMeta::delete_many()
                .filter(slice_meta::Column::ChunkId.eq(chunk_id as i64))
                .filter(slice_meta::Column::SliceId.is_in(slice_ids_to_delete))
                .exec(&txn)
                .await
                .map_err(MetaError::Database)?;
        }

        for slice in new_slices {
            let model = slice_meta::ActiveModel {
                chunk_id: Set(chunk_id as i64),
                slice_id: Set(slice.slice_id as i64),
                offset: Set(slice.offset.as_i64()),
                length: Set(slice.length.as_i64()),
                ..Default::default()
            };
            model.insert(&txn).await.map_err(MetaError::Database)?;
        }

        if !old_slices_to_delay.is_empty() {
            let now = Utc::now().timestamp();

            for chunk in old_slices_to_delay.chunks(20) {
                let slice_id = u64::from_le_bytes([
                    chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6], chunk[7],
                ]);
                let offset = u64::from_le_bytes([
                    chunk[8], chunk[9], chunk[10], chunk[11], chunk[12], chunk[13], chunk[14],
                    chunk[15],
                ]);
                let size = u32::from_le_bytes([chunk[16], chunk[17], chunk[18], chunk[19]]);

                let delayed_model = delayed_slice::ActiveModel {
                    slice_id: Set(slice_id as i64),
                    chunk_id: Set(chunk_id as i64),
                    offset: Set(offset as i64),
                    size: Set(size as i64),
                    created_at: Set(now),
                    reason: Set("compact".to_string()),
                    ..Default::default()
                };

                delayed_model
                    .insert(&txn)
                    .await
                    .map_err(MetaError::Database)?;
            }
        }

        txn.commit().await.map_err(MetaError::Database)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::config::{CacheConfig, ClientOptions, CompactConfig, DatabaseConfig};
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
            compact: CompactConfig::default(),
        }
    }

    fn file_db_config(path: &std::path::Path) -> Config {
        Config {
            database: DatabaseConfig {
                db_config: DatabaseType::Sqlite {
                    url: format!("sqlite://{}?mode=rwc", path.display()),
                },
            },
            cache: CacheConfig::default(),
            client: ClientOptions::default(),
            compact: CompactConfig::default(),
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
            compact: CompactConfig::default(),
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

    #[tokio::test]
    async fn test_next_id_unique_across_store_instances() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let db_path = temp_dir.path().join("counter-unique.db");
        let config = file_db_config(&db_path);

        let store1 = DatabaseMetaStore::from_config(config.clone())
            .await
            .expect("create store1");
        let store2 = DatabaseMetaStore::from_config(config)
            .await
            .expect("create store2");

        let parent = store1.root_ino();
        let ino1 = store1
            .create_file(parent, "counter_a".to_string())
            .await
            .expect("create file on store1");
        let ino2 = store2
            .create_file(parent, "counter_b".to_string())
            .await
            .expect("create file on store2");

        assert_ne!(ino1, ino2, "inode ids must be unique across stores");
        assert!(ino1 > 1);
        assert!(ino2 > 1);
    }

    /// Helper struct to manage multiple test sessions
    struct TestSessionManager {
        stores: Vec<DatabaseMetaStore>,
    }

    use std::sync::LazyLock;
    use tokio::sync::Mutex;

    // Static initialization to ensure execution happens only once
    static SHARED_DB_INIT: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    impl TestSessionManager {
        async fn new(session_count: usize) -> Self {
            // Acquire the lock to ensure serialized initialization
            let _guard = SHARED_DB_INIT.lock().await;

            use std::env;
            // Clean up existing shared test database
            let temp_dir = env::temp_dir();
            let db_path = temp_dir.join("slayerfs_shared_test.db");

            // Clean up only during the first initialization
            static FIRST_INIT: std::sync::Once = std::sync::Once::new();
            FIRST_INIT.call_once(|| {
                let _ = std::fs::remove_file(&db_path);
            });

            let mut stores = Vec::with_capacity(session_count);
            let mut session_ids = Vec::with_capacity(session_count);

            // Create the first store (this will initialize the database)
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

            // Subsequent stores reuse the already-initialized database
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
    #[ignore]
    async fn test_hardlink_parent_field_single_link() {
        // Test that single-link files use parent field for O(1) lookup
        let store = new_test_store().await;
        let parent = store.root_ino();

        // Create a file
        let file_ino = store
            .create_file(parent, "single_link_file.txt".to_string())
            .await
            .unwrap();

        // Verify file has nlink=1
        let file_meta = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(file_meta.nlink, 1);
        assert_eq!(
            file_meta.parent, parent,
            "Parent field should be set for single-link files"
        );

        // Verify no LinkParent entries exist
        let link_parents = LinkParentMeta::find()
            .filter(link_parent_meta::Column::Inode.eq(file_ino))
            .all(&store.db)
            .await
            .unwrap();

        assert!(
            link_parents.is_empty(),
            "No LinkParent entries should exist for single-link files"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_hardlink_transition_to_linkparent() {
        // Test transition from parent field to LinkParent when creating first hardlink
        let store = new_test_store().await;
        let parent = store.root_ino();

        // Create a file
        let file_ino = store
            .create_file(parent, "original_file.txt".to_string())
            .await
            .unwrap();

        // Verify initial state
        let file_before = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(file_before.nlink, 1);
        assert_eq!(file_before.parent, parent);

        // Create a hardlink
        let _attr = store.link(file_ino, parent, "hardlink.txt").await.unwrap();

        // Verify transition to LinkParent mode
        let file_after = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            file_after.nlink, 2,
            "nlink should be 2 after creating hardlink"
        );
        assert_eq!(
            file_after.parent, 0,
            "Parent field should be 0 after transition to LinkParent mode"
        );

        // Verify LinkParent entries for both links
        let link_parents = LinkParentMeta::find()
            .filter(link_parent_meta::Column::Inode.eq(file_ino))
            .all(&store.db)
            .await
            .unwrap();

        assert_eq!(link_parents.len(), 2, "Should have 2 LinkParent entries");

        // Verify both links are tracked
        let names: Vec<String> = link_parents
            .iter()
            .map(|lp| lp.entry_name.clone())
            .collect();
        assert!(names.contains(&"original_file.txt".to_string()));
        assert!(names.contains(&"hardlink.txt".to_string()));
    }

    #[tokio::test]
    #[ignore]
    async fn test_hardlink_no_reversion_to_parent() {
        // When nlink drops from 2 to 1, parent field is restored (optimization)
        let store = new_test_store().await;
        let parent = store.root_ino();

        let file_ino = store
            .create_file(parent, "file1.txt".to_string())
            .await
            .unwrap();

        // Create hardlink: nlink 1 -> 2, parent becomes 0 (LinkParent mode)
        store.link(file_ino, parent, "file2.txt").await.unwrap();

        let file = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(file.nlink, 2);
        assert_eq!(file.parent, 0);

        // Unlink: nlink 2 -> 1, parent restored for O(1) lookup
        store.unlink(parent, "file2.txt").await.unwrap();

        let file = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(file.nlink, 1);
        assert_eq!(file.parent, parent);

        // LinkParent entries should be removed
        let count = LinkParentMeta::find()
            .filter(link_parent_meta::Column::Inode.eq(file_ino))
            .count(&store.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    #[ignore]
    async fn test_hardlink_multiple_links() {
        // Test LinkParent with multiple hardlinks
        let store = new_test_store().await;
        let parent = store.root_ino();

        // Create original file
        let file_ino = store
            .create_file(parent, "link1.txt".to_string())
            .await
            .unwrap();

        // Create multiple hardlinks
        store.link(file_ino, parent, "link2.txt").await.unwrap();
        store.link(file_ino, parent, "link3.txt").await.unwrap();
        store.link(file_ino, parent, "link4.txt").await.unwrap();

        // Verify nlink count
        let file_meta = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(file_meta.nlink, 4, "Should have 4 links");
        assert_eq!(
            file_meta.parent, 0,
            "Parent field should be 0 for multi-link files"
        );

        // Verify all LinkParent entries
        let link_parents = LinkParentMeta::find()
            .filter(link_parent_meta::Column::Inode.eq(file_ino))
            .all(&store.db)
            .await
            .unwrap();

        assert_eq!(link_parents.len(), 4, "Should have 4 LinkParent entries");

        let names: Vec<String> = link_parents
            .iter()
            .map(|lp| lp.entry_name.clone())
            .collect();
        assert!(names.contains(&"link1.txt".to_string()));
        assert!(names.contains(&"link2.txt".to_string()));
        assert!(names.contains(&"link3.txt".to_string()));
        assert!(names.contains(&"link4.txt".to_string()));
    }

    #[tokio::test]
    async fn test_hardlink_last_unlink_cleanup() {
        // Test that last unlink marks file as deleted and cleans up LinkParent entries
        let store = new_test_store().await;
        let parent = store.root_ino();

        // Create file with hardlink
        let file_ino = store
            .create_file(parent, "fileA.txt".to_string())
            .await
            .unwrap();
        store.link(file_ino, parent, "fileB.txt").await.unwrap();

        // Unlink both files
        store.unlink(parent, "fileB.txt").await.unwrap();
        store.unlink(parent, "fileA.txt").await.unwrap();

        // Verify file is marked as deleted
        let file_meta = FileMeta::find_by_id(file_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(file_meta.nlink, 0, "nlink should be 0");
        assert_eq!(file_meta.parent, 0, "parent should be 0");
        assert!(file_meta.deleted, "File should be marked as deleted");

        // Verify all LinkParent entries are cleaned up
        let link_parents = LinkParentMeta::find()
            .filter(link_parent_meta::Column::Inode.eq(file_ino))
            .all(&store.db)
            .await
            .unwrap();

        assert!(
            link_parents.is_empty(),
            "All LinkParent entries should be cleaned up"
        );
    }

    #[tokio::test]
    async fn test_hardlink_dentry_binding_cross_dir_rename_unlink() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let dir_a = store.mkdir(root, "a".to_string()).await.unwrap();
        let dir_b = store.mkdir(root, "b".to_string()).await.unwrap();

        let ino = store.create_file(dir_a, "x".to_string()).await.unwrap();
        store.link(ino, dir_b, "y").await.unwrap();

        let names = store.get_names(ino).await.unwrap();
        assert!(names.contains(&(Some(dir_a), "x".to_string())));
        assert!(names.contains(&(Some(dir_b), "y".to_string())));

        assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

        store
            .rename(dir_b, "y", dir_b, "z".to_string())
            .await
            .unwrap();

        let names = store.get_names(ino).await.unwrap();
        assert!(names.contains(&(Some(dir_a), "x".to_string())));
        assert!(names.contains(&(Some(dir_b), "z".to_string())));
        assert!(!names.contains(&(Some(dir_b), "y".to_string())));

        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), None);
        assert_eq!(store.lookup(dir_b, "z").await.unwrap(), Some(ino));
        assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));

        store.unlink(dir_a, "x").await.unwrap();

        let names = store.get_names(ino).await.unwrap();
        assert_eq!(names, vec![(Some(dir_b), "z".to_string())]);
        assert_eq!(store.lookup(dir_b, "z").await.unwrap(), Some(ino));
    }

    #[tokio::test]
    async fn test_hardlink_dentry_binding_cross_dir_move_rename() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let dir_a = store.mkdir(root, "a".to_string()).await.unwrap();
        let dir_b = store.mkdir(root, "b".to_string()).await.unwrap();
        let dir_c = store.mkdir(root, "c".to_string()).await.unwrap();

        let ino = store.create_file(dir_a, "x".to_string()).await.unwrap();
        store.link(ino, dir_b, "y").await.unwrap();

        assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

        store
            .rename(dir_b, "y", dir_c, "z".to_string())
            .await
            .unwrap();

        let names = store.get_names(ino).await.unwrap();
        assert!(names.contains(&(Some(dir_a), "x".to_string())));
        assert!(names.contains(&(Some(dir_c), "z".to_string())));
        assert!(!names.contains(&(Some(dir_b), "y".to_string())));

        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), None);
        assert_eq!(store.lookup(dir_c, "z").await.unwrap(), Some(ino));
        assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
    }

    #[tokio::test]
    async fn test_symlink_uses_parent_field() {
        // Test that symlinks use parent field (they always have nlink=1)
        let store = new_test_store().await;
        let parent = store.root_ino();

        // Create a symlink
        let (symlink_ino, _attr) = store
            .symlink(parent, "my_symlink", "/target/path")
            .await
            .unwrap();

        // Verify symlink has nlink=1 and uses parent field
        let file_meta = FileMeta::find_by_id(symlink_ino)
            .one(&store.db)
            .await
            .unwrap()
            .unwrap();

        assert_eq!(file_meta.nlink, 1, "Symlink should have nlink=1");
        assert_eq!(file_meta.parent, parent, "Symlink should use parent field");
        assert_eq!(file_meta.symlink_target, Some("/target/path".to_string()));

        // Verify no LinkParent entries
        let link_parents = LinkParentMeta::find()
            .filter(link_parent_meta::Column::Inode.eq(symlink_ino))
            .all(&store.db)
            .await
            .unwrap();

        assert!(
            link_parents.is_empty(),
            "Symlinks should not have LinkParent entries"
        );
    }

    #[tokio::test]
    #[ignore]
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
    #[ignore]
    async fn test_multiple_read_locks() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: i64 = 1001;
        let owner2: i64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(
                parent,
                format!("test_multiple_read_locks_{}.txt", Uuid::now_v7()),
            )
            .await
            .unwrap();

        // First session acquires read lock
        store1
            .set_plock(
                file_ino,
                owner1,
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
                owner2,
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
        assert_eq!(
            lock_info2.pid, 0,
            "pid should be 0 for cross-session queries (security feature)"
        );
    }

    #[tokio::test]
    #[ignore]
    async fn test_write_lock_conflict() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: u64 = 1001;
        let owner2: u64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(
                parent,
                format!("test_write_lock_conflict_{}.txt", Uuid::now_v7()),
            )
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
    #[ignore]
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
            owner,
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
    #[ignore]
    async fn test_non_overlapping_locks() {
        // Create session manager with 2 sessions
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: i64 = 1001;
        let owner2: i64 = 1002;

        // Create a file first using the first session
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(
                parent,
                format!("test_non_overlapping_locks_{}.txt", Uuid::now_v7()),
            )
            .await
            .unwrap();

        // First session acquires lock on range 0-100
        store1
            .set_plock(
                file_ino,
                owner1,
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
                owner2,
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
    #[ignore]
    async fn test_concurrent_read_write_locks() {
        // Test multiple sessions acquiring different types of locks
        let session_mgr = TestSessionManager::new(3).await;

        // Create a file
        let store0 = session_mgr.get_store(0);
        let parent = store0.root_ino();
        let file_ino = store0
            .create_file(parent, format!("concurrent_test_{}.txt", Uuid::now_v7()))
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
                    owner1,
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
                    owner2,
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
                    owner3,
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
    #[ignore]
    async fn test_cross_session_lock_visibility() {
        // Test that locks set by one session are visible to another session
        let session_mgr = TestSessionManager::new(2).await;

        let owner1: u64 = 1001;

        // Create a file
        let store1 = session_mgr.get_store(0);
        let parent = store1.root_ino();
        let file_ino = store1
            .create_file(parent, format!("visibility_test_{}.txt", Uuid::now_v7()))
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

    // ==================== Compact and GC Tests ====================

    /// basic test for compact functionality
    /// verifies that compact_chunk correctly processes overlapping slices
    #[tokio::test]
    async fn test_compact_trigger_and_merge() {
        let store = new_test_store().await;
        let chunk_id = 1u64;

        // slice 2 fully covers slice 1 (completely overlapped)
        // slice 1: [0, 100), slice 2: [0, 150) -> slice 1 is fully covered by slice 2
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id,
                offset: 0,
                length: 150,
            },
        ];

        // insert slices into database
        let txn = store.db.begin().await.unwrap();
        for slice in &slices {
            let model = slice_meta::ActiveModel {
                slice_id: Set(slice.slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(slice.offset as i64),
                length: Set(slice.length as i64),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // verify initial state: 2 slices
        let initial_slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(initial_slices.len(), 2, "should have 2 slices initially");

        // call compact_chunk with correct chunk_id
        let result = store
            .compact_chunk(
                1,        // inode
                0,        // index
                &[],      // origin
                &slices,  // slices to compact
                0,        // skipped
                0,        // pos
                chunk_id, // chunk_id (MUST match the test data chunk_id=1)
                150,      // size
                &[],      // delayed
            )
            .await;

        // compact should succeed
        assert!(result.is_ok(), "compact should succeed: {:?}", result.err());

        // verify final state - slices should be replaced by merged ones
        let final_slices = store.get_slices(chunk_id).await.unwrap();
        info!("after compact: {} slices", final_slices.len());

        // after compact: 2 overlapping slices should become 1 merged slice
        // slice 1 [0, 100) + slice 2 [50, 150) -> merged [0, 150)
        assert_eq!(
            final_slices.len(),
            1,
            "should have 1 merged slice after compact, got {} slices",
            final_slices.len()
        );
        let total_length: u64 = final_slices.iter().map(|s| s.length).sum();
        assert_eq!(
            total_length, 150,
            "merged slice should cover range [0, 150)"
        );
        assert_eq!(final_slices[0].offset, 0);
        assert_eq!(final_slices[0].length, 150);

        info!(
            "compact basic test passed: {} slices -> {} slices, total length: {}",
            initial_slices.len(),
            final_slices.len(),
            total_length
        );
    }

    #[tokio::test]
    async fn test_soft_delete_and_gc() {
        let store = new_test_store().await;
        let chunk_id = 1u64;
        let old_slice_id = 1u64;

        // create initial slice
        let txn = store.db.begin().await.unwrap();
        let old_slice = slice_meta::ActiveModel {
            slice_id: Set(old_slice_id as i64),
            chunk_id: Set(chunk_id as i64),
            offset: Set(0),
            length: Set(100),
            ..Default::default()
        };
        old_slice.insert(&txn).await.unwrap();
        txn.commit().await.unwrap();

        // simulate compact process, mark old slice as delayed
        // delayed data format: 20 bytes per slice (8 bytes slice_id + 8 bytes offset + 4 bytes size)
        let mut delayed_data = Vec::new();
        delayed_data.extend_from_slice(&old_slice_id.to_le_bytes()); // 8 bytes slice_id
        delayed_data.extend_from_slice(&(0u64).to_le_bytes()); // 8 bytes offset
        delayed_data.extend_from_slice(&(100u32).to_le_bytes()); // 4 bytes size
        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // verify: old slice is in delayed_slice table (soft delete)
        let delayed_records: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::SliceId.eq(old_slice_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(
            delayed_records.len(),
            1,
            "old slice should be in delayed_slice table"
        );

        // verify: old slice is still in slice_meta table (not actually deleted)
        let old_slice_in_meta = SliceMeta::find_by_id(old_slice_id as i64)
            .one(&store.db)
            .await
            .unwrap();
        assert!(
            old_slice_in_meta.is_some(),
            "old slice should still be in slice_meta table"
        );

        // call process_delayed_slices (set age to -1 to ensure immediate processing)
        let deleted_slices = store.process_delayed_slices(100, -1).await.unwrap();
        assert_eq!(deleted_slices.len(), 1, "should delete 1 delayed slice");

        // verify: old slice is deleted from slice_meta table (hard delete)
        let old_slice_after_gc = SliceMeta::find_by_id(old_slice_id as i64)
            .one(&store.db)
            .await
            .unwrap();
        assert!(
            old_slice_after_gc.is_none(),
            "old slice should be deleted from slice_meta table after gc"
        );

        // verify: delayed_slice table record is also cleaned up
        let delayed_after_gc: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::SliceId.eq(old_slice_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(
            delayed_after_gc.len(),
            0,
            "delayed record should be cleaned up"
        );
    }

    #[tokio::test]
    async fn test_read_correctness_after_compact() {
        let store = new_test_store().await;
        let chunk_id = 1u64;

        // create 3 overlapping slices to simulate multiple writes to the same area
        // slice 1: offset 0, length 50 (old data)
        // slice 2: offset 0, length 80 (newer data, covers slice 1)
        // slice 3: offset 30, length 70 (latest data, covers part of slice 2)
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id,
                offset: 0,
                length: 80,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id,
                offset: 30,
                length: 70,
            },
        ];

        // insert slices
        let txn = store.db.begin().await.unwrap();
        for slice in &slices {
            let model = slice_meta::ActiveModel {
                slice_id: Set(slice.slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(slice.offset as i64),
                length: Set(slice.length as i64),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // read slices before compact
        let slices_before = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(
            slices_before.len(),
            3,
            "should have 3 slices before compact"
        );

        // trigger compact with correct chunk_id
        let result = store
            .compact_chunk(
                1,        // inode
                0,        // index
                &[],      // origin
                &slices,  // slices to compact
                0,        // skipped
                0,        // pos
                chunk_id, // chunk_id (MUST match the test data chunk_id=1)
                100,      // size
                &[],      // no delayed slices
            )
            .await;
        assert!(result.is_ok(), "compact should succeed");

        // read slices after compact
        let slices_after = store.get_slices(chunk_id).await.unwrap();
        info!("after compact: {} slices", slices_after.len());

        // verify: 3 overlapping slices should be merged into fewer slices
        // slice 1 [0, 50), slice 2 [0, 80), slice 3 [30, 100)
        // after merge: slice 2 [0, 80) + slice 3 [80, 100) = 2 slices
        assert!(
            slices_after.len() <= slices_before.len(),
            "slice count should not increase after compact"
        );

        // verify total coverage: merged slices should cover the same range [0, 100)
        let total_length_before: u64 = slices_before.iter().map(|s| s.length).sum();
        let total_length_after: u64 = slices_after.iter().map(|s| s.length).sum();

        // merged size should be <= original total size (because overlaps are removed)
        assert!(
            total_length_after <= total_length_before,
            "merged size should be <= original total size"
        );

        // actual data range covered should be [0, 100)
        let min_offset_after = slices_after.iter().map(|s| s.offset).min().unwrap_or(0);
        let max_end_after = slices_after
            .iter()
            .map(|s| s.offset + s.length)
            .max()
            .unwrap_or(0);
        assert_eq!(
            min_offset_after, 0,
            "merged slices should start at offset 0"
        );
        assert_eq!(
            max_end_after, 100,
            "merged slices should cover up to offset 100"
        );

        info!(
            "read correctness test passed: {} slices (len={}) -> {} slices (len={})",
            slices_before.len(),
            total_length_before,
            slices_after.len(),
            total_length_after
        );
    }

    #[tokio::test]
    async fn test_compact_threshold_trigger() {
        let store = new_test_store().await;
        let chunk_id = 1u64;

        // initial state: 0 slices
        let (should_compact, _is_sync) = store.should_compact_chunk(chunk_id).await.unwrap();
        assert!(!should_compact, "should not compact with 0 slices");

        // add 3 slices (less than threshold 5)
        let txn = store.db.begin().await.unwrap();
        for i in 1..=3 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i),
                chunk_id: Set(chunk_id as i64),
                offset: Set((i * 100) as i64),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // verify: should_compact_chunk returns false
        let (should_compact, _) = store.should_compact_chunk(chunk_id).await.unwrap();
        assert!(!should_compact, "should not compact with only 3 slices");

        // add 3 more slices (total 6, exceeds threshold 5)
        let txn = store.db.begin().await.unwrap();
        for i in 4..=6 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i),
                chunk_id: Set(chunk_id as i64),
                offset: Set((i * 100) as i64),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // verify: should_compact_chunk returns true (depending on fragmentation ratio)
        let (should_compact, is_sync) = store.should_compact_chunk(chunk_id).await.unwrap();
        info!(
            "threshold test: 6 slices, should_compact={}, is_sync={}",
            should_compact, is_sync
        );

        // verify: can get statistics
        let (slice_count, total_size, fragment_ratio) =
            store.get_chunk_compact_stats(chunk_id).await.unwrap();
        assert_eq!(slice_count, 6, "should have 6 slices");
        assert_eq!(total_size, 600, "total size should be 600");
        info!(
            "compact stats: {} slices, {} bytes, {:.2} fragmentation ratio",
            slice_count, total_size, fragment_ratio
        );

        info!("threshold trigger test passed");
    }

    #[tokio::test]
    async fn test_merge_slices_functionality() {
        let store = new_test_store().await;

        // test case 1: non-overlapping slices should all be kept
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 200,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 2, "non-overlapping slices should all be kept");
        assert_eq!(result[0].offset, 0);
        assert_eq!(result[0].length, 100);
        assert_eq!(result[1].offset, 200);
        assert_eq!(result[1].length, 100);

        // test case 2: partially overlapping slices — both kept intact
        // slice 1 (older): 0-100, slice 2 (newer): 50-150
        // slice 1 is NOT fully covered (only [50,100) is overlapped),
        // so it must be kept with original offset/length to preserve
        // block data addressing correctness.
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(
            result.len(),
            2,
            "partially overlapping slices should both be kept (no splitting)"
        );
        // result is sorted by offset
        assert_eq!(result[0].slice_id, 1, "older slice kept intact");
        assert_eq!(result[0].offset, 0);
        assert_eq!(
            result[0].length, 100,
            "slice 1 must keep original length (can't trim without rewriting block data)"
        );
        assert_eq!(result[1].slice_id, 2, "newer slice fully kept");
        assert_eq!(result[1].offset, 50);
        assert_eq!(result[1].length, 100);

        // test case 3: fully covered slice should be removed
        // slice 1 (older): 0-100, slice 2 (newer): 0-150 (fully covers slice 1)
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 150,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 1, "fully covered slice should be removed");
        assert_eq!(result[0].slice_id, 2, "newer slice should be kept");
        assert_eq!(result[0].offset, 0);
        assert_eq!(result[0].length, 150);

        // test case 4: multiple slices with some fully covered
        // slice 1: 0-50, slice 2: 0-100 (fully covers 1), slice 3: 150-200 (separate)
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 150,
                length: 50,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(
            result.len(),
            2,
            "slice 1 fully covered by 2, slices 2 and 3 kept"
        );
        assert_eq!(result[0].slice_id, 2);
        assert_eq!(result[1].slice_id, 3);

        // test case 5: empty input
        let slices: Vec<SliceDesc> = vec![];
        let result = store.merge_slices(&slices).await.unwrap();
        assert!(result.is_empty(), "empty input should return empty");

        // test case 6: single slice
        let slices = vec![SliceDesc {
            slice_id: 1,
            chunk_id: 1,
            offset: 0,
            length: 100,
        }];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].offset, 0);
        assert_eq!(result[0].length, 100);

        info!("merge slices functionality test passed");
    }

    /// Test merge_slices with non-zero offset slices
    /// This test verifies that partially covered slices with non-zero offset
    /// are handled correctly (should NOT change slice offset to avoid breaking
    /// block data addressing which uses slice-relative offsets)
    #[tokio::test]
    async fn test_merge_slices_nonzero_offset() {
        let store = new_test_store().await;

        // Test case: partially overlapping slices with non-zero offset
        // slice 1 (older): offset=100, length=100 -> [100, 200)
        // slice 2 (newer): offset=150, length=100 -> [150, 250)
        // slice 1's [150-200) is covered by slice 2, but [100-150) is still needed
        // Since we cannot split slice 1 without rewriting block data,
        // the entire slice 1 should be kept with its original offset
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 100,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 150,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();

        // Both slices should be kept (cannot split slice 1)
        assert_eq!(
            result.len(),
            2,
            "partially covered slice with non-zero offset should be kept entirely"
        );

        // Verify slice 1 keeps its original offset (100), not changed to uncovered portion
        let slice_1 = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(
            slice_1.offset, 100,
            "slice offset should NOT be changed - would break block data addressing"
        );
        assert_eq!(slice_1.length, 100, "slice length should remain unchanged");

        // Verify slice 2 is also kept
        let slice_2 = result.iter().find(|s| s.slice_id == 2).unwrap();
        assert_eq!(slice_2.offset, 150);
        assert_eq!(slice_2.length, 100);

        info!("merge slices with non-zero offset test passed");
    }

    /// Test that delayed_slice records preserve offset field correctly
    /// This test verifies the 20-byte format (slice_id + offset + size) is
    /// correctly parsed and the offset field is persisted in the database
    #[tokio::test]
    async fn test_delayed_slice_offset_persistence() {
        let store = new_test_store().await;
        let chunk_id = 1u64;
        let old_slice_id = 42u64;
        let old_slice_offset = 1234u64; // Non-zero offset to verify persistence
        let old_slice_size = 5678u32;

        // Create delayed data in 20-byte format:
        // slice_id (8 bytes) + offset (8 bytes) + size (4 bytes)
        let mut delayed_data = Vec::new();
        delayed_data.extend_from_slice(&old_slice_id.to_le_bytes());
        delayed_data.extend_from_slice(&old_slice_offset.to_le_bytes());
        delayed_data.extend_from_slice(&old_slice_size.to_le_bytes());
        assert_eq!(delayed_data.len(), 20, "delayed data should be 20 bytes");

        // Insert delayed slice record
        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // Verify the delayed slice record was inserted with correct offset
        let delayed_records: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::SliceId.eq(old_slice_id as i64))
            .all(&store.db)
            .await
            .unwrap();

        assert_eq!(
            delayed_records.len(),
            1,
            "should have exactly one delayed slice record"
        );

        let record = &delayed_records[0];
        assert_eq!(
            record.slice_id, old_slice_id as i64,
            "slice_id should match"
        );
        assert_eq!(
            record.offset, old_slice_offset as i64,
            "offset should be correctly persisted (was missing in bug)"
        );
        assert_eq!(record.size, old_slice_size as i64, "size should match");
        assert_eq!(record.chunk_id, chunk_id as i64, "chunk_id should match");

        info!("delayed slice offset persistence test passed");
    }

    // ==================== Comprehensive merge_slices tests ====================

    /// Verify that merge_slices enforces all slices belong to the same chunk.
    #[tokio::test]
    async fn test_merge_slices_different_chunk_ids_rejected() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 2,
                offset: 0,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await;
        assert!(
            result.is_err(),
            "should reject slices from different chunks"
        );
    }

    /// Partial overlap where newer covers the HEAD of older slice.
    /// Must keep both slices intact (cannot trim head without breaking addressing).
    #[tokio::test]
    async fn test_merge_slices_partial_head_overlap() {
        let store = new_test_store().await;
        // Slice A (older): [100, 200), Slice B (newer): [50, 150)
        // B covers A's head [100, 150), A's tail [150, 200) is uncovered
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 100,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 2, "both kept since A is not fully covered");
        let a = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(a.offset, 100, "slice A offset must not change");
        assert_eq!(a.length, 100, "slice A length must not change");
    }

    /// Partial overlap where newer covers the TAIL of older slice.
    #[tokio::test]
    async fn test_merge_slices_partial_tail_overlap() {
        let store = new_test_store().await;
        // Slice A (older): [0, 100), Slice B (newer): [80, 200)
        // B covers A's tail [80, 100), A's head [0, 80) is uncovered
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 80,
                length: 120,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 2);
        let a = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(a.offset, 0);
        assert_eq!(a.length, 100, "slice A kept intact, not trimmed");
    }

    /// Newer slice covers the MIDDLE of older slice (sandwich).
    #[tokio::test]
    async fn test_merge_slices_middle_overlap() {
        let store = new_test_store().await;
        // A (older): [0, 200), B (newer): [50, 150)
        // B covers [50, 150) of A, but A's [0, 50) and [150, 200) are uncovered
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 200,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 2, "A is NOT fully covered, must keep both");
        let a = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(a.offset, 0);
        assert_eq!(a.length, 200);
    }

    /// Non-zero offset scenario (the original bug scenario from the discussion).
    /// Slice A: [100, 200), Slice B: [150, 250) — partial overlap at tail,
    /// A is not fully covered, must keep intact.
    #[tokio::test]
    async fn test_merge_slices_nonzero_offset_tail_overlap() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 100,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 150,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 2);
        let a = result.iter().find(|s| s.slice_id == 1).unwrap();
        assert_eq!(a.offset, 100, "offset must stay at 100");
        assert_eq!(a.length, 100, "length must stay at 100");
    }

    /// Three slices where slice 1 is covered by the UNION of slices 2+3,
    /// but NOT by any single slice. Slice 1 must be kept.
    #[tokio::test]
    async fn test_merge_slices_union_coverage_not_removed() {
        let store = new_test_store().await;
        // A (oldest): [0, 100), B: [0, 60), C: [50, 100)
        // B∪C covers [0, 100) = A's range, but neither B nor C alone fully covers A.
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 60,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 50,
                length: 50,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        // Current implementation checks single-range coverage, not union.
        // A is NOT fully covered by any single covered_range entry.
        assert_eq!(
            result.len(),
            3,
            "none is fully covered by a single newer slice"
        );
    }

    /// Exact same range: newer fully covers older.
    #[tokio::test]
    async fn test_merge_slices_exact_same_range() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 50,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(
            result.len(),
            1,
            "older slice fully covered by exact same range"
        );
        assert_eq!(result[0].slice_id, 2);
    }

    /// Chain coverage: A fully covered by B, B fully covered by C.
    #[tokio::test]
    async fn test_merge_slices_chain_coverage() {
        let store = new_test_store().await;
        // A: [0, 50), B: [0, 80), C: [0, 100)
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 80,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 1, "only the newest slice survives");
        assert_eq!(result[0].slice_id, 3);
    }

    /// Adjacent but non-overlapping slices: both kept.
    #[tokio::test]
    async fn test_merge_slices_adjacent_no_overlap() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 100,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 2, "adjacent slices are non-overlapping");
    }

    /// Many slices, some fully covered and some not.
    #[tokio::test]
    async fn test_merge_slices_complex_scenario() {
        let store = new_test_store().await;
        // Timeline (oldest→newest): 1, 2, 3, 4, 5
        // 1: [0, 50)   — covered by 2 AND 5
        // 2: [0, 100)  — covered by 5
        // 3: [200, 300) — not covered by any single newer slice
        // 4: [250, 280) — newer than 3 but smaller
        // 5: [0, 100)
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 200,
                length: 100,
            },
            SliceDesc {
                slice_id: 4,
                chunk_id: 1,
                offset: 250,
                length: 30,
            },
            SliceDesc {
                slice_id: 5,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        let ids: Vec<u64> = result.iter().map(|s| s.slice_id).collect();
        // 1: fully covered by 2 AND 5 → removed
        assert!(!ids.contains(&1), "slice 1 fully covered by 2");
        // 2: fully covered by 5 → removed
        assert!(!ids.contains(&2), "slice 2 fully covered by 5");
        // 3: NOT fully covered (only [250, 280) covered by 4) → kept
        assert!(ids.contains(&3), "slice 3 not fully covered");
        // 4: newer, not covered → kept
        assert!(ids.contains(&4), "slice 4 newer, kept");
        // 5: newest in [0,100), not covered → kept
        assert!(ids.contains(&5), "slice 5 newest, kept");
    }

    /// Zero-length slice should be handled (edge case).
    #[tokio::test]
    async fn test_merge_slices_zero_length() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 0,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        // A zero-length slice at offset 0: start=0, end=0.
        // Coverage check: 0 >= 0 && 0 <= 100 → fully covered → removed
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].slice_id, 2);
    }

    // ==================== Comprehensive compact_chunk tests ====================

    /// Compact with partially overlapping slices — no slices removed (both kept).
    #[tokio::test]
    async fn test_compact_chunk_partial_overlap_no_change() {
        let store = new_test_store().await;
        let chunk_id = 10u64;
        // Slice 1: [0, 100), Slice 2: [50, 150) — partial overlap, neither fully covered
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id,
                offset: 50,
                length: 100,
            },
        ];
        let txn = store.db.begin().await.unwrap();
        for s in &slices {
            let model = slice_meta::ActiveModel {
                slice_id: Set(s.slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(s.offset as i64),
                length: Set(s.length as i64),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 150, &[])
            .await;
        assert!(result.is_ok());

        let final_slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(
            final_slices.len(),
            2,
            "partial overlap: no slices can be removed, count unchanged"
        );
    }

    /// Compact with chain of fully-covered slices → cascading removal.
    #[tokio::test]
    async fn test_compact_chunk_cascading_full_coverage() {
        let store = new_test_store().await;
        let chunk_id = 20u64;
        // 1: [0,50), 2: [0,80), 3: [0,100) — 1 covered by 2, 2 covered by 3
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id,
                offset: 0,
                length: 80,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id,
                offset: 0,
                length: 100,
            },
        ];
        let txn = store.db.begin().await.unwrap();
        for s in &slices {
            let model = slice_meta::ActiveModel {
                slice_id: Set(s.slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(s.offset as i64),
                length: Set(s.length as i64),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // Prepare delayed data for slices 1 and 2 (format: slice_id + offset + size = 20 bytes each)
        let mut delayed_data = Vec::new();
        // Slice 1: offset=0, length=50
        delayed_data.extend_from_slice(&1u64.to_le_bytes());
        delayed_data.extend_from_slice(&0u64.to_le_bytes());
        delayed_data.extend_from_slice(&50u32.to_le_bytes());
        // Slice 2: offset=0, length=80
        delayed_data.extend_from_slice(&2u64.to_le_bytes());
        delayed_data.extend_from_slice(&0u64.to_le_bytes());
        delayed_data.extend_from_slice(&80u32.to_le_bytes());

        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 100, &delayed_data)
            .await;
        assert!(result.is_ok());

        let final_slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(final_slices.len(), 1, "only slice 3 should remain");
        assert_eq!(final_slices[0].slice_id, 3);

        // Verify delayed slices were created for removed slices
        let delayed: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(
            delayed.len(),
            2,
            "slices 1 and 2 should be in delayed table"
        );
        let delayed_ids: Vec<i64> = delayed.iter().map(|d| d.slice_id).collect();
        assert!(delayed_ids.contains(&1));
        assert!(delayed_ids.contains(&2));
    }

    /// compact_chunk with invalid chunk_id=0 returns error.
    #[tokio::test]
    async fn test_compact_chunk_invalid_chunk_id() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 0,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 0,
                offset: 0,
                length: 100,
            },
        ];
        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, 0, 100, &[])
            .await;
        assert!(result.is_err(), "chunk_id=0 should be rejected");
    }

    /// compact_chunk with fewer than 2 slices is a no-op.
    #[tokio::test]
    async fn test_compact_chunk_single_slice_noop() {
        let store = new_test_store().await;
        let chunk_id = 30u64;
        let slices = vec![SliceDesc {
            slice_id: 1,
            chunk_id,
            offset: 0,
            length: 100,
        }];
        let txn = store.db.begin().await.unwrap();
        let model = slice_meta::ActiveModel {
            slice_id: Set(1),
            chunk_id: Set(chunk_id as i64),
            offset: Set(0),
            length: Set(100),
            ..Default::default()
        };
        model.insert(&txn).await.unwrap();
        txn.commit().await.unwrap();

        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 100, &[])
            .await;
        assert!(result.is_ok());

        let final_slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(final_slices.len(), 1, "single slice is unchanged");
    }

    /// compact_chunk with size=0 returns error.
    #[tokio::test]
    async fn test_compact_chunk_zero_size() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, 1, 0, &[])
            .await;
        assert!(result.is_err(), "size=0 should be rejected");
    }

    /// Slices that exceed chunk bounds are filtered out.
    #[tokio::test]
    async fn test_compact_chunk_out_of_bounds_slices_filtered() {
        let store = new_test_store().await;
        let chunk_id = 40u64;
        // Slice 1: [0, 50) — within bounds
        // Slice 2: [0, 100) — within bounds, covers slice 1
        // Slice 3: [90, 200) — exceeds size=150 → filtered out
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id,
                offset: 90,
                length: 110,
            },
        ];
        let txn = store.db.begin().await.unwrap();
        for s in &slices {
            let model = slice_meta::ActiveModel {
                slice_id: Set(s.slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(s.offset as i64),
                length: Set(s.length as i64),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // size=150, pos=0 → chunk_end=150. Slice 3 end=200 > 150 → filtered.
        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 150, &[])
            .await;
        assert!(result.is_ok());

        let final_slices = store.get_slices(chunk_id).await.unwrap();
        // Valid slices [1, 2]: 1 fully covered by 2 → removed.
        // replace_slices replaces ALL slices for the chunk with merged result.
        assert_eq!(final_slices.len(), 1);
        assert_eq!(final_slices[0].slice_id, 2);
    }

    /// Delayed data with invalid length (not multiple of 20) is rejected.
    #[tokio::test]
    async fn test_compact_chunk_invalid_delayed_data() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
        ];
        // 15 bytes — not a multiple of 20
        let bad_delayed = vec![0u8; 15];
        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, 1, 100, &bad_delayed)
            .await;
        assert!(
            result.is_err(),
            "invalid delayed data length should be rejected"
        );
    }

    // ==================== Comprehensive GC tests ====================

    /// GC respects min_age: delayed slices younger than threshold are NOT deleted.
    #[tokio::test]
    async fn test_gc_respects_min_age() {
        let store = new_test_store().await;
        let chunk_id = 50u64;

        // Insert a delayed slice with current timestamp
        let mut delayed_data = Vec::new();
        delayed_data.extend_from_slice(&(42u64).to_le_bytes());
        delayed_data.extend_from_slice(&(0u64).to_le_bytes());
        delayed_data.extend_from_slice(&(100u32).to_le_bytes());

        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // GC with max_age=3600 — slice was just created, should NOT be processed
        let deleted = store.process_delayed_slices(100, 3600).await.unwrap();
        assert!(deleted.is_empty(), "fresh delayed slice should not be GC'd");

        // Verify it's still in the delayed table
        let remaining: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1, "delayed slice should still exist");
    }

    /// GC processes multiple delayed slices in batch.
    #[tokio::test]
    async fn test_gc_batch_processing() {
        let store = new_test_store().await;
        let chunk_id = 60u64;

        // Create slice_meta entries so they can be deleted
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=5 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // Insert 5 delayed slices
        let mut delayed_data = Vec::new();
        for i in 1u64..=5 {
            delayed_data.extend_from_slice(&i.to_le_bytes());
            delayed_data.extend_from_slice(&(0u64).to_le_bytes());
            delayed_data.extend_from_slice(&(100u32).to_le_bytes());
        }
        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // GC with max_age=-1 (process immediately)
        let deleted = store.process_delayed_slices(100, -1).await.unwrap();
        assert_eq!(deleted.len(), 5, "all 5 delayed slices should be GC'd");

        // Verify all slice_meta entries are gone
        let remaining_slices = store.get_slices(chunk_id).await.unwrap();
        assert!(
            remaining_slices.is_empty(),
            "all slices should be hard-deleted"
        );

        // Verify delayed table is clean
        let remaining_delayed: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert!(
            remaining_delayed.is_empty(),
            "delayed table should be clean"
        );
    }

    /// GC batch_size limits the number of slices processed per run.
    #[tokio::test]
    async fn test_gc_batch_size_limit() {
        let store = new_test_store().await;
        let chunk_id = 70u64;

        // Create 5 slice_meta entries
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=5 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // Insert 5 delayed slices
        let mut delayed_data = Vec::new();
        for i in 1u64..=5 {
            delayed_data.extend_from_slice(&i.to_le_bytes());
            delayed_data.extend_from_slice(&(0u64).to_le_bytes());
            delayed_data.extend_from_slice(&(100u32).to_le_bytes());
        }
        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        // GC with batch_size=2 — only 2 should be processed
        let deleted = store.process_delayed_slices(2, -1).await.unwrap();
        assert_eq!(deleted.len(), 2, "only 2 should be processed (batch limit)");

        // Process the rest
        let deleted2 = store.process_delayed_slices(10, -1).await.unwrap();
        assert_eq!(deleted2.len(), 3, "remaining 3 should be processed");
    }

    /// GC correctly returns (slice_id, offset, size) for block store cleanup.
    #[tokio::test]
    async fn test_gc_returns_correct_block_cleanup_info() {
        let store = new_test_store().await;
        let chunk_id = 80u64;

        let txn = store.db.begin().await.unwrap();
        let model = slice_meta::ActiveModel {
            slice_id: Set(42),
            chunk_id: Set(chunk_id as i64),
            offset: Set(1234),
            length: Set(5678),
            ..Default::default()
        };
        model.insert(&txn).await.unwrap();
        txn.commit().await.unwrap();

        // Insert delayed with specific offset
        let mut delayed_data = Vec::new();
        delayed_data.extend_from_slice(&(42u64).to_le_bytes());
        delayed_data.extend_from_slice(&(1234u64).to_le_bytes());
        delayed_data.extend_from_slice(&(5678u32).to_le_bytes());

        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let deleted = store.process_delayed_slices(100, -1).await.unwrap();
        assert_eq!(deleted.len(), 1);
        let (sid, offset, size) = deleted[0];
        assert_eq!(sid, 42, "slice_id");
        assert_eq!(offset, 1234, "offset for block cleanup");
        assert_eq!(size, 5678, "size for block cleanup");
    }

    /// GC on empty delayed table is a no-op.
    #[tokio::test]
    async fn test_gc_empty_delayed_table() {
        let store = new_test_store().await;
        let deleted = store.process_delayed_slices(100, -1).await.unwrap();
        assert!(deleted.is_empty());
    }

    // ==================== Threshold trigger tests ====================

    /// should_compact_chunk with exact threshold boundary (5 slices, 0 fragmentation).
    #[tokio::test]
    async fn test_threshold_min_slices_no_fragmentation() {
        let store = new_test_store().await;
        let chunk_id = 90u64;

        // Add 5 non-overlapping slices — no fragmentation
        let txn = store.db.begin().await.unwrap();
        for i in 0u64..5 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64 + 1),
                chunk_id: Set(chunk_id as i64),
                offset: Set((i * 100) as i64),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        let (should, _is_sync) = store.should_compact_chunk(chunk_id).await.unwrap();
        assert!(
            !should,
            "5 non-overlapping slices have 0 fragmentation, skip"
        );
    }

    /// should_compact_chunk with high fragmentation triggers compact.
    #[tokio::test]
    async fn test_threshold_high_fragmentation_triggers() {
        let store = new_test_store().await;
        let chunk_id = 95u64;

        // 5 identical slices → 4 are fully covered → high fragmentation
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=5 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        let (should, _is_sync) = store.should_compact_chunk(chunk_id).await.unwrap();
        assert!(
            should,
            "5 identical slices = high fragmentation, should compact"
        );
    }

    /// get_chunk_compact_stats returns correct fragmentation ratio.
    #[tokio::test]
    async fn test_compact_stats_fragmentation_ratio() {
        let store = new_test_store().await;
        let chunk_id = 100u64;

        // 2 identical slices: total=200, merged=100, fragmentation=0.5
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=2 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        let (count, total_size, frag_ratio) =
            store.get_chunk_compact_stats(chunk_id).await.unwrap();
        assert_eq!(count, 2);
        assert_eq!(total_size, 200);
        assert!(
            (frag_ratio - 0.5).abs() < 0.01,
            "fragmentation should be ~0.5, got {}",
            frag_ratio
        );
    }

    // ==================== Integration: compact then GC end-to-end ====================

    /// End-to-end: write overlapping slices → compact → GC → verify final state.
    #[tokio::test]
    async fn test_end_to_end_compact_then_gc() {
        let store = new_test_store().await;
        let chunk_id = 200u64;

        // Write 3 slices where 1 and 2 are fully covered by 3
        let txn = store.db.begin().await.unwrap();
        for (id, offset, length) in [(1, 0, 50), (2, 10, 60), (3, 0, 100)] {
            let model = slice_meta::ActiveModel {
                slice_id: Set(id),
                chunk_id: Set(chunk_id as i64),
                offset: Set(offset),
                length: Set(length),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // Step 1: compact
        let slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 3);

        // Prepare delayed data for slices 1 and 2 (fully covered by slice 3)
        let mut delayed_data = Vec::new();
        // Slice 1: id=1, offset=0, length=50
        delayed_data.extend_from_slice(&1u64.to_le_bytes());
        delayed_data.extend_from_slice(&0u64.to_le_bytes());
        delayed_data.extend_from_slice(&50u32.to_le_bytes());
        // Slice 2: id=2, offset=10, length=60
        delayed_data.extend_from_slice(&2u64.to_le_bytes());
        delayed_data.extend_from_slice(&10u64.to_le_bytes());
        delayed_data.extend_from_slice(&60u32.to_le_bytes());

        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 100, &delayed_data)
            .await;
        assert!(result.is_ok());

        let after_compact = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(after_compact.len(), 1, "only slice 3 should remain");
        assert_eq!(after_compact[0].slice_id, 3);

        // Step 2: verify delayed slices
        let delayed: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(delayed.len(), 2, "slices 1 and 2 should be delayed");

        // Step 3: GC
        let gc_result = store.process_delayed_slices(100, -1).await.unwrap();
        assert_eq!(gc_result.len(), 2, "2 slices should be hard-deleted");

        // Step 4: verify delayed table is clean
        let delayed_after: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert!(delayed_after.is_empty());

        // Final metadata: only slice 3 remains
        let final_slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(final_slices.len(), 1);
        assert_eq!(final_slices[0].slice_id, 3);
    }

    /// End-to-end: repeated writes to same range → compact → verify data range.
    #[tokio::test]
    async fn test_end_to_end_repeated_overwrites() {
        let store = new_test_store().await;
        let chunk_id = 210u64;

        // Simulate 10 writes to the same range [0, 100), each newer fully covers previous
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=10 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        let slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 10);

        // Prepare delayed data for slices 1-9 (all fully covered by slice 10)
        let mut delayed_data = Vec::new();
        for i in 1u64..=9 {
            delayed_data.extend_from_slice(&i.to_le_bytes()); // slice_id
            delayed_data.extend_from_slice(&0u64.to_le_bytes()); // offset
            delayed_data.extend_from_slice(&100u32.to_le_bytes()); // length
        }

        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 100, &delayed_data)
            .await;
        assert!(result.is_ok());

        let after_compact = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(after_compact.len(), 1, "only the latest slice survives");
        assert_eq!(after_compact[0].slice_id, 10, "newest slice kept");

        // GC
        let gc_result = store.process_delayed_slices(100, -1).await.unwrap();
        assert_eq!(gc_result.len(), 9, "9 old slices should be GC'd");
    }

    /// compact_chunk_with_delay internal method test.
    #[tokio::test]
    async fn test_compact_chunk_with_delay() {
        let store = new_test_store().await;
        let chunk_id = 220u64;

        // 1: [0, 50), 2: [0, 100) — 1 is fully covered by 2
        let txn = store.db.begin().await.unwrap();
        for (id, len) in [(1i64, 50i64), (2, 100)] {
            let model = slice_meta::ActiveModel {
                slice_id: Set(id),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(len),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        store.compact_chunk_with_delay(chunk_id).await.unwrap();

        let slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].slice_id, 2);

        let delayed: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(delayed.len(), 1);
        assert_eq!(delayed[0].slice_id, 1);
    }

    /// run_compact_by_threshold processes multiple chunks.
    #[tokio::test]
    async fn test_run_compact_by_threshold_multi_chunk() {
        let store = new_test_store().await;

        // Chunk A: 5 identical slices → should compact
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=5 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(300),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        // Chunk B: 2 non-overlapping slices → should NOT compact (< 5)
        for i in 6u64..=7 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(301),
                offset: Set(((i - 6) * 200) as i64),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        let compacted = store.run_compact_by_threshold().await.unwrap();
        assert_eq!(compacted, 1, "only chunk A should be compacted");

        // Chunk A: 5→1
        let slices_a = store.get_slices(300).await.unwrap();
        assert_eq!(slices_a.len(), 1);
        // Chunk B: unchanged
        let slices_b = store.get_slices(301).await.unwrap();
        assert_eq!(slices_b.len(), 2);
    }

    // ==================== Edge case / robustness tests ====================

    /// merge_slices preserves order when multiple non-overlapping slices exist.
    #[tokio::test]
    async fn test_merge_slices_ordering() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 200,
                length: 50,
            },
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 100,
                length: 50,
            },
        ];
        let result = store.merge_slices(&slices).await.unwrap();
        assert_eq!(result.len(), 3);
        // Should be sorted by offset
        assert_eq!(result[0].offset, 0);
        assert_eq!(result[1].offset, 100);
        assert_eq!(result[2].offset, 200);
    }

    /// cleanup_delayed_slices rejects non-multiple-of-20 data.
    #[tokio::test]
    async fn test_cleanup_delayed_slices_invalid_length() {
        let store = new_test_store().await;
        let txn = store.db.begin().await.unwrap();
        let result = store.cleanup_delayed_slices(1, &[1, 2, 3], &txn).await;
        assert!(result.is_err());
    }

    /// cleanup_delayed_slices with empty data is a no-op.
    #[tokio::test]
    async fn test_cleanup_delayed_slices_empty() {
        let store = new_test_store().await;
        let txn = store.db.begin().await.unwrap();
        let result = store.cleanup_delayed_slices(1, &[], &txn).await;
        assert!(result.is_ok());
        txn.commit().await.unwrap();
    }

    /// Compact on a chunk where all valid slices are filtered produces cleanup.
    #[tokio::test]
    async fn test_compact_chunk_all_filtered() {
        let store = new_test_store().await;
        let chunk_id = 400u64;
        // All slices exceed chunk bounds
        let slices = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id,
                offset: 0,
                length: 200,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id,
                offset: 0,
                length: 300,
            },
        ];
        let txn = store.db.begin().await.unwrap();
        for s in &slices {
            let model = slice_meta::ActiveModel {
                slice_id: Set(s.slice_id as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(s.offset as i64),
                length: Set(s.length as i64),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // size=100 → both slices have end > 100, all filtered out
        let result = store
            .compact_chunk(1, 0, &[], &slices, 0, 0, chunk_id, 100, &[])
            .await;
        assert!(result.is_ok());
    }

    /// prepare_delayed_data encoding is consistent with cleanup_delayed_slices decoding.
    #[tokio::test]
    async fn test_delayed_data_encode_decode_roundtrip() {
        let store = new_test_store().await;
        let slices = vec![
            SliceDesc {
                slice_id: 42,
                chunk_id: 1,
                offset: 1234,
                length: 5678,
            },
            SliceDesc {
                slice_id: 99,
                chunk_id: 1,
                offset: 0,
                length: u32::MAX as u64,
            },
        ];
        let replaced_ids = vec![42u64, 99];
        let delayed_data = DatabaseMetaStore::prepare_delayed_data(&slices, &replaced_ids);

        assert_eq!(delayed_data.len(), 40, "2 slices x 20 bytes");

        // Decode first entry
        let sid1 = u64::from_le_bytes(delayed_data[0..8].try_into().unwrap());
        let off1 = u64::from_le_bytes(delayed_data[8..16].try_into().unwrap());
        let sz1 = u32::from_le_bytes(delayed_data[16..20].try_into().unwrap());
        assert_eq!(sid1, 42);
        assert_eq!(off1, 1234);
        assert_eq!(sz1, 5678);

        // Decode second entry
        let sid2 = u64::from_le_bytes(delayed_data[20..28].try_into().unwrap());
        let off2 = u64::from_le_bytes(delayed_data[28..36].try_into().unwrap());
        let sz2 = u32::from_le_bytes(delayed_data[36..40].try_into().unwrap());
        assert_eq!(sid2, 99);
        assert_eq!(off2, 0);
        assert_eq!(sz2, u32::MAX);

        // Feed to cleanup_delayed_slices → should insert correctly
        let chunk_id = 1u64;
        let txn = store.db.begin().await.unwrap();
        store
            .cleanup_delayed_slices(chunk_id, &delayed_data, &txn)
            .await
            .unwrap();
        txn.commit().await.unwrap();

        let records: Vec<delayed_slice::Model> = DelayedSlice::find()
            .order_by_asc(delayed_slice::Column::SliceId)
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].slice_id, 42);
        assert_eq!(records[0].offset, 1234);
        assert_eq!(records[0].size, 5678);
        assert_eq!(records[1].slice_id, 99);
        assert_eq!(records[1].offset, 0);
        assert_eq!(records[1].size, u32::MAX as i64);
    }

    /// find_replaced_slice_ids correctly identifies removed slices.
    #[tokio::test]
    async fn test_find_replaced_slice_ids() {
        let original = vec![
            SliceDesc {
                slice_id: 1,
                chunk_id: 1,
                offset: 0,
                length: 50,
            },
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 200,
                length: 50,
            },
        ];
        let merged = vec![
            SliceDesc {
                slice_id: 2,
                chunk_id: 1,
                offset: 0,
                length: 100,
            },
            SliceDesc {
                slice_id: 3,
                chunk_id: 1,
                offset: 200,
                length: 50,
            },
        ];
        let replaced = DatabaseMetaStore::find_replaced_slice_ids(&original, &merged);
        assert_eq!(replaced.len(), 1);
        assert!(replaced.contains(&1));
    }

    /// replace_slices_for_compact atomically replaces slices and creates delayed records.
    #[tokio::test]
    async fn test_replace_slices_for_compact() {
        let store = new_test_store().await;
        let chunk_id = 500u64;

        // Insert initial slices
        let txn = store.db.begin().await.unwrap();
        for i in 1u64..=3 {
            let model = slice_meta::ActiveModel {
                slice_id: Set(i as i64),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(100),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        // Replace: new slice 4 replaces slices 1 and 2, slice 3 remains unchanged
        // Note: replace_slices_for_compact only deletes old_slices_to_delay,
        // existing slices not in delete list but with same ID as new_slices would cause conflict.
        // In real usage, new_slices always have newly allocated IDs.
        let new_slices = vec![SliceDesc {
            slice_id: 4, // New slice ID
            chunk_id,
            offset: 0,
            length: 100,
        }];
        let mut delayed_data = Vec::new();
        for i in [1u64, 2] {
            delayed_data.extend_from_slice(&i.to_le_bytes());
            delayed_data.extend_from_slice(&(0u64).to_le_bytes());
            delayed_data.extend_from_slice(&(100u32).to_le_bytes());
        }

        store
            .replace_slices_for_compact(chunk_id, &new_slices, &delayed_data)
            .await
            .unwrap();

        // Verify: slices 3 and 4 in slice_meta (3 was kept, 4 was added)
        let slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 2);
        let slice_ids: Vec<u64> = slices.iter().map(|s| s.slice_id).collect();
        assert!(slice_ids.contains(&3));
        assert!(slice_ids.contains(&4));

        // Verify: 2 delayed records
        let delayed: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(delayed.len(), 2);
    }

    /// merge_overlapping_slices correctly applies merge to DB and creates delayed records.
    #[tokio::test]
    async fn test_merge_overlapping_slices_in_db() {
        let store = new_test_store().await;
        let chunk_id = 600u64;

        // 1: [0,50), 2: [0,100) — 1 is fully covered
        let txn = store.db.begin().await.unwrap();
        for (id, len) in [(1i64, 50i64), (2, 100)] {
            let model = slice_meta::ActiveModel {
                slice_id: Set(id),
                chunk_id: Set(chunk_id as i64),
                offset: Set(0),
                length: Set(len),
                ..Default::default()
            };
            model.insert(&txn).await.unwrap();
        }
        txn.commit().await.unwrap();

        store.merge_overlapping_slices(chunk_id).await.unwrap();

        // Verify: only slice 2 remains in slice_meta
        let slices = store.get_slices(chunk_id).await.unwrap();
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].slice_id, 2);

        // Verify: slice 1 is in delayed table (soft delete)
        let delayed: Vec<delayed_slice::Model> = DelayedSlice::find()
            .filter(delayed_slice::Column::ChunkId.eq(chunk_id as i64))
            .all(&store.db)
            .await
            .unwrap();
        assert_eq!(delayed.len(), 1, "removed slice should be in delayed table");
        assert_eq!(delayed[0].slice_id, 1);
        assert_eq!(delayed[0].offset, 0);
        assert_eq!(delayed[0].size, 50);
    }
}
