//! Redis-based metadata store implementation.
//!
//! This store focuses on the core interfaces needed by the VFS layer so that
//! the filesystem can persist metadata in Redis. It purposely keeps the key
//! layout simple (one key per inode plus a hash per directory) and uses JSON
//! serialization for file attributes. Advanced features (sessions, quota, etc.)
//! can be layered on later by extending the schema.

use super::{apply_truncate_plan, build_paths_from_names, trim_slices_in_place};
use crate::chunk::SliceDesc;
use crate::meta::client::session::{Session, SessionInfo};
use crate::meta::config::{Config, DatabaseType};
use crate::meta::file_lock::{
    FileLockInfo, FileLockQuery, FileLockRange, FileLockType, PlockRecord,
};
use crate::meta::store::{
    DirEntry, FileAttr, FileType, LockName, MetaError, MetaStore, SetAttrFlags, SetAttrRequest,
    StatFsSnapshot,
};
use crate::meta::{INODE_ID_KEY, SLICE_ID_KEY};
use async_trait::async_trait;
use chrono::Utc;
use redis::AsyncCommands;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;
use tokio::net::lookup_host;
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
const TRUNCATE_REWRITE_MAX_RETRIES: usize = 64;

const CHUNK_ID_BASE: u64 = 1_000_000_000u64;

const DELAYED_COUNTER_KEY: &str = "ds_counter";
const DELAYED_KEY_PREFIX: &str = "ds";
const DELAYED_INDEX_KEY: &str = "ds_idx";
const UNCOMMITTED_KEY_PREFIX: &str = "uc";
const UNCOMMITTED_PENDING_INDEX_KEY: &str = "uc_pending_idx";
const UNCOMMITTED_ORPHAN_INDEX_KEY: &str = "uc_orphan_idx";
const COMPACT_RETRY_LIMIT: usize = 64;

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

    -- Validate node state (defense against races)
    if node.deleted or node.attr.nlink == 0 then
        return cjson.encode({ok=false, error="node_not_found"})
    end
    if node.kind == "Dir" then
        return cjson.encode({ok=false, error="is_directory"})
    end
    if node.kind == "Symlink" then
        return cjson.encode({ok=false, error="is_symlink"})
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
    local expected_ino = tonumber(ARGV[4])

    -- Validate dentry still points to expected inode
    local dentry_ino = redis.call('HGET', dir_key, name)
    if not dentry_ino or tonumber(dentry_ino) ~= expected_ino then
        return cjson.encode({ok=false, error="not_found"})
    end

    -- Validate node exists before making any mutations
    local node_json = redis.call('GET', node_key)
    if not node_json then
        return cjson.encode({ok=false, error="node_not_found"})
    end
    local ok, node = pcall(cjson.decode, node_json)
    if not ok or not node or not node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- Remove from directory
    redis.call('HDEL', dir_key, name)

    -- Remove from link_parents (idempotent)
    local member = parent_ino .. ":" .. name
    redis.call('SREM', lp_key, member)

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

    -- Check dentry exists and matches expected inode
    local dentry_ino = redis.call('HGET', parent_dir_key, name)
    if not dentry_ino then
        return cjson.encode({ok=false, error="not_found", ino=parent_ino})
    end
    if tonumber(dentry_ino) ~= child_ino then
        return cjson.encode({ok=false, error="not_found", ino=parent_ino})
    end

    -- Get child node
    local child_json = redis.call('GET', child_node_key)
    if not child_json then
        return cjson.encode({ok=false, error="node_not_found", ino=child_ino})
    end

    -- Decode child node with pcall
    local ok, child_node = pcall(cjson.decode, child_json)
    if not ok or not child_node or not child_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- Check is directory
    if child_node.kind ~= "Dir" then
        return cjson.encode({ok=false, error="not_directory", ino=child_ino})
    end

    -- Check empty
    local child_len = redis.call('HLEN', child_dir_key)
    if child_len > 0 then
        return cjson.encode({ok=false, error="dir_not_empty", ino=child_ino})
    end

    -- Get parent node and update
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

    -- Atomic delete
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

    -- Get parent node
    local parent_json = redis.call('GET', parent_node_key)
    if not parent_json then
        return cjson.encode({ok=false, error="parent_not_found"})
    end

    -- Decode parent node with pcall
    local ok, parent_node = pcall(cjson.decode, parent_json)
    if not ok or not parent_node or not parent_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- Check parent is directory
    if parent_node.kind ~= "Dir" then
        return cjson.encode({ok=false, error="parent_not_directory"})
    end

    -- Check entry doesn't already exist
    local existing = redis.call('HEXISTS', parent_dir_key, name)
    if existing == 1 then
        return cjson.encode({ok=false, error="already_exists"})
    end

    -- Allocate new inode atomically
    local new_ino = redis.call('INCR', counter_key)

    local final_gid = gid
    local final_mode = default_mode
    if parent_has_setgid == 1 then
        final_gid = parent_gid
    end

    -- Determine nlink based on kind
    local nlink = 1
    if kind == "Dir" then
        nlink = 2
    end

    -- Create new node
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

    -- Save new node
    redis.call('SET', 'i' .. new_ino, cjson.encode(new_node))

    -- Add directory entry
    redis.call('HSET', parent_dir_key, name, new_ino)

    -- Update parent if creating directory (nlink++)
    if kind == "Dir" then
        parent_node.attr.nlink = parent_node.attr.nlink + 1
    end

    -- Update parent timestamps
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
    local expected_ino = tonumber(ARGV[6])

    -- Check source dentry exists and matches expected inode
    local dentry_ino = redis.call('HGET', old_parent_dir_key, old_name)
    if not dentry_ino then
        return cjson.encode({ok=false, error="not_found", ino=old_parent_ino})
    end
    if tonumber(dentry_ino) ~= expected_ino then
        return cjson.encode({ok=false, error="not_found", ino=old_parent_ino})
    end

    -- Check new_parent exists and is directory
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

    -- Check target doesn't exist
    local target_exists = redis.call('HEXISTS', new_parent_dir_key, new_name)
    if target_exists == 1 then
        return cjson.encode({ok=false, error="already_exists"})
    end

    -- Get child node
    local child_json = redis.call('GET', child_node_key)
    if not child_json then
        return cjson.encode({ok=false, error="node_not_found", ino=tonumber(dentry_ino)})
    end
    local ok_child, child_node = pcall(cjson.decode, child_json)
    if not ok_child or not child_node or not child_node.attr then
        return cjson.encode({ok=false, error="corrupt_node"})
    end

    -- Update node parent/name OR link_parents based on node kind/nlink
    -- Directories always track parent/name inline even though their nlink is >= 2.
    if child_node.kind == "Dir" or child_node.attr.nlink <= 1 then
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

    -- Update child timestamps
    child_node.attr.mtime = timestamp
    child_node.attr.ctime = timestamp

    -- Remove old dentry and add new dentry
    redis.call('HDEL', old_parent_dir_key, old_name)
    redis.call('HSET', new_parent_dir_key, new_name, dentry_ino)

    -- Save updated child node
    redis.call('SET', child_node_key, cjson.encode(child_node))

    -- Update parent directory timestamps and directory link counts
    local old_parent_json = redis.call('GET', old_parent_node_key)
    if old_parent_json then
        local ok_op, old_parent_node = pcall(cjson.decode, old_parent_json)
        if ok_op and old_parent_node and old_parent_node.attr then
            if child_node.kind == "Dir" and old_parent_ino ~= new_parent_ino then
                old_parent_node.attr.nlink = old_parent_node.attr.nlink - 1
            end
            old_parent_node.attr.mtime = timestamp
            old_parent_node.attr.ctime = timestamp
            redis.call('SET', old_parent_node_key, cjson.encode(old_parent_node))
        end
    end

    if child_node.kind == "Dir" and old_parent_ino ~= new_parent_ino then
        new_parent_node.attr.nlink = new_parent_node.attr.nlink + 1
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
    local expected_old_ino = tonumber(ARGV[6])
    local expected_new_ino = tonumber(ARGV[7])

    -- Check both entries exist and match expected inodes
    local old_dentry_ino = redis.call('HGET', old_parent_dir_key, old_name)
    if not old_dentry_ino then
        return cjson.encode({ok=false, error="not_found", ino=old_parent_ino})
    end
    if tonumber(old_dentry_ino) ~= expected_old_ino then
        return cjson.encode({ok=false, error="stale_conflict"})
    end

    local new_dentry_ino = redis.call('HGET', new_parent_dir_key, new_name)
    if not new_dentry_ino then
        return cjson.encode({ok=false, error="not_found", ino=new_parent_ino})
    end
    if tonumber(new_dentry_ino) ~= expected_new_ino then
        return cjson.encode({ok=false, error="stale_conflict"})
    end

    -- GET both nodes
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

    -- Pre-check link_parents for hardlinked nodes before swapping dentries
    if old_node.kind ~= "Dir" and old_node.attr.nlink > 1 then
        local old_member = old_parent_ino .. ":" .. old_name
        if redis.call('SISMEMBER', old_link_parents_key, old_member) == 0 then
            return cjson.encode({ok=false, error="link_parent_not_found"})
        end
    end
    if new_node.kind ~= "Dir" and new_node.attr.nlink > 1 then
        local new_member = new_parent_ino .. ":" .. new_name
        if redis.call('SISMEMBER', new_link_parents_key, new_member) == 0 then
            return cjson.encode({ok=false, error="link_parent_not_found"})
        end
    end

    -- Swap directory entries atomically
    redis.call('HSET', old_parent_dir_key, old_name, new_dentry_ino)
    redis.call('HSET', new_parent_dir_key, new_name, old_dentry_ino)

    -- Update old_node (hardlinked files use link_parents; directories keep parent/name)
    if old_node.kind ~= "Dir" and old_node.attr.nlink > 1 then
        local old_members = redis.call('SMEMBERS', old_link_parents_key)
        local new_old_members = {}
        local found = false

        for _, member in ipairs(old_members) do
            -- Find first colon only to handle filenames with colons
            local sep_pos = string.find(member, ":", 1, true)
            if sep_pos and sep_pos > 1 and sep_pos < #member then
                local parent_str = string.sub(member, 1, sep_pos - 1)
                local name_str = string.sub(member, sep_pos + 1)
                local parent_num = tonumber(parent_str)
                if parent_num == old_parent_ino and name_str == old_name then
                    table.insert(new_old_members, new_parent_ino .. ":" .. new_name)
                    found = true
                else
                    table.insert(new_old_members, member)
                end
            else
                table.insert(new_old_members, member)
            end
        end

        if not found then
            return cjson.encode({ok=false, error="link_parent_not_found"})
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

    -- Update new_node (hardlinked files use link_parents; directories keep parent/name)
    if new_node.kind ~= "Dir" and new_node.attr.nlink > 1 then
        local new_members = redis.call('SMEMBERS', new_link_parents_key)
        local new_new_members = {}
        local found = false

        for _, member in ipairs(new_members) do
            -- Find first colon only to handle filenames with colons
            local sep_pos = string.find(member, ":", 1, true)
            if sep_pos and sep_pos > 1 and sep_pos < #member then
                local parent_str = string.sub(member, 1, sep_pos - 1)
                local name_str = string.sub(member, sep_pos + 1)
                local parent_num = tonumber(parent_str)
                if parent_num == new_parent_ino and name_str == new_name then
                    table.insert(new_new_members, old_parent_ino .. ":" .. old_name)
                    found = true
                else
                    table.insert(new_new_members, member)
                end
            else
                table.insert(new_new_members, member)
            end
        end

        if not found then
            return cjson.encode({ok=false, error="link_parent_not_found"})
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

    -- Update timestamps for both nodes
    old_node.attr.mtime = timestamp
    old_node.attr.ctime = timestamp
    new_node.attr.mtime = timestamp
    new_node.attr.ctime = timestamp

    -- SET both nodes
    redis.call('SET', old_node_key, cjson.encode(old_node))
    redis.call('SET', new_node_key, cjson.encode(new_node))

    -- Update parent directory timestamps
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
struct LuaResponse {
    ok: bool,
    #[serde(default)]
    ino: Option<i64>,
    #[allow(dead_code)]
    #[serde(default)]
    updated: Option<bool>,
    #[serde(default)]
    nlink: Option<u32>,
    #[serde(default)]
    deleted: Option<bool>,
    #[allow(dead_code)]
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
    chunk_scan_cursor: std::sync::Mutex<Option<String>>,
    chunk_scan_buffer: std::sync::Mutex<Vec<u64>>,
    chunk_scan_next_cursor: std::sync::Mutex<Option<String>>,
    global_lock_tokens: std::sync::Mutex<HashMap<String, i64>>,
}

impl RedisMetaStore {
    async fn resolve_redis_url(url: &str) -> Result<String, MetaError> {
        let Some((scheme, rest)) = url.split_once("://") else {
            return Ok(url.to_string());
        };
        if scheme != "redis" && scheme != "rediss" {
            return Ok(url.to_string());
        }

        let (authority, suffix) = match rest.split_once('/') {
            Some((authority, suffix)) => (authority, format!("/{suffix}")),
            None => (rest, String::new()),
        };
        let (userinfo, hostport) = match authority.rsplit_once('@') {
            Some((userinfo, hostport)) => (Some(userinfo), hostport),
            None => (None, authority),
        };

        if hostport.is_empty() {
            return Ok(url.to_string());
        }

        let (host, port, had_explicit_port) = if let Some(stripped) = hostport.strip_prefix('[') {
            let Some(end_bracket) = stripped.find(']') else {
                return Ok(url.to_string());
            };
            let host = &stripped[..end_bracket];
            let remainder = &stripped[end_bracket + 1..];
            let port = remainder.strip_prefix(':').unwrap_or("6379");
            (host, port, remainder.starts_with(':'))
        } else if hostport.matches(':').count() <= 1 {
            match hostport.split_once(':') {
                Some((host, port)) => (host, port, true),
                None => (hostport, "6379", false),
            }
        } else {
            return Ok(url.to_string());
        };

        if host.is_empty()
            || host.parse::<IpAddr>().is_ok()
            || host.eq_ignore_ascii_case("localhost")
        {
            return Ok(url.to_string());
        }

        let port_num = port.parse::<u16>().map_err(|e| {
            MetaError::Config(format!("Failed to parse Redis URL port in {url}: {e}"))
        })?;

        let resolved_ip = lookup_host((host, port_num))
            .await
            .map_err(|e| MetaError::Config(format!("Failed to resolve Redis host '{host}': {e}")))?
            .next()
            .ok_or_else(|| {
                MetaError::Config(format!("Redis host '{host}' resolved to no addresses"))
            })?
            .ip();

        let resolved_host = match resolved_ip {
            IpAddr::V4(ip) => ip.to_string(),
            IpAddr::V6(ip) => format!("[{ip}]"),
        };

        let mut resolved_authority = String::new();
        if let Some(userinfo) = userinfo {
            resolved_authority.push_str(userinfo);
            resolved_authority.push('@');
        }
        resolved_authority.push_str(&resolved_host);
        if had_explicit_port || port_num != 6379 {
            resolved_authority.push(':');
            resolved_authority.push_str(&port_num.to_string());
        }

        Ok(format!("{scheme}://{resolved_authority}{suffix}"))
    }

    async fn from_config_inner(config: Config) -> Result<Self, MetaError> {
        let conn = Self::create_connection(&config).await?;
        let store = Self {
            conn,
            _config: config,
            sid: std::sync::OnceLock::new(),
            chunk_scan_cursor: std::sync::Mutex::new(None),
            chunk_scan_buffer: std::sync::Mutex::new(Vec::new()),
            chunk_scan_next_cursor: std::sync::Mutex::new(None),
            global_lock_tokens: std::sync::Mutex::new(HashMap::new()),
        };
        store.init_root_directory().await?;
        Ok(store)
    }

    /// Create or open the store from a backend path. The path is expected to
    /// contain a `slayerfs.yml` that specifies the Redis URL.
    #[allow(dead_code)]
    pub async fn new(backend_path: &Path) -> Result<Self, MetaError> {
        let config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;
        Self::from_config_inner(config).await
    }

    /// Build a store from the given configuration.
    #[allow(dead_code)]
    pub async fn from_config(config: Config) -> Result<Self, MetaError> {
        Self::from_config_inner(config).await
    }

    async fn create_connection(config: &Config) -> Result<ConnectionManager, MetaError> {
        match &config.database.db_config {
            DatabaseType::Redis { url } => {
                let resolved_url = Self::resolve_redis_url(url).await?;
                let client = redis::Client::open(resolved_url.as_str()).map_err(|e| {
                    MetaError::Config(format!(
                        "Failed to parse Redis URL {resolved_url} (from {url}): {e}"
                    ))
                })?;
                ConnectionManager::new(client).await.map_err(|e| {
                    MetaError::Config(format!(
                        "Failed to connect to Redis backend using {resolved_url}: {e}"
                    ))
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

    fn parse_chunk_id_from_chunk_key(key: &str) -> Option<u64> {
        let rest = key.strip_prefix(CHUNK_KEY_PREFIX)?;
        let (ino_str, idx_str) = rest.split_once('_')?;
        let ino: u64 = ino_str.parse().ok()?;
        let idx: u64 = idx_str.parse().ok()?;
        ino.checked_mul(CHUNK_ID_BASE)?.checked_add(idx)
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

    fn delayed_key(&self, delayed_id: i64) -> String {
        format!("{DELAYED_KEY_PREFIX}{delayed_id}")
    }

    fn uncommitted_key(&self, slice_id: u64) -> String {
        format!("{UNCOMMITTED_KEY_PREFIX}{slice_id}")
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
            symlink_target: None,
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
                let all_entries: std::collections::HashMap<String, String> =
                    conn.hgetall(&plock_key).await.map_err(redis_err)?;

                // Get current locks for this owner/session
                let current_records = if let Some(json) = all_entries.get(&field) {
                    serde_json::from_str(json).unwrap_or_default()
                } else {
                    Vec::new()
                };

                // Check for conflicts with other locks
                let mut conflict_found = false;

                for (other_field, other_records_json) in all_entries {
                    if other_field == field {
                        continue;
                    }

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

    async fn rewrite_trimmed_slices(
        &self,
        chunk_id: u64,
        cutoff_offset: u64,
    ) -> Result<(), MetaError> {
        let key = self.chunk_key(chunk_id);

        for _ in 0..TRUNCATE_REWRITE_MAX_RETRIES {
            let mut conn = Self::create_connection(&self._config).await?;

            redis::cmd("WATCH")
                .arg(&key)
                .exec_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(&key)
                .arg(0)
                .arg(-1)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let mut slices = Vec::with_capacity(raw.len());
            for entry in raw {
                let desc: SliceDesc = crate::meta::serialization::deserialize_meta(&entry)?;
                slices.push(desc);
            }
            trim_slices_in_place(&mut slices, cutoff_offset);

            let mut pipe = redis::pipe();
            pipe.atomic().cmd("DEL").arg(&key).ignore();
            for slice in &slices {
                let data = crate::meta::serialization::serialize_meta(slice)?;
                pipe.cmd("RPUSH").arg(&key).arg(data).ignore();
            }

            let rewritten: Option<()> = pipe.query_async(&mut conn).await.map_err(redis_err)?;
            if rewritten.is_some() {
                return Ok(());
            }
        }

        Err(MetaError::Internal(format!(
            "truncate rewrite retried too many times for chunk {chunk_id}"
        )))
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
                self.rewrite_trimmed_slices(chunk_id, cutoff_offset).await?;
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

    async fn from_config(config: Config) -> Result<Self, MetaError> {
        Self::from_config_inner(config).await
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
        if ino == ROOT_INODE {
            return Err(MetaError::NotSupported(
                "cannot create hard links to the root inode".into(),
            ));
        }
        self.ensure_parent_dir(parent).await?;

        let node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        if node.kind == NodeKind::Dir {
            return Err(MetaError::NotSupported(
                "cannot create hard links to directories".into(),
            ));
        }
        if node.kind == NodeKind::Symlink {
            return Err(MetaError::NotSupported(
                "cannot create hard links to symbolic links".into(),
            ));
        }
        if node.deleted || node.attr.nlink == 0 {
            return Err(MetaError::NotFound(ino));
        }

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
            Some("is_directory") => Err(MetaError::NotSupported(
                "cannot create hard links to directories".into(),
            )),
            Some("is_symlink") => Err(MetaError::NotSupported(
                "cannot create hard links to symbolic links".into(),
            )),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => {
                let attr_json = response
                    .attr
                    .ok_or_else(|| MetaError::Internal("missing attr in link response".into()))?;
                let stored_attr: StoredAttr = serde_json::from_value(attr_json)
                    .map_err(|e| MetaError::Internal(format!("attr parse error: {e}")))?;

                self.bump_dir_times(parent, now).await?;
                Ok(stored_attr.to_file_attr(ino, node.kind.into()))
            }
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
    }

    #[tracing::instrument(level = "trace", skip(self), fields(parent, name, target))]
    async fn symlink(
        &self,
        parent: i64,
        name: &str,
        target: &str,
    ) -> Result<(i64, FileAttr), MetaError> {
        let ino = self
            .create_entry(parent, name.to_string(), FileType::Symlink)
            .await?;
        let now = current_time();

        let mut node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;
        node.attr.size = target.len() as u64;
        node.attr.atime = now;
        node.attr.mtime = now;
        node.attr.ctime = now;
        node.symlink_target = Some(target.to_string());

        self.save_node(&node).await?;

        Ok((ino, node.as_file_attr()))
    }

    #[tracing::instrument(level = "trace", skip(self), fields(ino))]
    async fn read_symlink(&self, ino: i64) -> Result<String, MetaError> {
        let node = self.get_node(ino).await?.ok_or(MetaError::NotFound(ino))?;

        if node.kind != NodeKind::Symlink {
            return Err(MetaError::NotSupported(format!(
                "inode {ino} is not a symbolic link"
            )));
        }

        node.symlink_target
            .ok_or_else(|| MetaError::Internal(format!("symlink target missing for inode {ino}")))
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
        if !matches!(node.kind, NodeKind::File | NodeKind::Symlink) {
            return Err(MetaError::NotSupported(format!(
                "{child} is not unlinkable"
            )));
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
            .arg(child)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Lua response parse error: {e}")))?;

        match response.error.as_deref() {
            Some("not_found") => Err(MetaError::NotFound(parent)),
            Some("node_not_found") => Err(MetaError::NotFound(child)),
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some(other) => Err(MetaError::Internal(format!("Lua error: {other}"))),
            None if response.ok => {
                let nlink = response.nlink.unwrap_or(0);
                let deleted = response.deleted.unwrap_or(false);

                if deleted {
                    let mut node_mut = self
                        .get_node(child)
                        .await?
                        .ok_or(MetaError::NotFound(child))?;
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
            None => Err(MetaError::Internal("unexpected Lua response".into())),
        }
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
            .arg(child) // ARGV[6] - expected inode
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
            Some("target_dir_not_empty") => Err(MetaError::DirectoryNotEmpty(
                response.ino.unwrap_or(new_parent),
            )),
            Some("target_is_directory") => Err(MetaError::Io(std::io::Error::from(
                std::io::ErrorKind::IsADirectory,
            ))),
            Some("target_not_directory") => Err(MetaError::Io(std::io::Error::from(
                std::io::ErrorKind::NotADirectory,
            ))),
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
            .arg(old_ino)
            .arg(new_ino)
            .invoke_async(&mut self.conn.clone())
            .await
            .map_err(redis_err)?;

        let response: LuaResponse = serde_json::from_str(&result)
            .map_err(|e| MetaError::Internal(format!("Failed to parse Lua response: {e}")))?;
        match response.error.as_deref() {
            Some("stale_conflict") => Err(MetaError::ContinueRetry),
            Some("not_found") => Err(MetaError::NotFound(response.ino.unwrap_or(old_parent))),
            Some("internal") => {
                let msg = response.msg.unwrap_or_else(|| "unknown error".to_string());
                Err(MetaError::Internal(msg))
            }
            Some("corrupt_node") => Err(MetaError::Internal("corrupt node data".into())),
            Some("link_parent_not_found") => Err(MetaError::Internal(
                "expected link parent binding not found during exchange".into(),
            )),
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
            let kind_bits = node.attr.mode & 0o170000;
            node.attr.mode = kind_bits | (mode & 0o777);
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

        build_paths_from_names(ROOT_INODE, names, |current_ino| async move {
            let node = self.get_node(current_ino).await?;

            Ok(node.map(|node| (node.parent, node.name)))
        })
        .await
    }

    fn root_ino(&self) -> i64 {
        ROOT_INODE
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn initialize(&self) -> Result<(), MetaError> {
        self.init_root_directory().await
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn stat_fs(&self) -> Result<StatFsSnapshot, MetaError> {
        let mut conn = self.conn.clone();
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(format!("{NODE_KEY_PREFIX}*"))
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;

        let mut used_space = 0u64;
        let mut used_inodes = 0u64;

        for key in keys {
            let data: Option<Vec<u8>> = conn.get(&key).await.map_err(redis_err)?;
            let Some(bytes) = data else {
                continue;
            };

            let node: StoredNode = serde_json::from_slice(&bytes)
                .map_err(|e| MetaError::Internal(format!("Failed to parse node {key}: {e}")))?;

            if node.deleted || node.attr.nlink == 0 {
                continue;
            }

            used_space = used_space.saturating_add(node.attr.size);
            used_inodes = used_inodes.saturating_add(1);
        }

        Ok(StatFsSnapshot {
            total_space: used_space,
            available_space: 0,
            used_inodes,
            available_inodes: 0,
        })
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

    #[tracing::instrument(level = "trace", skip(self), fields(batch_size, max_age_secs))]
    // GC Phase 1: find aged delayed slices, delete their chunk meta, and mark them for block deletion.
    async fn process_delayed_slices(
        &self,
        batch_size: usize,
        max_age_secs: i64,
    ) -> Result<Vec<(u64, u64, u64, i64)>, MetaError> {
        if batch_size == 0 {
            return Ok(vec![]);
        }

        let mut conn = self.conn.clone();
        let cutoff = Utc::now().timestamp() - max_age_secs;

        let delayed_ids: Vec<i64> = redis::cmd("ZRANGEBYSCORE")
            .arg(DELAYED_INDEX_KEY)
            .arg("-inf")
            .arg(cutoff)
            .arg("LIMIT")
            .arg(0)
            .arg(batch_size)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;

        if delayed_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut result = Vec::new();

        for delayed_id in delayed_ids {
            let ds_key = self.delayed_key(delayed_id);
            let fields: std::collections::HashMap<String, String> =
                conn.hgetall(&ds_key).await.map_err(redis_err)?;

            if fields.is_empty() {
                tracing::warn!(
                    delayed_id = delayed_id,
                    "delayed slice hash missing, cleaning up stale index"
                );
                let _: () = redis::pipe()
                    .atomic()
                    .cmd("DEL")
                    .arg(&ds_key)
                    .ignore()
                    .cmd("ZREM")
                    .arg(DELAYED_INDEX_KEY)
                    .arg(delayed_id)
                    .ignore()
                    .query_async(&mut conn)
                    .await
                    .map_err(redis_err)?;
                continue;
            }

            let status = fields.get("st").cloned().unwrap_or_default();
            let slice_id = match fields.get("sid").and_then(|v| v.parse::<u64>().ok()) {
                Some(v) => v,
                None => {
                    tracing::warn!(
                        delayed_id = delayed_id,
                        "failed to parse sid from delayed slice hash, cleaning up"
                    );
                    let _: () = redis::pipe()
                        .atomic()
                        .cmd("DEL")
                        .arg(&ds_key)
                        .ignore()
                        .cmd("ZREM")
                        .arg(DELAYED_INDEX_KEY)
                        .arg(delayed_id)
                        .ignore()
                        .query_async(&mut conn)
                        .await
                        .map_err(redis_err)?;
                    continue;
                }
            };
            let offset = fields
                .get("off")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let size = fields
                .get("sz")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);

            if status == "meta_deleted" {
                result.push((slice_id, offset, size, delayed_id));
                continue;
            }

            if status != "pending" {
                continue;
            }

            let chunk_id = fields
                .get("cid")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let chunk_key = self.chunk_key(chunk_id);

            let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(&chunk_key)
                .arg(0)
                .arg(-1)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let mut target_entry = None;
            for entry in raw {
                let desc: SliceDesc = crate::meta::serialization::deserialize_meta(&entry)?;
                if desc.slice_id == slice_id {
                    target_entry = Some(entry);
                    break;
                }
            }

            let mut pipe = redis::pipe();
            pipe.atomic();

            if let Some(entry_bytes) = target_entry {
                pipe.cmd("LREM")
                    .arg(&chunk_key)
                    .arg(0)
                    .arg(&entry_bytes)
                    .ignore();
            }

            pipe.hset(&ds_key, "st", "meta_deleted").ignore();

            let _: () = pipe.query_async(&mut conn).await.map_err(redis_err)?;
            result.push((slice_id, offset, size, delayed_id));
        }

        Ok(result)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(delayed_count = delayed_ids.len()))]
    // GC Phase 2: permanently remove delayed slice records after their blocks have been deleted.
    async fn confirm_delayed_deleted(&self, delayed_ids: &[i64]) -> Result<(), MetaError> {
        if delayed_ids.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.clone();
        let mut pipe = redis::pipe();
        pipe.atomic();

        for delayed_id in delayed_ids {
            let ds_key = self.delayed_key(*delayed_id);
            pipe.cmd("DEL").arg(&ds_key).ignore();
            pipe.cmd("ZREM")
                .arg(DELAYED_INDEX_KEY)
                .arg(delayed_id)
                .ignore();
        }

        let _: () = pipe.query_async(&mut conn).await.map_err(redis_err)?;
        Ok(())
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

    #[tracing::instrument(level = "trace", skip(self), fields(limit))]
    async fn list_chunk_ids(&self, limit: usize) -> Result<Vec<u64>, MetaError> {
        if limit == 0 {
            return Ok(vec![]);
        }

        // Drain any buffered chunk IDs from a previous mid-page limit hit first.
        {
            let mut buffer = self.chunk_scan_buffer.lock().unwrap();
            if !buffer.is_empty() {
                let take = limit.min(buffer.len());
                let result: Vec<u64> = buffer.drain(..take).collect();
                if buffer.is_empty() {
                    let mut cursor = self.chunk_scan_cursor.lock().unwrap();
                    let mut next_cursor = self.chunk_scan_next_cursor.lock().unwrap();
                    *cursor = next_cursor.take();
                }
                return Ok(result);
            }
        }

        let page_size = limit.clamp(64, 256);
        let mut start_key = self.chunk_scan_cursor.lock().unwrap().clone();
        let started_from_cursor = start_key.is_some();
        let mut chunk_ids = Vec::new();
        let mut wrapped = false;
        let mut conn = self.conn.clone();

        loop {
            let (next_cursor, keys): (String, Vec<String>) = redis::cmd("SCAN")
                .arg(start_key.as_deref().unwrap_or("0"))
                .arg("MATCH")
                .arg(format!("{CHUNK_KEY_PREFIX}*"))
                .arg("COUNT")
                .arg(page_size)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let mut chunk_idx_in_page = 0;
            for key in &keys {
                if let Some(chunk_id) = Self::parse_chunk_id_from_chunk_key(key) {
                    chunk_ids.push(chunk_id);
                    chunk_idx_in_page += 1;
                    if chunk_ids.len() == limit {
                        // Any remaining chunk IDs in this SCAN page that we haven't
                        // consumed need to be buffered so they aren't skipped.
                        let remaining: Vec<u64> = keys
                            .iter()
                            .filter_map(|k| Self::parse_chunk_id_from_chunk_key(k))
                            .skip(chunk_idx_in_page)
                            .collect();
                        if !remaining.is_empty() {
                            let mut buffer = self.chunk_scan_buffer.lock().unwrap();
                            buffer.extend(remaining);
                            // Save the next cursor so we can resume from the following
                            // page once the buffer is fully drained.
                            let mut next_cursor_guard = self.chunk_scan_next_cursor.lock().unwrap();
                            *next_cursor_guard = Some(next_cursor);
                        } else {
                            let mut cursor = self.chunk_scan_cursor.lock().unwrap();
                            *cursor = Some(next_cursor);
                            let mut next_cursor_guard = self.chunk_scan_next_cursor.lock().unwrap();
                            *next_cursor_guard = None;
                        }
                        return Ok(chunk_ids);
                    }
                }
            }

            if next_cursor == "0" {
                if wrapped || !started_from_cursor {
                    let mut cursor = self.chunk_scan_cursor.lock().unwrap();
                    *cursor = None;
                    let mut next_cursor_guard = self.chunk_scan_next_cursor.lock().unwrap();
                    *next_cursor_guard = None;
                    break;
                }
                start_key = Some("0".to_string());
                wrapped = true;
                continue;
            }

            start_key = Some(next_cursor);
        }

        Ok(chunk_ids)
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, new_slices, old_slices_to_delay),
        fields(chunk_id)
    )]
    // Replace old chunk slices with new ones and create delayed records for the removed slices.
    async fn replace_slices_for_compact(
        &self,
        chunk_id: u64,
        new_slices: &[SliceDesc],
        old_slices_to_delay: &[u8],
    ) -> Result<(), MetaError> {
        if !old_slices_to_delay.is_empty() && !old_slices_to_delay.len().is_multiple_of(20) {
            tracing::warn!(
                chunk_id = chunk_id,
                delayed_len = old_slices_to_delay.len(),
                "replace_slices_for_compact: invalid delayed data length"
            );
            return Err(MetaError::Internal(
                "Invalid delayed data length".to_string(),
            ));
        }

        let delayed_slices = SliceDesc::decode_delayed_data(old_slices_to_delay)
            .ok_or_else(|| MetaError::Internal("Invalid delayed data length".to_string()))?;
        let delayed_ids: std::collections::HashSet<u64> =
            delayed_slices.iter().map(|(id, _, _)| *id).collect();

        let chunk_key = self.chunk_key(chunk_id);

        for _ in 0..COMPACT_RETRY_LIMIT {
            let mut conn = Self::create_connection(&self._config).await?;

            redis::cmd("WATCH")
                .arg(&chunk_key)
                .exec_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(&chunk_key)
                .arg(0)
                .arg(-1)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let mut kept = Vec::new();
            for entry in raw {
                let desc: SliceDesc = crate::meta::serialization::deserialize_meta(&entry)?;
                if !delayed_ids.contains(&desc.slice_id) {
                    kept.push(entry);
                }
            }

            let now = Utc::now().timestamp();

            let mut pipe = redis::pipe();
            pipe.atomic();

            // Replace chunk slices: keep non-delayed existing, append new
            pipe.cmd("DEL").arg(&chunk_key).ignore();
            for entry in &kept {
                pipe.cmd("RPUSH").arg(&chunk_key).arg(entry).ignore();
            }
            for slice in new_slices {
                let data = crate::meta::serialization::serialize_meta(slice)?;
                pipe.cmd("RPUSH").arg(&chunk_key).arg(data).ignore();
            }

            // Create delayed records for old slices
            if !delayed_slices.is_empty() {
                for (slice_id, offset, size) in &delayed_slices {
                    let delayed_id: i64 =
                        conn.incr(DELAYED_COUNTER_KEY, 1).await.map_err(redis_err)?;
                    let ds_key = self.delayed_key(delayed_id);
                    pipe.hset(&ds_key, "sid", slice_id.to_string());
                    pipe.hset(&ds_key, "off", offset.to_string());
                    pipe.hset(&ds_key, "sz", u64::from(*size).to_string());
                    pipe.hset(&ds_key, "st", "pending");
                    pipe.hset(&ds_key, "ca", now.to_string());
                    pipe.hset(&ds_key, "cid", chunk_id.to_string());
                    pipe.cmd("ZADD")
                        .arg(DELAYED_INDEX_KEY)
                        .arg(now)
                        .arg(delayed_id)
                        .ignore();
                }
            }

            let result: Option<()> = pipe.query_async(&mut conn).await.map_err(redis_err)?;
            if result.is_some() {
                return Ok(());
            }
        }

        Err(MetaError::ContinueRetry)
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, new_slices, old_slices_to_delay, expected_slices),
        fields(chunk_id)
    )]
    // Versioned slice replacement: verify chunk state matches expectations before swapping slices.
    async fn replace_slices_for_compact_with_version(
        &self,
        chunk_id: u64,
        new_slices: &[SliceDesc],
        old_slices_to_delay: &[u8],
        expected_slices: &[SliceDesc],
    ) -> Result<(), MetaError> {
        if !old_slices_to_delay.is_empty() && !old_slices_to_delay.len().is_multiple_of(20) {
            tracing::warn!(
                chunk_id = chunk_id,
                delayed_len = old_slices_to_delay.len(),
                "replace_slices_for_compact_with_version: invalid delayed data length"
            );
            return Err(MetaError::Internal(
                "Invalid delayed data length".to_string(),
            ));
        }

        let delayed_slices = SliceDesc::decode_delayed_data(old_slices_to_delay)
            .ok_or_else(|| MetaError::Internal("Invalid delayed data length".to_string()))?;

        let chunk_key = self.chunk_key(chunk_id);

        for _ in 0..COMPACT_RETRY_LIMIT {
            let mut conn = Self::create_connection(&self._config).await?;

            redis::cmd("WATCH")
                .arg(&chunk_key)
                .exec_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(&chunk_key)
                .arg(0)
                .arg(-1)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let mut current_slices = Vec::with_capacity(raw.len());
            for entry in raw {
                let desc: SliceDesc = crate::meta::serialization::deserialize_meta(&entry)?;
                current_slices.push(desc);
            }

            if current_slices.len() != expected_slices.len() {
                tracing::warn!(
                    chunk_id = chunk_id,
                    expected_count = expected_slices.len(),
                    actual_count = current_slices.len(),
                    "Concurrent modification detected: slice count mismatch"
                );
                continue;
            }

            let current_map: std::collections::HashMap<u64, (u64, u64)> = current_slices
                .iter()
                .map(|s| (s.slice_id, (s.offset, s.length)))
                .collect();

            let mut mismatch = false;
            for expected in expected_slices {
                match current_map.get(&expected.slice_id) {
                    Some((offset, length)) => {
                        if *offset != expected.offset || *length != expected.length {
                            tracing::warn!(
                                chunk_id = chunk_id,
                                slice_id = expected.slice_id,
                                "Concurrent modification detected: slice content changed"
                            );
                            mismatch = true;
                            break;
                        }
                    }
                    None => {
                        tracing::warn!(
                            chunk_id = chunk_id,
                            slice_id = expected.slice_id,
                            "Concurrent modification detected: slice missing"
                        );
                        mismatch = true;
                        break;
                    }
                }
            }

            if mismatch {
                continue;
            }

            let now = Utc::now().timestamp();

            let mut pipe = redis::pipe();
            pipe.atomic();

            // Replace chunk slices
            pipe.cmd("DEL").arg(&chunk_key).ignore();
            for slice in new_slices {
                let data = crate::meta::serialization::serialize_meta(slice)?;
                pipe.cmd("RPUSH").arg(&chunk_key).arg(data).ignore();
            }

            // Create delayed records for old slices
            if !delayed_slices.is_empty() {
                for (slice_id, offset, size) in &delayed_slices {
                    let delayed_id: i64 =
                        conn.incr(DELAYED_COUNTER_KEY, 1).await.map_err(redis_err)?;
                    let ds_key = self.delayed_key(delayed_id);
                    pipe.hset(&ds_key, "sid", slice_id.to_string());
                    pipe.hset(&ds_key, "off", offset.to_string());
                    pipe.hset(&ds_key, "sz", u64::from(*size).to_string());
                    pipe.hset(&ds_key, "st", "pending");
                    pipe.hset(&ds_key, "ca", now.to_string());
                    pipe.hset(&ds_key, "cid", chunk_id.to_string());
                    pipe.cmd("ZADD")
                        .arg(DELAYED_INDEX_KEY)
                        .arg(now)
                        .arg(delayed_id)
                        .ignore();
                }
            }

            // Clean up uncommitted records for new slices
            for slice in new_slices {
                let uc_key = self.uncommitted_key(slice.slice_id);
                pipe.cmd("DEL").arg(&uc_key).ignore();
                pipe.cmd("ZREM")
                    .arg(UNCOMMITTED_PENDING_INDEX_KEY)
                    .arg(slice.slice_id.to_string())
                    .ignore();
                pipe.cmd("ZREM")
                    .arg(UNCOMMITTED_ORPHAN_INDEX_KEY)
                    .arg(slice.slice_id.to_string())
                    .ignore();
            }

            let result: Option<()> = pipe.query_async(&mut conn).await.map_err(redis_err)?;
            if result.is_some() {
                return Ok(());
            }
        }

        Err(MetaError::ContinueRetry)
    }

    #[tracing::instrument(
        level = "trace",
        skip(self, operation),
        fields(slice_id, chunk_id, size)
    )]
    // Track a newly written slice as uncommitted so GC can clean it up if the commit fails.
    async fn record_uncommitted_slice(
        &self,
        slice_id: u64,
        chunk_id: u64,
        size: u64,
        operation: &str,
    ) -> Result<i64, MetaError> {
        let mut conn = self.conn.clone();
        let now = Utc::now().timestamp();
        let uc_key = self.uncommitted_key(slice_id);

        let mut pipe = redis::pipe();
        pipe.atomic();
        pipe.hset(&uc_key, "cid", chunk_id.to_string());
        pipe.hset(&uc_key, "sz", size.to_string());
        pipe.hset(&uc_key, "ca", now.to_string());
        pipe.hset(&uc_key, "op", operation);
        pipe.hset(&uc_key, "st", "pending");
        pipe.cmd("ZADD")
            .arg(UNCOMMITTED_PENDING_INDEX_KEY)
            .arg(now)
            .arg(slice_id.to_string())
            .ignore();

        let _: () = pipe.query_async(&mut conn).await.map_err(redis_err)?;
        Ok(slice_id as i64)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(slice_id))]
    // Mark an uncommitted slice as committed by removing its tracking record.
    async fn confirm_slice_committed(&self, slice_id: u64) -> Result<(), MetaError> {
        let mut conn = self.conn.clone();
        let uc_key = self.uncommitted_key(slice_id);

        let mut pipe = redis::pipe();
        pipe.atomic();
        pipe.cmd("DEL").arg(&uc_key).ignore();
        pipe.cmd("ZREM")
            .arg(UNCOMMITTED_PENDING_INDEX_KEY)
            .arg(slice_id.to_string())
            .ignore();
        pipe.cmd("ZREM")
            .arg(UNCOMMITTED_ORPHAN_INDEX_KEY)
            .arg(slice_id.to_string())
            .ignore();

        let _: () = pipe.query_async(&mut conn).await.map_err(redis_err)?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(max_age_secs, batch_size))]
    // Scan for stale uncommitted slices: mark orphans for block cleanup and remove committed leftovers.
    async fn cleanup_orphan_uncommitted_slices(
        &self,
        max_age_secs: i64,
        batch_size: usize,
    ) -> Result<Vec<(u64, u64)>, MetaError> {
        if batch_size == 0 {
            return Ok(vec![]);
        }

        let mut conn = self.conn.clone();
        let cutoff = Utc::now().timestamp() - max_age_secs;

        // Scan pending index
        let pending_ids: Vec<u64> = redis::cmd("ZRANGEBYSCORE")
            .arg(UNCOMMITTED_PENDING_INDEX_KEY)
            .arg("-inf")
            .arg(cutoff)
            .arg("LIMIT")
            .arg(0)
            .arg(batch_size)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;

        // Scan orphan index
        let orphan_ids: Vec<u64> = redis::cmd("ZRANGE")
            .arg(UNCOMMITTED_ORPHAN_INDEX_KEY)
            .arg(0)
            .arg(batch_size - 1)
            .query_async(&mut conn)
            .await
            .map_err(redis_err)?;

        if pending_ids.is_empty() && orphan_ids.is_empty() {
            return Ok(vec![]);
        }

        let mut cleaned = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for slice_id in pending_ids {
            if !seen.insert(slice_id) {
                continue;
            }

            let uc_key = self.uncommitted_key(slice_id);
            let fields: std::collections::HashMap<String, String> =
                conn.hgetall(&uc_key).await.map_err(redis_err)?;

            if fields.is_empty() {
                // Stale index entry, clean up
                let _: () = redis::pipe()
                    .atomic()
                    .cmd("ZREM")
                    .arg(UNCOMMITTED_PENDING_INDEX_KEY)
                    .arg(slice_id.to_string())
                    .ignore()
                    .query_async(&mut conn)
                    .await
                    .map_err(redis_err)?;
                continue;
            }

            let chunk_id = fields
                .get("cid")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let size = fields
                .get("sz")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            let chunk_key = self.chunk_key(chunk_id);

            let raw: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(&chunk_key)
                .arg(0)
                .arg(-1)
                .query_async(&mut conn)
                .await
                .map_err(redis_err)?;

            let mut exists = false;
            for entry in raw {
                let desc: SliceDesc = crate::meta::serialization::deserialize_meta(&entry)?;
                if desc.slice_id == slice_id {
                    exists = true;
                    break;
                }
            }

            if exists {
                // Committed, clean up
                let _: () = redis::pipe()
                    .atomic()
                    .cmd("DEL")
                    .arg(&uc_key)
                    .ignore()
                    .cmd("ZREM")
                    .arg(UNCOMMITTED_PENDING_INDEX_KEY)
                    .arg(slice_id.to_string())
                    .ignore()
                    .query_async(&mut conn)
                    .await
                    .map_err(redis_err)?;
            } else {
                // Orphan
                cleaned.push((slice_id, size));
                let mut pipe = redis::pipe();
                pipe.atomic();
                pipe.hset(&uc_key, "st", "orphan");
                pipe.cmd("ZREM")
                    .arg(UNCOMMITTED_PENDING_INDEX_KEY)
                    .arg(slice_id.to_string())
                    .ignore();
                pipe.cmd("ZADD")
                    .arg(UNCOMMITTED_ORPHAN_INDEX_KEY)
                    .arg(0)
                    .arg(slice_id.to_string())
                    .ignore();
                let _: () = pipe.query_async(&mut conn).await.map_err(redis_err)?;
            }
        }

        for slice_id in orphan_ids {
            if seen.insert(slice_id) {
                let uc_key = self.uncommitted_key(slice_id);
                let size: Option<String> = conn.hget(&uc_key, "sz").await.map_err(redis_err)?;
                let size_val = size.and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                cleaned.push((slice_id, size_val));
            }
        }

        Ok(cleaned)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(slice_count = slice_ids.len()))]
    // Delete uncommitted slice tracking records after their orphan blocks have been cleaned up.
    async fn delete_uncommitted_slices(&self, slice_ids: &[u64]) -> Result<(), MetaError> {
        if slice_ids.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn.clone();
        let mut pipe = redis::pipe();
        pipe.atomic();

        for slice_id in slice_ids {
            let uc_key = self.uncommitted_key(*slice_id);
            pipe.cmd("DEL").arg(&uc_key).ignore();
            pipe.cmd("ZREM")
                .arg(UNCOMMITTED_PENDING_INDEX_KEY)
                .arg(slice_id.to_string())
                .ignore();
            pipe.cmd("ZREM")
                .arg(UNCOMMITTED_ORPHAN_INDEX_KEY)
                .arg(slice_id.to_string())
                .ignore();
        }

        let _: () = pipe.query_async(&mut conn).await.map_err(redis_err)?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self), fields(key))]
    async fn next_id(&self, key: &str) -> Result<i64, MetaError> {
        self.alloc_id(key).await
    }

    #[tracing::instrument(level = "trace", skip(self), fields(name))]
    async fn get_counter(&self, name: &str) -> Result<i64, MetaError> {
        let mut conn = self.conn.clone();
        let value: Option<i64> = conn.get(name).await.map_err(redis_err)?;
        Ok(value.unwrap_or(0))
    }

    #[tracing::instrument(level = "trace", skip(self), fields(name, delta))]
    async fn incr_counter(&self, name: &str, delta: i64) -> Result<i64, MetaError> {
        let mut conn = self.conn.clone();
        conn.incr(name, delta).await.map_err(redis_err)
    }

    #[tracing::instrument(level = "trace", skip(self), fields(name, value, diff))]
    async fn set_counter_if_small(
        &self,
        name: &str,
        value: i64,
        diff: i64,
    ) -> Result<bool, MetaError> {
        let script = redis::Script::new(
            r#"
            local current = redis.call('GET', KEYS[1])
            local curr_val = tonumber(current) or 0
            local threshold = tonumber(ARGV[1]) - tonumber(ARGV[2])
            if curr_val < threshold then
                redis.call('SET', KEYS[1], tonumber(ARGV[1]))
                return true
            else
                return false
            end
            "#,
        );
        let mut conn = self.conn.clone();
        let result: bool = script
            .key(name)
            .arg(value)
            .arg(diff)
            .invoke_async(&mut conn)
            .await
            .map_err(redis_err)?;
        Ok(result)
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

    #[tracing::instrument(level = "trace", skip(self), fields(lock_name = ?lock_name, ttl_secs))]
    async fn get_global_lock(&self, lock_name: LockName, ttl_secs: u64) -> bool {
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
                redis.call("HSET", key, field, now_time)
                return now_time
            else
                last_updated = tonumber(last_updated)
                if now_time < last_updated + diff then
                    return false
                else
                    redis.call("HSET", key, field, now_time)
                    return now_time
                end
            end
            "#,
        );

        let diff = chrono::Duration::seconds(ttl_secs as i64).num_milliseconds();

        let resp: Result<redis::Value, _> = script
            .key(LOCKS_KEY)
            .arg(&lock_name)
            .arg(now)
            .arg(diff)
            .invoke_async(&mut conn)
            .await;

        match resp {
            Ok(redis::Value::BulkString(bytes)) => {
                // Lua returns the token (timestamp) as a number, which redis-rs
                // may encode as a bulk string in some versions.
                if let Ok(token_str) = std::str::from_utf8(&bytes)
                    && let Ok(token) = token_str.parse::<i64>()
                {
                    if let Ok(mut tokens) = self.global_lock_tokens.lock() {
                        tokens.insert(lock_name, token);
                    }
                    return true;
                }
                false
            }
            Ok(redis::Value::Int(token)) => {
                if let Ok(mut tokens) = self.global_lock_tokens.lock() {
                    tokens.insert(lock_name, token);
                }
                true
            }
            Ok(redis::Value::SimpleString(s)) if s == "false" || s == "0" => false,
            Ok(redis::Value::Nil) => false,
            Ok(other) => {
                tracing::warn!("Unexpected response from get_global_lock Lua: {:?}", other);
                false
            }
            Err(err) => {
                error!("{}", err.to_string());
                false
            }
        }
    }

    async fn is_global_lock_held(&self, lock_name: LockName, ttl_secs: u64) -> bool {
        let lock_name = lock_name.to_string();
        let mut conn = self.conn.clone();
        let now = Utc::now().timestamp_millis();
        let ttl_millis = chrono::Duration::seconds(ttl_secs as i64).num_milliseconds();

        let locked_at: Option<i64> = conn
            .hget(LOCKS_KEY, &lock_name)
            .await
            .map_err(redis_err)
            .ok()
            .flatten();

        match locked_at {
            Some(locked_at) => now <= locked_at + ttl_millis,
            None => false,
        }
    }

    async fn release_global_lock(&self, lock_name: LockName) -> bool {
        let lock_name = lock_name.to_string();
        let expected_token = match self.global_lock_tokens.lock() {
            Ok(tokens) => tokens.get(&lock_name).copied(),
            Err(err) => {
                error!("Error reading local lock token {}: {}", lock_name, err);
                None
            }
        };
        let Some(expected_token) = expected_token else {
            return false;
        };

        let mut conn = self.conn.clone();

        let script = redis::Script::new(
            r#"
            local key = KEYS[1]
            local field = ARGV[1]
            local expected = tonumber(ARGV[2])

            local current = redis.call("HGET", key, field)
            if current == false then
                return false
            end

            current = tonumber(current)
            if current == expected then
                redis.call("HDEL", key, field)
                return true
            else
                return false
            end
            "#,
        );

        let resp: Result<bool, _> = script
            .key(LOCKS_KEY)
            .arg(&lock_name)
            .arg(expected_token)
            .invoke_async(&mut conn)
            .await;

        match resp {
            Ok(released) => {
                if released && let Ok(mut tokens) = self.global_lock_tokens.lock() {
                    tokens.remove(&lock_name);
                }
                released
            }
            Err(err) => {
                error!("Error releasing lock {}: {}", lock_name, err);
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
        let current_field = format!("{}:{}", sid, query.owner);
        let plock_entries: std::collections::HashMap<String, String> =
            conn.hgetall(&plock_key).await.map_err(redis_err)?;

        // First, try to get locks from current session's field
        if let Some(records_json) = plock_entries.get(&current_field) {
            let records: Vec<PlockRecord> = serde_json::from_str(records_json).unwrap_or_default();
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
    #[serde(default)]
    symlink_target: Option<String>,
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
mod tests;
