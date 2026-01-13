use std::env;
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pprof::criterion::{Output, PProfProfiler};
use tempfile::TempDir;
use tokio::runtime::{Builder, Runtime};

use slayerfs::cadapter::client::ObjectClient;
use slayerfs::cadapter::localfs::LocalFsBackend;
use slayerfs::cadapter::s3::{S3Backend, S3Config};
use slayerfs::chuck::chunk::ChunkLayout;
use slayerfs::chuck::store::{BlockKey, BlockStore, ObjectBlockStore};
#[cfg(target_os = "linux")]
use slayerfs::fuse::mount::mount_vfs_unprivileged;
use slayerfs::meta::factory::create_meta_store_from_url;
use slayerfs::meta::stores::DatabaseMetaStore;
use slayerfs::vfs::fs::VFS;

const MB: usize = 1024 * 1024;
const KB: usize = 1024;

#[derive(Clone)]
struct BenchConfig {
    block_size_bytes: usize,
    big_file_bytes: usize,
    small_file_bytes: usize,
    small_file_count: usize,
    threads: usize,
    sample_size: usize,
    layout: ChunkLayout,
    backend: BackendMode,
    meta_url: String,
    mode: BenchMode,
}

#[derive(Clone)]
enum BackendMode {
    Local,
    S3(S3BackendOpts),
}

#[derive(Clone, Default)]
struct S3BackendOpts {
    bucket: String,
    region: Option<String>,
    endpoint: Option<String>,
    force_path_style: bool,
}

#[derive(Clone, Copy)]
enum BenchMode {
    Direct,
    Fuse,
}

impl BenchConfig {
    fn from_env() -> Self {
        const DEFAULT_BLOCK_MB: usize = 1;
        const DEFAULT_BIG_FILE_MB: usize = 512;
        const DEFAULT_SMALL_FILE_KB: usize = 128;
        const DEFAULT_SMALL_FILE_COUNT: usize = 100;
        const DEFAULT_THREADS: usize = 4;
        const DEFAULT_SAMPLE_SIZE: usize = 10;

        let block_mb = env_usize("SLAYERFS_BENCH_BLOCK_MB").unwrap_or(DEFAULT_BLOCK_MB);
        let big_mb = env_usize("SLAYERFS_BENCH_BIG_FILE_MB").unwrap_or(DEFAULT_BIG_FILE_MB);
        let small_kb = env_usize("SLAYERFS_BENCH_SMALL_FILE_KB").unwrap_or(DEFAULT_SMALL_FILE_KB);
        let small_file_count =
            env_usize("SLAYERFS_BENCH_SMALL_FILE_COUNT").unwrap_or(DEFAULT_SMALL_FILE_COUNT);
        let threads = env_usize("SLAYERFS_BENCH_THREADS").unwrap_or(DEFAULT_THREADS);
        let sample_size = env_usize("SLAYERFS_BENCH_SAMPLE_SIZE").unwrap_or(DEFAULT_SAMPLE_SIZE);

        let block_size_bytes = block_mb.max(1) * MB;
        let block_size_u32 = block_size_bytes
            .try_into()
            .expect("block size must fit into u32");
        let layout = ChunkLayout {
            block_size: block_size_u32,
            ..Default::default()
        };

        let backend = BackendMode::from_env();
        let meta_url =
            env::var("SLAYERFS_BENCH_META_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());
        let mode = BenchMode::from_env();

        Self {
            block_size_bytes,
            big_file_bytes: big_mb.max(1) * MB,
            small_file_bytes: small_kb.max(1) * KB,
            small_file_count: small_file_count.max(1),
            threads: threads.max(1),
            sample_size: sample_size.max(10),
            layout,
            backend,
            meta_url,
            mode,
        }
    }

    fn big_total_bytes(&self) -> u64 {
        (self.big_file_bytes * self.threads) as u64
    }

    fn small_total_files(&self) -> u64 {
        (self.small_file_count * self.threads) as u64
    }
}

impl BackendMode {
    fn from_env() -> Self {
        let value = env::var("SLAYERFS_BENCH_BACKEND")
            .unwrap_or_else(|_| "local".to_string())
            .to_lowercase();
        match value.as_str() {
            "s3" => {
                let bucket = env::var("SLAYERFS_BENCH_S3_BUCKET")
                    .expect("SLAYERFS_BENCH_S3_BUCKET must be set when backend is s3");
                let region = env::var("SLAYERFS_BENCH_S3_REGION").ok();
                let endpoint = env::var("SLAYERFS_BENCH_S3_ENDPOINT").ok();
                let force_path_style =
                    env_bool("SLAYERFS_BENCH_S3_FORCE_PATH_STYLE").unwrap_or(false);
                BackendMode::S3(S3BackendOpts {
                    bucket,
                    region,
                    endpoint,
                    force_path_style,
                })
            }
            _ => BackendMode::Local,
        }
    }
}

impl BenchMode {
    fn from_env() -> Self {
        let value = env::var("SLAYERFS_BENCH_MODE")
            .unwrap_or_else(|_| "direct".to_string())
            .to_lowercase();
        match value.as_str() {
            "fuse" => BenchMode::Fuse,
            _ => BenchMode::Direct,
        }
    }
}

struct BenchEnv {
    driver: BenchDriver,
    _data_root: Option<BenchRoot>,
    fuse_guard: Option<FuseGuard>,
}

enum BenchStore {
    Local(ObjectBlockStore<LocalFsBackend>),
    S3(ObjectBlockStore<S3Backend>),
}

#[async_trait]
impl BlockStore for BenchStore {
    async fn write_range(&self, key: BlockKey, offset: u32, data: &[u8]) -> anyhow::Result<u64> {
        match self {
            BenchStore::Local(store) => store.write_range(key, offset, data).await,
            BenchStore::S3(store) => store.write_range(key, offset, data).await,
        }
    }

    async fn read_range(&self, key: BlockKey, offset: u32, buf: &mut [u8]) -> anyhow::Result<()> {
        match self {
            BenchStore::Local(store) => store.read_range(key, offset, buf).await,
            BenchStore::S3(store) => store.read_range(key, offset, buf).await,
        }
    }

    async fn delete_range(&self, key: BlockKey, len: usize) -> anyhow::Result<()> {
        match self {
            BenchStore::Local(store) => store.delete_range(key, len).await,
            BenchStore::S3(store) => store.delete_range(key, len).await,
        }
    }
}

type BenchFs = VFS<BenchStore, Arc<DatabaseMetaStore>>;
type SharedFs = Arc<BenchFs>;

#[derive(Clone)]
struct BenchDriver(Arc<BenchDriverInner>);

enum BenchDriverInner {
    Direct(SharedFs),
    Fuse(FuseDriver),
}

#[derive(Clone)]
struct FuseDriver {
    mount_dir: Arc<PathBuf>,
}

impl FuseDriver {
    fn new(mount_dir: PathBuf) -> Self {
        Self {
            mount_dir: Arc::new(mount_dir),
        }
    }

    fn full_path(&self, path: &str) -> PathBuf {
        let trimmed = path.trim_start_matches('/');
        if trimmed.is_empty() {
            self.mount_dir.as_ref().clone()
        } else {
            self.mount_dir.join(trimmed)
        }
    }

    async fn mkdir_p(&self, path: &str) -> Result<()> {
        let full = self.full_path(path);
        if full == *self.mount_dir {
            return Ok(());
        }
        tokio::fs::create_dir_all(&full)
            .await
            .context("fuse mkdir")?;
        Ok(())
    }

    async fn create_file(&self, path: &str) -> Result<()> {
        let full = self.full_path(path);
        if let Some(parent) = full.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .context("fuse create mkdir")?;
        }
        tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&full)
            .await
            .context("fuse create")?;
        Ok(())
    }

    async fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        use tokio::io::{AsyncSeekExt, AsyncWriteExt};
        let full = self.full_path(path);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&full)
            .await
            .context("fuse write open")?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .context("fuse write seek")?;
        file.write_all(data).await.context("fuse write data")?;
        file.flush().await.context("fuse write flush")?;
        Ok(())
    }

    async fn read(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let full = self.full_path(path);
        let mut file = tokio::fs::OpenOptions::new()
            .read(true)
            .open(&full)
            .await
            .context("fuse read open")?;
        file.seek(std::io::SeekFrom::Start(offset))
            .await
            .context("fuse read seek")?;
        let mut buf = vec![0u8; len];
        let mut read = 0usize;
        while read < len {
            let n = file
                .read(&mut buf[read..])
                .await
                .context("fuse read data")?;
            if n == 0 {
                buf.truncate(read);
                break;
            }
            read += n;
        }
        Ok(buf)
    }

    async fn stat(&self, path: &str) -> Result<u64> {
        let full = self.full_path(path);
        let meta = tokio::fs::metadata(&full).await.context("fuse stat")?;
        Ok(meta.len())
    }
}

impl BenchDriver {
    fn direct(fs: SharedFs) -> Self {
        Self(Arc::new(BenchDriverInner::Direct(fs)))
    }

    fn fuse(driver: FuseDriver) -> Self {
        Self(Arc::new(BenchDriverInner::Fuse(driver)))
    }

    async fn mkdir_p(&self, path: &str) -> Result<()> {
        match &*self.0 {
            BenchDriverInner::Direct(fs) => {
                fs.mkdir_p(path).await.map_err(|e| anyhow!(e))?;
                Ok(())
            }
            BenchDriverInner::Fuse(driver) => driver.mkdir_p(path).await,
        }
    }

    async fn create_file(&self, path: &str) -> Result<()> {
        match &*self.0 {
            BenchDriverInner::Direct(fs) => {
                fs.create_file(path).await.map_err(|e| anyhow!(e))?;
                Ok(())
            }
            BenchDriverInner::Fuse(driver) => driver.create_file(path).await,
        }
    }

    async fn write(&self, path: &str, offset: u64, data: &[u8]) -> Result<()> {
        match &*self.0 {
            BenchDriverInner::Direct(fs) => {
                fs.write(path, offset, data).await.map_err(|e| anyhow!(e))?;
                Ok(())
            }
            BenchDriverInner::Fuse(driver) => driver.write(path, offset, data).await,
        }
    }

    async fn read(&self, path: &str, offset: u64, len: usize) -> Result<Vec<u8>> {
        match &*self.0 {
            BenchDriverInner::Direct(fs) => {
                fs.read(path, offset, len).await.map_err(|e| anyhow!(e))
            }
            BenchDriverInner::Fuse(driver) => driver.read(path, offset, len).await,
        }
    }

    async fn stat(&self, path: &str) -> Result<u64> {
        match &*self.0 {
            BenchDriverInner::Direct(fs) => {
                let attr = fs.stat(path).await.ok_or_else(|| anyhow!("not found"))?;
                Ok(attr.size)
            }
            BenchDriverInner::Fuse(driver) => driver.stat(path).await,
        }
    }
}

enum BenchRoot {
    Temp(TempDir),
    Managed(PathBuf),
}

impl BenchRoot {
    fn path(&self) -> &Path {
        match self {
            BenchRoot::Temp(dir) => dir.path(),
            BenchRoot::Managed(p) => p.as_path(),
        }
    }
}

impl Drop for BenchRoot {
    fn drop(&mut self) {
        if let BenchRoot::Managed(p) = self {
            let _ = fs::remove_dir_all(p);
        }
    }
}

#[cfg(target_os = "linux")]
struct FuseGuard {
    _mount_dir: TempDir,
    handle: rfuse3::raw::MountHandle,
}

#[cfg(target_os = "linux")]
impl FuseGuard {
    async fn unmount(self) -> std::io::Result<()> {
        self.handle.unmount().await
    }
}

#[cfg(not(target_os = "linux"))]
struct FuseGuard;

#[cfg(not(target_os = "linux"))]
impl FuseGuard {
    async fn unmount(self) -> std::io::Result<()> {
        Ok(())
    }
}

impl BenchEnv {
    async fn new(cfg: &BenchConfig) -> Result<Self> {
        let (store, root) = create_backend_store(cfg).await?;
        let meta = create_meta_store_from_url(&cfg.meta_url)
            .await
            .context("create meta store")?;
        let vfs = VFS::new(cfg.layout, store, meta.store())
            .await
            .map_err(|e| anyhow!("init vfs: {e}"))?;

        match cfg.mode {
            BenchMode::Direct => {
                let driver = BenchDriver::direct(Arc::new(vfs));
                Ok(Self {
                    driver,
                    _data_root: root,
                    fuse_guard: None,
                })
            }
            BenchMode::Fuse => {
                #[cfg(target_os = "linux")]
                {
                    let mount_dir = TempDir::new().context("create fuse mount dir")?;
                    let mount_path = mount_dir.path().to_path_buf();
                    let handle = mount_vfs_unprivileged(vfs, mount_dir.path())
                        .await
                        .context("mount fuse")?;
                    let driver = BenchDriver::fuse(FuseDriver::new(mount_path));
                    Ok(Self {
                        driver,
                        _data_root: root,
                        fuse_guard: Some(FuseGuard {
                            _mount_dir: mount_dir,
                            handle,
                        }),
                    })
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Err(anyhow!("FUSE mode is only supported on Linux"))
                }
            }
        }
    }

    fn driver(&self) -> BenchDriver {
        self.driver.clone()
    }

    async fn teardown(self) -> Result<()> {
        if let Some(guard) = self.fuse_guard {
            guard.unmount().await.context("unmount fuse")?;
        }
        Ok(())
    }
}

fn create_root_dir() -> Result<BenchRoot> {
    if let Ok(dir) = env::var("SLAYERFS_BENCH_DATA_DIR") {
        let base = PathBuf::from(dir);
        fs::create_dir_all(&base).context("create bench data dir")?;
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let run_dir = base.join(format!("slayerfs_bench_{}", stamp));
        fs::create_dir(&run_dir).context("create bench run dir")?;
        Ok(BenchRoot::Managed(run_dir))
    } else {
        let tmp = TempDir::new().context("create temp dir for bench object store")?;
        Ok(BenchRoot::Temp(tmp))
    }
}

async fn create_backend_store(cfg: &BenchConfig) -> Result<(BenchStore, Option<BenchRoot>)> {
    match &cfg.backend {
        BackendMode::Local => {
            let root = create_root_dir()?;
            let client = ObjectClient::new(LocalFsBackend::new(root.path()));
            let store = ObjectBlockStore::new(client);
            Ok((BenchStore::Local(store), Some(root)))
        }
        BackendMode::S3(opts) => {
            let s3_config = S3Config {
                bucket: opts.bucket.clone(),
                region: opts.region.clone(),
                endpoint: opts.endpoint.clone(),
                force_path_style: opts.force_path_style,
                ..Default::default()
            };
            let backend = S3Backend::with_config(s3_config)
                .await
                .context("initialize s3 backend")?;
            let client = ObjectClient::new(backend);
            let store = ObjectBlockStore::new(client);
            Ok((BenchStore::S3(store), None))
        }
    }
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn env_bool(name: &str) -> Option<bool> {
    env::var(name)
        .ok()
        .and_then(|v| match v.to_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        })
}

fn tokio_runtime(thread_num: usize) -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .worker_threads(thread_num.max(2))
        .build()
        .expect("failed to build tokio runtime")
}

async fn measure_future<F>(fut: F) -> Result<Duration>
where
    F: Future<Output = Result<()>>,
{
    let start = Instant::now();
    fut.await?;
    Ok(start.elapsed())
}

async fn run_big_write(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let driver = env.driver();
    let base = format!("/bench/run-{iter}/big");
    let cost = measure_future(write_big_files(driver, cfg, base)).await?;
    env.teardown().await?;
    Ok(cost)
}

async fn run_big_read(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let driver = env.driver();
    let base = format!("/bench/run-{iter}/big");
    write_big_files(driver.clone(), cfg, base.clone()).await?;
    let cost = measure_future(read_big_files(driver, cfg, base)).await?;
    env.teardown().await?;
    Ok(cost)
}

async fn run_small_write(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let driver = env.driver();
    let base = format!("/bench/run-{iter}/small");
    let cost = measure_future(write_small_files(driver, cfg, base)).await?;
    env.teardown().await?;
    Ok(cost)
}

async fn run_small_read(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let driver = env.driver();
    let base = format!("/bench/run-{iter}/small");
    write_small_files(driver.clone(), cfg, base.clone()).await?;
    let cost = measure_future(read_small_files(driver, cfg, base)).await?;
    env.teardown().await?;
    Ok(cost)
}

async fn run_small_stat(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let driver = env.driver();
    let base = format!("/bench/run-{iter}/small");
    write_small_files(driver.clone(), cfg, base.clone()).await?;
    let cost = measure_future(stat_small_files(driver, cfg, base)).await?;
    env.teardown().await?;
    Ok(cost)
}

async fn write_big_files(driver: BenchDriver, cfg: &BenchConfig, base: String) -> Result<()> {
    if cfg.big_file_bytes == 0 {
        return Ok(());
    }
    driver.mkdir_p(&base).await?;
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let path = format!("{base}/big-{tid}.dat");
        let driver = driver.clone();
        let block_size = cfg.block_size_bytes;
        let total = cfg.big_file_bytes;
        handles.push(tokio::spawn(async move {
            driver.create_file(&path).await?;
            let mut written = 0usize;
            let payload = make_block_payload(block_size, tid);
            while written < total {
                let len = (total - written).min(block_size);
                driver.write(&path, written as u64, &payload[..len]).await?;
                written += len;
            }
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    flush_os_caches();
    Ok(())
}

async fn read_big_files(driver: BenchDriver, cfg: &BenchConfig, base: String) -> Result<()> {
    if cfg.big_file_bytes == 0 {
        return Ok(());
    }
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let path = format!("{base}/big-{tid}.dat");
        let driver = driver.clone();
        let block_size = cfg.block_size_bytes;
        let total = cfg.big_file_bytes;
        handles.push(tokio::spawn(async move {
            let mut read = 0usize;
            while read < total {
                let len = (total - read).min(block_size);
                let data = driver.read(&path, read as u64, len).await?;
                if data.len() != len {
                    return Err(anyhow!(
                        "unexpected read length: expected {len}, got {}",
                        data.len()
                    ));
                }
                read += len;
            }
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

async fn write_small_files(driver: BenchDriver, cfg: &BenchConfig, base: String) -> Result<()> {
    driver.mkdir_p(&base).await?;
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let driver = driver.clone();
        let base = base.clone();
        let block_size = cfg.block_size_bytes;
        let file_size = cfg.small_file_bytes;
        let file_cnt = cfg.small_file_count;
        handles.push(tokio::spawn(async move {
            let payload = make_block_payload(block_size, tid);
            for idx in 0..file_cnt {
                let path = small_file_path(&base, tid, idx);
                driver.create_file(&path).await?;
                let mut written = 0usize;
                while written < file_size {
                    let len = (file_size - written).min(block_size);
                    driver.write(&path, written as u64, &payload[..len]).await?;
                    written += len;
                }
            }
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    flush_os_caches();
    Ok(())
}

async fn read_small_files(driver: BenchDriver, cfg: &BenchConfig, base: String) -> Result<()> {
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let driver = driver.clone();
        let base = base.clone();
        let file_size = cfg.small_file_bytes;
        let file_cnt = cfg.small_file_count;
        handles.push(tokio::spawn(async move {
            for idx in 0..file_cnt {
                let path = small_file_path(&base, tid, idx);
                let data = driver.read(&path, 0, file_size).await?;
                if data.len() != file_size {
                    return Err(anyhow!(
                        "unexpected read length: expected {file_size}, got {}",
                        data.len()
                    ));
                }
            }
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

async fn stat_small_files(driver: BenchDriver, cfg: &BenchConfig, base: String) -> Result<()> {
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let driver = driver.clone();
        let base = base.clone();
        let file_cnt = cfg.small_file_count;
        handles.push(tokio::spawn(async move {
            for idx in 0..file_cnt {
                let path = small_file_path(&base, tid, idx);
                driver.stat(&path).await?;
            }
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

fn make_block_payload(size: usize, salt: usize) -> Vec<u8> {
    let mut buf = vec![0u8; size];
    for (idx, byte) in buf.iter_mut().enumerate() {
        *byte = (salt as u8).wrapping_add((idx % 251) as u8);
    }
    buf
}

fn small_file_path(base: &str, tid: usize, idx: usize) -> String {
    format!("{base}/thread-{tid}/file-{idx}.dat")
}

#[cfg(unix)]
fn flush_os_caches() {
    unsafe { libc::sync() };
}

#[cfg(not(unix))]
fn flush_os_caches() {}

fn bench_big_files(c: &mut Criterion) {
    let cfg = BenchConfig::from_env();
    let runtime = tokio_runtime(cfg.threads);
    let mut group = c.benchmark_group("slayerfs_big_file");
    group.sample_size(cfg.sample_size);
    group.throughput(Throughput::Bytes(cfg.big_total_bytes()));

    group.bench_function(BenchmarkId::new("write", cfg.threads), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let elapsed = runtime
                    .block_on(run_big_write(&cfg, i as usize))
                    .expect("big write bench");
                total += elapsed;
            }
            total
        })
    });

    group.bench_function(BenchmarkId::new("read", cfg.threads), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let elapsed = runtime
                    .block_on(run_big_read(&cfg, i as usize))
                    .expect("big read bench");
                total += elapsed;
            }
            total
        })
    });

    group.finish();
}

fn bench_small_files(c: &mut Criterion) {
    let cfg = BenchConfig::from_env();
    let runtime = tokio_runtime(cfg.threads);
    let mut group = c.benchmark_group("slayerfs_small_file");
    group.sample_size(cfg.sample_size);
    group.throughput(Throughput::Elements(cfg.small_total_files()));

    group.bench_function(BenchmarkId::new("write", cfg.threads), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let elapsed = runtime
                    .block_on(run_small_write(&cfg, i as usize))
                    .expect("small write bench");
                total += elapsed;
            }
            total
        })
    });

    group.bench_function(BenchmarkId::new("read", cfg.threads), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let elapsed = runtime
                    .block_on(run_small_read(&cfg, i as usize))
                    .expect("small read bench");
                total += elapsed;
            }
            total
        })
    });

    group.finish();
}

fn bench_small_stats(c: &mut Criterion) {
    let cfg = BenchConfig::from_env();
    let runtime = tokio_runtime(cfg.threads);
    let mut group = c.benchmark_group("slayerfs_stat");
    group.sample_size(cfg.sample_size);
    group.throughput(Throughput::Elements(cfg.small_total_files()));

    group.bench_function(BenchmarkId::new("stat", cfg.threads), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let elapsed = runtime
                    .block_on(run_small_stat(&cfg, i as usize))
                    .expect("stat bench");
                total += elapsed;
            }
            total
        })
    });

    group.finish();
}

fn build_criterion() -> Criterion {
    let mut crit = Criterion::default().configure_from_args();
    if env::var_os("SLAYERFS_BENCH_FLAMEGRAPH").is_some() {
        eprintln!("[slayerfs_bench] Flamegraph profiler enabled");
        crit = crit.with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    }
    crit
}

criterion_group! {
    name = slayerfs_benches;
    config = build_criterion();
    targets = bench_big_files, bench_small_files, bench_small_stats
}
criterion_main!(slayerfs_benches);
