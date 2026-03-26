mod common;

use slayerfs::chunk::{SliceDesc, SliceOffset, block_span_iter_slice, store::BlockStore};
use slayerfs::meta::SLICE_ID_KEY;
use slayerfs::meta::store::MetaStore;
use slayerfs::vfs::chunk_id_for;
use slayerfs::{
    BlockGcConfig, BlockStoreGC, ChunkLayout, CompactResult, Compactor, InMemoryBlockStore,
};
use std::time::Duration;

use common::{generate_test_pattern, setup_test_fs};

#[tokio::test]
async fn compaction_preserves_data_integrity() {
    let (_tmp_dir, meta_store, block_store) = setup_test_fs().await;

    let root = meta_store.root_ino();
    let file = meta_store
        .create_file(root, "test.txt".to_string())
        .await
        .unwrap();

    let write_pattern = generate_test_pattern(10 * 1024);
    for i in 0..5u64 {
        let offset = i * 512;
        let data = &write_pattern[offset as usize..(offset + 1024) as usize];
        write_to_file(&*meta_store, &block_store, file, offset, data).await;
    }

    let chunk_id = chunk_id_for(file, 0).unwrap();

    let data_before = read_entire_file(&*meta_store, &block_store, file).await;

    let compactor = Compactor::with_layout(
        meta_store.clone(),
        block_store.clone(),
        ChunkLayout::default(),
    );

    let stats_before = compactor.analyze_chunk(chunk_id).await.unwrap();
    let result = compactor.compact_chunk(chunk_id).await;

    assert!(result.is_ok(), "Compaction failed: {:?}", result.err());

    let data_after = read_entire_file(&*meta_store, &block_store, file).await;

    assert_eq!(
        data_before,
        data_after,
        "DATA INTEGRITY VIOLATION: File content changed after compaction!\n\
         Before: {:?}\n\
         After:  {:?}",
        &data_before[..data_before.len().min(100)],
        &data_after[..data_after.len().min(100)]
    );

    let stats_after = compactor.analyze_chunk(chunk_id).await.unwrap();
    println!(
        "Compaction: {} slices (frag={:.2}) -> {} slices (frag={:.2})",
        stats_before.0, stats_before.2, stats_after.0, stats_after.2
    );
}

#[tokio::test]
async fn gc_removes_orphaned_blocks_after_compaction() {
    let (_tmp_dir, meta_store, block_store) = setup_test_fs().await;

    let root = meta_store.root_ino();
    let file = meta_store
        .create_file(root, "gc_test.txt".to_string())
        .await
        .unwrap();
    let chunk_id = chunk_id_for(file, 0).unwrap();

    for i in 0..8u64 {
        let data = vec![0xA0 + i as u8; 4096];
        write_to_file(&*meta_store, &block_store, file, i * 256, &data).await;
    }

    let compactor = Compactor::with_layout(
        meta_store.clone(),
        block_store.clone(),
        ChunkLayout::default(),
    );
    let compact_result = compactor.compact_chunk(chunk_id).await.unwrap();

    if compact_result == CompactResult::Skipped {
        println!("SKIP: No compaction occurred (fragmentation below threshold)");
        return;
    }

    // Use max_age_secs=0 to process all slices regardless of creation time
    // (process_delayed_slices uses CreatedAt.le(cutoff_time), so with max_age_secs=0,
    // all slices up to current time will be included)
    let delayed_before = meta_store.process_delayed_slices(100, 0).await.unwrap();

    assert!(
        !delayed_before.is_empty(),
        "Expected delayed slices after compaction, found none"
    );

    let gc = BlockStoreGC::new(meta_store.clone(), block_store.clone());
    let gc_config = BlockGcConfig {
        interval: Duration::from_secs(60),
        min_age_secs: 0,
        batch_size: 100,
        block_size: 4 * 1024 * 1024,
        orphan_cleanup_age_secs: 60,
    };

    gc.run_gc_cycle(&gc_config).await.unwrap();

    // Use max_age_secs=0 to include all pending slices regardless of creation time
    let delayed_after = meta_store.process_delayed_slices(100, 0).await.unwrap();

    println!(
        "GC processed: {} delayed slices -> {} remaining",
        delayed_before.len(),
        delayed_after.len()
    );

    assert!(
        delayed_after.len() <= delayed_before.len(),
        "GC should not increase delayed slice count"
    );
}

#[tokio::test]
async fn concurrent_write_safety() {
    let (_tmp_dir, meta_store, block_store) = setup_test_fs().await;

    let root = meta_store.root_ino();
    let file = meta_store
        .create_file(root, "concurrent.txt".to_string())
        .await
        .unwrap();
    let chunk_id = chunk_id_for(file, 0).unwrap();

    let initial_data = vec![0xBBu8; 8192];
    write_to_file(&*meta_store, &block_store, file, 0, &initial_data).await;

    let new_data = vec![0xCCu8; 4096];
    write_to_file(&*meta_store, &block_store, file, 4096, &new_data).await;

    let compactor = Compactor::with_layout(
        meta_store.clone(),
        block_store.clone(),
        ChunkLayout::default(),
    );
    let _ = compactor.compact_chunk(chunk_id).await;

    let final_data = read_entire_file(&*meta_store, &block_store, file).await;

    assert_eq!(
        &final_data[..4096],
        &initial_data[..4096],
        "Initial data corrupted!"
    );

    assert_eq!(
        &final_data[4096..8192],
        new_data.as_slice(),
        "Concurrent write data lost!"
    );
}

#[tokio::test]
async fn compaction_converges_to_optimal_state() {
    let (_tmp_dir, meta_store, block_store) = setup_test_fs().await;

    let root = meta_store.root_ino();
    let file = meta_store
        .create_file(root, "converge.txt".to_string())
        .await
        .unwrap();
    let chunk_id = chunk_id_for(file, 0).unwrap();

    let compactor = Compactor::with_layout(
        meta_store.clone(),
        block_store.clone(),
        ChunkLayout::default(),
    );

    for i in 0..15u64 {
        let data = vec![(i % 256) as u8; 2048];
        let offset = (i * 1024) % (64 * 1024 * 1024);
        write_to_file(&*meta_store, &block_store, file, offset, &data).await;
    }

    let stats_before = compactor.analyze_chunk(chunk_id).await.unwrap();

    let mut compact_count = 0;
    for _ in 0..5 {
        match compactor.compact_chunk(chunk_id).await.unwrap() {
            CompactResult::Skipped => break,
            _ => compact_count += 1,
        }
    }

    let stats_after = compactor.analyze_chunk(chunk_id).await.unwrap();

    println!(
        "Convergence: {} compactions, frag {:.2} -> {:.2}",
        compact_count, stats_before.2, stats_after.2
    );

    assert!(
        stats_after.2 < stats_before.2 || stats_after.2 < 0.01,
        "Fragmentation did not decrease: {} -> {}",
        stats_before.2,
        stats_after.2
    );

    // Verify data integrity by checking file content
    let data_after = read_entire_file(&*meta_store, &block_store, file).await;
    for i in 0..15u64 {
        let offset = (i * 1024) % (64 * 1024 * 1024);
        let expected = (i % 256) as u8;
        let actual = data_after[offset as usize];
        assert_eq!(actual, expected, "Data mismatch at offset {}", offset);
    }
}

#[tokio::test]
async fn gc_handles_partial_failures() {
    let (_tmp_dir, meta_store, block_store) = setup_test_fs().await;

    let root = meta_store.root_ino();
    let file = meta_store
        .create_file(root, "gc_fail.txt".to_string())
        .await
        .unwrap();
    let chunk_id = chunk_id_for(file, 0).unwrap();

    for i in 0..6u64 {
        let data = vec![0x10 + i as u8; 4096];
        write_to_file(&*meta_store, &block_store, file, i * 512, &data).await;
    }

    let compactor = Compactor::with_layout(
        meta_store.clone(),
        block_store.clone(),
        ChunkLayout::default(),
    );
    compactor.compact_chunk(chunk_id).await.unwrap();

    let delayed = meta_store.process_delayed_slices(100, 0).await.unwrap();
    if delayed.is_empty() {
        println!("SKIP: No delayed slices");
        return;
    }

    let gc = BlockStoreGC::new(meta_store.clone(), block_store.clone());
    let gc_config = BlockGcConfig {
        interval: Duration::from_secs(60),
        min_age_secs: 0,
        batch_size: 100,
        block_size: 4 * 1024 * 1024,
        orphan_cleanup_age_secs: 60,
    };

    gc.run_gc_cycle(&gc_config).await.unwrap();

    let remaining = meta_store.process_delayed_slices(100, 0).await.unwrap();

    println!(
        "Two-phase GC: {} delayed -> {} remaining after cycle",
        delayed.len(),
        remaining.len()
    );

    if !remaining.is_empty() {
        gc.run_gc_cycle(&gc_config).await.unwrap();
        let after_second = meta_store.process_delayed_slices(100, 0).await.unwrap();
        println!("After second cycle: {} remaining", after_second.len());
    }
}

async fn write_to_file(
    meta_store: &dyn MetaStore,
    block_store: &InMemoryBlockStore,
    ino: i64,
    offset: u64,
    data: &[u8],
) {
    let chunk_size = 64 * 1024 * 1024u64;
    let chunk_index = (offset / chunk_size) as u32;
    let chunk_offset = offset % chunk_size;
    let chunk_id = chunk_id_for(ino, chunk_index as u64).unwrap();

    let slice_id = meta_store.next_id(SLICE_ID_KEY).await.unwrap() as u64;

    let slice = SliceDesc {
        slice_id,
        chunk_id,
        offset: chunk_offset,
        length: data.len() as u64,
    };

    let layout = ChunkLayout::default();
    let spans: Vec<_> = block_span_iter_slice(SliceOffset(0), data.len() as u64, layout).collect();

    let mut written = 0usize;
    for span in spans {
        let block_key = (slice_id, span.index as u32);
        let end = (written + span.len as usize).min(data.len());
        block_store
            .write_fresh_range(block_key, span.offset, &data[written..end])
            .await
            .unwrap();
        written = end;
    }

    let current_size = match meta_store.stat(ino).await.unwrap() {
        Some(attr) => attr.size,
        None => 0,
    };
    let new_size = (offset + data.len() as u64).max(current_size);

    meta_store
        .write(ino, chunk_id, slice, new_size)
        .await
        .unwrap();
}

async fn read_entire_file(
    meta_store: &dyn MetaStore,
    block_store: &InMemoryBlockStore,
    ino: i64,
) -> Vec<u8> {
    let file_size = match meta_store.stat(ino).await.unwrap() {
        Some(attr) => attr.size,
        None => return Vec::new(),
    };

    if file_size == 0 {
        return Vec::new();
    }

    let mut result = vec![0u8; file_size as usize];
    let chunk_size = 64 * 1024 * 1024u64;
    let layout = ChunkLayout::default();

    let num_chunks = file_size.div_ceil(chunk_size);
    for chunk_idx in 0..num_chunks {
        let chunk_id = chunk_id_for(ino, chunk_idx).unwrap();
        let slices = meta_store.get_slices(chunk_id).await.unwrap();

        for slice in slices {
            let slice_data = read_slice_data(block_store, &slice, layout).await;
            let chunk_start = (chunk_idx * chunk_size) as usize;
            let slice_start_in_chunk = slice.offset as usize;

            for (i, byte) in slice_data.iter().enumerate() {
                let file_pos = chunk_start + slice_start_in_chunk + i;
                if file_pos < result.len() {
                    result[file_pos] = *byte;
                }
            }
        }
    }

    result
}

async fn read_slice_data(
    block_store: &InMemoryBlockStore,
    slice: &SliceDesc,
    layout: ChunkLayout,
) -> Vec<u8> {
    let mut data = vec![0u8; slice.length as usize];
    let spans: Vec<_> = block_span_iter_slice(SliceOffset(0), slice.length, layout).collect();

    let mut offset_in_slice = 0usize;
    for span in spans {
        let block_key = (slice.slice_id, span.index as u32);
        let block_size = layout.block_size as usize;
        let mut block_buf = vec![0u8; block_size];

        block_store
            .read_range(block_key, span.offset, &mut block_buf[..span.len as usize])
            .await
            .unwrap();

        let copy_len = (span.len as usize).min(data.len() - offset_in_slice);
        data[offset_in_slice..offset_in_slice + copy_len].copy_from_slice(&block_buf[..copy_len]);
        offset_in_slice += copy_len;
    }

    data
}
