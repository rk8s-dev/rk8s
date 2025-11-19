//! Redis-based metadata store implementation.
//!
//! This store focuses on the core interfaces needed by the VFS layer so that
//! the filesystem can persist metadata in Redis. It purposely keeps the key
//! layout simple (one key per inode plus a hash per directory) and uses JSON
//! serialization for file attributes. Advanced features (sessions, quota, etc.)
//! can be layered on later by extending the schema.

use crate::chuck::SliceDesc;
use crate::meta::config::{Config, DatabaseType};
use crate::meta::store::{DirEntry, FileAttr, FileType, MetaError, MetaStore, SessionInfo};
use crate::meta::{INODE_ID_KEY, SESSION_ID_KEY, SLICE_ID_KEY};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::Utc;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

const ROOT_INODE: i64 = 1;
const COUNTER_INODE_KEY: &str = "nextinode";
const COUNTER_SLICE_KEY: &str = "nextchunk";
const COUNTER_SESSION_KEY: &str = "nextsession";
const NODE_KEY_PREFIX: &str = "i";
const DIR_KEY_PREFIX: &str = "d";
const CHUNK_KEY_PREFIX: &str = "c";
const DELETED_SET_KEY: &str = "delslices";
const ALL_SESSIONS_KEY: &str = "allSessions";
const SESSION_INFOS_KEY: &str = "sessionInfos";
const CHUNK_ID_BASE: u64 = 1_000_000_000u64;
const SESSION_TIMEOUT_SECS: i64 = 120;
const SESSION_TIMEOUT_MILLIS: i64 = SESSION_TIMEOUT_SECS * 1000;

/// Minimal Redis-backed meta store.
pub struct RedisMetaStore {
    conn: ConnectionManager,
    _config: Config,
    session_state: Mutex<SessionState>,
}

#[derive(Default)]
struct SessionState {
    id: Option<u64>,
    payload: Option<Vec<u8>>,
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
            session_state: Mutex::new(SessionState::default()),
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

    fn deleted_set_key(&self) -> &'static str {
        DELETED_SET_KEY
    }

    fn counter_key(key: &str) -> Result<&'static str, MetaError> {
        let suffix = match key {
            INODE_ID_KEY => COUNTER_INODE_KEY,
            SLICE_ID_KEY => COUNTER_SLICE_KEY,
            SESSION_ID_KEY => COUNTER_SESSION_KEY,
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
        let node = StoredNode::new(ino, parent, name.clone(), kind);
        self.save_node(&node).await?;
        self.add_dir_entry(parent, &name, ino).await?;
        if matches!(kind, FileType::Dir) {
            self.update_nlink(parent, 1).await?;
        }
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
        self.save_node(node).await?;
        let field = ino.to_string();
        conn.hset(self.deleted_set_key(), field, 1)
            .await
            .map_err(redis_err)
    }

    fn current_session_id(&self) -> Option<u64> {
        self.session_state.lock().unwrap().id
    }

    fn session_snapshot(&self) -> Option<(u64, Vec<u8>)> {
        let guard = self.session_state.lock().unwrap();
        match (guard.id, guard.payload.clone()) {
            (Some(id), Some(payload)) => Some((id, payload)),
            _ => None,
        }
    }

    fn store_session_state(&self, session_id: u64, payload: Vec<u8>) {
        let mut guard = self.session_state.lock().unwrap();
        guard.id = Some(session_id);
        guard.payload = Some(payload);
    }

    fn clear_session_state(&self, session_id: u64) {
        let mut guard = self.session_state.lock().unwrap();
        if guard.id == Some(session_id) {
            guard.id = None;
            guard.payload = None;
        }
    }

    async fn allocate_session_id(&self) -> Result<u64, MetaError> {
        let raw = self.alloc_id(SESSION_ID_KEY).await?;
        u64::try_from(raw).map_err(|_| MetaError::Internal("session id overflow".to_string()))
    }

    async fn persist_session(&self, session_id: u64, payload: &[u8]) -> Result<(), MetaError> {
        let (now_ms, expire_ms) = Self::session_timestamps();
        let record = StoredSessionDetails::new(session_id, payload, now_ms);
        let serialized =
            serde_json::to_string(&record).map_err(|e| MetaError::Internal(e.to_string()))?;

        let mut conn = self.conn.clone();
        redis::pipe()
            .cmd("ZADD")
            .arg(ALL_SESSIONS_KEY)
            .arg(expire_ms)
            .arg(session_id.to_string())
            .cmd("HSET")
            .arg(SESSION_INFOS_KEY)
            .arg(session_id.to_string())
            .arg(serialized)
            .query_async::<()>(&mut conn)
            .await
            .map_err(redis_err)?;
        Ok(())
    }

    fn session_timestamps() -> (i64, i64) {
        let now = Utc::now().timestamp_millis();
        (now, now + SESSION_TIMEOUT_MILLIS)
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
        node.attr.mtime = current_time();
        node.attr.ctime = node.attr.mtime;
        self.save_node(&node).await
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        node.attr.size = size;
        node.attr.mtime = current_time();
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

    async fn new_session(&self, payload: &[u8], update: bool) -> Result<(), MetaError> {
        let session_id = if update {
            self.current_session_id()
        } else {
            None
        };
        let session_id = match session_id {
            Some(id) => id,
            None => self.allocate_session_id().await?,
        };

        self.persist_session(session_id, payload).await?;
        self.store_session_state(session_id, payload.to_vec());
        Ok(())
    }

    async fn refresh_session(&self) -> Result<(), MetaError> {
        let Some((session_id, payload)) = self.session_snapshot() else {
            return Err(MetaError::Internal(
                "no active session configured for refresh".to_string(),
            ));
        };
        self.persist_session(session_id, &payload).await
    }

    async fn find_stale_sessions(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<SessionInfo>, MetaError> {
        let now = Utc::now().timestamp_millis();
        let mut cmd = redis::cmd("ZRANGEBYSCORE");
        cmd.arg(ALL_SESSIONS_KEY).arg("-inf").arg(now);
        if let Some(limit) = limit {
            cmd.arg("LIMIT").arg(0).arg(limit as isize);
        }

        let mut conn = self.conn.clone();
        let expired: Vec<String> = cmd.query_async(&mut conn).await.map_err(redis_err)?;
        if expired.is_empty() {
            return Ok(Vec::new());
        }

        let mut sessions = Vec::with_capacity(expired.len());
        for member in expired {
            let raw: Option<String> = conn
                .hget(SESSION_INFOS_KEY, &member)
                .await
                .map_err(redis_err)?;
            let Some(value) = raw else {
                continue;
            };

            match serde_json::from_str::<StoredSessionDetails>(&value) {
                Ok(record) => match record.to_session_info() {
                    Ok(info) => sessions.push(info),
                    Err(err) => warn!("Failed to decode session info for {}: {err}", member),
                },
                Err(err) => warn!("Invalid session payload for {}: {err}", member),
            }
        }

        Ok(sessions)
    }

    async fn clean_stale_session(&self, session_id: u64) -> Result<(), MetaError> {
        let member = session_id.to_string();
        let mut conn = self.conn.clone();
        redis::pipe()
            .cmd("ZREM")
            .arg(ALL_SESSIONS_KEY)
            .arg(&member)
            .cmd("HDEL")
            .arg(SESSION_INFOS_KEY)
            .arg(&member)
            .query_async::<()>(&mut conn)
            .await
            .map_err(redis_err)?;
        self.clear_session_state(session_id);
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredSessionDetails {
    session_id: u64,
    payload_b64: String,
    updated_at_ms: i64,
}

impl StoredSessionDetails {
    fn new(session_id: u64, payload: &[u8], updated_at_ms: i64) -> Self {
        Self {
            session_id,
            payload_b64: B64.encode(payload),
            updated_at_ms,
        }
    }

    fn to_session_info(&self) -> Result<SessionInfo, MetaError> {
        let payload = B64
            .decode(self.payload_b64.as_bytes())
            .map_err(|e| MetaError::Internal(format!("decode session payload: {e}")))?;
        let updated_at = millis_to_system_time(self.updated_at_ms)?;
        Ok(SessionInfo {
            id: self.session_id,
            info: payload,
            updated_at,
        })
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
