
<div align="center">
	<img src="doc/icon.png" alt="SlayerFS icon" width="96" height="96" />
</div>

<h1 align="center">SlayerFS</h1>
<p align="center"><strong>High-performance Rust &amp; Layers-aware Distributed Filesystem</strong></p>
<p align="center"><a href="README.md"><b>English</b></a> | <a href="README_CN.md">中文</a></p>

[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/language-Rust-orange.svg)](https://www.rust-lang.org/)


## ✨ Project Overview

SlayerFS is a Rust-based distributed filesystem for container and AI scenarios. It uses a chunk/block layout and integrates with object storage backends (LocalFS implemented; S3/Rustfs reserved) to provide path-based read/write, directory operations, truncate, and other basic capabilities, making it easy to integrate with SDKs and FUSE.

Core idea: decouple compute from storage. Applications use POSIX-like interfaces to access data, while the scheduler/cache layers decide where the data lives and how it’s accessed.

## 🖼 Architecture

<div align="center">
	<img src="doc/SlayerFS.png" alt="SlayerFS architecture" width="1280" />
</div>

Components overview:
- chuck: ChunkLayout, ChunkReader/Writer. Maps file offsets to chunk/block and handles cross-block IO and zero-filling holes.
- cadapter: Object backend abstraction and implementations (LocalFs implemented; S3/Rustfs placeholders).
- meta: In-memory metadata + transactions (InMemoryMetaStore). Tracks size and slice, supports commit/rollback.
- fs: Path-based FileSystem (mkdir/mkdir_all/create/read/write/readdir/stat/unlink/rmdir/rename/truncate).
- vfs: Inode-based VFS for FUSE integration.
- sdk: App-facing lightweight client wrapper (FileSystem-backed, with LocalClient convenience).

## 🚀 Quick Start

### Requirements

- Rust: >= 1.75.0
- Operating system: Linux (Ubuntu 20.04+, CentOS 8+)

```bash
cargo run -q --bin sdk_demo -- /tmp/slayerfs-objroot
```
The demo will:
- Create nested directories/files, perform cross-block/chunk writes and read verification
- Do rename, truncate (shrink/extend), readdir and unlink/rmdir
- Print expected error scenarios and finally output "sdk demo: OK"
---

## 🌟 Current Features (MVP)

### Path-based FileSystem
- mkdir/mkdir_all/create/read/write/readdir/stat/exists/unlink/rmdir/rename/truncate
- Single mutex to protect the namespace (avoid multi-lock deadlocks); avoid awaiting under lock on hot paths

### Chunked IO with zero-fill
- 64MiB chunk + 4MiB block (default, configurable)
- Write path splits by block; read path returns zeros for holes

### Object-backed BlockStore
- LocalFs implemented (for tests/examples); S3/Rustfs placeholders

### Metadata with txn
- InMemoryMetaStore: alloc_inode, record_slice, update_size (truncate shrink works)
- Transaction commit/rollback tests are in place

More: see `doc/sdk.md` and inline rustdoc.

---

## 📚 Docs
- Design: `doc/arch.md`
- SDK: `doc/sdk.md`
- Benchmarks: `doc/bench.md`
- Docker image build: `doc/docker-image-build.md`

## 🐳 Docker Image Build

The maintained container flow is the SlayerFS image build in `project/slayerfs/docker/Dockerfile`.
The image contains:
- the `slayerfs` binary built from this workspace;
- the default container entrypoint in `project/slayerfs/docker/entrypoint.sh`;
- one xfstests helper binary copied from the prebuilt bundle, controlled by the `XFSTESTS_BINARY` build argument.

Before building, fetch the Git LFS tarball used by the Dockerfile:

```bash
git lfs install --local
git lfs pull --include="project/slayerfs/tests/scripts/xfstests-prebuilt/*.tar.gz"
```

Build from the `rk8s` repository root so the Docker build context matches the CI workflow:

```bash
docker build \
	-f project/slayerfs/docker/Dockerfile \
	-t slayerfs:local \
	project
```

The image currently builds with a `rust:1.91-bookworm` builder stage and a `debian:trixie-slim` runtime stage so the Rust toolchain and bundled `xfs_io` stay compatible with the checked-in lockfile and prebuilt artifacts.

To copy a different tool from the prebuilt xfstests bundle, override `XFSTESTS_BINARY`:

```bash
docker build \
	-f project/slayerfs/docker/Dockerfile \
	--build-arg XFSTESTS_BINARY=xfs_io \
	-t slayerfs:local \
	project
```

Default runtime behavior:
- entrypoint starts `slayerfs mount`;
- local data backend uses `/var/lib/slayerfs/data`;
- default metadata backend is sqlite at `/var/lib/slayerfs/metadata.db`;
- the extracted xfstests helper is placed under `/opt/xfstests/bin`;
- the image declares `/mnt/slayerfs` and `/var/lib/slayerfs` as volumes.

Minimal local-fs + sqlite run example:

```bash
docker run --rm \
	--device /dev/fuse \
	--cap-add SYS_ADMIN \
	--security-opt apparmor=unconfined \
	-v slayerfs-state:/var/lib/slayerfs \
	-v slayerfs-mount:/mnt/slayerfs \
	slayerfs:local
```

Redis metadata example:

```bash
docker run --rm \
	--device /dev/fuse \
	--cap-add SYS_ADMIN \
	--security-opt apparmor=unconfined \
	--network slayerfs_slayerfs-network \
	-e SLAYERFS_META_BACKEND=redis \
	-e SLAYERFS_META_URL=redis://redis:6379 \
	-v slayerfs-data:/var/lib/slayerfs \
	-v slayerfs-mount:/mnt/slayerfs \
	slayerfs:local
```

Etcd metadata example:

```bash
docker run --rm \
	--device /dev/fuse \
	--cap-add SYS_ADMIN \
	--security-opt apparmor=unconfined \
	--network slayerfs_slayerfs-network \
	-e SLAYERFS_META_BACKEND=etcd \
	-e SLAYERFS_META_ETCD_URLS=http://etcd:2379 \
	-v slayerfs-data:/var/lib/slayerfs \
	-v slayerfs-mount:/mnt/slayerfs \
	slayerfs:local
```

If `/dev/fuse` and the required container privileges are not provided, the image can still generate config and initialize metadata backends, but the FUSE mount step will fail.

CI uses `.github/workflows/slayerfs-docker.yml` to build this same image on pull requests and pushes to `main`. Docker Hub publishing is only enabled when `SLAYERFS_DOCKERHUB_REPOSITORY`, `DOCKERHUB_USERNAME`, and `DOCKERHUB_TOKEN` are configured in GitHub.


---


## 🤝 Contributing

Issues and PRs are welcome to improve architecture, implementation, and docs.
