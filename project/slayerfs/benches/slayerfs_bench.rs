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
use slayerfs::meta::{MetaStore, create_meta_store_from_url};
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
        let mut layout = ChunkLayout::default();
        layout.block_size = block_size_u32;

        let backend = BackendMode::from_env();
        let meta_url =
            env::var("SLAYERFS_BENCH_META_URL").unwrap_or_else(|_| "sqlite::memory:".to_string());

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

struct BenchEnv {
    fs: SharedFs,
    _root: Option<BenchRoot>,
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

type BenchFs = VFS<BenchStore, Arc<dyn MetaStore>>;
type SharedFs = Arc<BenchFs>;

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

impl BenchEnv {
    async fn new(cfg: &BenchConfig) -> Result<Self> {
        let (store, root) = create_backend_store(cfg).await?;
        let meta = create_meta_store_from_url(&cfg.meta_url)
            .await
            .context("create meta store")?;
        let fs = VFS::new(cfg.layout, store, meta)
            .await
            .map_err(|e| anyhow!("init vfs: {e}"))?;
        Ok(Self {
            fs: Arc::new(fs),
            _root: root,
        })
    }

    fn fs(&self) -> SharedFs {
        Arc::clone(&self.fs)
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

fn tokio_runtime() -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
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
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/big");
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
    measure_future(write_big_files(fs, cfg, base)).await
}

async fn run_big_read(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/big");
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
    write_big_files(fs.clone(), cfg, base.clone()).await?;
    measure_future(read_big_files(fs, cfg, base)).await
}

async fn run_small_write(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/small");
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
    measure_future(write_small_files(fs, cfg, base)).await
}

async fn run_small_read(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/small");
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
    write_small_files(fs.clone(), cfg, base.clone()).await?;
    measure_future(read_small_files(fs, cfg, base)).await
}

async fn run_small_stat(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/small");
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
    write_small_files(fs.clone(), cfg, base.clone()).await?;
    measure_future(stat_small_files(fs, cfg, base)).await
}

async fn write_big_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    if cfg.big_file_bytes == 0 {
        return Ok(());
    }
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let path = format!("{base}/big-{tid}.dat");
        let fs = fs.clone();
        let block_size = cfg.block_size_bytes;
        let total = cfg.big_file_bytes;
        handles.push(tokio::spawn(async move {
            fs.create_file(&path).await.map_err(|e| anyhow!(e))?;
            let mut written = 0usize;
            let payload = make_block_payload(block_size, tid);
            while written < total {
                let len = (total - written).min(block_size);
                fs.write(&path, written as u64, &payload[..len])
                    .await
                    .map_err(|e| anyhow!(e))?;
                written += len;
            }
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

async fn read_big_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    if cfg.big_file_bytes == 0 {
        return Ok(());
    }
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let path = format!("{base}/big-{tid}.dat");
        let fs = fs.clone();
        let block_size = cfg.block_size_bytes;
        let total = cfg.big_file_bytes;
        handles.push(tokio::spawn(async move {
            let mut read = 0usize;
            while read < total {
                let len = (total - read).min(block_size);
                let data = fs
                    .read(&path, read as u64, len)
                    .await
                    .map_err(|e| anyhow!(e))?;
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

async fn write_small_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let fs = fs.clone();
        let base = base.clone();
        let block_size = cfg.block_size_bytes;
        let file_size = cfg.small_file_bytes;
        let file_cnt = cfg.small_file_count;
        handles.push(tokio::spawn(async move {
            let payload = make_block_payload(block_size, tid);
            for idx in 0..file_cnt {
                let path = small_file_path(&base, tid, idx);
                fs.create_file(&path).await.map_err(|e| anyhow!(e))?;
                let mut written = 0usize;
                while written < file_size {
                    let len = (file_size - written).min(block_size);
                    fs.write(&path, written as u64, &payload[..len])
                        .await
                        .map_err(|e| anyhow!(e))?;
                    written += len;
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

async fn read_small_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let fs = fs.clone();
        let base = base.clone();
        let file_size = cfg.small_file_bytes;
        let file_cnt = cfg.small_file_count;
        handles.push(tokio::spawn(async move {
            for idx in 0..file_cnt {
                let path = small_file_path(&base, tid, idx);
                let data = fs.read(&path, 0, file_size).await.map_err(|e| anyhow!(e))?;
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

async fn stat_small_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let fs = fs.clone();
        let base = base.clone();
        let file_cnt = cfg.small_file_count;
        handles.push(tokio::spawn(async move {
            for idx in 0..file_cnt {
                let path = small_file_path(&base, tid, idx);
                if fs.stat(&path).await.is_none() {
                    return Err(anyhow!("stat failed for {path}"));
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

fn bench_big_files(c: &mut Criterion) {
    let cfg = BenchConfig::from_env();
    let runtime = tokio_runtime();
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
    let runtime = tokio_runtime();
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
    let runtime = tokio_runtime();
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
