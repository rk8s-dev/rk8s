use bytes::Bytes;
use slayerfs::{LocalFsBackend, ObjectBackend};
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
#[cfg(feature = "profiling")]
use tracing_subscriber::layer::SubscriberExt;
#[cfg(feature = "profiling")]
use tracing_subscriber::util::SubscriberInitExt;

fn parse_u64(args: &[String], idx: usize, default: u64) -> u64 {
    args.get(idx)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

fn build_pages(page: &Bytes, block_len: usize, page_size: usize) -> Vec<Bytes> {
    let full = block_len / page_size;
    let rem = block_len % page_size;
    let mut out = Vec::with_capacity(full + usize::from(rem > 0));

    for _ in 0..full {
        out.push(page.clone());
    }
    if rem > 0 {
        out.push(page.slice(0..rem));
    }
    out
}

#[cfg(feature = "profiling")]
fn init_tracing() -> Option<tracing_chrome::FlushGuard> {
    let env_filter = tracing_subscriber::EnvFilter::new(
        std::env::var("RUST_LOG").unwrap_or_else(|_| "slayerfs=info".to_string()),
    );
    let mut chrome_guard = None;
    let chrome_layer = match std::env::var("SLAYERFS_TRACE_CHROME") {
        Ok(path) => {
            let path_for_log = path.clone();
            let builder = tracing_chrome::ChromeLayerBuilder::new()
                .file(path)
                .trace_style(tracing_chrome::TraceStyle::Async)
                .include_args(true);
            let (layer, guard) = builder.build();
            eprintln!("[slayerfs] tracing-chrome enabled: {}", path_for_log);
            chrome_guard = Some(guard);
            Some(layer)
        }
        Err(_) => None,
    };

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().pretty())
        .with(env_filter)
        .with(chrome_layer)
        .init();

    chrome_guard
}

#[cfg(not(feature = "profiling"))]
fn init_tracing() {}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    init_tracing();
    let args: Vec<String> = std::env::args().collect();

    // Args: <root> [total_gib] [block_mib] [page_kib] [concurrency] [slice_id] [slice_blocks]
    let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("bench-data/localfs-write")
    });
    let total_gib = parse_u64(&args, 2, 16);
    let block_mib = parse_u64(&args, 3, 4);
    let page_kib = parse_u64(&args, 4, 4);
    let concurrency = parse_u64(&args, 5, 16) as usize;
    let base_slice_id = parse_u64(&args, 6, 1);
    let slice_blocks = parse_u64(&args, 7, 0);

    let total_bytes = total_gib * 1024 * 1024 * 1024;
    let block_size = block_mib * 1024 * 1024;
    let page_size = page_kib * 1024;
    if !block_size.is_multiple_of(page_size) {
        anyhow::bail!("block_size must be a multiple of page_size");
    }

    std::fs::create_dir_all(&root)?;
    let run_dir = root.join(format!(
        "run-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));
    std::fs::create_dir_all(&run_dir)?;

    let backend = std::sync::Arc::new(LocalFsBackend::new(&run_dir));
    let page = Bytes::from(vec![0u8; page_size as usize]);
    let base_pages = build_pages(&page, block_size as usize, page_size as usize);

    let blocks = total_bytes.div_ceil(block_size);
    let slice_blocks = if slice_blocks == 0 {
        blocks
    } else {
        slice_blocks
    };
    let slices = blocks.div_ceil(slice_blocks);
    println!("root={:?}", run_dir);
    println!(
        "total_gib={} block_mib={} page_kib={} concurrency={} blocks={}",
        total_gib, block_mib, page_kib, concurrency, blocks
    );
    println!(
        "slice_id_base={} slice_blocks={} slices={}",
        base_slice_id, slice_blocks, slices
    );

    let start = Instant::now();
    let sem = std::sync::Arc::new(Semaphore::new(concurrency));
    let mut join = JoinSet::new();

    for block_index in 0..blocks {
        let permit = sem.clone().acquire_owned().await?;
        let backend = backend.clone();
        let slice_id = base_slice_id + (block_index / slice_blocks);
        let block_in_slice = (block_index % slice_blocks) as u32;
        let key = format!("chunks/{slice_id}/{block_in_slice}");
        let remaining = total_bytes.saturating_sub(block_index * block_size);
        let block_len = remaining.min(block_size) as usize;
        let pages = if block_len == block_size as usize {
            base_pages.clone()
        } else {
            build_pages(&page, block_len, page_size as usize)
        };

        join.spawn(async move {
            let _permit = permit;
            backend.put_object_vectored(&key, pages).await?;
            Ok::<u64, anyhow::Error>(block_len as u64)
        });
    }

    let mut written = 0u64;
    while let Some(res) = join.join_next().await {
        written += res??;
    }

    let elapsed = start.elapsed().as_secs_f64();
    let gib = written as f64 / 1024.0 / 1024.0 / 1024.0;
    println!(
        "written_bytes={} elapsed_s={:.6} throughput_gib_s={:.3}",
        written,
        elapsed,
        gib / elapsed
    );

    Ok(())
}
