# SlayerFS SDK 使用说明

本文档介绍基于 `VfsClient` 的应用接口，便于在不挂载 FUSE 的情况下直接以“路径”读写文件。

## 设计目标
- 提供接近 POSIX 的基础路径 API：
  - 目录：`mkdir_p`、`readdir`、`rmdir`
- 文件：`create_file`、`write_at`、`read_at`、`stat`、`unlink`、`rename`、`truncate`
- 后端可插拔：
  - 数据由 `BlockStore`（本地目录/对象存储）承载
  - 元数据由 `MetaStore` 承载（当前内存实现 InMemory，用于单机/开发）
- 读写语义：按 Chunk/Block 分片；读遇到“洞”返回 0 填充；写完再聚合一次性更新文件大小

## 快速开始（本地目录后端）
```rust
use slayerfs::ChunkLayout;
use slayerfs::LocalClient;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let layout = ChunkLayout::default(); // 默认 64MiB chunk / 4MiB block
    let root = "/tmp/slayerfs-objstore"; // 用一个本地目录模拟对象存储
    let mut cli = LocalClient::new_local(root, layout).await.unwrap();

    // 目录 + 文件
    cli.mkdir_p("/a/b").await.unwrap();
    cli.create_file("/a/b/hello.txt", false).await.unwrap();

    // 写入跨块数据
    let half = (layout.block_size / 2) as usize;
    let len = layout.block_size as usize + half;
    let mut data = vec![0u8; len];
    for i in 0..len { data[i] = (i % 251) as u8; }
    cli.write_at("/a/b/hello.txt", half as u64, &data).await.unwrap();

    // 读取并校验
    let out = cli.read_at("/a/b/hello.txt", half as u64, len).await.unwrap();
    assert_eq!(out, data);

    // 目录与属性
    let entries = cli.readdir("/a/b").await.unwrap();
    assert!(entries.iter().any(|e| e.name == "hello.txt"));
    let st = cli.stat("/a/b/hello.txt").await.unwrap();
    println!("size={} kind={:?}", st.size, st.kind);
}
```

## API 速览
- `mkdir_p(path) -> io::Result<()>`：递归创建目录；中间段为文件时报错 `"not a directory"`
- `create_file(path, create_new) -> io::Result<()>`：创建文件；若同名目录存在报错 `"is a directory"`；`create_new=true` 时已存在会报错
- `write_at(path, offset, data) -> io::Result<usize>`：按文件偏移写入，跨 Chunk/Block 自动拆分
- `read_at(path, offset, len) -> io::Result<Vec<u8>>`：按文件偏移读取；对未写入区域 0 填充
- `readdir(path) -> io::Result<Vec<VfsDirEntry>>`：列目录；不包含 "." 与 ".."
- `stat(path) -> io::Result<VfsFileAttr>`：获取 kind/size；size 来自元数据层
- `unlink(path) -> io::Result<()>`：删除文件；目录会报错 `"is a directory"`
- `rmdir(path) -> io::Result<()>`：删除空目录；根目录不可删除；非空报错 `"directory not empty"`
- `rename(old, new) -> io::Result<()>`：仅文件；目标不得存在；目标父目录缺失会自动创建
- `truncate(path, size) -> io::Result<()>`：仅更新文件 size；收缩不立即清理块数据

类型摘录：
- `VfsDirEntry { name: String, ino: i64, kind: VfsFileType }`
- `VfsFileAttr { ino: i64, size: u64, kind: VfsFileType }`

## 后端与布局
- 布局：`ChunkLayout { chunk_size: u64, block_size: u32 }`，默认 64MiB/4MiB，可自定义传入
- 本地目录后端：`LocalClient::new_local(root, layout)`
- 对象后端：以 `ObjectBlockStore<B>` 对接 `ObjectBackend`（如 S3）；可自行构造 `VfsClient<S, M>`

## 语义与注意事项
- 错误返回为 `io::Error`，建议上层按需映射到 errno（ENOENT/EEXIST/ENOTDIR/EISDIR/ENOTEMPTY 等）
- 读零填充：读取未写入范围会返回 0 字节（JuiceFS 类似体验）
- 原子性：一次 `write_at` 的 size 更新在末尾聚合提交；跨多个 chunk 的强原子性后续可引入更细粒度事务
- GC：`unlink/rmdir` 仅更新命名空间与元数据，底层块/切片回收将由后续实现补充

## 测试与演示
- 本仓库内含端到端测试（`VfsClient`/VFS 层）与一个本地演示入口（`demo-localfs`）
- 你可以运行测试或示例来验证行为（见根目录 README）

---
如需在 README 中加入更多高级示例（如 S3 后端或并发写读场景），可以在此文档基础上继续扩展。
