# MetaStore / baseMeta API Mapping

_Date: 2025-11-13_

## Purpose
- Document how existing Rust `MetaStore` methods correspond to JuiceFS `baseMeta` functions.
- Highlight where names differ but semantics align, preventing redundant APIs.
- Track genuine gaps that require new trait methods.

## Mapping Table

| JuiceFS `baseMeta` | Rust `MetaStore` | Notes |
|--------------------|------------------|-------|
| `Load`, `GetAttr` (special root handling) | `stat`, higher-level logic | Rust client needs extra root/trash handling; method exists. |
| `Lookup` | `lookup` | Semantics match (case-insensitive handled in client). |
| `Readdir` | `readdir` | Requires client-side cache updates. |
| `Read` (chunks) | `read_slices` | Naming differs; function already present. Update doc to reflect parity. |
| `Write` | `write_slice` | Return values provide slices and metadata; extend with quota deltas via `WriteOutcome`. |
| `Truncate` | `truncate_file` (existing signature) | Already present; client must forward flags/quota updates. |
| `GetParents` | `get_names`, `get_dir_parent` | `get_names(ino)` returns all parent bindings (dir always single-binding; file may be multi-binding for hardlinks). `get_dir_parent(ino)` is the authoritative parent for directories (`..` resolution). |
| `GetPath` | `get_paths` | Returns all possible paths for an inode (potentially multiple for hardlinks). Client chooses how to present/resolve. |
| `StatFS` | `stat_fs` | Implemented for DatabaseMetaStore; remaining backends need wiring. |
| `SetAttr` | `set_attr` (+ legacy `set_file_size`) | DatabaseMetaStore now implements unified setter; evaluate deprecating legacy helpers. |
| `Open`, `Close` | `open`, `close` | DatabaseMetaStore returns authoritative attrs; etcd/in-memory still default to `NotImplemented`. |
| `Link`, `Symlink`, `ReadLink` | `link`, `symlink`, `read_symlink` | DatabaseMetaStore implements all three; MetaClient/VFS/FUSE now expose create/readlink flows. Etcd/in-memory stores still need target persistence. |
| `Fallocate` | `fallocate_file` | Already present. |
| `Rename` | `rename` | Flags support to be expanded. |
| `Unlink` / `Rmdir` | `unlink`, `rmdir` | Trash/quota logic handled higher-level. |
| `FindStaleSessions` etc. | Present | Parity work mostly in client. |
| `ReadLink` | `read_symlink` | DatabaseMetaStore returns stored target; clients must refuse non-symlink inodes. |
| `GetQuota` / `SetQuota` / `DeleteQuota` | `get_quota` / `set_quota` / `delete_quota` | Method names align; need to match JuiceFS return semantics (bool vs. error) when wiring client. |
| `LoadQuotas` / `FlushQuota` | `load_quotas` / `flush_quotas` | Existing trait methods map 1:1 to Go helpers. |
| `GetXattr` / `SetXattr` / `RemoveXattr` | `set_xattr`, `remove_xattr` (getter via `dump`/`stat` today) | Need explicit `get_xattr` helper or document fetch path; evaluate parity. |
| `SetAcl` / `GetAcl` | `set_acl`, `get_acl`, `cache_acls` | Struct placeholders exist; semantics require detailed spec (POSIX vs. NFSv4). |

## Duplicate / Overlapping APIs Checked
- **Chunk IO**: `read_slices` / `write_slice` already represent slice-level operations; removed redundant `read_chunks` / `write_chunk` proposal.
- **Truncate**: existing `truncate_file` is retained; no additional traits needed.
- **Slice Deletion**: `delete_slice` matches Go's delayed slice cleanup.
- **Stats Counters**: `get_counter`, `set_counter_if_small` etc. align with `baseMeta` helper usage.

## Outstanding Gaps
- Roll out `stat_fs`, `set_attr`, `open`/`close`, `link`/`symlink` implementations to the Etcd/in-memory stores; document migration guidance for existing deployments gaining the new schema fields.
- Expose `read_symlink` from the remaining backends and add client/VFS readlink plumbing so the new API becomes reachable.
- Consolidate `set_file_size` and `set_attr` semantics (decide whether to deprecate `set_file_size` once `set_attr` lands).
- Audit ACL and quota behaviours to ensure method coverage matches Go expectations.
- Decide whether to add an explicit `get_xattr` helper or expose retrieval via existing metadata queries.

## Next Steps
- Update team docs to use these mappings during development.
- Before adding any new trait method, check against this table to avoid duplication.

---
