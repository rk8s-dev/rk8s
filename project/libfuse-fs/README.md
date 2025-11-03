# FUSE Filesystem Library 

A ready-to-use filesystem library based on FUSE (Filesystem in Userspace). This library provides implementations for various filesystem types and makes it easy to develop custom filesystems.

Features:
- Asynchronous I/O support
- Overlay filesystem implementation
- Passthrough filesystem support
- Easy-to-use API for custom filesystem development


### Try
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

### Contributing
All commits must be signed (`git commit -s`) and GPG signed (`-S`) per project policy.
