# Profiling

本文档汇总 SlayerFS 的常用性能分析方法，包括：
- tracing-chrome（Perfetto/Chrome trace）
- Criterion 自带 flamegraph
- tracing-flame（运行时 trace -> flamegraph）
- tokio-console（异步任务/等待分析）
- jemalloc heap profiling（内存占用热点）

> 建议只在分析时开启 profiling，避免影响结果。

## Tracing Chrome（Bench）

```
SLAYERFS_BENCH_CHROME=/tmp/slayerfs_trace.json \
RUST_LOG=slayerfs=trace \
cargo bench --bench slayerfs_bench -- slayerfs_big_file/write
```

打开 trace 文件：
- Perfetto（推荐）：`https://ui.perfetto.dev`
- Chrome：`chrome://tracing`

相比之下tracing-chrome最通用，它可以看到一个包含on-cpu和off-cpu的完整时间线，并且可以用SQL查询关键指标。但是它只能看到被`tracing`打出的span，需要手动补充span和instrument，用起来较为繁琐。

### 常用的 span 添加方式

#### 1) 给函数加 `#[tracing::instrument]`

适合快速覆盖函数整体耗时。

```rust
#[tracing::instrument(level = "trace", skip(self, buf), fields(offset, len = buf.len()))]
async fn read_at(&self, offset: u64, buf: &mut [u8]) -> anyhow::Result<usize> {
    // ...
    Ok(buf.len())
}
```

#### 2) 只包一段逻辑（子阶段）

适合细分内部步骤，比如锁等待、IO、拷贝等。

```rust
let _span = tracing::trace_span!("read_at.split_spans", offset, len = actual_len).entered();
let spans = split_chunk_spans(self.config.layout, offset, actual_len);
```

#### 3) 给 async 片段打 span

```rust
use tracing::Instrument;

let out = async {
    fetcher.prepare_slices().await?;
    fetcher.read_at(start, len).await
}
.instrument(tracing::trace_span!("fetch.read_range", start, len))
.await?;
```

#### 4) 动态记录字段

当字段只有在运行时才能确定，可以在 span 内部补充：

```rust
let span = tracing::trace_span!("read_range", key = %key_str, offset, len);
let _enter = span.enter();
span.record("read_len", read_len);
```

> 实践建议：大对象用 `skip(...)`，只记录关键字段，避免 trace 文件过大。比如对`data: &[u8]`使用`skip(data)`，另加一个`fields(len = data.len())`记录长度即可。

## Tracing Chrome（主程序）

```
sudo RUST_LOG=slayerfs=trace \
     SLAYERFS_TRACE_CHROME=/tmp/slayerfs.trace.json \
     ./target/release/slayerfs mount /mnt/slayerfs \
       --meta-backend etcd \
       --meta-etcd-urls http://127.0.0.1:2379
```

用主程序进行tracing的好处是可以使用Fio等工具进行压测，得到压测结果的分析。

## Criterion 自带 Flamegraph（Bench）

```
SLAYERFS_BENCH_FLAMEGRAPH=1 \
cargo bench --bench slayerfs_bench -- slayerfs_big_file/write
```

输出位置示例：
```
target/criterion/slayerfs_big_file/write/1/profile/flamegraph.svg
```

Criterion自带的Flamegraph最为懒人，只需要一条环境变量即可开启，但是它是on-cpu的火焰图，看不到off-cpu的结果，这导致了一些问题:

曾经 VFS 每次调用 write 都会执行 `extend_file_size`，实际等待接近 4 秒，但火焰图中看不到这部分等待，导致看起来像是内存拷贝和`writev`各占一半（约0.5秒），从而导致误判瓶颈。

## tracing-flame（运行时 trace -> flamegraph）

主程序内置 `tracing-flame` 开关：

```
SLAYERFS_TRACE_FLAME=/tmp/slayerfs.folded \
RUST_LOG=slayerfs=trace \
./target/release/slayerfs mount /mnt/slayerfs \
  --meta-backend etcd \
  --meta-etcd-urls http://127.0.0.1:2379
```

停止进程后生成 `/tmp/slayerfs.folded`，再用 flamegraph 工具生成图：

```
inferno-flamegraph /tmp/slayerfs.folded > /tmp/slayerfs_flame.svg
```

> 需要关键路径存在 `tracing::instrument` 或显式 span 才有价值。

能看到整体时间分布，输出简单，易于对比不同版本，但是只能看到被`tracing::instrument`的函数，看不到函数内部的细粒度span。

## tokio-console（任务/等待分析）

主程序已集成 `console-subscriber`，通过环境变量开启：

```
RUSTFLAGS="--cfg tokio_unstable" \
TOKIO_CONSOLE=1 \
RUST_LOG=slayerfs=info \
./target/release/slayerfs mount /mnt/slayerfs \
  --meta-backend etcd \
  --meta-etcd-urls http://127.0.0.1:2379
```

另起终端运行：
```
tokio-console
```

默认连接 `http://127.0.0.1:6669`。若提示“不支持 state streaming”，请确认二进制使用
`RUSTFLAGS="--cfg tokio_unstable"` 编译。

tokio-console 专注于 tokio 任务/等待时间，定位异步瓶颈很有效，但是它只覆盖 tokio 运行时层面，缺少完整的调用栈视图。

## jemalloc heap profiling（内存热点）

需要使用

### 1) 采集 heap 文件

需要使用`cargo build --release --features jemalloc-profiling`构建。

```
sudo RUST_LOG=slayerfs::vfs::io::reader=trace --preserve-env=_RJEM_MALLOC_CONF \
              _RJEM_MALLOC_CONF="prof:true,prof_active:true,prof_final:true,lg_prof_interval:30,lg_prof_sample:21,prof_prefix:/tmp/slayerfs,confirm_conf:true" \
              ../target/release/slayerfs mount /mnt/slayerfs \
              --meta-backend etcd \
              --meta-etcd-urls http://127.0.0.1:2379
```

会生成类似：
```
/tmp/slayerfs_heap.<pid>.<seq>.heap
```

### 2) 生成 SVG/PDF

```
BIN=$(ls target/release/deps/slayerfs_bench-* | head -n 1)
jeprof --show_bytes --svg "$BIN" /tmp/slayerfs_heap.*.heap > /tmp/slayerfs_heap.svg
```

> 如果符号不完整，可用 `RUSTFLAGS="-g"` 重新编译再采集。

jemalloc heap profiling专用于堆内存分析，不反应 CPU/IO。
