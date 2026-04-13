# SlayerFS Docker 镜像构建与运行

本文档说明当前仓库里 SlayerFS runtime image 的构建入口、运行前提、环境变量约定，以及目前已经完成的验证范围。

## 1. 相关文件

- Dockerfile：`project/slayerfs/docker/Dockerfile`
- Entrypoint：`project/slayerfs/docker/entrypoint.sh`
- CI 工作流：`.github/workflows/slayerfs-docker.yml`

这份镜像的目标是：

1. 在 builder 阶段编译 `slayerfs` 主程序。
2. 在 runtime 阶段提供可直接启动的挂载容器入口。
3. 额外携带一个从 xfstests 预构建包中提取的辅助工具，默认是 `xfs_io`。

## 2. 构建前提

### 2.1 Git LFS 资源

Dockerfile 依赖以下 Git LFS 管理的预构建资源：

- `project/slayerfs/tests/scripts/xfstests-prebuilt/xfstests-prebuilt.tar.gz`

构建前先在 `rk8s` 仓库根目录执行：

```bash
git lfs install --local
git lfs pull --include="project/slayerfs/tests/scripts/xfstests-prebuilt/*.tar.gz"
```

如果没有先拉取该 tarball，构建阶段在解压预构建包时会失败。

### 2.2 Docker 构建上下文

必须从 `rk8s` 仓库根目录发起构建，并使用 `project` 作为 build context。这样才与 CI 中的路径保持一致。

## 3. 本地构建

在 `rk8s` 仓库根目录执行：

```bash
docker build \
  -f project/slayerfs/docker/Dockerfile \
  -t slayerfs:local \
  project
```

当前镜像分两段：

- builder：`rust:1.91-bookworm`
- runtime：`debian:trixie-slim`

这样做的原因是：

- 当前 `Cargo.lock` 解析出的依赖需要比 1.85 更高的 Rust 版本；
- 预构建包里的 `xfs_io` 运行时依赖与 `debian:trixie-slim` 兼容。

## 4. 可选构建参数

Dockerfile 当前暴露一个参数：

- `XFSTESTS_BINARY`：指定从预构建包中拷贝到镜像里的辅助工具名，默认值为 `xfs_io`

示例：

```bash
docker build \
  -f project/slayerfs/docker/Dockerfile \
  --build-arg XFSTESTS_BINARY=xfs_io \
  -t slayerfs:local \
  project
```

## 5. 镜像内容与默认行为

镜像构建完成后，关键内容如下：

- `/usr/local/bin/slayerfs`：主程序
- `/usr/local/bin/slayerfs-entrypoint`：默认入口脚本
- `/opt/xfstests/bin/<tool>`：从预构建包提取出的辅助工具
- `/usr/sbin/xfs_io`：runtime 基础镜像内 `xfsprogs` 提供的系统工具

容器默认入口会执行：

```bash
slayerfs mount --config /run/slayerfs/config.yaml /mnt/slayerfs
```

也就是说，容器启动时会先由 `entrypoint.sh` 生成配置文件，然后直接执行挂载。

## 6. 默认环境变量

镜像内置默认值：

- `SLAYERFS_HOME=/var/lib/slayerfs`
- `SLAYERFS_MOUNT_POINT=/mnt/slayerfs`
- `SLAYERFS_DATA_BACKEND=local-fs`
- `SLAYERFS_DATA_DIR=/var/lib/slayerfs/data`
- `SLAYERFS_META_BACKEND=sqlite`
- `SLAYERFS_SQLITE_PATH=/var/lib/slayerfs/metadata.db`
- `RUST_LOG=slayerfs=info`

默认 volume：

- `/mnt/slayerfs`
- `/var/lib/slayerfs`

因此默认 local-fs 数据目录和 sqlite 元数据文件都会落在 `/var/lib/slayerfs` 下面。

## 7. 运行前提

SlayerFS 通过 FUSE 挂载文件系统，因此运行容器时至少需要：

- `--device /dev/fuse`
- `--cap-add SYS_ADMIN`
- `--security-opt apparmor=unconfined`

如果没有这些权限：

- entrypoint 仍然可以生成 `/run/slayerfs/config.yaml`
- metadata backend 也可能初始化成功
- 但最终会在 FUSE 挂载阶段失败

## 8. 最小运行示例

### 8.1 默认 local-fs + sqlite

```bash
docker run --rm \
  --device /dev/fuse \
  --cap-add SYS_ADMIN \
  --security-opt apparmor=unconfined \
  -v slayerfs-state:/var/lib/slayerfs \
  -v slayerfs-mount:/mnt/slayerfs \
  slayerfs:local
```

### 8.2 Redis 元数据后端

先确保 Redis 与 SlayerFS 容器处于同一网络，例如 `slayerfs_slayerfs-network`。

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

### 8.3 Etcd 元数据后端

先确保 Etcd 与 SlayerFS 容器处于同一网络，例如 `slayerfs_slayerfs-network`。

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

## 9. 当前已验证范围

本次实际完成的验证是：

1. `docker build -f project/slayerfs/docker/Dockerfile -t slayerfs:local project` 可成功构建。
2. 容器 entrypoint 生成的 YAML 与 `src/config.rs` 期望的嵌套结构一致。
3. 在提供 FUSE 权限后：
   - Redis 元数据后端可成功挂载；
   - Etcd 元数据后端可成功挂载；
   - 两者都已完成基本文件读写 smoke test。

本次**没有**完成的验证：

- 没有跑完整 xfstests 测试集；
- 没有得出“xfstests 全部通过”的结论。

因此当前能确认的是：

- runtime image 能构建；
- Redis / Etcd 两条运行路径能挂载并做基础读写；
- 但 **xfstests 全量兼容性结果目前仍未知**。

## 10. CI 行为

`.github/workflows/slayerfs-docker.yml` 中当前包含两类 job：

1. `docker-build`
   - 在 pull request 和 push 到 `main` 时执行；
   - 会先安装 `git-lfs`，再拉取预构建 tarball 并构建镜像。

2. `docker-publish`
   - 仅在非 pull request 场景触发；
   - 只有同时配置以下仓库变量/密钥时才会执行发布：
     - `SLAYERFS_DOCKERHUB_REPOSITORY`
     - `DOCKERHUB_USERNAME`
     - `DOCKERHUB_TOKEN`

CI 与本地构建入口保持一致：

- Dockerfile：`project/slayerfs/docker/Dockerfile`
- build context：`project`