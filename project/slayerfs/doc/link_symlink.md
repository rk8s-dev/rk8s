# Hardlink and Symlink Implementation

## Overview

SlayerFS implements both hardlinks and symlinks with an optimized metadata strategy that provides O(1) parent directory lookup for
single-link files while efficiently managing multi-link scenarios.

## Design Principles

### 1. Immediate Transition Strategy

The linking mechanism uses an **immediate transition strategy**: once a hardlink is created (nlink becomes >1), the system permanently
switches to LinkParent tracking mode and never reverts back to using the parent field, even if nlink drops back to 1.

### 2. Hybrid Parent Tracking

- **Single-link files (nlink=1)**: Use the `parent` field in `file_meta` for O(1) parent directory lookup
- **Multi-link files (nlink>1)**: Use `LinkParentMeta` table (or etcd LinkParent keys) to track all parent directories
- **Key insight**: Once LinkParent mode is activated, it remains active for the lifetime of the file

## State Transitions

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

- Always `nlink = 1` (hardlinks to symlinks not supported)
- Always use `parent` field for O(1) parent lookup
- Target path stored in `symlink_target` field
- Target can be absolute or relative path
- Target may not exist (dangling symlink)
