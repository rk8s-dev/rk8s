# Hardlink and Symlink Implementation

## Overview

SlayerFS implements both hardlinks and symlinks with an optimized metadata strategy that provides O(1) parent directory lookup for
single-link files while efficiently managing multi-link scenarios.

## Design Principles

### 1. Reversible Transition Strategy

The linking mechanism uses a **reversible transition strategy** to optimize for the common single-link case:

- **Single-link files (nlink=1)**: Use the `parent` field in `file_meta` for O(1) parent directory lookup
- **Multi-link files (nlink>1)**: Use `LinkParentMeta` table (or etcd LinkParent keys) to track all parent directories
- **Directories**: POSIX directories naturally have `nlink >= 2` ("." and ".."), but directories are **never** tracked via LinkParent mode. Directory name/parent is always derived from its single `content_meta` binding.
- **Revert to single-link mode (nlink: 2→1)**: When the last hard link is removed and nlink drops back to 1, the system
  restores the `parent` field from the remaining `content_meta` entry and deletes all `LinkParent` records

**Rationale**: Since hard links are rare in most workloads, reverting to single-link mode keeps most files in the optimized O(1) parent lookup path,
minimizing the overhead of `LinkParentMeta` storage and queries.

### 2. Hybrid Parent Tracking

- **Single-link files (nlink=1)**: Use the `parent` field in `file_meta` for O(1) parent directory lookup
- **Multi-link files (nlink>1)**: Use `LinkParentMeta` table (or etcd LinkParent keys) to track all parent directories
- **Key insight**: LinkParent mode is activated on first hard link (1→2) and deactivated when reverting to single-link (2→1)

## State Transitions

## Data Integrity

This implementation assumes the following invariants:

- For a file with `nlink == 1`, the inode has exactly one directory entry, and the store can return a single `(parent_inode, entry_name)` binding.
- For a file with `nlink > 1`, the store must be able to return all parent/name bindings.

If the metadata becomes inconsistent (for example `nlink == 2` but there are no remaining link-parent bindings), the store should return an `Internal` error rather than `NotFound`, since this indicates corruption rather than a missing inode.

### Creating a File

```
Initial state:
- nlink = 1
- parent = <parent_directory_inode>
- No LinkParent entries
```

### Creating First Hardlink (nlink: 1 → 2)

```
Transition process:
1. Read original entry from ContentMeta to get (old_parent, old_entry_name)
2. Set file.parent = 0
3. Create LinkParent entry for original link: (inode, old_parent, old_entry_name)
4. Create LinkParent entry for new link: (inode, new_parent, new_entry_name)
5. Update file.nlink = 2
```

### Creating Additional Hardlinks (nlink: 2 → 3, 3 → 4, ...)

```
Process:
1. Add new LinkParent entry
2. Increment file.nlink
```

### Unlinking (nlink > 1)

```
Process:
1. Delete specific LinkParent entry
2. Decrement file.nlink
```

### Unlinking with 2→1 Reversion (nlink: 2 → 1)

When `nlink` transitions from 2 to 1, the system reverts to single-link mode to optimize performance:

```
Transition process:
1. Delete specific `content_meta` entry (parent_inode, entry_name)
2. Delete specific `link_parent_meta` entry (inode, parent_inode, entry_name)
3. Decrement file.nlink to 1
4. Restore parent field from remaining content_meta:
   - Query remaining `content_meta` entry: `WHERE inode = ?` (should return exactly 1 row)
   - Set `file_meta.parent = remaining_entry.parent_inode`
5. Delete all remaining `link_parent_meta` entries for this inode
6. Update file_meta.modify_time and file_meta.create_time
```

**Key invariants:**
- After 2→1 transition, `file_meta.parent != 0` and no `link_parent_meta` entries exist
- The remaining `content_meta` entry uniquely identifies the single parent directory
- Future link operations will trigger 1→2 transition again, recreating `link_parent_meta`

### Unlinking Last Link (nlink: 1 → 0)

```
Process:
1. Delete the file entry from ContentMeta
2. Mark file.deleted = true
3. Set file.nlink = 0
4. Set file.parent = 0
```

## Symlink Semantics

**POSIX Standard Behavior**:

- `lstat()`: Return symlink metadata
- `stat()`: Follow symlink and return target file metadata

According to these semantics, `resolve_path` & `resolve_path_follow` is implemented.

### Symlink Properties

- Always `nlink = 1` (hardlinks to symlinks not supported, so never enters multi-link mode)
- Always use `parent` field for O(1) parent lookup (never transitions to LinkParent mode)
- Target path stored in `symlink_target` field
- Target can be absolute or relative path
- Target may not exist (dangling symlink)
