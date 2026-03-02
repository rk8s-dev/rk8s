# SlayerFS Fuzz 测试指南（libFuzzer）

本文档说明如何在 SlayerFS 中运行 `libFuzzer`、定位并最小化复现用例，以及生成和查看覆盖率报告。

当前默认 fuzz target：`fs_ops`（`fuzz/fuzz_targets/fs_ops.rs`），覆盖以下操作组合：

- `Write`
- `Read`
- `Truncate`
- `Rename`
- `Hard link`
- `Symbolic link`

## 1. 环境准备

在项目根目录执行（即 `slayerfs/` 目录）：

```bash
rustup toolchain install nightly
rustup component add --toolchain nightly llvm-tools-preview
cargo install cargo-fuzz
```

检查 fuzz targets：

```bash
cargo +nightly fuzz list
```

## 2. 常用目录说明

- Fuzz target：`fuzz/fuzz_targets/fs_ops.rs`
- 语料库（corpus）：`fuzz/corpus/fs_ops/`
- 崩溃样本（artifact）：`fuzz/artifacts/fs_ops/`
- 覆盖率原始数据：`fuzz/coverage/fs_ops/raw/*.profraw`
- 覆盖率合并数据：`fuzz/coverage/fs_ops/coverage.profdata`

## 3. 如何运行 fuzz 测试

### 3.1 快速冒烟

```bash
cargo +nightly fuzz run fs_ops -- -runs=1
```

### 3.2 持续 fuzz（建议）

```bash
# 跑 30 分钟
cargo +nightly fuzz run fs_ops -- -max_total_time=1800

# 并行（示例：8 worker）
cargo +nightly fuzz run fs_ops -- -max_total_time=3600 -jobs=8 -workers=8

# 若出现 out-of-memory，可提高 libFuzzer RSS 限制
cargo +nightly fuzz run fs_ops -- -max_total_time=1800 -rss_limit_mb=4096
```

### 3.3 控制输入规模/复现随机性

```bash
# 限制输入长度，避免样本过大
cargo +nightly fuzz run fs_ops -- -max_len=1024 -max_total_time=1800

# 固定 seed 方便调试（不是严格跨版本稳定）
cargo +nightly fuzz run fs_ops -- -seed=12345 -runs=10000
```

## 4. 如何定位与最小化复现用例

### 4.1 复现崩溃

崩溃后会在 `fuzz/artifacts/fs_ops/` 生成文件（例如 `crash-*`）。

直接复现：

```bash
cargo +nightly fuzz run fs_ops fuzz/artifacts/fs_ops/crash-xxxx
```

### 4.2 最小化单个崩溃样本（tmin）

```bash
cargo +nightly fuzz tmin fs_ops fuzz/artifacts/fs_ops/crash-xxxx -r 1000
```

可追加 libFuzzer 参数：

```bash
cargo +nightly fuzz tmin fs_ops fuzz/artifacts/fs_ops/crash-xxxx -r 1000 -- -timeout=20
```

### 4.3 最小化语料库（cmin）

```bash
# 使用默认 corpus 目录
cargo +nightly fuzz cmin fs_ops

# 或显式指定 corpus 目录
cargo +nightly fuzz cmin fs_ops fuzz/corpus/fs_ops
```

## 5. 如何生成和查看覆盖率

### 5.1 生成覆盖率数据

```bash
cargo +nightly fuzz coverage fs_ops
```

执行成功后会生成：

- `fuzz/coverage/fs_ops/raw/default-fs_ops.profraw`
- `fuzz/coverage/fs_ops/coverage.profdata`

### 5.2 汇总查看（llvm-cov report）

> 注意：`cargo fuzz coverage` 对应的可执行文件在 `target/.../coverage/...` 下，
> 不是 `fuzz/target/...`。如果用错二进制，常见报错是 `no coverage data found`。

```bash
BIN=$(find target -type f -path "*/coverage/*/release/fs_ops" | head -n1)
PROF=fuzz/coverage/fs_ops/coverage.profdata

rustup run nightly llvm-cov report "$BIN" \
  -instr-profile="$PROF" \
  --ignore-filename-regex='(\.cargo/registry|/rustc/|/rustup/toolchains/|/target/.*/build/)'
```

### 5.3 查看源码逐行命中（llvm-cov show）

```bash
rustup run nightly llvm-cov show "$BIN" \
  -instr-profile="$PROF" \
  --show-line-counts-or-regions \
  src/vfs/fs.rs
```

### 5.4 生成 HTML 报告

```bash
rustup run nightly llvm-cov show "$BIN" \
  -instr-profile="$PROF" \
  --format=html \
  --output-dir=fuzz/coverage/fs_ops/html \
  --ignore-filename-regex='(\.cargo/registry|/rustc/|/rustup/toolchains/|/target/.*/build/)'
```

HTML 入口：

- `fuzz/coverage/fs_ops/html/index.html`

### 5.5 `llvm-profdata show` 输出如何理解

```bash
rustup run nightly llvm-profdata show --counts fuzz/coverage/fs_ops/coverage.profdata
```

该命令输出是 profile 统计摘要，不是“覆盖率百分比”。

- `Instrumentation level: Front-end`：前端插桩（源码级 coverage 数据）
- `Total functions`：profile 中记录到的函数数（含依赖）
- `Total number of blocks`：参与统计的基本块数
- `Total count`：所有计数器命中次数之和

## 6. 运行时 `cov/ft` 和报告覆盖率的区别

`cargo fuzz run` 终端中显示的 `cov`/`ft`：

- 来自 LLVM SanitizerCoverage（IR 反馈）
- 用于指导变异，不等价于源码行覆盖率

`cargo fuzz coverage` + `llvm-cov report/show`：

- 用于查看函数/行/region 覆盖情况
- 适合评估“哪些代码尚未被触达”

## 7. 常见问题

### 7.1 `error: no such command: fuzz`

说明 `cargo-fuzz` 未安装：

```bash
cargo install cargo-fuzz
```

### 7.2 `Coverage information requires existing input files`

说明 corpus 为空。先跑一段 fuzz 生成语料，再执行 coverage：

```bash
cargo +nightly fuzz run fs_ops -- -max_total_time=120
cargo +nightly fuzz coverage fs_ops
```

### 7.3 `no coverage data found`

通常是 `llvm-cov` 使用了错误的二进制路径。请使用：

- `target/<triple>/coverage/<triple>/release/fs_ops`

而不是：

- `fuzz/target/.../fs_ops`

### 7.4 `libfuzzer out-of-memory`

默认情况下命令启用了Sanitizer, 会造成相当高的内存占用，在命令加上`-s none`来禁用消毒器即可。
