use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Once, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use bytes::Bytes;
use clap::{Parser, ValueEnum};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use pprof::criterion::{Output, PProfProfiler};
use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::runtime::{Builder, Runtime};
use tracing_chrome::{ChromeLayerBuilder, TraceStyle};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use slayerfs::cadapter::client::ObjectClient;
use slayerfs::cadapter::localfs::LocalFsBackend;
use slayerfs::cadapter::s3::{S3Backend, S3Config};
use slayerfs::chuck::chunk::ChunkLayout;
use slayerfs::chuck::store::{BlockKey, BlockStore, ObjectBlockStore};
use slayerfs::meta::client::MetaClient;
use slayerfs::meta::config::{CacheConfig, ClientOptions, Config, DatabaseConfig, DatabaseType};
use slayerfs::meta::factory::MetaStoreFactory;
use slayerfs::meta::stores::{DatabaseMetaStore, EtcdMetaStore};
use slayerfs::meta::{MetaLayer, MetaStore};
use slayerfs::vfs::fs::VFS;

const MB: usize = 1024 * 1024;
const KB: usize = 1024;

static TRACING_INIT: Once = Once::new();
static CHROME_GUARD: OnceLock<tracing_chrome::FlushGuard> = OnceLock::new();

fn init_tracing(chrome_trace: Option<&Path>) {
    TRACING_INIT.call_once(|| {
        let filter = EnvFilter::from_default_env();
        let fmt_layer = tracing_subscriber::fmt::layer();

        if let Some(path) = chrome_trace {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let (chrome_layer, guard) = ChromeLayerBuilder::new()
                .file(path)
                .include_args(true)
                .trace_style(TraceStyle::Async)
                .build();
            let _ = CHROME_GUARD.set(guard);
            eprintln!("[slayerfs_bench] Chrome trace enabled: {}", path.display());
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt_layer)
                .with(chrome_layer)
                .init();
            return;
        }
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .init();
    });
}

#[derive(Parser, Debug, Clone)]
#[command(
    name = "slayerfs-bench",
    about = "SlayerFS benchmark config (env-driven)"
)]
struct BenchArgs {
    #[arg(long, env = "SLAYERFS_BENCH_BLOCK_MB", default_value_t = 1)]
    block_mb: usize,
    #[arg(long, env = "SLAYERFS_BENCH_BIG_FILE_MB", default_value_t = 512)]
    big_file_mb: usize,
    #[arg(long, env = "SLAYERFS_BENCH_SMALL_FILE_KB", default_value_t = 128)]
    small_file_kb: usize,
    #[arg(long, env = "SLAYERFS_BENCH_SMALL_FILE_COUNT", default_value_t = 100)]
    small_file_count: usize,
    #[arg(long, env = "SLAYERFS_BENCH_THREADS", default_value_t = 4)]
    threads: usize,
    #[arg(long, env = "SLAYERFS_BENCH_SAMPLE_SIZE", default_value_t = 10)]
    sample_size: usize,
    #[arg(long, env = "SLAYERFS_BENCH_META_BACKEND", value_enum, default_value_t = MetaBackendKind::Sqlx)]
    meta_backend: MetaBackendKind,
    #[arg(
        long,
        env = "SLAYERFS_BENCH_META_URL",
        default_value = "sqlite::memory:"
    )]
    meta_url: String,
    #[arg(long, env = "SLAYERFS_BENCH_META_ETCD_URLS")]
    meta_etcd_urls: Option<String>,
    #[arg(long, env = "SLAYERFS_BENCH_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "SLAYERFS_BENCH_BACKEND", value_enum, default_value_t = BackendKind::Local)]
    backend: BackendKind,
    #[arg(long, env = "SLAYERFS_BENCH_S3_BUCKET")]
    s3_bucket: Option<String>,
    #[arg(long, env = "SLAYERFS_BENCH_S3_REGION")]
    s3_region: Option<String>,
    #[arg(long, env = "SLAYERFS_BENCH_S3_ENDPOINT")]
    s3_endpoint: Option<String>,
    #[arg(long, env = "SLAYERFS_BENCH_S3_FORCE_PATH_STYLE")]
    s3_force_path_style: Option<String>,
    #[arg(long, env = "SLAYERFS_BENCH_FLAMEGRAPH")]
    flamegraph: Option<String>,
    #[arg(long, env = "SLAYERFS_BENCH_CHROME")]
    chrome: Option<String>,
}

#[derive(ValueEnum, Debug, Clone)]
enum BackendKind {
    Local,
    S3,
}

#[derive(ValueEnum, Debug, Clone)]
enum MetaBackendKind {
    Sqlx,
    Etcd,
}

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
    meta_backend: MetaBackend,
    data_dir: Option<PathBuf>,
    flamegraph: bool,
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

#[derive(Clone)]
enum MetaBackend {
    Sqlx { url: String },
    Etcd { urls: Vec<String> },
}

impl BenchConfig {
    fn from_env() -> Self {
        let args = BenchArgs::parse_from(["slayerfs-bench"]);
        let chrome_trace = match args.chrome.as_deref() {
            Some(value) if parse_env_bool(Some(value)) => {
                Some(PathBuf::from("slayerfs_bench_trace.json"))
            }
            Some(value) => Some(PathBuf::from(value)),
            None => None,
        };
        init_tracing(chrome_trace.as_deref());
        let block_mb = args.block_mb.max(1);
        let big_mb = args.big_file_mb.max(1);
        let small_kb = args.small_file_kb.max(1);
        let small_file_count = args.small_file_count.max(1);
        let threads = args.threads.max(1);
        let sample_size = args.sample_size.max(10);

        let block_size_bytes = block_mb * MB;
        let block_size_u32 = block_size_bytes
            .try_into()
            .expect("block size must fit into u32");
        let layout = ChunkLayout {
            block_size: block_size_u32,
            ..Default::default()
        };

        let backend = match args.backend {
            BackendKind::Local => BackendMode::Local,
            BackendKind::S3 => {
                let bucket = args.s3_bucket.unwrap_or_else(|| {
                    panic!("SLAYERFS_BENCH_S3_BUCKET must be set when backend is s3")
                });
                BackendMode::S3(S3BackendOpts {
                    bucket,
                    region: args.s3_region,
                    endpoint: args.s3_endpoint,
                    force_path_style: parse_env_bool(args.s3_force_path_style.as_deref()),
                })
            }
        };

        let meta_backend = match args.meta_backend {
            MetaBackendKind::Sqlx => MetaBackend::Sqlx { url: args.meta_url },
            MetaBackendKind::Etcd => {
                let urls = args
                    .meta_etcd_urls
                    .as_deref()
                    .map(parse_csv_urls)
                    .unwrap_or_default();
                if urls.is_empty() {
                    panic!("SLAYERFS_BENCH_META_ETCD_URLS must be set when meta backend is etcd");
                }
                MetaBackend::Etcd { urls }
            }
        };

        Self {
            block_size_bytes,
            big_file_bytes: big_mb * MB,
            small_file_bytes: small_kb * KB,
            small_file_count,
            threads,
            sample_size,
            layout,
            backend,
            meta_backend,
            data_dir: args.data_dir,
            flamegraph: parse_env_bool(args.flamegraph.as_deref()),
        }
    }

    fn big_total_bytes(&self) -> u64 {
        (self.big_file_bytes * self.threads) as u64
    }

    fn small_total_files(&self) -> u64 {
        (self.small_file_count * self.threads) as u64
    }
}

struct BenchEnv {
    fs: SharedFs,
    _data_root: Option<BenchRoot>,
}

enum BenchStore {
    Local(ObjectBlockStore<LocalFsBackend>),
    S3(ObjectBlockStore<S3Backend>),
}

#[async_trait]
impl BlockStore for BenchStore {
    async fn write_vectored(&self, key: BlockKey, offset: u32, chunks: Vec<Bytes>) -> Result<u64> {
        match self {
            BenchStore::Local(store) => store.write_vectored(key, offset, chunks).await,
            BenchStore::S3(store) => store.write_vectored(key, offset, chunks).await,
        }
    }

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

type BenchFs = VFS<BenchStore, MetaClient<Arc<dyn MetaStore>>>;
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
        let meta = create_meta_store(cfg).await.context("create meta store")?;
        let vfs = VFS::new(cfg.layout, store, meta)
            .await
            .map_err(|e| anyhow!(e))?;

        Ok(Self {
            fs: Arc::new(vfs),
            _data_root: root,
        })
    }

    fn fs(&self) -> SharedFs {
        self.fs.clone()
    }

    fn teardown(self) -> Result<()> {
        Ok(())
    }
}

fn create_root_dir(cfg: &BenchConfig) -> Result<BenchRoot> {
    if let Some(dir) = cfg.data_dir.as_ref() {
        let base = dir.to_path_buf();
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

fn create_baseline_root(cfg: &BenchConfig) -> Result<BenchRoot> {
    if let Some(dir) = cfg.data_dir.as_ref() {
        let base = dir.to_path_buf();
        fs::create_dir_all(&base).context("create bench data dir")?;
        let stamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let run_dir = base.join(format!("slayerfs_baseline_{}", stamp));
        fs::create_dir(&run_dir).context("create baseline run dir")?;
        Ok(BenchRoot::Managed(run_dir))
    } else {
        let tmp = TempDir::new().context("create temp dir for baseline")?;
        Ok(BenchRoot::Temp(tmp))
    }
}

async fn create_backend_store(cfg: &BenchConfig) -> Result<(BenchStore, Option<BenchRoot>)> {
    match &cfg.backend {
        BackendMode::Local => {
            let root = create_root_dir(cfg)?;
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

fn tokio_runtime(thread_num: usize) -> Runtime {
    Builder::new_multi_thread()
        .enable_all()
        .worker_threads(thread_num.max(2))
        .build()
        .expect("failed to build tokio runtime")
}

async fn create_meta_store(cfg: &BenchConfig) -> Result<Arc<dyn MetaStore>> {
    match &cfg.meta_backend {
        MetaBackend::Sqlx { url } => {
            let client = ClientOptions {
                no_background_jobs: true,
                ..ClientOptions::default()
            };
            let config = Config {
                database: DatabaseConfig {
                    db_config: database_type_from_url(url),
                },
                cache: CacheConfig::default(),
                client,
            };
            let handle = MetaStoreFactory::<DatabaseMetaStore>::create_from_config(config)
                .await
                .context("create sqlx meta store")?;
            Ok(handle.store() as Arc<dyn MetaStore>)
        }
        MetaBackend::Etcd { urls } => {
            let client = ClientOptions {
                no_background_jobs: true,
                ..ClientOptions::default()
            };
            let config = Config {
                database: DatabaseConfig {
                    db_config: DatabaseType::Etcd { urls: urls.clone() },
                },
                cache: CacheConfig::default(),
                client,
            };
            let handle = MetaStoreFactory::<EtcdMetaStore>::create_from_config(config)
                .await
                .context("create etcd meta store")?;
            Ok(handle.store() as Arc<dyn MetaStore>)
        }
    }
}

fn database_type_from_url(url: &str) -> DatabaseType {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("postgres://") || lower.starts_with("postgresql://") {
        DatabaseType::Postgres {
            url: url.to_string(),
        }
    } else {
        DatabaseType::Sqlite {
            url: url.to_string(),
        }
    }
}

fn parse_env_bool(value: Option<&str>) -> bool {
    match value.map(|v| v.to_ascii_lowercase()) {
        Some(v) if v == "1" || v == "true" || v == "yes" => true,
        Some(v) if v == "0" || v == "false" || v == "no" => false,
        _ => false,
    }
}

fn parse_csv_urls(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect()
}

async fn measure_future<F>(fut: F) -> Result<Duration>
where
    F: Future<Output = Result<()>>,
{
    let start = Instant::now();
    fut.await?;
    Ok(start.elapsed())
}

async fn open_handle(fs: &SharedFs, path: &str, read: bool, write: bool) -> Result<u64> {
    let attr = fs.stat(path).await?;
    Ok(fs.open(attr.ino, attr, read, write).await?)
}

async fn close_handle(fs: &SharedFs, fh: u64) -> Result<()> {
    fs.close(fh).await.map_err(|e| anyhow!(e))
}

async fn run_big_write(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/big");
    let cost = measure_future(write_big_files(fs, cfg, base)).await?;
    env.teardown()?;
    Ok(cost)
}

async fn run_big_write_baseline(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let root = create_baseline_root(cfg)?;
    let base = root.path().join(format!("baseline-run-{iter}/big"));
    let cost = measure_future(write_big_files_baseline(cfg, base)).await?;
    drop(root);
    Ok(cost)
}

async fn run_big_read(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/big");
    write_big_files(fs.clone(), cfg, base.clone()).await?;
    let cost = measure_future(read_big_files(fs, cfg, base)).await?;
    env.teardown()?;
    Ok(cost)
}

async fn run_small_write(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/small");
    let cost = measure_future(write_small_files(fs, cfg, base)).await?;
    env.teardown()?;
    Ok(cost)
}

async fn run_small_read(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/small");
    write_small_files(fs.clone(), cfg, base.clone()).await?;
    let cost = measure_future(read_small_files(fs, cfg, base)).await?;
    env.teardown()?;
    Ok(cost)
}

async fn run_small_stat(cfg: &BenchConfig, iter: usize) -> Result<Duration> {
    let env = BenchEnv::new(cfg).await?;
    let fs = env.fs();
    let base = format!("/bench/run-{iter}/small");
    write_small_files(fs.clone(), cfg, base.clone()).await?;
    let cost = measure_future(stat_small_files(fs, cfg, base)).await?;
    env.teardown()?;
    Ok(cost)
}

async fn write_big_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    if cfg.big_file_bytes == 0 {
        return Ok(());
    }
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let path = format!("{base}/big-{tid}.dat");
        let fs = fs.clone();
        let block_size = cfg.block_size_bytes;
        let total = cfg.big_file_bytes;
        handles.push(tokio::spawn(async move {
            fs.create_file(&path).await.map_err(|e| anyhow!(e))?;
            let fh = open_handle(&fs, &path, false, true).await?;
            let mut written = 0usize;
            let payload = make_block_payload(block_size, tid);
            while written < total {
                let len = (total - written).min(block_size);
                let nw = fs.write(fh, written as u64, &payload[..len]).await?;
                if nw != len {
                    return Err(anyhow!("short write: expected {len}, got {nw}"));
                }
                written += len;
            }
            fs.fsync(fh, false).await.map_err(|e| anyhow!(e))?;
            close_handle(&fs, fh).await?;
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

async fn write_big_files_baseline(cfg: &BenchConfig, base: PathBuf) -> Result<()> {
    if cfg.big_file_bytes == 0 {
        return Ok(());
    }
    tokio::fs::create_dir_all(&base)
        .await
        .context("create baseline base dir")?;

    let mut handles = Vec::with_capacity(cfg.threads);
    for tid in 0..cfg.threads {
        let path = base.join(format!("big-{tid}.dat"));
        let block_size = cfg.block_size_bytes;
        let total = cfg.big_file_bytes;
        handles.push(tokio::spawn(async move {
            let mut f = tokio::fs::File::create(&path).await?;
            let mut written = 0usize;
            let payload = make_block_payload(block_size, tid);
            while written < total {
                let len = (total - written).min(block_size);
                f.write_all(&payload[..len]).await?;
                written += len;
            }
            f.sync_all().await?;
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
            let fh = open_handle(&fs, &path, true, false).await?;
            let mut read = 0usize;
            while read < total {
                let len = (total - read).min(block_size);
                let data = fs.read(fh, read as u64, len).await?;
                if data.len() != len {
                    return Err(anyhow!(
                        "unexpected read length: expected {len}, got {}",
                        data.len()
                    ));
                }
                read += len;
            }
            close_handle(&fs, fh).await?;
            Result::<()>::Ok(())
        }));
    }
    for handle in handles {
        handle.await??;
    }
    Ok(())
}

async fn write_small_files(fs: SharedFs, cfg: &BenchConfig, base: String) -> Result<()> {
    fs.mkdir_p(&base).await.map_err(|e| anyhow!(e))?;
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
                let fh = open_handle(&fs, &path, false, true).await?;
                let mut written = 0usize;
                while written < file_size {
                    let len = (file_size - written).min(block_size);
                    let nw = fs.write(fh, written as u64, &payload[..len]).await?;
                    if nw != len {
                        return Err(anyhow!("short write: expected {len}, got {nw}"));
                    }
                    written += len;
                }
                fs.fsync(fh, false).await.map_err(|e| anyhow!(e))?;
                close_handle(&fs, fh).await?;
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
                let fh = open_handle(&fs, &path, true, false).await?;
                let data = fs.read(fh, 0, file_size).await?;
                if data.len() != file_size {
                    return Err(anyhow!(
                        "unexpected read length: expected {file_size}, got {}",
                        data.len()
                    ));
                }
                close_handle(&fs, fh).await?;
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
                fs.stat(&path).await?;
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

    group.bench_function(BenchmarkId::new("write_baseline", cfg.threads), |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for i in 0..iters {
                let elapsed = runtime
                    .block_on(run_big_write_baseline(&cfg, i as usize))
                    .expect("big write baseline bench");
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
    let cfg = BenchConfig::from_env();
    let mut crit = Criterion::default().configure_from_args();
    if cfg.flamegraph {
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
