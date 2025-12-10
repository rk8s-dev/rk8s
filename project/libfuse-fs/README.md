# FUSE Filesystem Library 

A ready-to-use filesystem library based on FUSE (Filesystem in Userspace). This library provides implementations for various filesystem types and makes it easy to develop custom filesystems.

Features:
- Asynchronous I/O support
- Overlay filesystem implementation
- Passthrough filesystem support
- Easy-to-use API for custom filesystem development


## Try
```bash
# run OverlayFS demo
cargo run --example overlay -- -o lowerdir=/usr:/bin,upperdir=/tmp/ovl_upper,workdir=/tmp/ovl_work overlay_test /tmp/ovl_mnt

# run PassthroughFS demo  
cargo run --example passthrough -- /tmp/source_dir /tmp/pt_mnt

```

## Integration Tests (OverlayFS & PassthroughFS)

We provide an integration script that mounts overlayfs and passthroughfs examples, then runs:

* IOR for basic POSIX read/write validation.
* fio for sequential write/read and random mixed workloads.

### Run Locally
Requirements: `fio`, `ior`, `fuse3` (providing `fusermount3`), Rust toolchain.

```bash
cd project/libfuse-fs
./tests/integration_test.sh
```

Set `WORKDIR=/custom/tmp` to control temporary directory. Logs are stored under `$WORKDIR/logs`.

### GitHub Actions
Workflow file: `.github/workflows/libfuse-fs-integration.yml` (runs on PR touching this crate).

### Examples
Example binaries used by the integration tests:
```bash
cargo run --example overlayfs -- \
	--mountpoint /tmp/ovl_mnt --upperdir /tmp/ovl_upper \
	--lowerdir /usr --lowerdir /bin

cargo run --example passthrough -- \
	--mountpoint /tmp/pt_mnt --rootdir /var/tmp
```

### Rootless Execution

For rootless execution of the passthrough filesystem, you need to grant the necessary capabilities to the binary:

```bash
# Build the example first
cargo build --example passthrough

# Grant capabilities for rootless operation
sudo setcap cap_dac_read_search+ep ../target/debug/examples/passthrough
```

This allows the passthrough filesystem to access files with elevated permissions without requiring the entire process to run as root.

## UnionFS vs OverlayFS Implementation Details

This library provides both UnionFS and OverlayFS implementations with significant architectural differences:

### Core Architecture Differences

**UnionFS (`src/unionfs/`)**:
- **Dynamic Layer System**: Uses `BoxedLayer = dyn Layer` trait objects for flexible layer composition
- **Async Trait System**: Layer trait extends `ObjectSafeFilesystem` with `#[async_trait]` for true async operations
- **Context-Aware Operations**: Supports `OperationContext` for UID/GID overriding during copy-up operations
- **Advanced Layer Methods**: Implements context-aware creation methods:
  - `create_with_context()` - File creation with custom ownership
  - `mkdir_with_context()` - Directory creation with custom ownership
  - `symlink_with_context()` - Symlink creation with custom ownership
  - `do_getattr_helper()` - Raw metadata retrieval bypassing ID mapping

**OverlayFS (`src/overlayfs/`)**:
- **Static Layer System**: Uses concrete `BoxedLayer = PassthroughFs` type for better performance
- **Simpler Trait System**: Layer trait extends basic `Filesystem` without async_trait overhead
- **Basic Operations**: Provides only fundamental overlay operations (whiteouts, opaque handling)
- **Direct Passthrough**: Layers are directly bound to PassthroughFs instances

### Type System Comparison

| Aspect | UnionFS | OverlayFS |
|--------|---------|-----------|
| Layer Type | `dyn Layer` (trait object) | `PassthroughFs` (concrete) |
| Trait Bounds | `ObjectSafeFilesystem + async_trait` | `Filesystem` |
| Inode Storage | `Arc<BoxedLayer>` | `Arc<PassthroughFs>` |
| Async Support | Full async trait methods | Standard filesystem methods |

### Implementation Complexity

**UnionFS Advanced Features**:
```rust
// Context-aware file creation with custom UID/GID
async fn create_with_context(
    &self,
    ctx: OperationContext,
    parent: Inode,
    name: &OsStr,
    mode: u32,
    flags: u32,
) -> Result<ReplyCreated>

// Helper for metadata bypassing ID mapping
async fn do_getattr_helper(
    &self,
    inode: Inode,
    handle: Option<u64>,
) -> std::io::Result<(libc::stat64, Duration)>
```

**OverlayFS Simplified Approach**:
```rust
// Basic layer trait with essential overlay operations
pub trait Layer: Filesystem {
    fn root_inode(&self) -> Inode;
    async fn create_whiteout(...) -> Result<ReplyEntry>;
    async fn delete_whiteout(...) -> Result<()>;
    async fn set_opaque(...) -> Result<()>;
    async fn is_opaque(...) -> Result<bool>;
}
```

### Performance vs Flexibility Trade-offs

**UnionFS**:
- ✅ Maximum flexibility with dynamic layer composition
- ✅ Advanced context-aware operations for complex scenarios
- ✅ True async support for better concurrency
- ❌ Higher runtime overhead due to trait objects
- ❌ More complex codebase

**OverlayFS**:
- ✅ Better performance with static typing
- ✅ Simpler implementation, easier to maintain
- ✅ Lower memory overhead
- ❌ Limited to PassthroughFs layers only
- ❌ Less flexibility for advanced use cases

### Use Case Recommendations

- **Choose UnionFS** when:
  - You need custom layer implementations beyond PassthroughFs
  - You require advanced copy-up with custom ownership contexts
  - You need maximum flexibility for complex overlay scenarios
  - Async performance is critical for your workload

- **Choose OverlayFS** when:
  - You only need PassthroughFs-based layers
  - Performance is the primary concern
  - You prefer a simpler, more maintainable codebase
  - Basic overlay functionality suffices for your needs

## Contributing
All commits must be signed (`git commit -s`) and GPG signed (`-S`) per project policy.

## Changelog
See the [CHANGELOG](./CHANGELOG.md) for version history.
