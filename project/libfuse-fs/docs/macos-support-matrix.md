# macOS Support Matrix

Status of `libfuse-fs` and `rfuse3` on macOS (macFUSE).

Legend: ✅ supported · ⚠️ degraded / partial · ❌ not implemented

## rfuse3 — FUSE protocol layer

| Area | Status | Notes |
|---|---|---|
| Standard FUSE handlers (LOOKUP/GETATTR/READ/WRITE/...) | ✅ | All 41 handlers wired through the macOS worker. |
| `FUSE_RENAME2` | ✅ | Linux-style flags accepted. |
| `FUSE_FALLOCATE` (forwarded to fs trait) | ✅ | rfuse3 forwards; backend support depends on the filesystem implementation. |
| `FUSE_COPY_FILE_RANGE` | ✅ | rfuse3 forwards; see libfuse-fs row. |
| `FUSE_SETVOLNAME` (macOS only) | ✅ | Trait method `Filesystem::setvolname`; default returns ENOSYS. |
| `FUSE_GETXTIMES` (macOS only) | ✅ | Trait method `Filesystem::getxtimes` returning `ReplyXTimes`. |
| `FUSE_EXCHANGE` (macOS only) | ✅ | Trait method `Filesystem::exchange`. |
| Mount via `mount_macfuse` | ✅ | Auto-detects `/Library/Filesystems/macfuse.fs/Contents/Resources/mount_macfuse`. |
| `intr` mount option | ✅ | macOS/FreeBSD specific. |
| `dirsync`/`nodiratime`/`nodev`/`rootmode` mount options | ❌ | Linux-only; not advertised on macOS. |
| `force_readdir_plus` | ❌ | Linux-only on the server side. |

## libfuse-fs — passthrough

| Op / feature | Status | Notes |
|---|---|---|
| Lookup / GetAttr / SetAttr | ✅ | |
| Read / Write / Open / Release / Flush / Fsync | ✅ | |
| Mknod / Mkdir / Unlink / Rmdir / Symlink / Link | ✅ | |
| Rename | ✅ | |
| Rename2 (`RENAME_NOREPLACE`/`RENAME_EXCHANGE`) | ✅ | macOS uses `renamex_np` with `RENAME_EXCL`/`RENAME_SWAP`. `RENAME_WHITEOUT` and combined-flags return `ENOTSUP`. |
| Statfs | ✅ | |
| Access / Create | ✅ | |
| Opendir / Readdir / Readdirplus / Releasedir | ✅ | |
| `force_readdir_plus` | ❌ | Linux-only server flag (see above). |
| Setxattr / Getxattr / Listxattr / Removexattr | ✅ | macOS uses `f*xattr` directly. `setxattr` honors the FUSE `position` (resource-fork support). `security.*`/`trusted.*`/`system.*` namespaces return `ENOTSUP` early. **Errors now propagate** — earlier "fakes success" behavior was removed (`setxattr`/`removexattr` return real errno). |
| Fallocate (mode == 0, extend) | ✅ | macOS implementation: `fcntl(F_PREALLOCATE)` + `ftruncate` (tries `F_ALLOCATECONTIG` first). |
| Fallocate (PUNCH_HOLE / COLLAPSE_RANGE / ZERO_RANGE / ...) | ❌ | Returns `ENOTSUP` — no equivalent in macOS. |
| Copy_file_range | ✅ | macOS uses `fcopyfile(.., COPYFILE_CLONE \| COPYFILE_DATA)` for whole-file copies (offset_in==offset_out==0, length>=src_size) — APFS O(1) clone with kernel data-copy fallback. Partial/offset copies fall through to a 64 KiB `pread`+`pwrite` loop. |
| Lseek (SEEK_DATA / SEEK_HOLE) | ✅ | |
| Inode metadata cache (entry/attr timeout) | ✅ default 5 s | macOS uses the `Reopenable` `InodeHandle` variant (`Config::macos_lazy_inode_fd`, default `true`) so the lazy-fd path lets cache TTLs go above zero without pinning fds. Setting the flag to `false` falls back to eager fds + TTL=0. Measured impact (clean A/B): meta-stat **2.76×** lazy/eager, randread-4k 1.42×, seqread-1m 1.26×. |
| Procfs paths (`/proc/self/fd`, `/proc/self/mountinfo`) | ⚠️ | `fd_path_cstr()` helper centralizes the platform difference (macOS uses `fcntl(F_GETPATH)`). `mountinfo` is replaced with `/dev/null` placeholder. |
| File handles (`name_to_handle_at`, `open_by_handle_at`) | ❌ | Returns `ENOTSUP`. macOS has no equivalent kernel API — needs a `(st_dev, st_ino, st_gen)` self-built handle to be added. Tracked as Phase 2.4. |

## libfuse-fs — overlayfs / unionfs

| Op / feature | Status | Notes |
|---|---|---|
| `init()` for lower/upper layers | ✅ | Was previously gated to Linux only; now runs on macOS too. |
| Whiteout — CharDev format (`mknod` 0/0) | ✅ on Linux · ⚠️ on macOS | macOS `mknod` for char dev needs root. Use `OciWhiteout` instead. |
| Whiteout — OciWhiteout format (`.wh.<name>`) | ✅ | Selected by default on macOS (`Config::whiteout_format`). Empty regular files marker, opaque dir = `.wh..wh..opq`. unionfs/overlayfs `lookup_child` checks for sibling `.wh.<name>` to detect whiteouts; `readdir` filters out `.wh.*` entries from user-visible results. |
| `O_DIRECT` clearing in `create()` | n/a | Linux-only since macOS has no `O_DIRECT`. |
| Layer test mount with `force_readdir_plus` | n/a | Linux-only. |
| Rest of operations | inherited from passthrough | See above table. |

## Compile / Test status

| Target | rfuse3 | libfuse-fs |
|---|---|---|
| `cargo check` macOS (aarch64-apple-darwin) | ✅ | ✅ |
| `cargo check` Linux (x86_64-unknown-linux-gnu) | ✅ | ⚠️ blocked by transitive `openssl-sys` (env, not code) |
| `cargo test --lib` macOS | ✅ 7/7 | ✅ (after gating `qlean` dev-dep to Linux) |
| `cargo build --release --example passthrough` macOS | n/a | ✅ |
| GitHub Actions macOS workflow (`libfuse-fs-macos.yml`) | covered | covered (check + test --lib + build --example, no real macFUSE mount) |

## Real-world baselines (macOS Tahoe / Darwin 25.4.0, macFUSE 4.x)

### pjdfstest conformance — pre-A baseline

| Total | Pass | Fail |
| ---: | ---: | ---: |
| 8677 | 815 | 7862 |

Fail count is dominated by tests that require root and APFS/HFS+ semantic
differences from Linux — see [`macos-conformance-baseline.md`](./macos-conformance-baseline.md).

### fio passthrough — pre-A vs after-A (lazy inode fd)

| Job | pre-A IOPS | after-A IOPS | pre-A p50 (µs) | after-A p50 (µs) |
| --- | ---: | ---: | ---: | ---: |
| randread-4k | 105,686 | 72,764 | 317 | 481 |
| seqread-1m | 1,821 | 1,223 | 4227 | 6128 |
| meta-stat | 55,870 | **77,905** | 32 | **22** |

Caveat: `randread-4k` and `seqread-1m` regressions look real on the surface,
but in same-session A/B tests against the eager fallback (lazy `false`),
those workloads drop to ~46k IOPS and ~520 IOPS respectively — i.e. the
"regression" is system noise from concurrent macFUSE mounts, not from the
A.3 changes. `meta-stat` consistently improves with lazy enabled (3–4× over
eager in matched-environment tests). Full history in
[`macos-performance-baseline.md`](./macos-performance-baseline.md).

## Known remaining gaps

- **A.4 — delete `Config::macos_lazy_inode_fd`**: kept as escape hatch.
  Clean same-environment A/B (phase 7) shows lazy beats eager **2.76×** on
  meta-stat — the lazy path is genuinely faster and is the default. The
  flag stays as an experimental dial; doc-comment marks it as not API-stable.
- **2.4 — macOS file handle equivalent**: not done. `file_handle.rs` /
  `mount_fd.rs` are documented as Linux-effective only with `debug_assert!`
  guarding the unreachable macOS stubs. A full module-level `cfg(linux)` gate
  would touch ~37 sites; deferred to phase 8.
- **`force_readdir_plus` on macOS**: rfuse3 advertises this server-side option
  Linux-only. Whether macFUSE supports `READDIRPLUS` needs verification.
- **Cross-mount inode dedup**: explicitly unsupported on macOS (no
  `name_to_handle_at`).

## Phase 6 (2026-04-30) — fd lifecycle hardening

| Feature | Status | Notes |
|---|---|---|
| LRU bound on cached lazy fds | ✅ | `LazyFdLru` in `passthrough/mod.rs`. Cap = `Config::macos_lazy_fd_lru_max` (default `RLIMIT_NOFILE_soft / 2`). Eviction drops the cached `Arc<File>`; path stays so the next `get_file()` reopens. Stress test (`macos_lazy_fd_pressure_caps_real_fds`): 200 files × cap=8 keeps process fd usage capped. |
| Reopen counter | ✅ | `PassthroughFs::macos_lazy_fd_reopen_count() -> Option<u64>`. Bumped on every cache miss / post-eviction reopen. Surface for telemetry / dashboards. |
| Recursive `lazy_path` rewrite on directory rename | ✅ | `macos_lazy_after_rename` walks the inode store under read-lock, prefix-matches old path, rewrites all descendants. O(N_inodes) per directory rename — acceptable since rename is rare. Test: `macos_lazy_dir_rename_rewrites_descendants`. |
| `Config::macos_lazy_fd_lru_max` | ✅ | `Option<NonZeroUsize>`; `None` ⇒ auto-from-rlimit. Inspectable via `macos_lazy_fd_cap()` / `macos_lazy_fd_cache_len()`. |

## Phase 7 (2026-05-01) — testing & CI closure

| Feature | Status | Notes |
|---|---|---|
| pjdfstest baseline refresh | ✅ | Re-run after phase 2/6 changes. New: 816 pass / 7861 fail (was 815 / 7862). Stale "known-issue" section removed. |
| `setvolname` / `getxtimes` / `exchange` opcodes | ✅ | Real impls on `PassthroughFs` (was trait default ENOSYS). `setvolname` accepts and ignores; `getxtimes` returns `st_birthtimespec`; `exchange` routes to `renamex_np(RENAME_SWAP)` with the same lazy-path bookkeeping as `rename2`. Three integration tests added (`macos_setvolname_*`, `macos_getxtimes_*`, `macos_exchange_*`). |
| GH Actions real-mount smoke | ✅ | `mount-smoke` job in `libfuse-fs-macos.yml` installs macFUSE, mounts the example, round-trips a read/write. `continue-on-error: true` because GH-hosted runners often refuse kext load — advisory until self-hosted lands. |
| A/B benchmark harness | ✅ | `script/macfuse_bench.sh --ab` runs lazy vs eager back-to-back, aborts if other macFUSE mounts exist (set `AB_ALLOW_NOISE=1` to override), records `concurrent_mounts=N` in baseline. First clean A/B (no concurrent mounts): meta-stat **2.76× lazy/eager**, randread-4k 1.42×, seqread-1m 1.26× — confirms lazy-fd is genuinely faster, not noise-driven. |
| `Config` / `passthrough::config` re-exported | ✅ | Module changed from `mod config` → `pub mod config` so the example (and downstream callers) can build custom configs without going through `new_passthroughfs_layer`. |
| `--macos-eager` flag on the example | ✅ | Lets the bench harness mount the same binary in both lazy and eager modes — required for honest A/B. |

## Phase 9 (2026-05-01) — APFS performance mining

| Feature | Status | Notes |
|---|---|---|
| `Layer::host_path_of` trait method | ✅ | Resolves an inode to its absolute host filesystem path; default impl returns `None`. `PassthroughFs` returns `Reopenable.path` directly when lazy mode is on, otherwise `F_GETPATH` on the cached fd (`fd_path_cstr`). Wired on both overlayfs and unionfs Layer traits. Public method `PassthroughFs::passthrough_host_path(inode)` exposes the resolution for non-trait callers. |
| `passthrough::util::try_apfs_clonefile` | ✅ | Thin wrapper around `clonefile(2)`. Returns `Ok(true)` on success, `Ok(false)` on `ENOTSUP` / `EXDEV` (caller falls back), `Err` on real errors. Roundtrip test `macos_apfs_clone_roundtrip` proves the helper produces byte-identical output and surfaces `EEXIST` instead of overwriting. |
| `overlayfs::copy_regfile_up` macOS APFS fast path | ✅ | Before the create+read+write loop, query both layers' `host_path_of`. If both resolve and the destination filesystem accepts `clonefile(2)`, skip the slow path entirely — `do_lookup` materializes the upper inode and `add_upper_inode` wires it in. Falls back silently on cross-volume / non-APFS / non-passthrough layers. |
| `unionfs::copy_regfile_up` macOS APFS fast path | ✅ | Mirror of the overlayfs change, lifted into `try_macos_apfs_clone_up_unionfs`. Same fall-back semantics. |
| `force_readdir_plus` on macOS — kernel probe | ✅ negative result | macFUSE 4.x INIT advertises `kernel_flags=0xef800008`; bits 13 (`FUSE_DO_READDIRPLUS`) and 14 (`FUSE_READDIRPLUS_AUTO`) are both clear. **macFUSE does not support READDIRPLUS** — the existing Linux-only gate on `MountOptions::force_readdir_plus(true)` matches kernel reality. Probe instrumentation kept at `debug!` so a future macFUSE upgrade is observable from `RUST_LOG=debug`. |
| `O_NOFOLLOW_ANY` for lazy-fd open | ❌ — incompatible | Empirical finding on macOS Tahoe (Darwin 25.4.0): `/tmp` is a symlink to `/private/tmp`, so any cached path under `/tmp` triggers `ELOOP` under `O_NOFOLLOW_ANY`. Combining `O_NOFOLLOW_ANY \| O_NOFOLLOW` returns `EINVAL` — the flags are mutually exclusive on Darwin. The safe-traversal hardening would require canonicalizing `cfg.root_dir` at startup before storing paths, then using `O_NOFOLLOW_ANY` standalone. Tracked as future work. The two-step `O_NOFOLLOW` → `O_SYMLINK` retry remains. |

## Phase 8 (2026-05-01) — dead-code & platform-abstraction cleanup

| Feature | Status | Notes |
|---|---|---|
| `passthrough::file_handle` Linux-only | ✅ | Module top-of-file `#![cfg(target_os = "linux")]`. The macOS stub branches inside `from_name_at` and `OpenableFileHandle::open` (debug_assert + ENOTSUP) deleted — the module no longer compiles on Darwin. |
| `passthrough::mount_fd` Linux-only | ✅ | Same treatment. The `cfg(target_os = "macos") let f = libc::open(.., O_RDONLY)` and `cfg(macos) return Ok("/")` shims gone. |
| `InodeHandle::Handle` variant Linux-only | ✅ | All match arms, `InodeMap::get_alt_locked`, `InodeStore::insert/remove/clear/get_by_handle/inode_by_handle`, the `handle_cache: Cache<FileUniqueKey, Arc<FileHandle>>` field, and `to_openable_handle` are gated to Linux. macOS now ships only `InodeHandle::File` (eager fallback) and `InodeHandle::Reopenable` (lazy default). |
| Zero `dead_code` warnings on macOS build | ✅ | `cargo check` clean. Variants/fields that were dead on macOS (`InodeFile::Owned`, `InodeData::btime`, `MOUNT_INFO_FILE`) cfg-gated or `#[allow(dead_code)]` with reason. |
| Linux-only test gates dropped where redundant | ✅ | `file_handle::tests::test_file_handle_from_name_at`, `mount_fd::tests::test_mount_fd_get`, the `#[cfg(target_os = "linux")]` import guards in those test modules — all redundant once the module itself is Linux-only. Removed. |
| `Config::macos_use_lazy_inode_fd` → `macos_lazy_inode_fd` | ✅ | A.4: not deleted (still escape hatch). Renamed (drop "use_") with doc-comment marking it experimental and not API-stable; doc points to the phase-7 A/B numbers (~2.76× meta-stat) as the reason lazy is the default. |
| Test count delta | macOS 35 / Linux 39 | macOS lost the 4 tests that exercised Linux-only types (`test_inode_store`, `test_file_handle_derives`, `test_c_file_handle_wrapper`, `test_mpr_error`). Linux side keeps all 39 — verified by CI; cross-compile from macOS blocked by env-only `openssl-sys`. |
