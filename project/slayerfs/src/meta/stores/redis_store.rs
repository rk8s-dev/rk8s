//! Redis-based metadata store implementation.
//!
//! This store focuses on the core interfaces needed by the VFS layer so that
//! the filesystem can persist metadata in Redis. It purposely keeps the key
//! layout simple (one key per inode plus a hash per directory) and uses JSON
//! serialization for file attributes. Advanced features (sessions, quota, etc.)
//! can be layered on later by extending the schema.

use crate::chuck::{SliceDesc, chunk::DEFAULT_CHUNK_SIZE};
use crate::meta::client::session::{Session, SessionInfo};
use crate::meta::config::{Config, DatabaseType};
use crate::meta::file_lock::{
    FileLockInfo, FileLockQuery, FileLockRange, FileLockType, PlockRecord,
};
use crate::meta::store::{
    DirEntry, FileAttr, FileType, LockName, MetaError, MetaStore, SetAttrFlags, SetAttrRequest,
};
use crate::meta::{INODE_ID_KEY, SLICE_ID_KEY};
use async_trait::async_trait;
use chrono::Utc;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::error;
use uuid::Uuid;

const ROOT_INODE: i64 = 1;
const COUNTER_INODE_KEY: &str = "nextinode";
const COUNTER_SLICE_KEY: &str = "nextchunk";
const NODE_KEY_PREFIX: &str = "i";
const DIR_KEY_PREFIX: &str = "d";
const CHUNK_KEY_PREFIX: &str = "c";
const DELETED_SET_KEY: &str = "delslices";
const ALL_SESSIONS_KEY: &str = "allsessions";
const SESSION_INFOS_KEY: &str = "sessioninfos";
const PLOCK_PREFIX: &str = "plock";
const LOCKS_KEY: &str = "locks";
const CHUNK_ID_BASE: u64 = 1_000_000_000u64;

/// Minimal Redis-backed meta store.
pub struct RedisMetaStore {
    conn: ConnectionManager,
    _config: Config,
    sid: std::sync::OnceLock<Uuid>,
}

impl RedisMetaStore {
    /// Create or open the store from a backend path. The path is expected to
    /// contain a `slayerfs.yml` that specifies the Redis URL.
    #[allow(dead_code)]
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;
        Self::from_config(config).await
    }

    /// Build a store from the given configuration.
    pub async fn from_config(config: Config) -> Result<Self, MetaError> {
        let conn = Self::create_connection(&config).await?;
        let store = Self {
            conn,
            _config: config,
            sid: std::sync::OnceLock::new(),
        };
        store.init_root_directory().await?;
        Ok(store)
    }

    async fn create_connection(config: &Config) -> Result<ConnectionManager, MetaError> {
        match &config.database.db_config {
            DatabaseType::Redis { url } => {
                let client = redis::Client::open(url.as_str()).map_err(|e| {
                    MetaError::Config(format!("Failed to parse Redis URL {url}: {e}"))
                })?;
                ConnectionManager::new(client).await.map_err(|e| {
                    MetaError::Config(format!("Failed to connect to Redis backend: {e}"))
                })
            }
            _ => Err(MetaError::Config(
                "RedisMetaStore requires database.type = redis".to_string(),
            )),
        }
    }
    fn node_key(&self, ino: i64) -> String {
        format!("{NODE_KEY_PREFIX}{ino}")
    }

    fn dir_key(&self, ino: i64) -> String {
        format!("{DIR_KEY_PREFIX}{ino}")
    }

    fn chunk_key(&self, chunk_id: u64) -> String {
        let inode = chunk_id / CHUNK_ID_BASE;
        let chunk_index = chunk_id % CHUNK_ID_BASE;
        format!("{CHUNK_KEY_PREFIX}{inode}_{chunk_index}")
    }

    fn chunk_id(&self, ino: i64, chunk_index: u64) -> u64 {
        let ino_u64 = u64::try_from(ino).expect("inode must be non-negative");
        ino_u64
            .checked_mul(CHUNK_ID_BASE)
            .and_then(|v| v.checked_add(chunk_index))
            .unwrap_or_else(|| {
                panic!(
                    "chunk_id overflow for inode {} chunk_index {}",
                    ino, chunk_index
                )
            })
    }

    fn deleted_set_key(&self) -> &'static str {
        DELETED_SET_KEY
    }

    fn counter_key(key: &str) -> Result<&'static str, MetaError> {
        let suffix = match key {
            INODE_ID_KEY => COUNTER_INODE_KEY,
            SLICE_ID_KEY => COUNTER_SLICE_KEY,
            other => {
                return Err(MetaError::NotSupported(format!(
                    "counter {other} not supported by RedisMetaStore"
                )));
            }
        };
        Ok(suffix)
    }

    async fn init_root_directory(&self) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let root_key = self.node_key(ROOT_INODE);
        let exists: bool = conn.exists(root_key).await.map_err(redis_err)?;
        if exists {
            return Ok(());
        }

        let now = current_time();
        let attr = StoredAttr {
            size: 0,
            mode: 0o040755,
            uid: 0,
            gid: 0,
            atime: now,
            mtime: now,
            ctime: now,
            nlink: 2,
        };
        let root = StoredNode {
            ino: ROOT_INODE,
            parent: ROOT_INODE,
            name: "/".to_string(),
            kind: NodeKind::Dir,
            attr,
            deleted: false,
        };

        let data = serde_json::to_vec(&root).map_err(|e| MetaError::Internal(e.to_string()))?;
        let _: () = conn
            .set(self.node_key(ROOT_INODE), data)
            .await
            .map_err(redis_err)?;
        // Ensure the root directory hash exists for emptiness checks.
        let _: () = redis::cmd("HSET")
            .arg(self.dir_key(ROOT_INODE))
            .arg("__root__")
            .arg(ROOT_INODE)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
        let _: () = redis::cmd("HDEL")
            .arg(self.dir_key(ROOT_INODE))
            .arg("__root__")
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
        // Initialize counters so new inodes/slices start after the root.
        let inode_counter = Self::counter_key(INODE_ID_KEY)?;
        let _: () = redis::cmd("SETNX")
            .arg(inode_counter)
            .arg(ROOT_INODE + 1)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
        let slice_counter = Self::counter_key(SLICE_ID_KEY)?;
        let _: () = redis::cmd("SETNX")
            .arg(slice_counter)
            .arg(1)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
        Ok(())
    }

    async fn get_node(&self, ino: i64) -> Result<Option<StoredNode>, MetaError> {
        let mut conn = self.conn.clone();
        let data: Option<Vec<u8>> = conn.get(self.node_key(ino)).await.map_err(redis_err)?;
        if let Some(bytes) = data {
            let node =
                serde_json::from_slice(&bytes).map_err(|e| MetaError::Internal(e.to_string()))?;
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    async fn save_node(&self, node: &StoredNode) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let data = serde_json::to_vec(node).map_err(|e| MetaError::Internal(e.to_string()))?;
        let _: () = conn
            .set(self.node_key(node.ino), data)
            .await
            .map_err(redis_err)?;
        Ok(())
    }

    async fn delete_node(&self, ino: i64) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        conn.del(self.node_key(ino)).await.map_err(redis_err)
    }

    async fn add_dir_entry(&self, parent: i64, name: &str, child: i64) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        conn.hset(self.dir_key(parent), name, child)
            .await
            .map_err(redis_err)
    }

    async fn remove_dir_entry(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        conn.hdel(self.dir_key(parent), name)
            .await
            .map_err(redis_err)
    }

    async fn bump_dir_times(&self, ino: i64, now: i64) -> Result<(), MetaError> {
        if let Some(mut node) = self.get_node(ino).await?
            && node.kind == NodeKind::Dir
        {
            node.attr.mtime = now;
            node.attr.ctime = now;
            self.save_node(&node).await?;
        }
        Ok(())
    }

    async fn directory_child(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        let mut conn = self.conn.clone();
        let value: Option<i64> = conn
            .hget(self.dir_key(parent), name)
            .await
            .map_err(redis_err)?;
        Ok(value)
    }

    async fn directory_len(&self, ino: i64) -> Result<i64, MetaError> {
        let mut conn = self.conn.clone();
        conn.hlen(self.dir_key(ino)).await.map_err(redis_err)
    }

    async fn ensure_parent_dir(&self, parent: i64) -> Result<StoredNode, MetaError> {
        let parent_node = self
            .get_node(parent)
            .await?
            .ok_or(MetaError::ParentNotFound(parent))?;
        if parent_node.kind != NodeKind::Dir {
            return Err(MetaError::NotDirectory(parent));
        }
        Ok(parent_node)
    }

    async fn create_entry(
        &self,
        parent: i64,
        name: String,
        kind: FileType,
    ) -> Result<i64, MetaError> {
        self.ensure_parent_dir(parent).await?;
        if self.directory_child(parent, &name).await?.is_some() {
            return Err(MetaError::AlreadyExists { parent, name });
        }
        let ino = self.alloc_id(INODE_ID_KEY).await?;
        let mut node = StoredNode::new(ino, parent, name.clone(), kind);

        // Inherit gid and setgid bit from parent if parent has setgid bit set
        if let Some(parent_node) = self.get_node(parent).await? {
            let parent_has_setgid = (parent_node.attr.mode & 0o2000) != 0;
            if parent_has_setgid {
                node.attr.gid = parent_node.attr.gid;
                // Directories inherit setgid bit from parent
                if matches!(kind, FileType::Dir) {
                    node.attr.mode |= 0o2000;
                }
            }
        }

        self.save_node(&node).await?;
        self.add_dir_entry(parent, &name, ino).await?;
        if matches!(kind, FileType::Dir) {
            self.update_nlink(parent, 1).await?;
        }
        let now = current_time();
        self.bump_dir_times(parent, now).await?;
        Ok(ino)
    }

    async fn update_nlink(&self, ino: i64, delta: i32) -> Result<(), MetaError> {
        if let Some(mut node) = self.get_node(ino).await? {
            let value = node.attr.nlink as i64 + delta as i64;
            node.attr.nlink = value.max(0) as u32;
            node.attr.ctime = current_time();
            self.save_node(&node).await?;
        }
        Ok(())
    }

    async fn alloc_id(&self, key: &str) -> Result<i64, MetaError> {
        let mut conn = self.conn.clone();
        let redis_key = Self::counter_key(key)?;
        conn.incr(redis_key, 1).await.map_err(redis_err)
    }

    async fn mark_deleted(&self, ino: i64, node: &mut StoredNode) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        node.deleted = true;
        node.attr.nlink = 0;
        node.attr.ctime = current_time();
        self.save_node(node).await?;
        let field = ino.to_string();
        conn.hset(self.deleted_set_key(), field, 1)
            .await
            .map_err(redis_err)
    }

    fn plock_key(&self, inode: i64) -> String {
        format!("{}:{}", PLOCK_PREFIX, inode)
    }

    fn plock_field(&self, sid: &Uuid, owner: i64) -> String {
        format!("{}:{}", sid, owner)
    }

    async fn try_set_plock(
        &self,
        inode: i64,
        owner: i64,
        new_lock: PlockRecord,
        lock_type: FileLockType,
        range: FileLockRange,
    ) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let plock_key = self.plock_key(inode);
        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not set".to_string()))?;
        let field = self.plock_field(sid, owner);

        // Check if file exists
        if self.get_node(inode).await?.is_none() {
            return Err(MetaError::NotFound(inode));
        }

        match lock_type {
            FileLockType::UnLock => {
                // Handle unlock
                let current_json: Option<String> =
                    conn.hget(&plock_key, &field).await.map_err(redis_err)?;

                if let Some(json) = current_json {
                    let records: Vec<PlockRecord> = serde_json::from_str(&json).unwrap_or_default();

                    if records.is_empty() {
                        // Remove the field if no records
                        let _: () = conn.hdel(&plock_key, &field).await.map_err(redis_err)?;
                        return Ok(());
                    }

                    let new_records = PlockRecord::update_locks(records, new_lock);

                    if new_records.is_empty() {
                        // Remove the field if no records after update
                        let _: () = conn.hdel(&plock_key, &field).await.map_err(redis_err)?;
                    } else {
                        let new_json = serde_json::to_string(&new_records).map_err(|e| {
                            MetaError::Internal(format!("Serialization error: {e}"))
                        })?;
                        let _: () = conn
                            .hset(&plock_key, &field, new_json)
                            .await
                            .map_err(redis_err)?;
                    }
                }
                Ok(())
            }
            _ => {
                // Handle lock request (ReadLock or WriteLock)
                let current_json: Option<String> =
                    conn.hget(&plock_key, &field).await.map_err(redis_err)?;

                // Get current locks for this owner/session
                let current_records = if let Some(json) = current_json {
                    serde_json::from_str(&json).unwrap_or_default()
                } else {
                    Vec::new()
                };

                // Check for conflicts with other locks
                let all_fields: Vec<String> = conn.hkeys(&plock_key).await.map_err(redis_err)?;
                let mut conflict_found = false;

                for other_field in all_fields {
                    if other_field == field {
                        continue;
                    }

                    let other_records_json: String = conn
                        .hget(&plock_key, &other_field)
                        .await
                        .map_err(redis_err)?;
                    let other_records: Vec<PlockRecord> =
                        serde_json::from_str(&other_records_json).unwrap_or_default();

                    conflict_found =
                        PlockRecord::check_conflict(&lock_type, &range, &other_records);
                    if conflict_found {
                        break;
                    }
                }

                if conflict_found {
                    return Err(MetaError::LockConflict {
                        inode,
                        owner,
                        range,
                    });
                }

                // Update locks
                let new_records = PlockRecord::update_locks(current_records, new_lock);
                let new_json = serde_json::to_string(&new_records)
                    .map_err(|e| MetaError::Internal(format!("Serialization error: {e}")))?;

                let _: () = conn
                    .hset(&plock_key, &field, new_json)
                    .await
                    .map_err(redis_err)?;
                Ok(())
            }
        }
    }

    async fn rewrite_slices(&self, chunk_id: u64, slices: &[SliceDesc]) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let key = self.chunk_key(chunk_id);
        let mut pipe = redis::pipe();
        pipe.cmd("DEL").arg(&key);
        for slice in slices {
            let data = serde_json::to_vec(slice).map_err(|e| MetaError::Internal(e.to_string()))?;
            pipe.cmd("RPUSH").arg(&key).arg(data);
        }
        pipe.query_async::<()>(&mut conn).await.map_err(redis_err)?;
        Ok(())
    }

    async fn prune_slices_for_truncate(
        &self,
        ino: i64,
        new_size: u64,
        old_size: u64,
    ) -> Result<(), MetaError> {
        if new_size >= old_size {
            return Ok(());
        }

        let chunk_size = DEFAULT_CHUNK_SIZE;
        let cutoff_chunk = new_size / chunk_size;
        let cutoff_offset = (new_size % chunk_size) as u32;
        let old_chunk_count = old_size.div_ceil(chunk_size);

        // Trim the partially truncated chunk, if any.
        if cutoff_offset > 0 {
            let chunk_id = self.chunk_id(ino, cutoff_chunk);
            let mut slices = self.get_slices(chunk_id).await?;
            slices.retain(|s| s.offset < cutoff_offset);
            for slice in slices.iter_mut() {
                let end = slice.offset + slice.length;
                if end > cutoff_offset {
                    slice.length = cutoff_offset - slice.offset;
                }
            }
            self.rewrite_slices(chunk_id, &slices).await?;
        }

        // Drop any chunks completely past the new EOF.
        let drop_start = if cutoff_offset == 0 {
            cutoff_chunk
        } else {
            cutoff_chunk + 1
        };
        for idx in drop_start..old_chunk_count {
            let chunk_id = self.chunk_id(ino, idx);
            let key = self.chunk_key(chunk_id);
            let mut conn = self.conn.clone();
            redis::cmd("DEL")
                .arg(&key)
                .query_async::<()>(&mut conn)
                .await
                .map_err(redis_err)?;
        }

        Ok(())
    }
}

#[async_trait]
impl MetaStore for RedisMetaStore {
    fn name(&self) -> &'static str {
        "redis-meta-store"
    }

    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        Ok(self.get_node(ino).await?.map(|n| n.as_file_attr()))
    }

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        self.directory_child(parent, name).await
    }

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError> {
        if path.is_empty() {
            return Ok(None);
        }
        if path == "/" {
            return Ok(Some((ROOT_INODE, FileType::Dir)));
        }
        let mut current = ROOT_INODE;
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            let Some(next) = self.lookup(current, segment).await? else {
                return Ok(None);
            };
            current = next;
        }
        if let Some(node) = self.get_node(current).await? {
            Ok(Some((node.ino, node.kind.into())))
        } else {
            Ok(None)
        }
    }

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError> {
        let node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        if node.kind != NodeKind::Dir {
            return Err(MetaError::NotDirectory(ino));
        }
        let mut conn = self.conn.clone();
        let entries: Vec<(String, i64)> =
            conn.hgetall(self.dir_key(ino)).await.map_err(redis_err)?;
        let mut result = Vec::new();
        for (name, child) in entries {
            if let Some(node) = self.get_node(child).await? {
                result.push(DirEntry {
                    name,
                    ino: child,
                    kind: node.kind.into(),
                });
            }
        }
        Ok(result)
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_entry(parent, name, FileType::Dir).await
    }

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let Some(child) = self.lookup(parent, name).await? else {
            return Err(MetaError::NotFound(parent));
        };
        let node = self
            .get_node(child)
            .await?
            .ok_or(MetaError::NotFound(child))?;
        if node.kind != NodeKind::Dir {
            return Err(MetaError::NotDirectory(child));
        }
        let len = self.directory_len(child).await?;
        if len > 0 {
            return Err(MetaError::DirectoryNotEmpty(child));
        }
        self.remove_dir_entry(parent, name).await?;
        self.delete_node(child).await?;
        self.update_nlink(parent, -1).await?;
        let now = current_time();
        self.bump_dir_times(parent, now).await?;
        Ok(())
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_entry(parent, name, FileType::File).await
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let Some(child) = self.lookup(parent, name).await? else {
            return Err(MetaError::NotFound(parent));
        };
        let mut node = self
            .get_node(child)
            .await?
            .ok_or(MetaError::NotFound(child))?;
        if node.kind != NodeKind::File {
            return Err(MetaError::NotSupported(format!("{child} is not a file")));
        }
        self.remove_dir_entry(parent, name).await?;
        self.mark_deleted(child, &mut node).await?;
        let now = current_time();
        self.bump_dir_times(parent, now).await?;
        Ok(())
    }

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        let Some(child) = self.lookup(old_parent, old_name).await? else {
            return Err(MetaError::NotFound(old_parent));
        };
        self.ensure_parent_dir(new_parent).await?;
        if self.lookup(new_parent, &new_name).await?.is_some() {
            return Err(MetaError::AlreadyExists {
                parent: new_parent,
                name: new_name,
            });
        }
        let mut node = self
            .get_node(child)
            .await?
            .ok_or(MetaError::NotFound(child))?;
        self.remove_dir_entry(old_parent, old_name).await?;
        self.add_dir_entry(new_parent, &new_name, child).await?;
        node.parent = new_parent;
        node.name = new_name;
        let now = current_time();
        node.attr.mtime = now;
        node.attr.ctime = now;
        self.save_node(&node).await?;
        self.bump_dir_times(old_parent, now).await?;
        self.bump_dir_times(new_parent, now).await?;
        Ok(())
    }

    async fn set_attr(
        &self,
        ino: i64,
        req: &SetAttrRequest,
        flags: SetAttrFlags,
    ) -> Result<FileAttr, MetaError> {
        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        let old_size = node.attr.size;
        let mut ctime_update = false;
        let now = current_time();

        if let Some(mode) = req.mode {
            // Preserve the existing file type bits while updating permission bits.
            let kind_bits = node.attr.mode & 0o170000;
            node.attr.mode = kind_bits | (mode & 0o7777);
            ctime_update = true;
        }

        if let Some(uid) = req.uid {
            node.attr.uid = uid;
            ctime_update = true;
        }
        if let Some(gid) = req.gid {
            node.attr.gid = gid;
            ctime_update = true;
        }

        if flags.contains(SetAttrFlags::CLEAR_SUID) {
            node.attr.mode &= !0o4000;
            ctime_update = true;
        }
        if flags.contains(SetAttrFlags::CLEAR_SGID) {
            node.attr.mode &= !0o2000;
            ctime_update = true;
        }

        if let Some(size) = req.size {
            if node.kind != NodeKind::File {
                return Err(MetaError::NotSupported(
                    "truncate flag only supported for regular files".into(),
                ));
            }
            if node.attr.size != size {
                node.attr.size = size;
                node.attr.mtime = now;
            }
            ctime_update = true;
        }

        if flags.contains(SetAttrFlags::SET_ATIME_NOW) {
            node.attr.atime = now;
            ctime_update = true;
        } else if let Some(atime) = req.atime {
            node.attr.atime = atime;
            ctime_update = true;
        }

        if flags.contains(SetAttrFlags::SET_MTIME_NOW) {
            node.attr.mtime = now;
            ctime_update = true;
        } else if let Some(mtime) = req.mtime {
            node.attr.mtime = mtime;
            ctime_update = true;
        }

        if let Some(ctime) = req.ctime {
            node.attr.ctime = ctime;
        } else if ctime_update {
            node.attr.ctime = now;
        }

        if let Some(size) = req.size {
            self.prune_slices_for_truncate(ino, size, old_size).await?;
        }

        self.save_node(&node).await?;
        Ok(node.attr.to_file_attr(node.ino, node.kind.into()))
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        let old_size = node.attr.size;
        let now = current_time();
        self.prune_slices_for_truncate(ino, size, old_size).await?;
        node.attr.size = size;
        node.attr.mtime = now;
        node.attr.ctime = now;
        self.save_node(&node).await
    }

    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError> {
        Ok(self.get_node(ino).await?.map(|n| n.parent))
    }

    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError> {
        Ok(self.get_node(ino).await?.map(|n| n.name))
    }

    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError> {
        let mut current = self.get_node(ino).await?;
        let mut segments = Vec::new();
        while let Some(node) = current {
            if node.ino == ROOT_INODE {
                segments.push(String::new());
                break;
            }
            segments.push(node.name.clone());
            current = self.get_node(node.parent).await?;
        }
        if segments.is_empty() {
            return Ok(None);
        }
        segments.reverse();
        let path = if segments.len() == 1 {
            "/".to_string()
        } else {
            segments.join("/")
        };
        Ok(Some(path))
    }

    fn root_ino(&self) -> i64 {
        ROOT_INODE
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        self.init_root_directory().await
    }

    async fn get_deleted_files(&self) -> Result<Vec<i64>, MetaError> {
        let mut conn = self.conn.clone();
        let raw: Vec<String> = conn
            .hkeys(self.deleted_set_key())
            .await
            .map_err(redis_err)?;
        let mut inodes = Vec::with_capacity(raw.len());
        for key in raw {
            match key.parse::<i64>() {
                Ok(id) => inodes.push(id),
                Err(e) => {
                    tracing::warn!("invalid inode id in delSlices: {key}, err={e}");
                }
            }
        }
        Ok(inodes)
    }

    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .hdel(self.deleted_set_key(), ino.to_string())
            .await
            .map_err(redis_err)?;
        self.delete_node(ino).await
    }

    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let mut conn = self.conn.clone();
        let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
            .arg(self.chunk_key(chunk_id))
            .arg(0)
            .arg(-1)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
        let mut slices = Vec::new();
        for entry in raw {
            let desc: SliceDesc =
                serde_json::from_slice(&entry).map_err(|e| MetaError::Internal(e.to_string()))?;
            slices.push(desc);
        }
        Ok(slices)
    }

    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let data = serde_json::to_vec(&slice).map_err(|e| MetaError::Internal(e.to_string()))?;
        let _: () = redis::cmd("RPUSH")
            .arg(self.chunk_key(chunk_id))
            .arg(data)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
        Ok(())
    }

    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.alloc_id(key).await
    }

    async fn new_session(&self, session_info: SessionInfo) -> Result<Session, MetaError> {
        let mut conn = self.conn.clone();

        let session_id = Uuid::now_v7();
        let expire = (Utc::now() + chrono::Duration::minutes(5)).timestamp_millis();
        let session = Session {
            session_id,
            session_info: session_info.clone(),
            expire,
        };

        let session_info_json = serde_json::to_string(&session_info)
            .map_err(|err| MetaError::Internal(err.to_string()))?;

        let session_id_string = session_id.to_string();

        redis::pipe()
            .atomic()
            .zadd(ALL_SESSIONS_KEY, &session_id_string, expire)
            .hset(SESSION_INFOS_KEY, &session_id_string, session_info_json)
            .exec_async(&mut conn)
            .await
            .map_err(|err| MetaError::Internal(err.to_string()))?;

        Ok(session)
    }

    async fn refresh_session(&self, session_id: Uuid) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let session_id_string = session_id.to_string();
        let expire = (Utc::now() + chrono::Duration::minutes(5)).timestamp_millis();
        redis::Cmd::zadd(ALL_SESSIONS_KEY, session_id_string, expire)
            .exec_async(&mut conn)
            .await
            .map_err(|err| MetaError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn shutdown_session(&self, session_id: Uuid) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let session_id_string = session_id.to_string();

        redis::pipe()
            .atomic()
            .zrem(ALL_SESSIONS_KEY, &session_id_string)
            .hdel(SESSION_INFOS_KEY, &session_id_string)
            .exec_async(&mut conn)
            .await
            .map_err(|err| MetaError::Internal(err.to_string()))?;

        Ok(())
    }

    async fn cleanup_sessions(&self) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let now = Utc::now().timestamp_millis();
        let sessions: Vec<String> = redis::Cmd::zrangebyscore(ALL_SESSIONS_KEY, "-inf", now)
            .query_async(&mut conn)
            .await
            .map_err(|err| MetaError::Internal(err.to_string()))?;
        for session in sessions {
            let session_id =
                Uuid::from_str(&session).map_err(|err| MetaError::Internal(err.to_string()))?;
            self.shutdown_session(session_id).await?;
        }
        Ok(())
    }

    async fn get_lock(&self, lock_name: LockName) -> bool {
        let lock_name = lock_name.to_string();
        let mut conn = self.conn.clone();
        let now = Utc::now().timestamp_millis();

        let script = redis::Script::new(
            r#"
            local key = KEYS[1]
            local field = ARGV[1]
            local now_time = tonumber(ARGV[2])
            local diff = tonumber(ARGV[3])

            local last_updated = redis.call("HGET",key,field)

            if last_updated == false then
                redis.call("HSET",key,field,new_value)
                return true
            else
                last_updated = tonumber(last_updated)
                if now_time < last_updated + diff then
                    return false
                else
                    redis.call('HSET', key,field, new_value)
                    return true
                end
            end
            "#,
        );

        let diff = chrono::Duration::seconds(7).num_milliseconds();

        let resp: Result<bool, _> = script
            .key(LOCKS_KEY)
            .arg(lock_name)
            .arg(now)
            .arg(diff)
            .invoke_async(&mut conn)
            .await;

        match resp {
            Ok(v) => v,
            Err(err) => {
                error!("{}", err.to_string());
                false
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    // returns the current lock owner for a range on a file.
    async fn get_plock(
        &self,
        inode: i64,
        query: &FileLockQuery,
    ) -> Result<FileLockInfo, MetaError> {
        let mut conn = self.conn.clone();
        let plock_key = self.plock_key(inode);
        let sid = self
            .sid
            .get()
            .ok_or_else(|| MetaError::Internal("sid not seted".to_string()))?;

        // First, try to get locks from current session's field
        let current_field = format!("{}:{}", sid, query.owner);
        let records_json: Result<String, _> = conn.hget(&plock_key, &current_field).await;
        if let Ok(records_json) = records_json {
            let records: Vec<PlockRecord> = serde_json::from_str(&records_json).unwrap_or_default();
            if let Some(v) = PlockRecord::get_plock(&records, query, sid, sid) {
                return Ok(v);
            }
        }

        // Get all plock entries for this inode
        let plock_entries: Vec<String> = conn.hkeys(&plock_key).await.map_err(redis_err)?;

        for field in plock_entries {
            // Skip current field as we already checked it
            if field == current_field {
                continue;
            }

            let parts: Vec<&str> = field.split(':').collect();
            if parts.len() != 2 {
                continue;
            }

            let lock_sid = Uuid::parse_str(parts[0])
                .map_err(|_| MetaError::Internal("Invalid sid in plock field".to_string()))?;
            let _lock_owner: i64 = parts[1]
                .parse()
                .map_err(|_| MetaError::Internal("Invalid owner in plock field".to_string()))?;

            let records_json: String = conn.hget(&plock_key, &field).await.map_err(redis_err)?;
            let records: Vec<PlockRecord> = serde_json::from_str(&records_json).unwrap_or_default();

            if let Some(v) = PlockRecord::get_plock(&records, query, sid, &lock_sid) {
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
                .try_set_plock(inode, owner, new_lock, lock_type, range)
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
            .map_err(|_| MetaError::Internal("sid already been set".to_string()))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredNode {
    ino: i64,
    parent: i64,
    name: String,
    kind: NodeKind,
    attr: StoredAttr,
    deleted: bool,
}

impl StoredNode {
    fn new(ino: i64, parent: i64, name: String, kind: FileType) -> Self {
        let attr = StoredAttr::new(kind);
        Self {
            ino,
            parent,
            name,
            kind: kind.into(),
            attr,
            deleted: false,
        }
    }

    fn as_file_attr(&self) -> FileAttr {
        self.attr.to_file_attr(self.ino, self.kind.into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredAttr {
    size: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    atime: i64,
    mtime: i64,
    ctime: i64,
    nlink: u32,
}

impl StoredAttr {
    fn new(kind: FileType) -> Self {
        let now = current_time();
        let (mode, nlink) = match kind {
            FileType::Dir => (0o040755, 2),
            FileType::File => (0o100644, 1),
            FileType::Symlink => (0o120777, 1),
        };
        Self {
            size: 0,
            mode,
            uid: 0,
            gid: 0,
            atime: now,
            mtime: now,
            ctime: now,
            nlink,
        }
    }

    fn to_file_attr(&self, ino: i64, kind: FileType) -> FileAttr {
        FileAttr {
            ino,
            size: self.size,
            kind,
            mode: self.mode,
            uid: self.uid,
            gid: self.gid,
            atime: self.atime,
            mtime: self.mtime,
            ctime: self.ctime,
            nlink: self.nlink,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum NodeKind {
    File,
    Dir,
    Symlink,
}

impl From<FileType> for NodeKind {
    fn from(value: FileType) -> Self {
        match value {
            FileType::File => NodeKind::File,
            FileType::Dir => NodeKind::Dir,
            FileType::Symlink => NodeKind::Symlink,
        }
    }
}

impl From<NodeKind> for FileType {
    fn from(value: NodeKind) -> Self {
        match value {
            NodeKind::File => FileType::File,
            NodeKind::Dir => FileType::Dir,
            NodeKind::Symlink => FileType::Symlink,
        }
    }
}

fn current_time() -> i64 {
    Utc::now().timestamp_nanos_opt().unwrap_or(0)
}

#[allow(dead_code)]
fn millis_to_system_time(ms: i64) -> Result<SystemTime, MetaError> {
    if ms < 0 {
        return Err(MetaError::Internal(format!(
            "invalid session timestamp {ms}"
        )));
    }
    Ok(UNIX_EPOCH + Duration::from_millis(ms as u64))
}

fn redis_err(err: redis::RedisError) -> MetaError {
    MetaError::Internal(format!("Redis error: {err}"))
}

#[cfg(test)]
mod tests {
    use crate::meta::MetaStore;
    use crate::meta::config::Config;
    use crate::meta::config::{CacheConfig, ClientOptions, DatabaseConfig, DatabaseType};
    use crate::meta::file_lock::{FileLockQuery, FileLockRange, FileLockType};
    use crate::meta::store::MetaError;
    use crate::meta::stores::RedisMetaStore;
    use tokio::time;
    use uuid::Uuid;

    fn test_config() -> Config {
        Config {
            database: DatabaseConfig {
                db_config: DatabaseType::Redis {
                    url: "redis://127.0.0.1:6379/0".to_string(),
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
                db_config: DatabaseType::Redis {
                    url: "redis://127.0.0.1:6379/0".to_string(),
                },
            },
            cache: CacheConfig::default(),
            client: ClientOptions::default(),
        }
    }

    async fn new_test_store() -> RedisMetaStore {
        RedisMetaStore::from_config(test_config())
            .await
            .expect("Failed to create test database store")
    }

    /// Create a new test store with pre-configured session ID
    async fn new_test_store_with_session(session_id: Uuid) -> RedisMetaStore {
        let store = new_test_store().await;
        store.set_sid(session_id).expect("Failed to set session ID");
        store
    }

    /// Helper struct to manage multiple test sessions
    struct TestSessionManager {
        stores: Vec<RedisMetaStore>,
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

            static FIRST_INIT: std::sync::Once = std::sync::Once::new();
            FIRST_INIT.call_once(|| {
                let _ = std::fs::remove_file(&db_path);
            });

            let mut stores = Vec::with_capacity(session_count);
            let mut session_ids = Vec::with_capacity(session_count);

            let config = shared_db_config();
            let first_store = RedisMetaStore::from_config(config.clone())
                .await
                .expect("Failed to create shared test database store");

            let first_session_id = Uuid::now_v7();
            first_store
                .set_sid(first_session_id)
                .expect("Failed to set session ID");

            stores.push(first_store);
            session_ids.push(first_session_id);

            for _ in 1..session_count {
                let store = RedisMetaStore::from_config(config.clone())
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

        fn get_store(&self, index: usize) -> &RedisMetaStore {
            &self.stores[index]
        }
    }

    #[tokio::test]
    async fn test_basic_read_lock() {
        let store = new_test_store().await;
        let session_id = Uuid::now_v7();
        let owner: i64 = 1001;

        // Set session
        store.set_sid(session_id).unwrap();

        // Create a file first
        let parent = store.root_ino();
        let file_ino = store
            .create_file(parent, "test_basic_read_lock_file.txt".to_string())
            .await
            .unwrap();

        // Acquire read lock
        store
            .set_plock(
                file_ino,
                owner,
                false,
                FileLockType::Read,
                FileLockRange { start: 0, end: 100 },
                1234,
            )
            .await
            .unwrap();

        // Verify lock exists
        let query = FileLockQuery {
            owner: owner,
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
            lock_type: FileLockType::Write,
            range: FileLockRange { start: 0, end: 100 },
        };

        let query2 = FileLockQuery {
            owner: owner2,
            lock_type: FileLockType::Read,
            range: FileLockRange { start: 0, end: 100 },
        };

        let lock_info1 = store1.get_plock(file_ino, &query1).await.unwrap();
        assert_eq!(lock_info1.lock_type, FileLockType::Read);
        assert_eq!(lock_info1.range.start, 0);
        assert_eq!(lock_info1.range.end, 100);
        assert_eq!(lock_info1.pid, 1234);

        let lock_info2 = store2.get_plock(file_ino, &query2).await.unwrap();
        assert_eq!(lock_info2.lock_type, FileLockType::UnLock);
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
            .create_file(parent, "test_lock_release_file.txt".to_string())
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
            owner: owner,
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
            .create_file(parent, "test_concurrent_read_write_locks.txt".to_string())
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
            .create_file(parent, "test_cross_session_lock_visibility.txt".to_string())
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
