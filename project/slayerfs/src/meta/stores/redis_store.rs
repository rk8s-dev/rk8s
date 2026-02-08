//! Redis-based metadata store implementation.
//!
//! This store focuses on the core interfaces needed by the VFS layer so that
//! the filesystem can persist metadata in Redis. It purposely keeps the key
//! layout simple (one key per inode plus a hash per directory) and uses JSON
//! serialization for file attributes. Advanced features (sessions, quota, etc.)
//! can be layered on later by extending the schema.

use super::{apply_truncate_plan, trim_slices_in_place};
use crate::chuck::SliceDesc;
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
use std::time::Duration;
use tokio::select;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, error};
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
const LOCKED_KEY: &str = "locked";
const LINK_PARENT_KEY_PREFIX: &str = "lp:";

const CHUNK_ID_BASE: u64 = 1_000_000_000u64;

// Lua script for atomically extending file size
const EXTEND_FILE_SIZE_LUA: &str = r#"
    local node_json = redis.call('GET', KEYS[1])
    if not node_json then
        return cjson.encode({ok=false, error="node_not_found"})
    end
    local ok, node = pcall(cjson.decode, node_json)
    if not ok or not node or not node.attr or not node.attr.size then
        return cjson.encode({ok=false, error="corrupt_node"})
    end
    local new_size = tonumber(ARGV[1])
    local timestamp = tonumber(ARGV[2])
    if new_size <= node.attr.size then
        return cjson.encode({ok=true, updated=false})
    end
    node.attr.size = new_size
    node.attr.mtime = timestamp
    node.attr.ctime = timestamp
    -- POSIX: clear setuid/setgid bits on write (security: prevent privilege escalation)
    if node.attr.mode then
        node.attr.mode = bit.band(node.attr.mode, bit.bnot(6144))  -- Clear 06000 (setuid+setgid)
    end
    redis.call('SET', KEYS[1], cjson.encode(node))
    return cjson.encode({ok=true, updated=true})
"#;

// Lua script for atomically incrementing nlink and updating link_parents
const LINK_LUA: &str = r#"
    local node_key = KEYS[1]
    local lp_key = KEYS[2]
    local dir_key = KEYS[3]
    local parent_ino = ARGV[1]
    local name = ARGV[2]
    local timestamp = tonumber(ARGV[3])

    local node_json = redis.call('GET', node_key)
    if not node_json then
        return cjson.encode({ok=false, error="node_not_found"})
    end
    local ok, node = pcall(cjson.decode, node_json)
    if not ok or not node or not node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- Check if link name already exists in directory
    local existing = redis.call('HEXISTS', dir_key, name)
    if existing == 1 then
        return cjson.encode({ok=false, error="already_exists"})
    end

    -- If transitioning from nlink=1 to nlink=2, save original parent/name to link_parents
    if node.attr.nlink == 1 then
        local original_member = node.parent .. ":" .. node.name
        redis.call('SADD', lp_key, original_member)
        -- Transition to hardlink state: parent=0, name=""
        node.parent = 0
        node.name = ""
    end

    -- Increment nlink
    node.attr.nlink = node.attr.nlink + 1
    node.attr.ctime = timestamp

    -- Add new link to link_parents set
    local member = parent_ino .. ":" .. name
    redis.call('SADD', lp_key, member)

    -- Add to directory
    redis.call('HSET', dir_key, name, node.ino)

    -- Save node
    redis.call('SET', node_key, cjson.encode(node))

    return cjson.encode({ok=true, attr=node.attr})
"#;

// Lua script for atomically decrementing nlink and updating link_parents
const UNLINK_LUA: &str = r#"
    local node_key = KEYS[1]
    local lp_key = KEYS[2]
    local dir_key = KEYS[3]
    local parent_ino = ARGV[1]
    local name = ARGV[2]
    local timestamp = tonumber(ARGV[3])

    -- Remove from directory (idempotent)
    redis.call('HDEL', dir_key, name)

    -- Remove from link_parents (idempotent)
    local member = parent_ino .. ":" .. name
    redis.call('SREM', lp_key, member)

    -- Try to get node
    local node_json = redis.call('GET', node_key)
    if not node_json then
        return cjson.encode({ok=true, nlink=0, deleted=true})
    end
    local ok, node = pcall(cjson.decode, node_json)
    if not ok or not node or not node.attr then
        return cjson.encode({ok=true, nlink=0, deleted=true})
    end

    -- Decrement nlink
    if node.attr.nlink > 0 then
        node.attr.nlink = node.attr.nlink - 1
    end
    node.attr.ctime = timestamp

    -- If transitioning from nlink=2 to nlink=1, restore parent/name from remaining link_parent
    if node.attr.nlink == 1 then
        local remaining_members = redis.call('SMEMBERS', lp_key)
        if #remaining_members == 1 then
            local parts = {}
            for part in string.gmatch(remaining_members[1], "[^:]+") do
                table.insert(parts, part)
            end
            if #parts >= 2 then
                node.parent = tonumber(parts[1])
                node.name = table.concat(parts, ":", 2)
            end
            -- Clear link_parents set
            redis.call('DEL', lp_key)
        end
    end

    -- Save node
    redis.call('SET', node_key, cjson.encode(node))

    local deleted = node.attr.nlink == 0
    return cjson.encode({ok=true, nlink=node.attr.nlink, deleted=deleted})
"#;

// Lua script for atomically removing directory entry and updating parent nlink
const RMDIR_LUA: &str = r#"
    local cjson = cjson

    local parent_dir_key = KEYS[1]
    local child_node_key = KEYS[2]
    local parent_node_key = KEYS[3]
    local child_dir_key = KEYS[4]
    local name = ARGV[1]
    local child_ino = tonumber(ARGV[2])
    local parent_ino = tonumber(ARGV[3])
    local timestamp = tonumber(ARGV[4])

    -- 1. Check dentry exists
    local dentry_ino = redis.call('HGET', parent_dir_key, name)
    if not dentry_ino then
        return cjson.encode({ok=false, error="not_found", ino=parent_ino})
    end

    -- 2. Get child node
    local child_json = redis.call('GET', child_node_key)
    if not child_json then
        return cjson.encode({ok=false, error="node_not_found", ino=child_ino})
    end

    -- 3. Decode child node with pcall
    local ok, child_node = pcall(cjson.decode, child_json)
    if not ok or not child_node or not child_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- 4. Check is directory
    if child_node.kind ~= "Dir" then
        return cjson.encode({ok=false, error="not_directory", ino=child_ino})
    end

    -- 5. Check empty
    local child_len = redis.call('HLEN', child_dir_key)
    if child_len > 0 then
        return cjson.encode({ok=false, error="dir_not_empty", ino=child_ino})
    end

    -- 6. Get parent node and update
    local parent_json = redis.call('GET', parent_node_key)
    if parent_json then
        local ok_p, parent_node = pcall(cjson.decode, parent_json)
        if ok_p and parent_node and parent_node.attr then
            parent_node.attr.nlink = parent_node.attr.nlink - 1
            parent_node.attr.mtime = timestamp
            parent_node.attr.ctime = timestamp
            redis.call('SET', parent_node_key, cjson.encode(parent_node))
        end
    end

    -- 7. Atomic delete
    redis.call('HDEL', parent_dir_key, name)
    redis.call('DEL', child_node_key)
    redis.call('DEL', child_dir_key)

    return cjson.encode({ok=true})
"#;

// Lua script for atomically creating directory entry with inode allocation
const CREATE_ENTRY_LUA: &str = r#"
    local cjson = cjson

    local parent_dir_key = KEYS[1]
    local parent_node_key = KEYS[2]
    local counter_key = KEYS[3]
    local name = ARGV[1]
    local kind = ARGV[2]
    local timestamp = tonumber(ARGV[3])
    local parent_ino = tonumber(ARGV[4])
    local default_mode = tonumber(ARGV[5])
    local uid = tonumber(ARGV[6])
    local gid = tonumber(ARGV[7])
    local parent_gid = tonumber(ARGV[8])
    local parent_has_setgid = tonumber(ARGV[9])

    -- 1. Get parent node
    local parent_json = redis.call('GET', parent_node_key)
    if not parent_json then
        return cjson.encode({ok=false, error="parent_not_found"})
    end

    -- 2. Decode parent node with pcall
    local ok, parent_node = pcall(cjson.decode, parent_json)
    if not ok or not parent_node or not parent_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- 3. Check parent is directory
    if parent_node.kind ~= "Dir" then
        return cjson.encode({ok=false, error="parent_not_directory"})
    end

    -- 4. Check entry doesn't already exist
    local existing = redis.call('HEXISTS', parent_dir_key, name)
    if existing == 1 then
        return cjson.encode({ok=false, error="already_exists"})
    end

    -- 5. Allocate new inode atomically
    local new_ino = redis.call('INCR', counter_key)

    -- 6. Apply setgid inheritance
    local final_gid = gid
    local final_mode = default_mode
    if parent_has_setgid == 1 then
        final_gid = parent_gid
        if kind == "Dir" then
            final_mode = bit.bor(final_mode, 2048)  -- 0o2000 setgid bit
        end
    end

    -- 7. Determine nlink based on kind
    local nlink = 1
    if kind == "Dir" then
        nlink = 2
    end

    -- 8. Create new node
    local new_node = {
        ino = new_ino,
        parent = parent_ino,
        name = name,
        kind = kind,
        attr = {
            size = 0,
            mode = final_mode,
            uid = uid,
            gid = final_gid,
            atime = timestamp,
            mtime = timestamp,
            ctime = timestamp,
            nlink = nlink
        },
        deleted = false
    }

    -- 9. Save new node
    redis.call('SET', 'i' .. new_ino, cjson.encode(new_node))

    -- 10. Add directory entry
    redis.call('HSET', parent_dir_key, name, new_ino)

    -- 11. Update parent if creating directory (nlink++)
    if kind == "Dir" then
        parent_node.attr.nlink = parent_node.attr.nlink + 1
    end

    -- 12. Update parent timestamps
    parent_node.attr.mtime = timestamp
    parent_node.attr.ctime = timestamp
    redis.call('SET', parent_node_key, cjson.encode(parent_node))

    return cjson.encode({ok=true, ino=new_ino})
"#;

// Lua script for atomically renaming file or directory (no overwrite)
const RENAME_LUA: &str = r#"
    local cjson = cjson

    local old_parent_dir_key = KEYS[1]
    local new_parent_dir_key = KEYS[2]
    local child_node_key = KEYS[3]
    local old_parent_node_key = KEYS[4]
    local new_parent_node_key = KEYS[5]
    local link_parents_key = KEYS[6]
    local old_name = ARGV[1]
    local new_name = ARGV[2]
    local old_parent_ino = tonumber(ARGV[3])
    local new_parent_ino = tonumber(ARGV[4])
    local timestamp = tonumber(ARGV[5])

    -- 1. Check source dentry exists
    local dentry_ino = redis.call('HGET', old_parent_dir_key, old_name)
    if not dentry_ino then
        return cjson.encode({ok=false, error="not_found", ino=old_parent_ino})
    end

    -- 2. Check new_parent exists and is directory
    local new_parent_json = redis.call('GET', new_parent_node_key)
    if not new_parent_json then
        return cjson.encode({ok=false, error="parent_not_found", ino=new_parent_ino})
    end
    local ok_np, new_parent_node = pcall(cjson.decode, new_parent_json)
    if not ok_np or not new_parent_node or not new_parent_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end
    if new_parent_node.kind ~= "Dir" then
        return cjson.encode({ok=false, error="parent_not_directory", ino=new_parent_ino})
    end

    -- 3. Check target doesn't exist
    local target_exists = redis.call('HEXISTS', new_parent_dir_key, new_name)
    if target_exists == 1 then
        return cjson.encode({ok=false, error="already_exists"})
    end

    -- 4. Get child node
    local child_json = redis.call('GET', child_node_key)
    if not child_json then
        return cjson.encode({ok=false, error="node_not_found", ino=tonumber(dentry_ino)})
    end
    local ok_child, child_node = pcall(cjson.decode, child_json)
    if not ok_child or not child_node or not child_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- 5. Update node parent/name OR link_parents based on nlink
    if child_node.attr.nlink <= 1 then
        -- Single parent: update node directly
        child_node.parent = new_parent_ino
        child_node.name = new_name
    else
        -- Hardlink: update link_parents set
        local members = redis.call('SMEMBERS', link_parents_key)
        local new_members = {}
        local found = false

        for _, member in ipairs(members) do
            -- Find first colon only to handle filenames with colons
            local sep_pos = string.find(member, ":", 1, true)
            if sep_pos and sep_pos > 1 and sep_pos < #member then
                local parent_str = string.sub(member, 1, sep_pos - 1)
                local name_str = string.sub(member, sep_pos + 1)
                local parent_num = tonumber(parent_str)
                if parent_num == old_parent_ino and name_str == old_name then
                    table.insert(new_members, new_parent_ino .. ":" .. new_name)
                    found = true
                else
                    table.insert(new_members, member)
                end
            else
                table.insert(new_members, member)
            end
        end

        if not found then
            return cjson.encode({ok=false, error="link_parent_not_found"})
        end

        -- Replace link_parents set atomically
        redis.call('DEL', link_parents_key)
        for _, member in ipairs(new_members) do
            redis.call('SADD', link_parents_key, member)
        end

        -- Hardlinked files have parent=0, name=""
        child_node.parent = 0
        child_node.name = ""
    end

    -- 6. Update child timestamps
    child_node.attr.mtime = timestamp
    child_node.attr.ctime = timestamp

    -- 7. Remove old dentry and add new dentry
    redis.call('HDEL', old_parent_dir_key, old_name)
    redis.call('HSET', new_parent_dir_key, new_name, dentry_ino)

    -- 8. Save updated child node
    redis.call('SET', child_node_key, cjson.encode(child_node))

    -- 9. Update both parent directory times (but NOT nlink)
    local old_parent_json = redis.call('GET', old_parent_node_key)
    if old_parent_json then
        local ok_op, old_parent_node = pcall(cjson.decode, old_parent_json)
        if ok_op and old_parent_node and old_parent_node.attr then
            old_parent_node.attr.mtime = timestamp
            old_parent_node.attr.ctime = timestamp
            redis.call('SET', old_parent_node_key, cjson.encode(old_parent_node))
        end
    end

    new_parent_node.attr.mtime = timestamp
    new_parent_node.attr.ctime = timestamp
    redis.call('SET', new_parent_node_key, cjson.encode(new_parent_node))

    return cjson.encode({ok=true})
"#;

const RENAME_EXCHANGE_LUA: &str = r#"
    local cjson = cjson

    local old_parent_dir_key = KEYS[1]
    local new_parent_dir_key = KEYS[2]
    local old_node_key = KEYS[3]
    local new_node_key = KEYS[4]
    local old_parent_node_key = KEYS[5]
    local new_parent_node_key = KEYS[6]
    local old_link_parents_key = KEYS[7]
    local new_link_parents_key = KEYS[8]
    local old_name = ARGV[1]
    local new_name = ARGV[2]
    local old_parent_ino = tonumber(ARGV[3])
    local new_parent_ino = tonumber(ARGV[4])
    local timestamp = tonumber(ARGV[5])

    -- 1. Check both entries exist
    local old_dentry_ino = redis.call('HGET', old_parent_dir_key, old_name)
    if not old_dentry_ino then
        return cjson.encode({ok=false, error="internal", msg="Entry '" .. old_name .. "' not found in parent " .. old_parent_ino .. " for exchange"})
    end

    local new_dentry_ino = redis.call('HGET', new_parent_dir_key, new_name)
    if not new_dentry_ino then
        return cjson.encode({ok=false, error="internal", msg="Entry '" .. new_name .. "' not found in parent " .. new_parent_ino .. " for exchange"})
    end

    -- 2. GET both nodes
    local old_node_json = redis.call('GET', old_node_key)
    if not old_node_json then
        return cjson.encode({ok=false, error="corrupt_node"})
    end
    local ok_old, old_node = pcall(cjson.decode, old_node_json)
    if not ok_old or not old_node or not old_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    local new_node_json = redis.call('GET', new_node_key)
    if not new_node_json then
        return cjson.encode({ok=false, error="corrupt_node"})
    end
    local ok_new, new_node = pcall(cjson.decode, new_node_json)
    if not ok_new or not new_node or not new_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- 3. Swap directory entries atomically
    redis.call('HSET', old_parent_dir_key, old_name, new_dentry_ino)
    redis.call('HSET', new_parent_dir_key, new_name, old_dentry_ino)

    -- 4. Update old_node (nlink>1: update link_parents, nlink<=1: update parent/name)
    if old_node.attr.nlink > 1 then
        local old_members = redis.call('SMEMBERS', old_link_parents_key)
        local new_old_members = {}

        for _, member in ipairs(old_members) do
            -- Find first colon only to handle filenames with colons
            local sep_pos = string.find(member, ":", 1, true)
            if sep_pos and sep_pos > 1 and sep_pos < #member then
                local parent_str = string.sub(member, 1, sep_pos - 1)
                local name_str = string.sub(member, sep_pos + 1)
                local parent_num = tonumber(parent_str)
                if parent_num == old_parent_ino and name_str == old_name then
                    table.insert(new_old_members, new_parent_ino .. ":" .. new_name)
                else
                    table.insert(new_old_members, member)
                end
            else
                table.insert(new_old_members, member)
            end
        end

        redis.call('DEL', old_link_parents_key)
        for _, member in ipairs(new_old_members) do
            redis.call('SADD', old_link_parents_key, member)
        end

        old_node.parent = 0
        old_node.name = ""
    else
        old_node.parent = new_parent_ino
        old_node.name = new_name
    end

    -- 5. Update new_node (nlink>1: update link_parents, nlink<=1: update parent/name)
    if new_node.attr.nlink > 1 then
        local new_members = redis.call('SMEMBERS', new_link_parents_key)
        local new_new_members = {}

        for _, member in ipairs(new_members) do
            -- Find first colon only to handle filenames with colons
            local sep_pos = string.find(member, ":", 1, true)
            if sep_pos and sep_pos > 1 and sep_pos < #member then
                local parent_str = string.sub(member, 1, sep_pos - 1)
                local name_str = string.sub(member, sep_pos + 1)
                local parent_num = tonumber(parent_str)
                if parent_num == new_parent_ino and name_str == new_name then
                    table.insert(new_new_members, old_parent_ino .. ":" .. old_name)
                else
                    table.insert(new_new_members, member)
                end
            else
                table.insert(new_new_members, member)
            end
        end

        redis.call('DEL', new_link_parents_key)
        for _, member in ipairs(new_new_members) do
            redis.call('SADD', new_link_parents_key, member)
        end

        new_node.parent = 0
        new_node.name = ""
    else
        new_node.parent = old_parent_ino
        new_node.name = old_name
    end

    -- 6. Update timestamps for both nodes
    old_node.attr.mtime = timestamp
    old_node.attr.ctime = timestamp
    new_node.attr.mtime = timestamp
    new_node.attr.ctime = timestamp

    -- 7. SET both nodes
    redis.call('SET', old_node_key, cjson.encode(old_node))
    redis.call('SET', new_node_key, cjson.encode(new_node))

    -- 8. Update parent directory timestamps
    local old_parent_json = redis.call('GET', old_parent_node_key)
    if old_parent_json then
        local ok_op, old_parent_node = pcall(cjson.decode, old_parent_json)
        if ok_op and old_parent_node and old_parent_node.attr then
            old_parent_node.attr.mtime = timestamp
            old_parent_node.attr.ctime = timestamp
            redis.call('SET', old_parent_node_key, cjson.encode(old_parent_node))
        end
    end

    local new_parent_json = redis.call('GET', new_parent_node_key)
    if new_parent_json then
        local ok_np, new_parent_node = pcall(cjson.decode, new_parent_json)
        if ok_np and new_parent_node and new_parent_node.attr then
            new_parent_node.attr.mtime = timestamp
            new_parent_node.attr.ctime = timestamp
            redis.call('SET', new_parent_node_key, cjson.encode(new_parent_node))
        end
    end

    return cjson.encode({ok=true})
"#;

/// Response structure for Lua script results
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct LuaResponse {
    ok: bool,
    #[serde(default)]
    ino: Option<i64>,
    #[serde(default)]
    updated: Option<bool>,
    #[serde(default)]
    nlink: Option<u32>,
    #[serde(default)]
    deleted: Option<bool>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    attr: Option<serde_json::Value>,
    #[serde(default)]
    msg: Option<String>, // For Internal error details
}

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

    fn locked_key(sid: Uuid) -> String {
        format!("{}{}", LOCKED_KEY, sid)
    }

    fn link_parent_key(ino: i64) -> String {
        format!("{LINK_PARENT_KEY_PREFIX}{ino}")
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

    async fn load_link_parents(&self, ino: i64) -> Result<Vec<(i64, String)>, MetaError> {
        let mut conn = self.conn.clone();
        let members: Vec<String> = conn
            .smembers(Self::link_parent_key(ino))
            .await
            .map_err(redis_err)?;

        let mut out = Vec::with_capacity(members.len());
        for m in members {
            let Some((p, name)) = m.split_once(':') else {
                continue;
            };
            if let Ok(parent) = p.parse::<i64>() {
                out.push((parent, name.to_string()));
            }
        }

        out.sort();
        out.dedup();
        Ok(out)
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
        // Step 1: Get parent node for setgid check
        let parent_node = self
            .get_node(parent)
            .await?
            .ok_or(MetaError::ParentNotFound(parent))?;
        if parent_node.kind != NodeKind::Dir {
            return Err(MetaError::NotDirectory(parent));
        }

        // Step 2: Construct Redis keys
        let parent_dir_key = self.dir_key(parent);
        let parent_node_key = self.node_key(parent);
        let counter_key = COUNTER_INODE_KEY;

        // Step 3: Prepare ARGV parameters
        let kind_str = match kind {
            FileType::File => "File",
            FileType::Dir => "Dir",
            FileType::Symlink => "Symlink",
        };
        let default_mode = if kind == FileType::Dir {
            0o040755
        } else if kind == FileType::Symlink {
            0o120777
        } else {
            0o100644
        };
        let now = current_time();
        let parent_gid = parent_node.attr.gid;
        let parent_has_setgid = if (parent_node.attr.mode & 0o2000) != 0 {
            1
        } else {
            0
        };

        // Step 4: Invoke Lua script atomically
        let script = redis::Script::new(CREATE_ENTRY_LUA);
        let result: String = script
            .key(&parent_dir_key) // KEYS[1]
            .key(&parent_node_key) // KEYS[2]
            .key(counter_key) // KEYS[3]
            .arg(&name) // ARGV[1]
            .arg(kind_str) // ARGV[2]
            .arg(now) // ARGV[3]
            .arg(parent) // ARGV[4]
            .arg(default_mode) // ARGV[5]
            .arg(0u32) // ARGV[6] - uid (default 0)
            .arg(0u32) // ARGV[7] - gid (default 0)
            .arg(parent_gid) // ARGV[8]
            .arg(parent_has_setgid) // ARGV[9]
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        // Step 5: Parse response and map errors
        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        match response.error.as_deref() {
            Some("parent_not_found") => Err(MetaError::ParentNotFound(parent)),
            Some("parent_not_directory") => Err(MetaError::NotDirectory(parent)),
            Some("already_exists") => Err(MetaError::AlreadyExists { parent, name }),
            Some("corrupt_node") => Err(MetaError::Internal("corrupt parent node".into())),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => {
                let new_ino = response
                    .ino
                    .ok_or_else(|| MetaError::Internal("missing ino in response".into()))?;
                Ok(new_ino)
            }
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
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
                        let _: () = redis::pipe()
                            .atomic()
                            .hdel(&plock_key, &field)
                            .srem(Self::locked_key(*sid), inode)
                            .exec_async(&mut conn)
                            .await
                            .map_err(redis_err)?;
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

                let _: () = redis::pipe()
                    .atomic()
                    .hset(&plock_key, &field, new_json)
                    .sadd(Self::locked_key(*sid), inode)
                    .exec_async(&mut conn)
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
            let data = crate::meta::serialization::serialize_meta(slice)?;
            pipe.cmd("RPUSH").arg(&key).arg(data);
        }
        pipe.query_async::<()>(&mut conn).await.map_err(redis_err)?;
        Ok(())
    }

    async fn shutdown_session_by_id(&self, session_id: Uuid) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let session_id_string = session_id.to_string();

        let locked_key = Self::locked_key(session_id);

        let locked_files: Vec<i64> = conn
            .smembers(&locked_key)
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to get locked files: {}", e)))?;
        let mut pipe = redis::pipe();
        let pipe_atomic = pipe.atomic();
        for file in locked_files {
            let owners: Vec<String> = conn
                .hkeys(self.plock_key(file))
                .await
                .map_err(|e| MetaError::Internal(e.to_string()))?;

            let mut fields: Vec<String> = Vec::new();
            for owner in owners {
                let owner_sid = owner.split(':').next().ok_or_else(|| {
                    MetaError::Internal(format!(
                        "Malformed lock owner format in Redis: '{}'",
                        owner
                    ))
                })?;
                if owner_sid == session_id_string {
                    fields.push(owner);
                }
            }

            if !fields.is_empty() {
                pipe_atomic.hdel(self.plock_key(file), &fields);
            }

            pipe_atomic.srem(&locked_key, file);
        }

        pipe_atomic
            .zrem(ALL_SESSIONS_KEY, &session_id_string)
            .hdel(SESSION_INFOS_KEY, &session_id_string)
            .exec_async(&mut conn)
            .await
            .map_err(|err| MetaError::Internal(err.to_string()))?;

        Ok(())
    }

    async fn prune_slices_for_truncate(
        &self,
        ino: i64,
        new_size: u64,
        old_size: u64,
        chunk_size: u64,
    ) -> Result<(), MetaError> {
        apply_truncate_plan(
            new_size,
            old_size,
            chunk_size,
            |cutoff_chunk, cutoff_offset| async move {
                let chunk_id = self.chunk_id(ino, cutoff_chunk);
                let mut slices = self.get_slices(chunk_id).await?;
                trim_slices_in_place(&mut slices, cutoff_offset);
                self.rewrite_slices(chunk_id, &slices).await?;
                Ok(())
            },
            |start, end| async move {
                for idx in start..end {
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
            },
        )
        .await
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

    async fn refresh_session(
        session_id: Uuid,
        mut conn: ConnectionManager,
    ) -> Result<(), MetaError> {
        let session_id_string = session_id.to_string();
        let expire = (Utc::now() + chrono::Duration::minutes(5)).timestamp_millis();
        redis::Cmd::zadd(ALL_SESSIONS_KEY, session_id_string, expire)
            .exec_async(&mut conn)
            .await
            .map_err(|err| MetaError::Internal(err.to_string()))?;
        Ok(())
    }

    async fn life_cycle(token: CancellationToken, session_id: Uuid, conn: ConnectionManager) {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            select! {
                _ = interval.tick() => {
                    // refresh session
                    match Self::refresh_session(session_id, conn.clone()).await {
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
}

#[async_trait]
impl MetaStore for RedisMetaStore {
    fn name(&self) -> &'static str {
        "redis-meta-store"
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        Ok(self.get_node(ino).await?.map(|n| n.as_file_attr()))
    }

    /// Batch stat implementation using Redis MGET for optimal performance
    #[tracing::instrument(
        level = "trace",
        skip(self, inodes),
        fields(inode_count = inodes.len())
    )]
    async fn batch_stat(&self, inodes: &[i64]) -> Result<Vec<Option<FileAttr>>, MetaError> {
        if inodes.is_empty() {
            return Ok(Vec::new());
        }

        // Build keys for all inodes
        let keys: Vec<String> = inodes.iter().map(|&ino| self.node_key(ino)).collect();

        // Use MGET to fetch all nodes in a single round trip
        let mut conn = self.conn.clone();
        let values: Vec<Option<String>> = conn.get(&keys).await.map_err(redis_err)?;

        // Parse results and convert to FileAttr
        let mut results = Vec::with_capacity(inodes.len());
        for value in values {
            match value {
                Some(json_str) => match serde_json::from_str::<StoredNode>(&json_str) {
                    Ok(node) => results.push(Some(node.as_file_attr())),
                    Err(e) => {
                        error!("Failed to deserialize node from Redis: {}", e);
                        results.push(None);
                    }
                },
                None => results.push(None),
            }
        }

        Ok(results)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        self.directory_child(parent, name).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(path))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_entry(parent, name, FileType::Dir).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        // Step 1: Lookup child ino first (preserves NotFound(parent) behavior)
        let Some(child) = self.lookup(parent, name).await? else {
            return Err(MetaError::NotFound(parent));
        };

        // Step 2: Construct Redis keys
        let parent_dir_key = self.dir_key(parent);
        let child_node_key = self.node_key(child);
        let parent_node_key = self.node_key(parent);
        let child_dir_key = self.dir_key(child);
        let now = current_time();

        // Step 3: Invoke Lua script atomically
        let script = redis::Script::new(RMDIR_LUA);
        let result: String = script
            .key(&parent_dir_key)
            .key(&child_node_key)
            .key(&parent_node_key)
            .key(&child_dir_key)
            .arg(name)
            .arg(child)
            .arg(parent)
            .arg(now)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        // Step 4: Parse response and map errors
        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        match response.error.as_deref() {
            Some("not_found") => Err(MetaError::NotFound(response.ino.unwrap_or(parent))),
            Some("node_not_found") => Err(MetaError::NotFound(response.ino.unwrap_or(child))),
            Some("not_directory") => Err(MetaError::NotDirectory(response.ino.unwrap_or(child))),
            Some("dir_not_empty") => {
                Err(MetaError::DirectoryNotEmpty(response.ino.unwrap_or(child)))
            }
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => Ok(()),
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        self.create_entry(parent, name, FileType::File).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, parent, name))]
    async fn link(&self, ino: i64, parent: i64, name: &str) -> Result<FileAttr, MetaError> {
        self.ensure_parent_dir(parent).await?;

        let node_key = self.node_key(ino);
        let lp_key = Self::link_parent_key(ino);
        let dir_key = self.dir_key(parent);
        let now = current_time();

        let script = redis::Script::new(LINK_LUA);
        let result: String = script
            .key(&node_key)
            .key(&lp_key)
            .key(&dir_key)
            .arg(parent)
            .arg(name)
            .arg(now)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        match response.error.as_deref() {
            Some("node_not_found") => Err(MetaError::NotFound(ino)),
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some("already_exists") => Err(MetaError::AlreadyExists {
                parent,
                name: name.to_string(),
            }),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => {
                let attr_json = response
                    .attr
                    .ok_or_else(|| MetaError::Internal("missing attr in link response".into()))?;
                let stored_attr: StoredAttr = serde_json::from_value(attr_json)
                    .map_err(|e| MetaError::Internal(format!("attr parse error: {e}")))?;

                self.bump_dir_times(parent, now).await?;
                Ok(stored_attr.to_file_attr(ino, FileType::File))
            }
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name))]
    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        let Some(child) = self.lookup(parent, name).await? else {
            return Err(MetaError::NotFound(parent));
        };

        let node = self
            .get_node(child)
            .await?
            .ok_or(MetaError::NotFound(child))?;
        if node.kind != NodeKind::File {
            return Err(MetaError::NotSupported(format!("{child} is not a file")));
        }

        let node_key = self.node_key(child);
        let lp_key = Self::link_parent_key(child);
        let dir_key = self.dir_key(parent);
        let now = current_time();

        let script = redis::Script::new(UNLINK_LUA);
        let result: String = script
            .key(&node_key)
            .key(&lp_key)
            .key(&dir_key)
            .arg(parent)
            .arg(name)
            .arg(now)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        if !response.ok {
            let err = response
                .error
                .unwrap_or_else(|| "unknown error".to_string());
            return Err(MetaError::Internal(format!("Lua error: {err}")));
        }

        let nlink = response.nlink.unwrap_or(0);
        let deleted = response.deleted.unwrap_or(false);

        if deleted {
            let mut node_mut = node;
            self.mark_deleted(child, &mut node_mut).await?;
        } else if nlink <= 1 {
            let mut link_parents = self.load_link_parents(child).await?;
            link_parents.retain(|(p, n)| !(*p == parent && n.as_str() == name));

            if let Some(remaining) = link_parents.into_iter().next() {
                let mut node_mut = node;
                node_mut.parent = remaining.0;
                node_mut.name = remaining.1;
                node_mut.attr.nlink = nlink;
                node_mut.attr.ctime = now;

                let key = Self::link_parent_key(child);
                let data = serde_json::to_vec(&node_mut)
                    .map_err(|e| MetaError::Internal(e.to_string()))?;

                let mut conn = self.conn.clone();
                let _: () = redis::pipe()
                    .atomic()
                    .del(key)
                    .set(self.node_key(node_mut.ino), data)
                    .query_async(&mut conn)
                    .await
                    .map_err(redis_err)?;
            }
        }

        self.bump_dir_times(parent, now).await?;
        Ok(())
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
        // Self-rename optimization: no-op if same location
        if old_parent == new_parent && old_name == new_name {
            return Ok(());
        }

        // Step 1: Lookup source dentry to get child ino (preserves NotFound(old_parent) error)
        let Some(child) = self.lookup(old_parent, old_name).await? else {
            return Err(MetaError::NotFound(old_parent));
        };

        // Step 2: Construct Redis keys
        let old_parent_dir_key = self.dir_key(old_parent);
        let new_parent_dir_key = self.dir_key(new_parent);
        let child_node_key = self.node_key(child);
        let old_parent_node_key = self.node_key(old_parent);
        let new_parent_node_key = self.node_key(new_parent);
        let link_parents_key = Self::link_parent_key(child);
        let now = current_time();

        // Step 3: Invoke Lua script atomically
        let script = redis::Script::new(RENAME_LUA);
        let result: String = script
            .key(&old_parent_dir_key) // KEYS[1]
            .key(&new_parent_dir_key) // KEYS[2]
            .key(&child_node_key) // KEYS[3]
            .key(&old_parent_node_key) // KEYS[4]
            .key(&new_parent_node_key) // KEYS[5]
            .key(&link_parents_key) // KEYS[6]
            .arg(old_name) // ARGV[1]
            .arg(&new_name) // ARGV[2]
            .arg(old_parent) // ARGV[3]
            .arg(new_parent) // ARGV[4]
            .arg(now) // ARGV[5]
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        // Step 4: Parse response and map errors
        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        match response.error.as_deref() {
            Some("not_found") => Err(MetaError::NotFound(response.ino.unwrap_or(old_parent))),
            Some("parent_not_found") => Err(MetaError::ParentNotFound(new_parent)),
            Some("parent_not_directory") => Err(MetaError::NotDirectory(new_parent)),
            Some("already_exists") => Err(MetaError::AlreadyExists {
                parent: new_parent,
                name: new_name,
            }),
            Some("node_not_found") => Err(MetaError::NotFound(child)),
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some("link_parent_not_found") => Err(MetaError::Internal(format!(
                "expected link parent binding {old_parent}/{old_name} for inode {child}"
            ))),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => Ok(()),
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
    }

    async fn rename_exchange(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: &str,
    ) -> Result<(), MetaError> {
        if old_parent == new_parent && old_name == new_name {
            return Ok(());
        }

        let Some(old_ino) = self.lookup(old_parent, old_name).await? else {
            return Err(MetaError::Internal(format!(
                "Entry '{}' not found in parent {} for exchange",
                old_name, old_parent
            )));
        };

        let Some(new_ino) = self.lookup(new_parent, new_name).await? else {
            return Err(MetaError::Internal(format!(
                "Entry '{}' not found in parent {} for exchange",
                new_name, new_parent
            )));
        };

        let old_parent_dir_key = self.dir_key(old_parent);
        let new_parent_dir_key = self.dir_key(new_parent);
        let old_node_key = self.node_key(old_ino);
        let new_node_key = self.node_key(new_ino);
        let old_parent_node_key = self.node_key(old_parent);
        let new_parent_node_key = self.node_key(new_parent);
        let old_link_parents_key = Self::link_parent_key(old_ino);
        let new_link_parents_key = Self::link_parent_key(new_ino);
        let now = current_time();

        let script = redis::Script::new(RENAME_EXCHANGE_LUA);
        let result: String = script
            .key(&old_parent_dir_key)
            .key(&new_parent_dir_key)
            .key(&old_node_key)
            .key(&new_node_key)
            .key(&old_parent_node_key)
            .key(&new_parent_node_key)
            .key(&old_link_parents_key)
            .key(&new_link_parents_key)
            .arg(old_name)
            .arg(new_name)
            .arg(old_parent)
            .arg(new_parent)
            .arg(now)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Failed to parse Lua response: {e}")))?;
        match response.error.as_deref() {
            Some("internal") => {
                let msg = response.msg.unwrap_or_else(|| "unknown error".to_string());
                Err(MetaError::Internal(msg))
            }
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => Ok(()),
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
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
        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
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

        self.save_node(&node).await?;
        Ok(node.attr.to_file_attr(node.ino, node.kind.into()))
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        let now = current_time();
        node.attr.size = size;
        node.attr.mtime = now;
        node.attr.ctime = now;
        self.save_node(&node).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size))]
    async fn extend_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        let script = redis::Script::new(EXTEND_FILE_SIZE_LUA);
        let node_key = self.node_key(ino);
        let now = current_time();

        let result: String = script
            .key(&node_key)
            .arg(size)
            .arg(now)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        match response.error.as_deref() {
            Some("node_not_found") => Err(MetaError::NotFound(ino)),
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => Ok(()),
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino, size, chunk_size))]
    async fn truncate(&self, ino: i64, size: u64, chunk_size: u64) -> Result<(), MetaError> {
        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        let old_size = node.attr.size;
        let now = current_time();
        self.prune_slices_for_truncate(ino, size, old_size, chunk_size)
            .await?;
        node.attr.size = size;
        node.attr.mtime = now;
        node.attr.ctime = now;
        self.save_node(&node).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn get_names(&self, ino: i64) -> Result<Vec<(Option<i64>, String)>, MetaError> {
        let Some(node) = self.get_node(ino).await? else {
            return Ok(vec![]);
        };

        if node.ino == ROOT_INODE {
            return Ok(vec![(None, "/".to_string())]);
        }

        if node.deleted || node.attr.nlink == 0 {
            return Ok(vec![]);
        }

        if node.kind == NodeKind::Dir || node.attr.nlink <= 1 {
            return Ok(vec![(Some(node.parent), node.name)]);
        }

        let link_parents = self.load_link_parents(ino).await?;
        let mut out = Vec::with_capacity(link_parents.len());
        for (p, n) in link_parents {
            out.push((Some(p), n));
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn get_paths(&self, ino: i64) -> Result<Vec<String>, MetaError> {
        if ino == ROOT_INODE {
            return Ok(vec!["/".to_string()]);
        }

        let names = self.get_names(ino).await?;
        let mut out = Vec::with_capacity(names.len());

        for (parent_opt, name) in names {
            let Some(parent) = parent_opt else {
                continue;
            };

            let mut segments = vec![name];
            let mut current = self.get_node(parent).await?;
            while let Some(node) = current {
                if node.ino == ROOT_INODE {
                    segments.push(String::new());
                    break;
                }
                segments.push(node.name.clone());
                current = self.get_node(node.parent).await?;
            }

            if segments.is_empty() {
                continue;
            }

            segments.reverse();
            let path = if segments.len() == 1 {
                "/".to_string()
            } else {
                segments.join("/")
            };
            out.push(path);
        }

        out.sort();
        out.dedup();
        Ok(out)
    }

    fn root_ino(&self) -> i64 {
        ROOT_INODE
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn initialize(&self) -> Result<(), MetaError> {
        self.init_root_directory().await
    }

    #[tracing::instrument(level = "trace", skip(self))]
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

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn remove_file_metadata(&self, ino: i64) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let _: () = conn
            .hdel(self.deleted_set_key(), ino.to_string())
            .await
            .map_err(redis_err)?;
        self.delete_node(ino).await
    }

    #[tracing::instrument(
        level = "trace",
        skip(self),
        fields(chunk_id, slice_count = tracing::field::Empty)
    )]
    async fn get_slices(&self, chunk_id: u64) -> Result<Vec<SliceDesc>, MetaError> {
        let mut conn = self.conn.clone();
        let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
            .arg(self.chunk_key(chunk_id))
            .arg(0)
            .arg(-1)
            .query_async(&mut conn)
            .instrument(tracing::trace_span!("get_slices.redis_lrange", chunk_id))
            .await
            .map_err(redis_err)?;
        let mut slices = Vec::new();
        for entry in raw {
            let desc: SliceDesc = crate::meta::serialization::deserialize_meta(&entry)?;
            slices.push(desc);
        }
        tracing::Span::current().record("slice_count", slices.len());
        Ok(slices)
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, slice),
        fields(chunk_id, slice_id = slice.slice_id, offset = slice.offset, len = slice.length)
    )]
    async fn append_slice(&self, chunk_id: u64, slice: SliceDesc) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let data = crate::meta::serialization::serialize_meta(&slice)?;
        let _: () = redis::cmd("RPUSH")
            .arg(self.chunk_key(chunk_id))
            .arg(data)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;
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
        self.append_slice(chunk_id, slice).await?;
        self.extend_file_size(ino, new_size).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(key))]
    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.alloc_id(key).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(pid = session_info.process_id))]
    async fn start_session(
        &self,
        session_info: SessionInfo,
        token: CancellationToken,
    ) -> Result<Session, MetaError> {
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
        self.set_sid(session_id)?;

        tokio::spawn(Self::life_cycle(
            token.clone(),
            session_id,
            self.conn.clone(),
        ));

        Ok(session)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn shutdown_session(&self) -> Result<(), MetaError> {
        let session_id = self.get_sid()?;
        self.shutdown_session_by_id(*session_id).await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
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
            self.shutdown_session_by_id(session_id).await?;
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(lock_name = ?lock_name))]
    async fn get_global_lock(&self, lock_name: LockName) -> bool {
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
    #[tracing::instrument(level = "trace", skip(self, query), fields(inode, owner = query.owner))]
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
            .ok_or_else(|| MetaError::Internal("sid not set".to_string()))?;

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
    fn as_file_attr(&self) -> FileAttr {
        self.attr.to_file_attr(self.ino, self.kind.into())
    }
}

/// Deserializer that accepts both integer and floating-point numbers.
/// Redis cjson encodes large integers (like epoch millis) as scientific notation
/// floats (e.g., 1.7698324007242e+18), which serde_json rejects for i64 fields.
fn deserialize_i64_from_number<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{Error, Visitor};

    struct I64OrFloatVisitor;

    impl<'de> Visitor<'de> for I64OrFloatVisitor {
        type Value = i64;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer or floating-point number")
        }

        fn visit_i64<E: Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_u64<E: Error>(self, v: u64) -> Result<Self::Value, E> {
            i64::try_from(v).map_err(|_| E::custom("u64 out of i64 range"))
        }

        fn visit_f64<E: Error>(self, v: f64) -> Result<Self::Value, E> {
            // Validate finite value
            if !v.is_finite() {
                return Err(E::custom("non-finite float for i64 field"));
            }

            // Truncate and validate range
            let truncated = v.trunc();
            if truncated < i64::MIN as f64 || truncated > i64::MAX as f64 {
                return Err(E::custom("float out of i64 range"));
            }

            Ok(truncated as i64)
        }
    }

    deserializer.deserialize_any(I64OrFloatVisitor)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct StoredAttr {
    size: u64,
    mode: u32,
    uid: u32,
    gid: u32,
    #[serde(deserialize_with = "deserialize_i64_from_number")]
    atime: i64,
    #[serde(deserialize_with = "deserialize_i64_from_number")]
    mtime: i64,
    #[serde(deserialize_with = "deserialize_i64_from_number")]
    ctime: i64,
    nlink: u32,
}

impl StoredAttr {
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
    use serial_test::serial;
    use std::sync::Arc;
    use tokio::time;
    use uuid::Uuid;

    async fn cleanup_test_data() -> Result<(), MetaError> {
        let url = "redis://127.0.0.1:6379/0";
        let client = redis::Client::open(url)
            .map_err(|e| MetaError::Config(format!("Failed to create Redis client: {}", e)))?;
        let mut conn = client
            .get_multiplexed_async_connection()
            .await
            .map_err(|e| MetaError::Config(format!("Failed to connect to Redis: {}", e)))?;

        let _: () = redis::cmd("FLUSHDB")
            .query_async(&mut conn)
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to flush Redis DB: {}", e)))?;

        let config = test_config();
        let _store = RedisMetaStore::from_config(config.clone())
            .await
            .map_err(|e| MetaError::Internal(format!("Failed to reinitialize root: {}", e)))?;

        Ok(())
    }

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
        if let Err(e) = cleanup_test_data().await {
            eprintln!("Failed to cleanup Redis test data: {}", e);
        }

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

    #[serial]
    #[tokio::test]
    #[ignore]
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

    #[serial]
    #[tokio::test]
    #[ignore]
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

    #[serial]
    #[tokio::test]
    #[ignore]
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
            owner,
            lock_type: FileLockType::Read,
            range: FileLockRange { start: 0, end: 100 },
        };

        let lock_info = store.get_plock(file_ino, &query).await.unwrap();
        assert_eq!(lock_info.lock_type, FileLockType::UnLock);
    }

    #[serial]
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

    #[serial]
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

    #[serial]
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

    #[serial]
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
            .create_file(parent, "test_none_overlapping_locks_file.txt".to_string())
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

    #[serial]
    #[tokio::test]
    #[ignore]
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

    #[serial]
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

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_extend_file_size_lua_concurrent() {
        use crate::meta::MetaStore;

        let store = new_test_store().await;
        let root = store.root_ino();
        let ino = store
            .create_file(root, "test.txt".to_string())
            .await
            .unwrap();

        let store1 = std::sync::Arc::new(store);
        let store2 = store1.clone();
        let store3 = store1.clone();
        let store4 = store1.clone();

        let h1 = tokio::spawn(async move { store2.extend_file_size(ino, 1000).await });
        let h2 = tokio::spawn(async move { store3.extend_file_size(ino, 2000).await });
        let h3 = tokio::spawn(async move { store4.extend_file_size(ino, 1500).await });

        h1.await.unwrap().unwrap();
        h2.await.unwrap().unwrap();
        h3.await.unwrap().unwrap();

        let attr = store1.stat(ino).await.unwrap().unwrap();
        assert_eq!(attr.size, 2000);
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_extend_file_size_lua_idempotent() {
        use crate::meta::MetaStore;

        let store = new_test_store().await;
        let root = store.root_ino();
        let ino = store
            .create_file(root, "test.txt".to_string())
            .await
            .unwrap();

        store.extend_file_size(ino, 1000).await.unwrap();
        let attr1 = store.stat(ino).await.unwrap().unwrap();
        assert_eq!(attr1.size, 1000);

        store.extend_file_size(ino, 500).await.unwrap();
        let attr2 = store.stat(ino).await.unwrap().unwrap();
        assert_eq!(attr2.size, 1000);

        store.extend_file_size(ino, 1000).await.unwrap();
        let attr3 = store.stat(ino).await.unwrap().unwrap();
        assert_eq!(attr3.size, 1000);
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_extend_file_size_lua_missing_node() {
        use crate::meta::MetaStore;

        let store = new_test_store().await;
        let result = store.extend_file_size(99999, 1000).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::NotFound(ino) => assert_eq!(ino, 99999),
            other => panic!("expected NotFound error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_link_unlink_lua_atomicity() {
        use crate::meta::MetaStore;

        let store = new_test_store().await;
        let root = store.root_ino();

        let dir_a = store.mkdir(root, "a".to_string()).await.unwrap();
        let dir_b = store.mkdir(root, "b".to_string()).await.unwrap();

        let ino = store.create_file(dir_a, "x".to_string()).await.unwrap();

        let attr1 = store.link(ino, dir_b, "y").await.unwrap();
        assert_eq!(attr1.nlink, 2);

        assert_eq!(store.lookup(dir_a, "x").await.unwrap(), Some(ino));
        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

        let result = store.link(ino, dir_b, "y").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::AlreadyExists { parent, name } => {
                assert_eq!(parent, dir_b);
                assert_eq!(name, "y");
            }
            other => panic!("expected AlreadyExists error, got {:?}", other),
        }

        store.unlink(dir_a, "x").await.unwrap();
        assert_eq!(store.lookup(dir_a, "x").await.unwrap(), None);
        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), Some(ino));

        let attr2 = store.stat(ino).await.unwrap().unwrap();
        assert_eq!(attr2.nlink, 1);

        store.unlink(dir_b, "y").await.unwrap();
        assert_eq!(store.lookup(dir_b, "y").await.unwrap(), None);

        let deleted = store.get_deleted_files().await.unwrap();
        assert!(deleted.contains(&ino));
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rmdir_lua_concurrent() {
        let store = Arc::new(new_test_store().await);
        let root = store.root_ino();

        let _test_dir = store.mkdir(root, "testdir".to_string()).await.unwrap();

        let store1 = store.clone();
        let store2 = store.clone();
        let store3 = store.clone();
        let store4 = store.clone();

        let h1 = tokio::spawn(async move { store1.rmdir(root, "testdir").await });
        let h2 = tokio::spawn(async move { store2.rmdir(root, "testdir").await });
        let h3 = tokio::spawn(async move { store3.rmdir(root, "testdir").await });
        let h4 = tokio::spawn(async move { store4.rmdir(root, "testdir").await });

        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        let r3 = h3.await.unwrap();
        let r4 = h4.await.unwrap();

        let results = [r1, r2, r3, r4];
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            success_count, 1,
            "Exactly one rmdir should succeed, got {} successes",
            success_count
        );

        let not_found_count = results
            .iter()
            .filter(|r| matches!(r, Err(MetaError::NotFound(ino)) if ino == &root))
            .count();
        assert_eq!(
            not_found_count, 3,
            "Three rmdir should return NotFound(parent), got {}",
            not_found_count
        );

        assert_eq!(store.lookup(root, "testdir").await.unwrap(), None);
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rmdir_lua_not_empty() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let parent_dir = store.mkdir(root, "parent".to_string()).await.unwrap();
        let _child_dir = store.mkdir(parent_dir, "child".to_string()).await.unwrap();

        let result = store.rmdir(root, "parent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::DirectoryNotEmpty(ino) => assert_eq!(ino, parent_dir),
            other => panic!("expected DirectoryNotEmpty error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rmdir_lua_not_found() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let result = store.rmdir(root, "nonexistent").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::NotFound(ino) => assert_eq!(ino, root),
            other => panic!("expected NotFound(parent) error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rmdir_lua_not_directory() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let file_ino = store
            .create_file(root, "file.txt".to_string())
            .await
            .unwrap();

        let result = store.rmdir(root, "file.txt").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::NotDirectory(ino) => assert_eq!(ino, file_ino),
            other => panic!("expected NotDirectory error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_create_entry_lua_concurrent() {
        let store = Arc::new(new_test_store().await);
        let root = store.root_ino();

        let store1 = store.clone();
        let store2 = store.clone();
        let store3 = store.clone();
        let store4 = store.clone();

        let h1 = tokio::spawn(async move { store1.mkdir(root, "newdir".to_string()).await });
        let h2 = tokio::spawn(async move { store2.mkdir(root, "newdir".to_string()).await });
        let h3 = tokio::spawn(async move { store3.mkdir(root, "newdir".to_string()).await });
        let h4 = tokio::spawn(async move { store4.mkdir(root, "newdir".to_string()).await });

        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        let r3 = h3.await.unwrap();
        let r4 = h4.await.unwrap();

        let results = [r1, r2, r3, r4];
        let success_count = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            success_count, 1,
            "Exactly one mkdir should succeed, got {} successes",
            success_count
        );

        let already_exists_count = results
            .iter()
            .filter(|r| matches!(r, Err(MetaError::AlreadyExists { parent, name }) if parent == &root && name == "newdir"))
            .count();
        assert_eq!(
            already_exists_count, 3,
            "Three mkdir should return AlreadyExists, got {}",
            already_exists_count
        );

        let ino = store.lookup(root, "newdir").await.unwrap();
        assert!(ino.is_some());
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_create_entry_lua_already_exists() {
        let store = new_test_store().await;
        let root = store.root_ino();

        store.mkdir(root, "existing".to_string()).await.unwrap();

        let result = store.mkdir(root, "existing".to_string()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::AlreadyExists { parent, name } => {
                assert_eq!(parent, root);
                assert_eq!(name, "existing");
            }
            other => panic!("expected AlreadyExists error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_create_entry_lua_parent_not_found() {
        let store = new_test_store().await;

        let result = store.mkdir(999999, "newdir".to_string()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::ParentNotFound(ino) => assert_eq!(ino, 999999),
            other => panic!("expected ParentNotFound error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_create_entry_lua_parent_not_directory() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let file_ino = store
            .create_file(root, "file.txt".to_string())
            .await
            .unwrap();

        let result = store.mkdir(file_ino, "newdir".to_string()).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::NotDirectory(ino) => assert_eq!(ino, file_ino),
            other => panic!("expected NotDirectory error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_lua_concurrent() {
        let store = Arc::new(new_test_store().await);
        let root = store.root_ino();

        let file_ino = store
            .create_file(root, "file.txt".to_string())
            .await
            .unwrap();
        store.mkdir(root, "dir1".to_string()).await.unwrap();
        store.mkdir(root, "dir2".to_string()).await.unwrap();
        store.mkdir(root, "dir3".to_string()).await.unwrap();
        store.mkdir(root, "dir4".to_string()).await.unwrap();

        let store1 = store.clone();
        let store2 = store.clone();
        let store3 = store.clone();
        let store4 = store.clone();

        let h1 = tokio::spawn(async move {
            store1
                .rename(root, "file.txt", root, "moved1.txt".to_string())
                .await
        });
        let h2 = tokio::spawn(async move {
            store2
                .rename(root, "file.txt", root, "moved2.txt".to_string())
                .await
        });
        let h3 = tokio::spawn(async move {
            store3
                .rename(root, "file.txt", root, "moved3.txt".to_string())
                .await
        });
        let h4 = tokio::spawn(async move {
            store4
                .rename(root, "file.txt", root, "moved4.txt".to_string())
                .await
        });

        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        let r3 = h3.await.unwrap();
        let r4 = h4.await.unwrap();

        let success_count = [&r1, &r2, &r3, &r4].iter().filter(|r| r.is_ok()).count();
        assert_eq!(success_count, 1, "exactly one rename should succeed");

        let not_found_count = [&r1, &r2, &r3, &r4]
            .iter()
            .filter(|r| matches!(r, Err(MetaError::NotFound(ino)) if *ino == root))
            .count();
        assert_eq!(
            not_found_count, 3,
            "three renames should return NotFound(parent)"
        );

        let final_node = store.get_node(file_ino).await.unwrap().unwrap();
        assert!(
            final_node.name.starts_with("moved") && final_node.name.ends_with(".txt"),
            "file should be renamed to one of the target names"
        );
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_lua_source_not_found() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let result = store
            .rename(root, "nonexistent.txt", root, "moved.txt".to_string())
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::NotFound(ino) => assert_eq!(ino, root),
            other => panic!("expected NotFound(parent) error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_lua_target_exists() {
        let store = new_test_store().await;
        let root = store.root_ino();

        store
            .create_file(root, "file1.txt".to_string())
            .await
            .unwrap();
        store
            .create_file(root, "file2.txt".to_string())
            .await
            .unwrap();

        let result = store
            .rename(root, "file1.txt", root, "file2.txt".to_string())
            .await;
        assert!(result.is_err());
        match result.unwrap_err() {
            MetaError::AlreadyExists { parent, name } => {
                assert_eq!(parent, root);
                assert_eq!(name, "file2.txt");
            }
            other => panic!("expected AlreadyExists error, got {:?}", other),
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_lua_same_name() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let file_ino = store
            .create_file(root, "file.txt".to_string())
            .await
            .unwrap();
        let node_before = store.get_node(file_ino).await.unwrap().unwrap();

        let result = store
            .rename(root, "file.txt", root, "file.txt".to_string())
            .await;
        assert!(result.is_ok(), "self-rename should be no-op");

        let node_after = store.get_node(file_ino).await.unwrap().unwrap();
        assert_eq!(
            node_before.attr.mtime, node_after.attr.mtime,
            "mtime should not change"
        );
        assert_eq!(
            node_before.attr.ctime, node_after.attr.ctime,
            "ctime should not change"
        );
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_lua_hardlink() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let file_ino = store
            .create_file(root, "file.txt".to_string())
            .await
            .unwrap();
        store.link(file_ino, root, "link.txt").await.unwrap();

        let node_before = store.get_node(file_ino).await.unwrap().unwrap();
        assert_eq!(node_before.attr.nlink, 2, "file should have nlink=2");
        assert_eq!(
            node_before.parent, 0,
            "hardlinked file should have parent=0"
        );
        assert_eq!(node_before.name, "", "hardlinked file should have name=''");

        let link_parents_before = store.load_link_parents(file_ino).await.unwrap();
        assert_eq!(link_parents_before.len(), 2);
        assert!(link_parents_before.contains(&(root, "file.txt".to_string())));
        assert!(link_parents_before.contains(&(root, "link.txt".to_string())));

        let result = store
            .rename(root, "file.txt", root, "renamed.txt".to_string())
            .await;
        assert!(result.is_ok());

        let node_after = store.get_node(file_ino).await.unwrap().unwrap();
        assert_eq!(node_after.attr.nlink, 2, "nlink should remain 2");
        assert_eq!(node_after.parent, 0, "parent should remain 0");
        assert_eq!(node_after.name, "", "name should remain ''");

        let link_parents_after = store.load_link_parents(file_ino).await.unwrap();
        assert_eq!(link_parents_after.len(), 2);
        assert!(link_parents_after.contains(&(root, "renamed.txt".to_string())));
        assert!(link_parents_after.contains(&(root, "link.txt".to_string())));
        assert!(!link_parents_after.contains(&(root, "file.txt".to_string())));
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_exchange_lua_concurrent() {
        let store = Arc::new(new_test_store().await);
        let root = store.root_ino();

        let file1 = store
            .create_file(root, "file1.txt".to_string())
            .await
            .unwrap();
        let file2 = store
            .create_file(root, "file2.txt".to_string())
            .await
            .unwrap();

        let mut handles = vec![];
        for _ in 0..4 {
            let store_clone = Arc::clone(&store);
            let handle = tokio::spawn(async move {
                store_clone
                    .rename_exchange(root, "file1.txt", root, "file2.txt")
                    .await
            });
            handles.push(handle);
        }

        let results: Vec<_> = futures::future::join_all(handles)
            .await
            .into_iter()
            .map(|r| r.unwrap())
            .collect();

        let successes = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(successes, 4, "all exchanges should succeed (idempotent)");

        let lookup1 = store.lookup(root, "file1.txt").await.unwrap();
        let lookup2 = store.lookup(root, "file2.txt").await.unwrap();
        assert!(
            (lookup1 == Some(file1) && lookup2 == Some(file2))
                || (lookup1 == Some(file2) && lookup2 == Some(file1)),
            "entries should be exchanged or restored to original"
        );
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_exchange_lua_old_not_found() {
        let store = new_test_store().await;
        let root = store.root_ino();

        store
            .create_file(root, "file2.txt".to_string())
            .await
            .unwrap();

        let result = store
            .rename_exchange(root, "nonexistent.txt", root, "file2.txt")
            .await;

        assert!(result.is_err());
        if let Err(MetaError::Internal(msg)) = result {
            assert!(
                msg.contains("Entry 'nonexistent.txt' not found in parent")
                    && msg.contains("for exchange"),
                "error message should match format: got '{}'",
                msg
            );
        } else {
            panic!("expected Internal error");
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_exchange_lua_new_not_found() {
        let store = new_test_store().await;
        let root = store.root_ino();

        store
            .create_file(root, "file1.txt".to_string())
            .await
            .unwrap();

        let result = store
            .rename_exchange(root, "file1.txt", root, "nonexistent.txt")
            .await;

        assert!(result.is_err());
        if let Err(MetaError::Internal(msg)) = result {
            assert!(
                msg.contains("Entry 'nonexistent.txt' not found in parent")
                    && msg.contains("for exchange"),
                "error message should match format: got '{}'",
                msg
            );
        } else {
            panic!("expected Internal error");
        }
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_exchange_lua_same_entry() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let file_ino = store
            .create_file(root, "file.txt".to_string())
            .await
            .unwrap();
        let node_before = store.get_node(file_ino).await.unwrap().unwrap();

        let result = store
            .rename_exchange(root, "file.txt", root, "file.txt")
            .await;
        assert!(result.is_ok(), "self-exchange should be no-op");

        let node_after = store.get_node(file_ino).await.unwrap().unwrap();
        assert_eq!(
            node_before.attr.mtime, node_after.attr.mtime,
            "mtime should not change"
        );
        assert_eq!(
            node_before.attr.ctime, node_after.attr.ctime,
            "ctime should not change"
        );
    }

    #[serial]
    #[tokio::test]
    #[ignore]
    async fn test_rename_exchange_lua_hardlinks() {
        let store = new_test_store().await;
        let root = store.root_ino();

        let file1 = store
            .create_file(root, "file1.txt".to_string())
            .await
            .unwrap();
        store.link(file1, root, "link1.txt").await.unwrap();

        let file2 = store
            .create_file(root, "file2.txt".to_string())
            .await
            .unwrap();
        store.link(file2, root, "link2.txt").await.unwrap();

        let node1_before = store.get_node(file1).await.unwrap().unwrap();
        assert_eq!(node1_before.attr.nlink, 2);
        assert_eq!(node1_before.parent, 0);
        assert_eq!(node1_before.name, "");

        let node2_before = store.get_node(file2).await.unwrap().unwrap();
        assert_eq!(node2_before.attr.nlink, 2);
        assert_eq!(node2_before.parent, 0);
        assert_eq!(node2_before.name, "");

        let result = store
            .rename_exchange(root, "file1.txt", root, "file2.txt")
            .await;
        assert!(result.is_ok());

        let link_parents1 = store.load_link_parents(file1).await.unwrap();
        assert_eq!(link_parents1.len(), 2);
        assert!(link_parents1.contains(&(root, "file2.txt".to_string())));
        assert!(link_parents1.contains(&(root, "link1.txt".to_string())));
        assert!(!link_parents1.contains(&(root, "file1.txt".to_string())));

        let link_parents2 = store.load_link_parents(file2).await.unwrap();
        assert_eq!(link_parents2.len(), 2);
        assert!(link_parents2.contains(&(root, "file1.txt".to_string())));
        assert!(link_parents2.contains(&(root, "link2.txt".to_string())));
        assert!(!link_parents2.contains(&(root, "file2.txt".to_string())));

        let node1_after = store.get_node(file1).await.unwrap().unwrap();
        assert_eq!(node1_after.attr.nlink, 2);
        assert_eq!(node1_after.parent, 0);
        assert_eq!(node1_after.name, "");

        let node2_after = store.get_node(file2).await.unwrap().unwrap();
        assert_eq!(node2_after.attr.nlink, 2);
        assert_eq!(node2_after.parent, 0);
        assert_eq!(node2_after.name, "");
    }

    #[test]
    fn test_deserialize_i64_from_number() {
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct TestStruct {
            #[serde(deserialize_with = "super::deserialize_i64_from_number")]
            value: i64,
        }

        // Integer input (normal case)
        let json = r#"{"value": 1234567890}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, 1234567890);

        // Float input (the bug case - scientific notation)
        let json = r#"{"value": 1.7698324007242e+18}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert!(result.value > 1_700_000_000_000_000_000); // ~1.77e18

        // Negative value
        let json = r#"{"value": -1000}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, -1000);

        // Zero
        let json = r#"{"value": 0}"#;
        let result: TestStruct = serde_json::from_str(json).unwrap();
        assert_eq!(result.value, 0);
    }
}
