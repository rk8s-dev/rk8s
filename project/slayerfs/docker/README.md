# SlayerFS Docker 目录说明

本文档说明 `docker/` 目录下的 Dockerfile、compose 文件和本地脚本的职责与推荐用法。

## 1. 脚本列表

当前目录下与 Docker / 本地集成运行相关的脚本有：

- `run_integration_tests.sh`
- `install_xfstests_deps.sh`
- `manage_xfstests_backend_services.sh`
- `run_xfstests_backend.sh`
- `run_xfstests_sqlite.sh`
- `run_xfstests_redis.sh`
- `run_xfstests_etcd.sh`

其中：

- `run_integration_tests.sh` 用于本地 qlean smoke / integration 流程。
- `run_xfstests_sqlite.sh`、`run_xfstests_redis.sh`、`run_xfstests_etcd.sh` 是三个直接入口。
- `run_xfstests_backend.sh` 是共享执行器，三个入口脚本最终都会调用它。
- `install_xfstests_deps.sh` 负责准备依赖和 Git LFS 资源。
- `manage_xfstests_backend_services.sh` 负责启动和停止 redis / etcd 后端服务。

与这组脚本配套的 compose 文件也按后端拆分为：

- `docker compose.integration.yml`
- `docker compose.sqlite.yml`
- `docker compose.redis.yml`
- `docker compose.etcd.yml`

本目录已有的 Docker 构建文件包括：

- `Dockerfile`
- `entrypoint.sh`

## 2. 推荐执行顺序

推荐按下面顺序执行：

1. 准备依赖和 Git LFS 资源。
2. 如果需要 qlean smoke，使用 `run_integration_tests.sh`。
3. 如果需要 xfstests，按后端选择 sqlite / redis / etcd 入口脚本。
4. 如果是 redis 或 etcd，启动对应后端服务。
5. 测试结束后停止后端服务。

SQLite 后端不需要单独启动 docker compose 服务。

## 3. compose 文件结构

当前 compose 已按用途拆分：

- `docker compose.integration.yml`
  用于本地 integration / smoke 路径，包含 etcd、redis、postgres。
- `docker compose.sqlite.yml`
  用于 sqlite 场景的 image 维护入口。
- `docker compose.redis.yml`
  用于 redis 场景的 image 维护和 redis 后端服务。
- `docker compose.etcd.yml`
  用于 etcd 场景的 image 维护和 etcd 后端服务。

其中每个后端 compose 都保留了 `slayerfs-image` 服务，便于在对应 compose 下维护本地 `slayerfs:local` image。
同时每个后端 compose 还提供了挂在 `s3-stack` profile 下的 `rustfs`、`rustfs-init` 和 `slayerfs` 服务，用于拉起 RustFS 对象存储以及与之对应配置的 SlayerFS 容器。

## 4. integration 脚本

脚本：`run_integration_tests.sh`

作用：

- 复用 `docker compose.integration.yml` 启动 etcd / redis / postgres。
- 运行本地 qlean smoke 集成测试。
- 可选执行 fuzz 探索。

帮助：

```bash
./run_integration_tests.sh --help
```

常用示例：

```bash
./run_integration_tests.sh
./run_integration_tests.sh --skip-deps --skip-services
```

## 5. 依赖准备脚本

脚本：`install_xfstests_deps.sh`

作用：

- 安装 xfstests 本地运行所需系统依赖。
- 拉取 xfstests 相关 Git LFS 资源。

默认行为：

- 执行 `sudo apt-get update` 与依赖安装。
- 执行 `git lfs install --local`。
- 拉取以下资源：
  - `project/slayerfs/tests/scripts/xfstests-prebuilt/*.tar.gz`
  - `project/slayerfs/tests/scripts/fuse3-bundle/fusermount3`

帮助：

```bash
./install_xfstests_deps.sh --help
```

常用示例：

```bash
./install_xfstests_deps.sh
./install_xfstests_deps.sh --skip-system-deps
./install_xfstests_deps.sh --skip-lfs
```

## 6. 后端服务管理脚本

脚本：`manage_xfstests_backend_services.sh`

作用：

- 启动或停止 redis / etcd 对应的 docker compose 服务。
- 在 `up` 时等待服务可用。

帮助：

```bash
./manage_xfstests_backend_services.sh --help
```

命令格式：

```bash
./manage_xfstests_backend_services.sh <up|down> <sqlite|redis|etcd>
```

说明：

- `sqlite` 不依赖 docker compose 服务，脚本会直接返回。
- `sqlite` 对应 `docker compose.sqlite.yml`。
- `redis` 会操作 `docker compose.redis.yml` 中的 `redis` 服务。
- `etcd` 会操作 `docker compose.etcd.yml` 中的 `etcd` 服务。
- 这些脚本默认不会启用 `s3-stack` profile，因此不会主动拉起 `rustfs`、`rustfs-init` 或 `slayerfs`。

常用示例：

```bash
./manage_xfstests_backend_services.sh up redis
./manage_xfstests_backend_services.sh down redis

./manage_xfstests_backend_services.sh up etcd
./manage_xfstests_backend_services.sh down etcd
```

## 7. 共享执行器脚本

脚本：`run_xfstests_backend.sh`

作用：

- 按指定元数据后端运行 KVM xfstests 集成测试。
- 按参数决定是否准备依赖、是否启动后端服务、是否构建 `persistence_demo`。
- 调用 Rust 测试入口：
  - `test_slayerfs_kvm_xfstests_sqlite`
  - `test_slayerfs_kvm_xfstests_redis`
  - `test_slayerfs_kvm_xfstests_etcd`

默认行为：

- 默认会调用 `install_xfstests_deps.sh`。
- 默认会在 redis / etcd 场景下调用 `manage_xfstests_backend_services.sh up`。
- 默认会执行：

```bash
cargo build -p slayerfs --example persistence_demo --release
```

- 默认使用仓库中的 exclude 文件：

```text
project/slayerfs/tests/scripts/xfstests_slayer.exclude
```

也就是说，这个脚本不再通过命令行参数指定单个 case，而是走仓库当前维护的 exclude 集。

帮助：

```bash
./run_xfstests_backend.sh --help
```

命令格式：

```bash
./run_xfstests_backend.sh <sqlite|redis|etcd> [选项]
```

支持选项：

- `--skip-deps`：跳过 apt 系统依赖安装。
- `--skip-lfs`：跳过 Git LFS 拉取。
- `--skip-build`：跳过 `persistence_demo` 构建。
- `--skip-services`：跳过 docker compose 服务启停。
- `--keep-services`：测试结束时不停止服务。
- `--timeout-secs <秒>`：覆盖 `SLAYERFS_XFSTESTS_TIMEOUT_SECS`。
- `--force-reclone <0|1>`：覆盖 `SLAYERFS_XFSTESTS_FORCE_RECLONE`。
- `--artifact-root <目录>`：覆盖 `SLAYERFS_XFSTESTS_HOST_ARTIFACT_ROOT`。

常用示例：

```bash
./run_xfstests_backend.sh sqlite
./run_xfstests_backend.sh redis
./run_xfstests_backend.sh etcd

./run_xfstests_backend.sh redis --skip-deps --keep-services
./run_xfstests_backend.sh etcd --timeout-secs 14400
./run_xfstests_backend.sh sqlite --artifact-root /tmp/slayerfs-kvm-xfstests/manual/sqlite
```

## 8. 三个直接入口脚本

### 8.1 SQLite

脚本：`run_xfstests_sqlite.sh`

作用：

- 等价于：

```bash
./run_xfstests_backend.sh sqlite
```

示例：

```bash
./run_xfstests_sqlite.sh
./run_xfstests_sqlite.sh --skip-deps
```

### 8.2 Redis

脚本：`run_xfstests_redis.sh`

作用：

- 等价于：

```bash
./run_xfstests_backend.sh redis
```

示例：

```bash
./run_xfstests_redis.sh
./run_xfstests_redis.sh --keep-services
```

### 8.3 Etcd

脚本：`run_xfstests_etcd.sh`

作用：

- 等价于：

```bash
./run_xfstests_backend.sh etcd
```

示例：

```bash
./run_xfstests_etcd.sh
./run_xfstests_etcd.sh --skip-build --timeout-secs 14400
```

## 9. 推荐的手动执行方式

如果你想把步骤拆开执行，建议使用下面的方式。

### 9.1 SQLite

```bash
cd project/slayerfs/docker
./install_xfstests_deps.sh
./run_xfstests_sqlite.sh --skip-deps
```

### 9.2 Redis

```bash
cd project/slayerfs/docker
./install_xfstests_deps.sh
./manage_xfstests_backend_services.sh up redis
./run_xfstests_redis.sh --skip-deps --skip-services
./manage_xfstests_backend_services.sh down redis
```

### 9.3 Etcd

```bash
cd project/slayerfs/docker
./install_xfstests_deps.sh
./manage_xfstests_backend_services.sh up etcd
./run_xfstests_etcd.sh --skip-deps --skip-services
./manage_xfstests_backend_services.sh down etcd
```

## 10. 结果产物

默认情况下，测试产物根目录为：

```text
/tmp/slayerfs-kvm-xfstests/local/<backend>
```

其中 `<backend>` 是：

- `sqlite`
- `redis`
- `etcd`

如果需要改目录，可以通过：

```bash
--artifact-root <目录>
```

来覆盖。

## 11. image 维护说明

如果只想在某个后端 compose 上维护 `slayerfs:local` image，可以直接使用对应 compose 的 `slayerfs-image` 服务。

示例：

```bash
docker compose -f docker compose.sqlite.yml build slayerfs-image
docker compose -f docker compose.redis.yml build slayerfs-image
docker compose -f docker compose.etcd.yml build slayerfs-image
```

如果想直接拉起与 RustFS 对齐配置的 SlayerFS 栈，可以显式启用 `s3-stack` profile，例如：

```bash
docker compose -f docker compose.sqlite.yml --profile s3-stack up -d rustfs rustfs-init slayerfs
docker compose -f docker compose.redis.yml --profile s3-stack up -d rustfs rustfs-init slayerfs
docker compose -f docker compose.etcd.yml --profile s3-stack up -d rustfs rustfs-init slayerfs
```

## 12. Compose 文件说明

当前本地 compose 已按元数据后端拆分：

- `docker compose.integration.yml`
- `docker compose.sqlite.yml`
- `docker compose.redis.yml`
- `docker compose.etcd.yml`

设计目的：

- 让 redis / etcd 的后端服务管理按后端隔离。
- 给每个后端保留各自的 `slayerfs-image` 定义，便于后续单独维护 `slayerfs:local` image。
- 给每个后端保留一套与 RustFS 对齐的数据后端配置，便于按 profile 启动完整的对象存储 + SlayerFS 组合。

其中后端 compose 下的 `slayerfs-image` 服务使用：

- `image: slayerfs:local`
- `build.context: ../..`
- `build.dockerfile: slayerfs/docker/Dockerfile`

它默认挂在 `image-maintenance` profile 下，当前这组 xfstests 本地脚本不会主动拉起它。

后端 compose 中额外的 `rustfs`、`rustfs-init` 和 `slayerfs` 服务则挂在 `s3-stack` profile 下：

- `rustfs` 使用 `rustfs/rustfs:latest` 提供 S3 兼容对象存储。
- `rustfs-init` 使用 `amazon/aws-cli:2` 以 path-style 方式确保 `slayerfs-data` bucket 存在。
- `slayerfs` 使用本地 `slayerfs:local` image，并默认配置为通过 `http://rustfs:9000` 访问 RustFS。

`docker compose.integration.yml` 则保留给本地 integration / qlean smoke 路径使用。

## 13. 注意事项

- 这些脚本的目标是对齐 GitHub Actions 里的 xfstests 本地跑法，而不是替代仓库中的所有集成测试脚本。
- `run_xfstests_backend.sh` 当前默认依赖仓库中的 exclude 文件，不支持再从命令行直接传单个 case。
- Redis / Etcd 场景如果使用了 `--skip-services`，需要你自己确保对应后端已经可用。
- 如果使用了 `--skip-build`，需要你自己确保 `persistence_demo` 已经提前构建完成。