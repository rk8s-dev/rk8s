//! Compaction Performance Test
//!
//! This test measures compaction performance metrics:
//! - Throughput: chunks per second
//! - Latency: time per compaction operation
//! - Concurrent vs Sequential: verify current implementation behavior
//!
//! Run with: cargo test -p slayerfs --test compaction_perf_test -- --nocapture
//!
//! Environment variables:
//! - SLAYERFS_TEST_BACKEND: "sqlite" (default), "etcd", or "redis"
//! - SLAYERFS_TEST_ETCD_URL: etcd endpoints, default "http://localhost:2379"
//! - SLAYERFS_TEST_REDIS_URL: redis URL, default "redis://localhost:6379"
// WIP！！！

#[cfg(test)]
mod common;

use slayerfs::meta::SLICE_ID_KEY;
use slayerfs::{
    ChunkLayout, CompactResult, Compactor, Config, DatabaseMetaStore, DatabaseType, EtcdMetaStore,
    InMemoryBlockStore, MetaStore, RedisMetaStore,
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::Semaphore;

use common::create_fragmented_file;

fn get_backend_type() -> String {
    std::env::var("SLAYERFS_TEST_BACKEND").unwrap_or_else(|_| "sqlite".to_string())
}

#[derive(Debug, Clone)]
struct CompactionMetrics {
    pub chunks_processed: usize,
    pub total_slices_before: usize,
    pub total_slices_after: usize,
    pub total_duration: Duration,
    pub compactions_succeeded: usize,
    pub compactions_skipped: usize,
    pub compactions_failed: usize,
}

impl CompactionMetrics {
    fn new() -> Self {
        Self {
            chunks_processed: 0,
            total_slices_before: 0,
            total_slices_after: 0,
            total_duration: Duration::ZERO,
            compactions_succeeded: 0,
            compactions_skipped: 0,
            compactions_failed: 0,
        }
    }

    fn throughput_chunks_per_sec(&self) -> f64 {
        if self.total_duration.as_secs_f64() > 0.0 {
            self.chunks_processed as f64 / self.total_duration.as_secs_f64()
        } else {
            0.0
        }
    }

    fn avg_latency_ms(&self) -> f64 {
        let total_ops =
            self.compactions_succeeded + self.compactions_skipped + self.compactions_failed;
        if total_ops > 0 {
            self.total_duration.as_secs_f64() * 1000.0 / total_ops as f64
        } else {
            0.0
        }
    }

    fn slice_reduction_ratio(&self) -> f64 {
        if self.total_slices_before > 0 {
            1.0 - (self.total_slices_after as f64 / self.total_slices_before as f64)
        } else {
            0.0
        }
    }
}

impl std::fmt::Display for CompactionMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Compaction Performance Metrics ===")?;
        writeln!(f, "Chunks processed: {}", self.chunks_processed)?;
        writeln!(f, "Total slices before: {}", self.total_slices_before)?;
        writeln!(f, "Total slices after: {}", self.total_slices_after)?;
        writeln!(
            f,
            "Slice reduction: {:.2}%",
            self.slice_reduction_ratio() * 100.0
        )?;
        writeln!(f, "Total duration: {:?}", self.total_duration)?;
        writeln!(
            f,
            "Throughput: {:.2} chunks/sec",
            self.throughput_chunks_per_sec()
        )?;
        writeln!(
            f,
            "Average latency: {:.2} ms/compaction",
            self.avg_latency_ms()
        )?;
        writeln!(
            f,
            "Compactions: {} succeeded, {} skipped, {} failed",
            self.compactions_succeeded, self.compactions_skipped, self.compactions_failed
        )
    }
}

/// Detailed per-chunk timing information
#[derive(Debug, Clone)]
struct ChunkTiming {
    chunk_id: u64,
    total_time_ms: u128,
    result: String,
}

/// Concurrent execution analysis
#[derive(Debug)]
struct ConcurrencyAnalysis {
    sequential_total_ms: u128,
    concurrent_total_ms: u128,
    individual_times: Vec<ChunkTiming>,
    overlap_ratio: f64,
}

impl ConcurrencyAnalysis {
    fn print_report(&self) {
        println!("\n=== Detailed Concurrency Analysis ===");
        println!("Sequential total: {} ms", self.sequential_total_ms);
        println!("Concurrent total: {} ms", self.concurrent_total_ms);
        println!("Overlap ratio: {:.2}%", self.overlap_ratio * 100.0);

        println!("\n--- Individual Chunk Times ---");
        let mut sorted = self.individual_times.clone();
        sorted.sort_by_key(|t| t.total_time_ms);

        for t in &sorted {
            println!(
                "  Chunk {}: {} ms - {}",
                t.chunk_id, t.total_time_ms, t.result
            );
        }

        // Calculate statistics
        if !self.individual_times.is_empty() {
            let avg_time = self
                .individual_times
                .iter()
                .map(|t| t.total_time_ms)
                .sum::<u128>()
                / self.individual_times.len() as u128;
            let min_time = sorted.first().map(|t| t.total_time_ms).unwrap_or(0);
            let max_time = sorted.last().map(|t| t.total_time_ms).unwrap_or(0);

            println!("\n--- Timing Statistics ---");
            println!("  Average: {} ms", avg_time);
            println!("  Min: {} ms", min_time);
            println!("  Max: {} ms", max_time);
            println!("  Spread: {} ms (max-min)", max_time - min_time);

            // Theoretical analysis
            let theoretical_best = if self.individual_times.len() >= 4 {
                // With 4 concurrent tasks, ideal time would be ceil(N/4) * avg
                let batches = self.individual_times.len().div_ceil(4);
                batches as u128 * avg_time
            } else {
                avg_time
            };
            println!("\n--- Theoretical Analysis ---");
            println!("  Theoretical best (4 concurrent): {} ms", theoretical_best);
            println!("  Actual concurrent: {} ms", self.concurrent_total_ms);
            println!(
                "  Efficiency: {:.1}%",
                (theoretical_best as f64 / self.concurrent_total_ms as f64) * 100.0
            );
        }
    }
}

async fn setup_test_fs() -> (
    Option<TempDir>,
    Arc<dyn MetaStore>,
    Arc<InMemoryBlockStore>,
    String,
) {
    let backend = get_backend_type();
    let block_store = Arc::new(InMemoryBlockStore::new());

    match backend.as_str() {
        "etcd" => {
            let etcd_url = std::env::var("SLAYERFS_TEST_ETCD_URL")
                .unwrap_or_else(|_| "http://localhost:2379".to_string());
            println!("  Using etcd backend: {}", etcd_url);

            let tmp_dir = TempDir::new().unwrap();
            let meta_store = Arc::new(
                EtcdMetaStore::new(tmp_dir.path())
                    .await
                    .expect("Failed to connect to etcd"),
            );
            meta_store
                .initialize()
                .await
                .expect("Failed to initialize etcd");
            (
                Some(tmp_dir),
                meta_store as Arc<dyn MetaStore>,
                block_store,
                backend,
            )
        }
        "redis" => {
            let redis_url = std::env::var("SLAYERFS_TEST_REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string());
            println!("  Using Redis backend: {}", redis_url);

            let tmp_dir = TempDir::new().unwrap();
            let meta_store = Arc::new(
                RedisMetaStore::new(tmp_dir.path())
                    .await
                    .expect("Failed to connect to Redis"),
            );
            meta_store
                .initialize()
                .await
                .expect("Failed to initialize Redis");
            (
                Some(tmp_dir),
                meta_store as Arc<dyn MetaStore>,
                block_store,
                backend,
            )
        }
        _ => {
            let tmp_dir = TempDir::new().unwrap();
            let db_path = tmp_dir.path().join("test.db");
            let config = Config {
                database: slayerfs::DatabaseConfig {
                    db_config: DatabaseType::Sqlite {
                        url: format!("sqlite://{}?mode=rwc", db_path.display()),
                    },
                },
                cache: Default::default(),
                client: Default::default(),
                compact: Default::default(),
            };
            let meta_store = Arc::new(DatabaseMetaStore::from_config(config).await.unwrap());
            meta_store.initialize().await.unwrap();
            (
                Some(tmp_dir),
                meta_store as Arc<dyn MetaStore>,
                block_store,
                backend,
            )
        }
    }
}

#[tokio::test]
async fn test_sequential_compaction_performance() {
    let backend = get_backend_type();
    println!(
        "\n[Perf Test] Sequential Compaction Performance (backend: {})",
        backend
    );

    let (_tmp, meta_store, block_store, _) = setup_test_fs().await;
    let root = meta_store.root_ino();

    let num_files = 10;
    let writes_per_file = 20;
    let mut chunk_ids = Vec::new();

    println!("Creating {} fragmented files...", num_files);
    for i in 0..num_files {
        let file = meta_store
            .create_file(root, format!("frag_{}.txt", i))
            .await
            .unwrap();
        let chunk_id =
            create_fragmented_file(&meta_store, &block_store, file, writes_per_file).await;
        chunk_ids.push(chunk_id);
    }

    let compactor = Compactor::with_layout(
        meta_store.clone(),
        block_store.clone(),
        ChunkLayout::default(),
    );

    let mut total_slices_before = 0;
    for &chunk_id in &chunk_ids {
        let slices = meta_store.get_slices(chunk_id).await.unwrap();
        total_slices_before += slices.len();
    }

    let mut metrics = CompactionMetrics::new();
    metrics.chunks_processed = chunk_ids.len();
    metrics.total_slices_before = total_slices_before;

    let start = Instant::now();
    for chunk_id in &chunk_ids {
        match compactor.compact_chunk(*chunk_id).await {
            Ok(CompactResult::Light { .. }) => metrics.compactions_succeeded += 1,
            Ok(CompactResult::Heavy { .. }) => metrics.compactions_succeeded += 1,
            Ok(CompactResult::Skipped) => metrics.compactions_skipped += 1,
            Err(_) => metrics.compactions_failed += 1,
        }
    }
    metrics.total_duration = start.elapsed();

    let mut total_slices_after = 0;
    for &chunk_id in &chunk_ids {
        let slices = meta_store.get_slices(chunk_id).await.unwrap();
        total_slices_after += slices.len();
    }
    metrics.total_slices_after = total_slices_after;

    println!("{}", metrics);
    assert!(
        metrics.compactions_succeeded > 0,
        "At least some compactions should succeed"
    );

    println!("\n[CI_METRICS backend={}]", backend);
    println!(
        "throughput_chunks_per_sec={:.2}",
        metrics.throughput_chunks_per_sec()
    );
    println!("avg_latency_ms={:.2}", metrics.avg_latency_ms());
    println!(
        "slice_reduction_ratio={:.4}",
        metrics.slice_reduction_ratio()
    );
}

#[tokio::test]
async fn test_concurrent_potential() {
    let backend = get_backend_type();
    println!(
        "\n[Perf Test] Concurrent Processing Potential (backend: {})",
        backend
    );

    let (_tmp_seq, meta_store_seq, block_store_seq, _) = setup_test_fs().await;
    let (_tmp_conc, meta_store_conc, block_store_conc, _) = setup_test_fs().await;

    let root_seq = meta_store_seq.root_ino();
    let root_conc = meta_store_conc.root_ino();

    let num_files = 20;
    let mut chunk_ids_seq = Vec::new();
    let mut chunk_ids_conc = Vec::new();

    println!(
        "Creating {} fragmented files for sequential test...",
        num_files
    );
    for i in 0..num_files {
        let file_seq = meta_store_seq
            .create_file(root_seq, format!("seq_test_{}.txt", i))
            .await
            .unwrap();
        let chunk_id_seq =
            create_fragmented_file(&meta_store_seq, &block_store_seq, file_seq, 15).await;
        chunk_ids_seq.push(chunk_id_seq);
    }

    println!(
        "Creating {} fragmented files for concurrent test...",
        num_files
    );
    for i in 0..num_files {
        let file_conc = meta_store_conc
            .create_file(root_conc, format!("conc_test_{}.txt", i))
            .await
            .unwrap();
        let chunk_id_conc =
            create_fragmented_file(&meta_store_conc, &block_store_conc, file_conc, 15).await;
        chunk_ids_conc.push(chunk_id_conc);
    }

    let compactor_seq = Arc::new(Compactor::with_layout(
        meta_store_seq.clone(),
        block_store_seq.clone(),
        ChunkLayout::default(),
    ));

    let compactor_conc = Arc::new(Compactor::with_layout(
        meta_store_conc.clone(),
        block_store_conc.clone(),
        ChunkLayout::default(),
    ));

    // Sequential processing with detailed timing
    println!("\n--- Starting Sequential Processing ---");
    let sequential_start = Instant::now();
    let mut sequential_success = 0;
    let mut seq_timings = Vec::new();

    for chunk_id in &chunk_ids_seq {
        let chunk_start = Instant::now();
        let result = match compactor_seq.compact_chunk(*chunk_id).await {
            Ok(CompactResult::Light { .. }) | Ok(CompactResult::Heavy { .. }) => {
                sequential_success += 1;
                "success".to_string()
            }
            Ok(CompactResult::Skipped) => "skipped".to_string(),
            Err(e) => format!("error: {:?}", e),
        };
        let elapsed = chunk_start.elapsed();
        seq_timings.push(ChunkTiming {
            chunk_id: *chunk_id,
            total_time_ms: elapsed.as_millis(),
            result,
        });
    }
    let sequential_duration = sequential_start.elapsed();

    // Concurrent processing with detailed timing
    println!("--- Starting Concurrent Processing (4 tasks) ---");
    let concurrency_level = 4;
    let semaphore = Arc::new(Semaphore::new(concurrency_level));
    let concurrent_start = Instant::now();
    let mut handles = Vec::new();

    for chunk_id in chunk_ids_conc {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
        let compactor = compactor_conc.clone();
        let handle = tokio::spawn(async move {
            #[allow(clippy::used_underscore_binding)]
            let _permit = permit;
            let task_start = Instant::now();
            let (success, skipped, failed, result_str) =
                match compactor.compact_chunk(chunk_id).await {
                    Ok(CompactResult::Light { .. }) => (1, 0, 0, "light".to_string()),
                    Ok(CompactResult::Heavy { .. }) => (1, 0, 0, "heavy".to_string()),
                    Ok(CompactResult::Skipped) => (0, 1, 0, "skipped".to_string()),
                    Err(e) => {
                        println!("  Chunk {} compaction error: {:?}", chunk_id, e);
                        (0, 0, 1, format!("error: {:?}", e))
                    }
                };
            let elapsed = task_start.elapsed();
            (
                chunk_id,
                success,
                skipped,
                failed,
                elapsed.as_millis(),
                result_str,
            )
        });
        handles.push(handle);
    }

    let mut concurrent_success = 0;
    let mut concurrent_skipped = 0;
    let mut concurrent_failed = 0;
    let mut conc_timings = Vec::new();

    for handle in handles {
        let (chunk_id, s, sk, f, ms, result) = handle.await.unwrap();
        concurrent_success += s;
        concurrent_skipped += sk;
        concurrent_failed += f;
        conc_timings.push(ChunkTiming {
            chunk_id,
            total_time_ms: ms,
            result,
        });
    }
    let concurrent_duration = concurrent_start.elapsed();

    // Calculate overlap ratio
    let seq_total_ms: u128 = seq_timings.iter().map(|t| t.total_time_ms).sum();
    let overlap_ratio = if seq_total_ms > 0 {
        (seq_total_ms as f64 - concurrent_duration.as_millis() as f64) / seq_total_ms as f64
    } else {
        0.0
    };

    // Print results
    println!("\n=== Results Summary ===");
    println!(
        "Sequential: {} chunks in {:?} (success: {})",
        num_files, sequential_duration, sequential_success
    );
    println!(
        "Concurrent ({} tasks): {} chunks in {:?} (success: {}, skipped: {}, failed: {})",
        concurrency_level,
        num_files,
        concurrent_duration,
        concurrent_success,
        concurrent_skipped,
        concurrent_failed
    );

    // Detailed analysis
    let analysis = ConcurrencyAnalysis {
        sequential_total_ms: sequential_duration.as_millis(),
        concurrent_total_ms: concurrent_duration.as_millis(),
        individual_times: conc_timings,
        overlap_ratio: overlap_ratio.max(0.0),
    };
    analysis.print_report();

    if sequential_duration > Duration::ZERO && sequential_success > 0 && concurrent_success > 0 {
        let speedup = sequential_duration.as_secs_f64() / concurrent_duration.as_secs_f64();
        println!("\n=== Speedup Analysis ===");
        println!("Potential speedup with concurrency: {:.2}x", speedup);

        println!("\n[CI_METRICS backend={}]", backend);
        println!("sequential_duration_ms={}", sequential_duration.as_millis());
        println!("concurrent_duration_ms={}", concurrent_duration.as_millis());
        println!("sequential_success={}", sequential_success);
        println!("concurrent_success={}", concurrent_success);
        println!("concurrent_skipped={}", concurrent_skipped);
        println!("concurrent_failed={}", concurrent_failed);
        println!("potential_speedup={:.2}", speedup);
        println!("overlap_ratio={:.2}", overlap_ratio);

        // Backend-specific analysis
        match backend.as_str() {
            "etcd" => {
                println!("\n=== etcd Backend Analysis ===");
                println!("  - etcd uses Raft consensus: all writes go through a single leader");
                println!("  - Write operations are replicated to quorum before returning");
                println!(
                    "  - Even with concurrent clients, server processes requests sequentially"
                );
                if speedup < 1.2 {
                    println!(
                        "  - Low speedup expected: etcd is optimized for consistency, not throughput"
                    );
                }
            }
            "redis" => {
                println!("\n=== Redis Backend Analysis ===");
                println!("  - Redis is single-threaded: all commands execute sequentially");
                println!(
                    "  - Even with connection pooling, server processes one command at a time"
                );
                if speedup < 1.2 {
                    println!(
                        "  - Low speedup expected: Redis bottleneck is CPU-bound single thread"
                    );
                }
            }
            _ => {
                println!("\n=== SQLite Backend Analysis ===");
                println!("  - SQLite uses file-level locking");
                println!("  - WAL mode allows concurrent reads but writes are serialized");
            }
        }

        if concurrent_failed > 0 {
            println!(
                "\n⚠️  WARNING: {} compactions failed in concurrent mode!",
                concurrent_failed
            );
            println!("    This suggests lock contention or transaction conflicts.");
        } else if speedup > 1.5 {
            println!("\n✅ Significant speedup potential detected!");
        } else if speedup > 1.1 {
            println!("\n⚠️  Modest speedup. Check individual task times above for details.");
        } else {
            println!("\nℹ️  Minimal speedup with concurrency");
            println!("    This is expected behavior for single-threaded backends like etcd/Redis.");
            println!(
                "    The backend processes requests sequentially regardless of client concurrency."
            );
        }
    } else {
        println!("\n[CI_METRICS backend={}]", backend);
        println!("sequential_duration_ms={}", sequential_duration.as_millis());
        println!("concurrent_duration_ms={}", concurrent_duration.as_millis());
        println!("sequential_success={}", sequential_success);
        println!("concurrent_success={}", concurrent_success);
        println!("concurrent_failed={}", concurrent_failed);
        println!("note=test_setup_issue");
    }
}

/// Test to verify if the issue is with MetaStore contention vs actual work
#[tokio::test]
async fn test_metastore_contention_analysis() {
    let backend = get_backend_type();
    println!(
        "\n[Perf Test] MetaStore Contention Analysis (backend: {})",
        backend
    );

    // Test 1: Sequential vs Concurrent next_id latency
    // Use a fresh store for this test
    {
        let (_tmp, meta_store, _block_store, _) = setup_test_fs().await;

        println!("\n--- Testing next_id latency ---");
        let id_start = Instant::now();
        for _ in 0..100 {
            meta_store.next_id(SLICE_ID_KEY).await.unwrap();
        }
        let id_seq_duration = id_start.elapsed();
        let avg_id_latency = id_seq_duration.as_micros() as f64 / 100.0;
        println!(
            "Sequential next_id (100 calls): {:?} (avg {:.2} µs/call)",
            id_seq_duration, avg_id_latency
        );

        println!("--- Testing concurrent next_id latency ---");
        let id_concurrent_start = Instant::now();
        let mut id_handles = Vec::new();
        for _ in 0..100 {
            let meta = meta_store.clone();
            let h = tokio::spawn(async move {
                meta.next_id(SLICE_ID_KEY).await.unwrap();
            });
            id_handles.push(h);
        }
        for h in id_handles {
            h.await.unwrap();
        }
        let id_concurrent_duration = id_concurrent_start.elapsed();
        let avg_id_concurrent_latency = id_concurrent_duration.as_micros() as f64 / 100.0;
        println!(
            "Concurrent next_id (100 calls, 100 tasks): {:?} (avg {:.2} µs/call)",
            id_concurrent_duration, avg_id_concurrent_latency
        );
        println!(
            "ID allocation speedup: {:.2}x",
            id_seq_duration.as_secs_f64() / id_concurrent_duration.as_secs_f64()
        );
    }

    // Test 2: Compaction with varying concurrency levels
    // Each concurrency level gets its OWN fresh store to avoid data pollution
    println!("\n--- Testing compaction with varying concurrency levels ---");

    for concurrency in [1, 2, 4] {
        // Create FRESH store for each concurrency test
        let (_tmp, meta_store, block_store, _) = setup_test_fs().await;
        let root = meta_store.root_ino();

        let num_files = 12;
        let mut test_chunk_ids = Vec::new();

        // Create fragmented files
        for i in 0..num_files {
            let file = meta_store
                .create_file(root, format!("contention_test_c{}_{}.txt", concurrency, i))
                .await
                .unwrap();
            let chunk_id = create_fragmented_file(&meta_store, &block_store, file, 15).await;
            test_chunk_ids.push(chunk_id);
        }

        println!(
            "\n  Created {} files for concurrency {} test",
            num_files, concurrency
        );

        // Run compaction with specified concurrency
        let sem = Arc::new(Semaphore::new(concurrency));
        let start = Instant::now();
        let mut handles = Vec::new();

        for chunk_id in test_chunk_ids {
            let permit = sem.clone().acquire_owned().await.unwrap();
            let meta = meta_store.clone();
            let block = block_store.clone();
            let h = tokio::spawn(async move {
                #[allow(clippy::used_underscore_binding)]
                let _permit = permit;
                let compactor =
                    Compactor::with_layout(meta.clone(), block.clone(), ChunkLayout::default());
                let chunk_start = Instant::now();
                let result = compactor.compact_chunk(chunk_id).await;
                let elapsed = chunk_start.elapsed();
                (chunk_id, result, elapsed)
            });
            handles.push(h);
        }

        let mut success_count = 0;
        let mut skip_count = 0;
        let mut fail_count = 0;
        let mut timings = Vec::new();

        for h in handles {
            let (chunk_id, result, elapsed) = h.await.unwrap();
            match result {
                Ok(CompactResult::Light { .. }) | Ok(CompactResult::Heavy { .. }) => {
                    success_count += 1
                }
                Ok(CompactResult::Skipped) => skip_count += 1,
                Err(_) => fail_count += 1,
            }
            timings.push((chunk_id, elapsed.as_millis()));
        }

        let duration = start.elapsed();

        // Calculate statistics
        let avg_time = if !timings.is_empty() {
            timings.iter().map(|(_, t)| t).sum::<u128>() / timings.len() as u128
        } else {
            0
        };
        let min_time = timings.iter().map(|(_, t)| *t).min().unwrap_or(0);
        let max_time = timings.iter().map(|(_, t)| *t).max().unwrap_or(0);

        println!(
            "  Concurrency {}: {:?} ({:.2} chunks/sec)",
            concurrency,
            duration,
            num_files as f64 / duration.as_secs_f64()
        );
        println!(
            "    Results: {} succeeded, {} skipped, {} failed",
            success_count, skip_count, fail_count
        );
        println!(
            "    Timing: avg={} ms, min={} ms, max={} ms",
            avg_time, min_time, max_time
        );

        // Sanity check: if most chunks were skipped, something is wrong
        if skip_count > num_files / 2 {
            println!(
                "    ⚠️ WARNING: Too many chunks skipped ({}/{}). Data may be corrupted.",
                skip_count, num_files
            );
        }

        // Show distribution if there's high variance
        if max_time > min_time * 2 && max_time > 0 && min_time > 0 {
            println!(
                "    ⚠️ High variance: {}x difference between min and max",
                max_time as f64 / min_time as f64
            );
        }

        // Drop stores before next iteration to ensure clean state
        drop(meta_store);
        drop(block_store);
    }

    println!("\n=== Contention Analysis Summary ===");
    println!("Backend: {}", backend);

    match backend.as_str() {
        "etcd" => {
            println!("\n📊 etcd-specific observations:");
            println!(
                "  - If next_id concurrent < 1x: etcd's sequential write processing is the bottleneck"
            );
            println!(
                "  - If chunk times vary widely: lock contention on shared keys (e.g., SLICE_ID_KEY)"
            );
        }
        "redis" => {
            println!("\n📊 Redis-specific observations:");
            println!("  - Redis pipeline can improve throughput despite single-threaded server");
            println!("  - Lua scripts execute atomically, blocking other commands");
        }
        _ => {}
    }
}
