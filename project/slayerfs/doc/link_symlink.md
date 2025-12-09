# Symbolic Links and Hard Links in SlayerFS

This document describes the implementation of symbolic links (symlinks) and hard links in SlayerFS, including their semantics, limitations, and usage.

## Overview

SlayerFS supports two types of links as defined by POSIX:

- **Symbolic Link (Symlink)**: A special file that contains a reference to another file or directory as a path string.
- **Hard Link**: An additional directory entry that refers to the same underlying inode as an existing file.

## Symbolic Links

### Semantics

- A symlink stores the target path as a raw string without normalization.
- When reading a symlink (`readlink`), the stored target path is returned as-is.
- Symlinks can point to files, directories, or even non-existent paths.
- The symlink itself has `FileType::Symlink` and permission mode `0o120777`.

### Operations

| Operation | Behavior |
|-----------|----------|
| `symlink(parent, name, target)` | Create a new symlink at `parent/name` pointing to `target` |
| `readlink(path)` | Return the target path stored in the symlink |
| `unlink(path)` | Remove the symlink entry (does not affect the target) |
| `stat(path)` | Return attributes of the symlink itself (not the target) |

### Path Resolution

When resolving a path that contains a symlink in an **intermediate** component:

1. **Intermediate symlinks are followed**: If `/a/link/b` is resolved and `link` is a symlink to `/x/y`, the path becomes `/x/y/b`.
2. **Final component behavior** depends on the method used:
   - `resolve_path()` - **lstat semantics**: final symlink is NOT followed
   - `resolve_path_follow()` - **stat semantics**: final symlink IS followed
3. **Relative symlinks**: Resolved relative to the symlink's parent directory.
4. **Absolute symlinks**: Resolved from the filesystem root.

**API Methods**:

| Method | Semantics | Final Symlink |
|--------|-----------|---------------|
| `resolve_path(path)` | lstat | Not followed |
| `resolve_path_follow(path)` | stat | Followed |

**Limitations**:
- **TODO**: Cycle detection is not implemented. Circular symlinks may cause infinite loops.
- **TODO**: Maximum symlink depth limit (POSIX SYMLOOP_MAX = 8) is not enforced.

### Example

```rust
// Create a symlink
vfs.create_symlink("/links/my_symlink", "/data/target_file.txt").await?;

// Read the symlink target
let target = vfs.readlink("/links/my_symlink").await?;
assert_eq!(target, "/data/target_file.txt");

// Remove the symlink (target file is unaffected)
vfs.unlink("/links/my_symlink").await?;
```

## Hard Links

### Semantics

- A hard link is an additional directory entry that shares the same inode as an existing file.
- All hard links to a file are equal; there is no "original" and "copy".
- The inode maintains a link count (`nlink`) that tracks how many directory entries reference it.
- File data is only deleted when `nlink` reaches zero.
- Hard links cannot be created for directories (POSIX restriction).

### Operations

| Operation | Behavior |
|-----------|----------|
| `link(existing_path, new_path)` | Create a new hard link at `new_path` referencing `existing_path` |
| `unlink(path)` | Remove the directory entry; decrement `nlink`; delete data only if `nlink == 0` |
| `stat(path)` | Return file attributes including current `nlink` count |

### Link Count Behavior

1. **Creating a hard link**: `nlink` is incremented.
2. **Unlinking**: `nlink` is decremented.
   - If `nlink > 1`: The file remains accessible via other links.
   - If `nlink == 1`: The underlying file data is marked for deletion.

### Example

```rust
// Create original file
vfs.create_file("/data/original.txt").await?;
vfs.write("/data/original.txt", 0, b"hello").await?;

// Create hard link
let attr = vfs.link("/data/original.txt", "/data/hardlink.txt").await?;
assert!(attr.nlink >= 2);

// Both paths access the same data
let data1 = vfs.read("/data/original.txt", 0, 5).await?;
let data2 = vfs.read("/data/hardlink.txt", 0, 5).await?;
assert_eq!(data1, data2);

// Delete original - hard link still works
vfs.unlink("/data/original.txt").await?;
let remaining = vfs.stat("/data/hardlink.txt").await?;
assert_eq!(remaining.nlink, 1);

// Data still accessible via hard link
let data = vfs.read("/data/hardlink.txt", 0, 5).await?;
assert_eq!(data, b"hello");
```

## Entry Types in Metadata

The metadata layer uses the following entry types:

| `EntryType` | `FileType` | Description |
|-------------|------------|-------------|
| `File` | `File` | Regular file |
| `Directory` | `Dir` | Directory |
| `Symlink` | `Symlink` | Symbolic link |
| `Hardlink` | `Hardlink` | Hard link entry (shares inode with target) |

Note: In FUSE layer, both `File` and `Hardlink` are mapped to `FuseFileType::RegularFile` since hard links are semantically regular files.

## Limitations and TODOs

The following features are intentionally not implemented in this version:

1. **Cycle Detection**: Symlinks that form a cycle (A → B → A) are not detected and may cause infinite loops during path resolution.

2. **Maximum Symlink Depth**: No limit on symlink chain length is enforced (POSIX recommends SYMLOOP_MAX = 8).

3. **Cross-Partition Validation**: Hard links across different mount points or partitions are not validated.

These are marked with `TODO` comments in the codebase for future implementation.

## POSIX Compliance

| Requirement | Status |
|-------------|--------|
| Symlink stores raw target path | Implemented |
| Hard links share same inode | Implemented |
| `nlink` correctly maintained | Implemented |
| `unlink` on symlink removes symlink only | Implemented |
| `unlink` on hard link decrements `nlink` | Implemented |
| Data deleted only when `nlink == 0` | Implemented |
| No hard links to directories | Enforced |
| Intermediate symlinks followed in path resolution | Implemented |
| Final symlink not followed (lstat semantics) | Implemented |
| Cycle detection | Not implemented |
| Maximum symlink depth limit | Not implemented |

## Testing

Unit tests are available in `src/vfs/sdk.rs`:

- `test_sdk_local_links`: Tests symlink creation, reading, and deletion; tests hard link creation, nlink maintenance, and data persistence after partial unlink.

Run tests with:

```bash
cargo test -p slayerfs test_sdk_local_links
```
