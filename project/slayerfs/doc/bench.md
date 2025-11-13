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

| 变量 | 含义 |
| ---- | ---- |
| `SLAYERFS_BENCH_THREADS` (4) | 并发线程数 |
| `SLAYERFS_BENCH_BLOCK_MB` (1) | 单次 IO 块大小（MiB），同时写入 `ChunkLayout.block_size` |
| `SLAYERFS_BENCH_BIG_FILE_MB` (1024) | 每个大文件的逻辑大小（MiB） |
| `SLAYERFS_BENCH_SMALL_FILE_KB` (128) | 小文件大小（KiB） |
| `SLAYERFS_BENCH_SMALL_FILE_COUNT` (100) | 每线程小文件数量 |
| `SLAYERFS_BENCH_SAMPLE_SIZE` (≥10) | Criterion 样本数，至少 10 个 |
| `SLAYERFS_BENCH_FLAMEGRAPH`（未设置） | 任意值即可开启火焰图采集 |
| `SLAYERFS_BENCH_DATA_DIR`（未设置） | 指定对象根目录；默认用系统临时目录并在结束后删除 |

不设置 `SLAYERFS_BENCH_DATA_DIR` 时，测试数据会写入 `TempDir` 并随运行结束自动清理。若指定该变量，每次运行都会在该目录下创建 `slayerfs_bench_<timestamp>` 子目录并在结束后删除，方便放在大容量磁盘上做长测。

示例（缩小数据量，适合快速检查）：

```
SLAYERFS_BENCH_THREADS=2 \
SLAYERFS_BENCH_BIG_FILE_MB=4 \
SLAYERFS_BENCH_SMALL_FILE_COUNT=8 \
cargo bench --bench slayerfs_bench -- --warm-up-time 1
```

输出示例：

```
slayerfs_big_file/write/2  time: [4.69 ms 4.78 ms]  thrpt: [1.63 GiB/s 1.66 GiB/s]
slayerfs_small_file/read/2 time: [656 µs 669 µs]   thrpt: [23.9 Kops/s 24.4 Kops/s]
slayerfs_stat/stat/2       time: [48.8 µs 50.4 µs] thrpt: [317 Kops/s 328 Kops/s]
```

Criterion 会把每个基准的 HTML 报告写入 `target/criterion/<group>/<bench>/report/`，方便跨分支或后端对比。

## 火焰图

1. **准备 perf**：Linux 需安装 `perf` 并放宽权限，例如 `sudo sysctl kernel.perf_event_paranoid=-1`、`sudo sysctl kernel.kptr_restrict=0`。
2. **启用采样**：设置 `SLAYERFS_BENCH_FLAMEGRAPH=1` 并在 Criterion 参数中加入 `--profile-time <秒>`；未指定时不会采样，也不会生成 SVG。

示例命令：

```
SLAYERFS_BENCH_DATA_DIR=. \
SLAYERFS_BENCH_FLAMEGRAPH=1 \
SLAYERFS_BENCH_BIG_FILE_MB=256 \
cargo bench --bench slayerfs_bench -- --warm-up-time 1 --profile-time 5
```

运行结束后，可在 `target/criterion/slayerfs_big_file/read/1/profile/flamegraph.svg` 等目录中查看火焰图。
