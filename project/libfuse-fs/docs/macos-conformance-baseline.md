# macOS pjdfstest Conformance Baseline

`tests/conformance/run_pjdfstest.sh` runs the [pjdfstest](https://github.com/pjd/pjdfstest)
POSIX conformance suite against the `passthrough` example mounted via macFUSE.

## How to run

```bash
# Prereqs: macFUSE installed, pjdfstest cloned somewhere
git clone https://github.com/pjd/pjdfstest.git ~/src/pjdfstest
(cd ~/src/pjdfstest && autoreconf -ifs && ./configure && make pjdfstest)

# From repo root:
PJDFSTEST_DIR=~/src/pjdfstest \
    bash project/libfuse-fs/tests/conformance/run_pjdfstest.sh
```

## Baseline

Update these numbers whenever the expected fail count changes; the script
treats any run that produces *more* failures than `Expected fail count` as a
regression and exits non-zero.

| Metric | Value |
| --- | --- |
| Total tests | 8677 |
| Expected pass count | 816 |
| Expected fail count | 7861 |
| Last updated | 2026-04-30 (post phase-6) |
| Tested on | macOS Tahoe (Darwin 25.4.0, aarch64), macFUSE 4.x, eli@laptop |
| Mode | unprivileged user (non-root) |

### History

| Run | Pass | Fail | Notes |
| --- | ---: | ---: | --- |
| 2026-04-30 (pre-A) | 815 | 7862 | Initial baseline |
| 2026-04-30 (post phase-6) | **816** | **7861** | After xattr error propagation, rename2 (renamex_np), OCI whiteout, lazy-fd LRU + recursive directory rename. Net +1 — most fails are root-required (unaffected by these changes). |

## Known-fail categories

The 7862 failures are dominated by two factors that have nothing to do with
our implementation — they're expected and form the floor that future PRs
should not exceed.

### 1. Tests that require root

pjdfstest assumes it runs as root. Many cases set/unset SUID/SGID bits, change
file ownership across UIDs, or test sticky-directory deletion semantics that
unprivileged callers can't exercise. Examples observed:

- All of `chown/*`, large parts of `chmod/00.t`, `chflags/*` later cases
- `unlink/00.t` (all 112), `unlink/11.t` (all 270) — sticky-bit on parent dir

### 2. macOS APFS / HFS+ semantic differences

- `mknod` of character/block devices requires root on macOS regardless of FUSE.
- `link(2)` semantics around immutable / append-only flags differ.
- Some `truncate` and `unlink` cases expect Linux-overlayfs / FreeBSD-UFS behavior.

### 3. Resolved (no longer apply)

- ✅ `setxattr` "fakes success" — fixed in phase 2; real errno now propagates.
  Linux-only namespaces (`security.*`, `trusted.*`, `system.*`) return
  `ENOTSUP` early.
- ✅ `RENAME_NOREPLACE` / `RENAME_EXCHANGE` — fixed in phase 2; macOS path
  uses `renamex_np` with `RENAME_EXCL` / `RENAME_SWAP`.

These items used to surface as known-issues but are no longer present in
the baseline. Listed here so reviewers don't expect them.

## Regression policy

- A PR that increases the fail count over baseline must update this document
  with a brief reason; the script blocks (`exit 1`) when actual fail > baseline.
- Reductions in fail count should also update this document with the new floor.
- Run as the same user (non-root) for comparable numbers; root would pass more.
