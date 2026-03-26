# SlayerFS Permission Model

## Overview

SlayerFS persists POSIX-style permission bits and ownership (uid/gid) in file
and directory metadata. Permissions are stored as part of each inode's
`Permission` record and are returned through `stat` / `getattr` to the FUSE
layer.

## Supported Features

| Feature | Status |
|---------|--------|
| Standard permission bits (`rwxrwxrwx`, 0o777) | ✅ Supported |
| `chmod` (mode changes via FUSE `setattr`) | ✅ Supported |
| `chown` (uid/gid changes via FUSE `setattr`) | ✅ Supported |
| File type preservation across `chmod` | ✅ Supported |
| Default file permissions (0644) | ✅ Supported |
| Default directory permissions (0755) | ✅ Supported |

## Not Supported

| Feature | Reason |
|---------|--------|
| Setuid bit (0o4000) | Stripped on `chmod`; not enforced |
| Setgid bit (0o2000) | Stripped on `chmod`; not enforced |
| Sticky bit (0o1000) | Stripped on `chmod`; not enforced |
| POSIX ACLs | Not implemented |
| umask synchronization | VFS defaults are hard-coded; FUSE layer may apply umask at creation time |

## Default Permissions

- **Files** are created with mode `0o100644` (`-rw-r--r--`).
- **Directories** are created with mode `0o040755` (`drwxr-xr-x`).

When files or directories are created through the FUSE layer, SlayerFS strips
unsupported special bits and then applies `umask` to the persisted
`rwxrwxrwx` permission bits:

```
effective_mode = (mode & 0o777) & !(umask & 0o777)
```

## chmod Behavior

When `chmod` is called (either via the VFS `chmod` method or via a FUSE
`setattr` with the mode field set):

1. **Setuid (0o4000), setgid (0o2000), and sticky (0o1000) bits are stripped.**
   Only the standard `rwxrwxrwx` permission bits (0o777) are persisted.
2. The file type bits in the mode word are preserved automatically.
3. The `ctime` (change time) is updated.

### Example

```text
chmod 4755 /mnt/slayerfs/file.txt
# Resulting mode: 0755 (setuid bit silently removed)
```

## chown Behavior

When `chown` is called (either via the VFS `chown` method or via a FUSE
`setattr` with uid/gid fields set):

1. Either `uid` or `gid` may be changed independently — omitting one leaves
   the corresponding field unchanged.
2. Permission bits are **not** altered by ownership changes.
3. The `ctime` (change time) is updated.

### Example

```text
chown 1000:1000 /mnt/slayerfs/file.txt
# uid → 1000, gid → 1000, mode unchanged
```

## Error Handling

| Condition | Error |
|-----------|-------|
| `chmod` on nonexistent inode | `ENOENT` |
| `chown` on nonexistent inode | `ENOENT` |
| Invalid mode bits (above 0o777) | Silently masked before write |

## Concurrency

Permission and ownership changes are atomic within each backend:

- **SQLite/PostgreSQL**: Uses database transactions.
- **etcd**: Uses compare-and-swap with optimistic locking.
- **Redis**: Uses atomic node save operations.

## Future Work

- POSIX ACL support.
- Setuid/setgid enforcement if security use-cases arise.
