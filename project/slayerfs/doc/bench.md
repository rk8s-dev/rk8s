# SlayerFS 基准测试

SlayerFS 自带一套基于 Criterion 的基准测试，用来复现 `juicefs bench` 的典型工作负载。当前覆盖三种场景：

| 阶段 | 场景说明 | 输出指标 |
| ---- | -------- | -------- |
| 大文件读写 | 多线程顺序写/读，每个线程操作一个大文件，块级顺序 IO | GiB/s 或 MiB/s 吞吐 |
| 小文件读写 | 每线程创建/读取大量 128 KiB 文件，压测元数据与对象存储管线 | 每秒文件数、单文件时延 |
| 纯 `stat` | 对同一批小文件反复 `stat` | 元数据操作数/秒 |

每次执行都会启动一个临时 LocalFs 对象根目录与内存元数据实例，测试完成后自动清理。

## 运行方式

```
cargo bench --bench slayerfs_bench
```

常用环境变量（括号内为默认值）：

| 变量                                      | 含义                                           |
|-----------------------------------------|----------------------------------------------|
| `SLAYERFS_BENCH_THREADS` (4)            | 并发线程数                                        |
| `SLAYERFS_BENCH_BLOCK_MB` (1)           | 单次 IO 块大小（MiB），同时写入 `ChunkLayout.block_size` |
| `SLAYERFS_BENCH_BIG_FILE_MB` (512)      | 每个大文件的逻辑大小（MiB）                              |
| `SLAYERFS_BENCH_SMALL_FILE_KB` (128)    | 小文件大小（KiB）                                   |
| `SLAYERFS_BENCH_SMALL_FILE_COUNT` (100) | 每线程小文件数量                                     |
| `SLAYERFS_BENCH_SAMPLE_SIZE` (≥10)      | Criterion 样本数，至少 10 个                        |
| `SLAYERFS_BENCH_FLAMEGRAPH`（未设置）        | 任意值即可开启火焰图采集                                 |
| `SLAYERFS_BENCH_DATA_DIR`（未设置）          | 指定对象根目录；默认用系统临时目录并在结束后删除                     |
| `SLAYERFS_BENCH_MODE`（direct）            | 压测模式：`direct` 直接调用 VFS，`fuse` 通过 FUSE 挂载（仅 Linux） |
| `SLAYERFS_BENCH_BACKEND`（local）         | 对象存储类型：`local` 或 `s3`                        |
| `SLAYERFS_BENCH_S3_BUCKET`（空）           | 当 `BACKEND=s3` 时必须指定的 S3 桶                   |
| `SLAYERFS_BENCH_S3_REGION`（空）           | 可选，S3 区域                                     |
| `SLAYERFS_BENCH_S3_ENDPOINT`（空）         | 可选，自定义兼容端点/MinIO                             |
| `SLAYERFS_BENCH_S3_FORCE_PATH_STYLE`（未设置） | 可选布尔值，设为 `true` 可强制 path-style 访问            |
| `SLAYERFS_BENCH_META_URL`（`sqlite::memory:`） | 元数据后端 URL，兼容 SQLite/PostgreSQL/Redis/Etcd 等 |

SLAYERFS_BENCH_BIG_FILE_MB=256 \
cargo bench --bench slayerfs_bench -- --warm-up-time 1 --profile-time 5
```

运行结束后，可在 `target/criterion/slayerfs_big_file/read/1/profile/flamegraph.svg` 等目录中查看火焰图。

> **提示**：这里展示的吞吐/延迟更多用于排查热点、配合火焰图定位问题，并不直接等同于生产环境的绝对性能。默认配置会大量命中缓存，也跳过 FUSE/内核调度开销（结果是导致结果异常偏高）；若要做真实性能测试，请开启 `SLAYERFS_BENCH_MODE=fuse` 或直接在挂载点用 fio 之类工具，并视情况关闭缓存、提前 `drop_caches`。

## 使用 perf 分析 FUSE 场景

当你在 FUSE 挂载点运行 fio 等压力测试时，可以用 `perf` 直接对 `slayerfs` 进程取样：

```
# 1. 用带符号的 release 版本启动 FUSE
RUSTFLAGS="-C force-frame-pointers=yes" CARGO_PROFILE_RELEASE_DEBUG=true \
  cargo build --release --example mount_local
sudo perf record -F 99 -g -- \
  ./target/release/examples/mount_local data mountpoint

# 2. 在另一个终端运行 fio（或其他工作负载）
fio --directory=mountpoint --rw=read --size=4G --bs=1M --direct=1 ...

# 3. 压测结束后回到 perf 那个终端按 Ctrl+C，生成火焰图
sudo perf script | inferno-collapse-perf > out.folded
inferno-flamegraph out.folded > flame.svg
```

wrap 模式会捕获进程的所有线程，并覆盖从启动到关闭的完整生命周期；如果服务器需要长时间常驻，可以在 wrap 命令中配合 `timeout` 或 `sleep` 控制采样窗口，避免生成过大的 `perf.data`。同样务必开启 `debug = true` 与 `RUSTFLAGS="-C force-frame-pointers=yes"`，以免火焰图里充满 `unknown`。最终得到的 `flame.svg` 可以与 `slayerfs_bench` 的结果互相印证，帮助你在真实 FUSE 负载下定位瓶颈如果缺少`inferno-flamegraph`请使用`cargo install inferno`，并确保`~/.cargo/bin`已经加入了PATH。

