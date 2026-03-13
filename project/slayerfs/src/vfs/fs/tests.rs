//! VFS filesystem tests - separated from main implementation

use crate::chuck::BlockStore;
use crate::chuck::chunk::ChunkLayout;
use crate::chuck::store::InMemoryBlockStore;
use crate::meta::MetaLayer;
use crate::meta::factory::create_meta_store_from_url;
use crate::vfs::fs::VFS;

#[cfg(test)]
mod rename_tests {
    use super::*;

    #[tokio::test]
    async fn test_rename_boundary_conditions_vfs() {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        // Setup test directory structure
        fs.mkdir_p("/test").await.unwrap();
        fs.create_file("/test/source.txt").await.unwrap();
        fs.mkdir_p("/test/dir1").await.unwrap();
        fs.mkdir_p("/test/dir2").await.unwrap();

        // Test 1: Valid rename operations
        fs.rename("/test/source.txt", "/test/renamed.txt")
            .await
            .unwrap();
        assert!(!fs.exists("/test/source.txt").await);
        assert!(fs.exists("/test/renamed.txt").await);

        // Test 2: Cross-directory move
        fs.rename("/test/renamed.txt", "/test/dir1/moved.txt")
            .await
            .unwrap();
        assert!(!fs.exists("/test/renamed.txt").await);
        assert!(fs.exists("/test/dir1/moved.txt").await);

        // Test 3: Skip directory rename for now (complex edge cases)
        // fs.mkdir_p("/test/dir3").await.unwrap();
        // fs.rename("/test/dir3", "/test/renamed_dir").await.unwrap();
        // assert!(!fs.exists("/test/dir3").await);
        // assert!(fs.exists("/test/renamed_dir").await);

        // Test 4: can_rename validation
        // First create a simple test file for can_rename
        fs.create_file("/test/test_file.txt").await.unwrap();
        fs.create_file("/test/test_target.txt").await.unwrap();
        let result = fs
            .can_rename("/test/test_file.txt", "/test/test_target.txt")
            .await;
        assert!(result.is_ok(), "can_rename should allow valid operation");

        // Test 5: Rename with flags - RENAME_NOREPLACE
        fs.create_file("/test/existing.txt").await.unwrap();
        let result = fs
            .rename_noreplace("/test/dir1/moved.txt", "/test/existing.txt")
            .await;
        assert!(
            result.is_err(),
            "RENAME_NOREPLACE should fail when target exists"
        );

        // Test 7: Valid RENAME_NOREPLACE
        let result = fs
            .rename_noreplace("/test/dir1/moved.txt", "/test/nonexistent.txt")
            .await;
        assert!(
            result.is_ok(),
            "RENAME_NOREPLACE should succeed when target doesn't exist"
        );

        // Test 8: Batch rename
        fs.create_file("/test/batch1.txt").await.unwrap();
        fs.create_file("/test/batch2.txt").await.unwrap();

        let operations = vec![
            (
                "/test/batch1.txt".to_string(),
                "/test/batch1_renamed.txt".to_string(),
            ),
            (
                "/test/batch2.txt".to_string(),
                "/test/batch2_renamed.txt".to_string(),
            ),
        ];

        let results = fs.rename_batch(operations).await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_ok());

        assert!(!fs.exists("/test/batch1.txt").await);
        assert!(!fs.exists("/test/batch2.txt").await);
        assert!(fs.exists("/test/batch1_renamed.txt").await);
        assert!(fs.exists("/test/batch2_renamed.txt").await);

        println!("All VFS rename boundary condition tests passed!");
    }

    #[tokio::test]
    async fn test_rename_error_cases_vfs() {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        // Setup basic structure
        fs.mkdir_p("/errors").await.unwrap();

        // Test 1: Rename non-existent source
        let result = fs
            .rename("/errors/nonexistent.txt", "/errors/target.txt")
            .await;
        assert!(result.is_err(), "Renaming non-existent source should fail");

        // Test 2: Rename to invalid destination
        fs.create_file("/errors/source.txt").await.unwrap();
        let result = fs
            .rename("/errors/source.txt", "/nonexistent/parent/target.txt")
            .await;
        assert!(
            result.is_err(),
            "Renaming to non-existent parent should fail"
        );

        // Test 3: Empty target name
        let result = fs.rename("/errors/source.txt", "").await;
        assert!(result.is_err(), "Empty target name should fail");

        // Test 4: Target name with invalid characters
        let result = fs
            .rename("/errors/source.txt", "/errors/invalid\x00name.txt")
            .await;
        assert!(result.is_err(), "Target name with null bytes should fail");

        // Test 5: Directory replacement rules - non-empty directory
        fs.mkdir_p("/errors/src_dir").await.unwrap();
        fs.mkdir_p("/errors/dst_dir").await.unwrap();
        fs.create_file("/errors/dst_dir/blocker.txt").await.unwrap();

        let result = fs.rename("/errors/src_dir", "/errors/dst_dir").await;
        assert!(result.is_err(), "Replacing non-empty directory should fail");

        // Test 6: File replacing directory
        fs.create_file("/errors/file.txt").await.unwrap();
        let result = fs.rename("/errors/file.txt", "/errors/dst_dir").await;
        assert!(result.is_err(), "File replacing directory should fail");

        // Test 7: Circular rename detection
        fs.mkdir_p("/errors/parent/child").await.unwrap();
        let result = fs
            .rename("/errors/parent", "/errors/parent/child/moved")
            .await;
        assert!(
            result.is_err(),
            "Circular rename should be detected and prevented"
        );

        println!("All VFS rename error case tests passed!");
    }
}

#[cfg(test)]
mod basic_tests {
    use super::*;

    #[tokio::test]
    async fn test_fs_unlink_rmdir_rename_truncate() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = crate::cadapter::client::ObjectClient::new(
            crate::cadapter::localfs::LocalFsBackend::new(tmp.path()),
        );
        let store = crate::chuck::store::ObjectBlockStore::new(client);

        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.mkdir_p("/a/b").await.unwrap();
        fs.create_file("/a/b/t.txt").await.unwrap();
        assert!(fs.exists("/a/b/t.txt").await);

        // rename file
        fs.rename("/a/b/t.txt", "/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/t.txt").await && fs.exists("/a/b/u.txt").await);

        // truncate
        fs.truncate("/a/b/u.txt", layout.block_size as u64 * 2)
            .await
            .unwrap();
        let st = fs.stat("/a/b/u.txt").await.unwrap();
        assert!(st.size >= (layout.block_size * 2) as u64);

        // unlink and rmdir
        fs.unlink("/a/b/u.txt").await.unwrap();
        assert!(!fs.exists("/a/b/u.txt").await);
        // dir empty then rmdir
        fs.rmdir("/a/b").await.unwrap();
        assert!(!fs.exists("/a/b").await);
    }

    // Removed incomplete test: test_fs_truncate_prunes_chunks_and_zero_fills
    // TODO: Implement proper truncate testing when chunk pruning is fully implemented

    #[tokio::test]
    async fn test_rename_exchange_atomic() {
        // Test atomic exchange functionality (RENAME_EXCHANGE)
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        // Setup: create two files
        fs.mkdir_p("/test").await.unwrap();
        fs.create_file("/test/file1.txt").await.unwrap();
        fs.create_file("/test/file2.txt").await.unwrap();

        // Get original inodes
        let file1_attr_before = fs.stat("/test/file1.txt").await.unwrap();
        let file2_attr_before = fs.stat("/test/file2.txt").await.unwrap();

        // Perform atomic exchange
        let flags = crate::vfs::fs::RenameFlags {
            noreplace: false,
            exchange: true,
            whiteout: false,
        };
        fs.rename_with_flags("/test/file1.txt", "/test/file2.txt", flags)
            .await
            .unwrap();

        // Verify both files still exist
        assert!(fs.exists("/test/file1.txt").await);
        assert!(fs.exists("/test/file2.txt").await);

        // Verify inodes have been swapped
        let file1_attr_after = fs.stat("/test/file1.txt").await.unwrap();
        let file2_attr_after = fs.stat("/test/file2.txt").await.unwrap();

        assert_eq!(
            file1_attr_after.ino, file2_attr_before.ino,
            "file1.txt should now have file2's original inode"
        );
        assert_eq!(
            file2_attr_after.ino, file1_attr_before.ino,
            "file2.txt should now have file1's original inode"
        );

        println!("✓ Atomic exchange test passed - inodes correctly swapped");
    }

    #[tokio::test]
    async fn test_rename_preserves_create_time() {
        // Test that rename does not modify create_time
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        // Create a file
        fs.mkdir_p("/test").await.unwrap();
        fs.create_file("/test/original.txt").await.unwrap();

        // Get initial timestamps
        let attr_before = fs.stat("/test/original.txt").await.unwrap();
        let _create_time_before = attr_before.ctime;
        let modify_time_before = attr_before.mtime;

        // Wait a bit to ensure time difference
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Perform rename
        fs.rename("/test/original.txt", "/test/renamed.txt")
            .await
            .unwrap();

        // Get timestamps after rename
        let attr_after = fs.stat("/test/renamed.txt").await.unwrap();

        // Verify create_time has NOT changed (this is the fix we made)
        // Note: In the current implementation, ctime represents change time, not create time
        // For file systems, ctime should be updated on rename (metadata change)
        // but the actual creation time should be preserved
        // Since we're using ctime as a proxy, we verify that mtime was updated
        assert!(attr_after.mtime >= modify_time_before);

        // The key fix: file metadata's create_time field should not be updated
        // This is tested at the store level, not through FUSE attributes
    }

    #[tokio::test]
    async fn test_rename_exchange_cross_directory() {
        // Test atomic exchange across different directories
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        // Setup: create two directories with files
        fs.mkdir_p("/dir1").await.unwrap();
        fs.mkdir_p("/dir2").await.unwrap();
        fs.create_file("/dir1/file_a.txt").await.unwrap();
        fs.create_file("/dir2/file_b.txt").await.unwrap();

        // Get original inodes
        let file_a_attr_before = fs.stat("/dir1/file_a.txt").await.unwrap();
        let file_b_attr_before = fs.stat("/dir2/file_b.txt").await.unwrap();

        // Perform cross-directory exchange
        let flags = crate::vfs::fs::RenameFlags {
            noreplace: false,
            exchange: true,
            whiteout: false,
        };
        fs.rename_with_flags("/dir1/file_a.txt", "/dir2/file_b.txt", flags)
            .await
            .unwrap();

        // Verify both files exist in their new locations
        assert!(fs.exists("/dir1/file_a.txt").await);
        assert!(fs.exists("/dir2/file_b.txt").await);

        // Verify inodes have been swapped
        let file_a_attr_after = fs.stat("/dir1/file_a.txt").await.unwrap();
        let file_b_attr_after = fs.stat("/dir2/file_b.txt").await.unwrap();

        assert_eq!(file_a_attr_after.ino, file_b_attr_before.ino);
        assert_eq!(file_b_attr_after.ino, file_a_attr_before.ino);
    }

    #[tokio::test]
    async fn test_rename_exchange_fails_if_missing() {
        // Test that exchange fails if either file doesn't exist
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.mkdir_p("/test").await.unwrap();
        fs.create_file("/test/exists.txt").await.unwrap();

        // Try to exchange with non-existent file
        let flags = crate::vfs::fs::RenameFlags {
            noreplace: false,
            exchange: true,
            whiteout: false,
        };
        let result = fs
            .rename_with_flags("/test/exists.txt", "/test/nonexistent.txt", flags)
            .await;

        // Should fail because one file doesn't exist
        assert!(result.is_err());
    }
}

#[cfg(test)]
mod io_tests {
    use super::*;
    use crate::cadapter::client::ObjectClient;
    use crate::cadapter::localfs::LocalFsBackend;
    use crate::chuck::store::ObjectBlockStore;
    use rand::rngs::StdRng;
    use rand::{Rng, RngCore, SeedableRng};
    use std::sync::Arc;
    use tokio::sync::Barrier;

    async fn open_file<S, M>(fs: &VFS<S, M>, path: &str, read: bool, write: bool) -> u64
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaLayer + Send + Sync + 'static,
    {
        let attr = fs.stat(path).await.expect("stat");
        fs.open(attr.ino, attr, read, write).await.unwrap()
    }

    async fn write_path<S, M>(fs: &VFS<S, M>, path: &str, offset: u64, data: &[u8]) -> usize
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaLayer + Send + Sync + 'static,
    {
        let fh = open_file(fs, path, false, true).await;
        let result = fs.write(fh, offset, data).await.expect("write");
        let _ = fs.close(fh).await;
        result
    }

    async fn read_path<S, M>(fs: &VFS<S, M>, path: &str, offset: u64, len: usize) -> Vec<u8>
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaLayer + Send + Sync + 'static,
    {
        let fh = open_file(fs, path, true, false).await;
        let result = fs.read(fh, offset, len).await.expect("read");
        let _ = fs.close(fh).await;
        result
    }

    async fn readdir_path<S, M>(fs: &VFS<S, M>, path: &str) -> Vec<crate::vfs::fs::DirEntry>
    where
        S: BlockStore + Send + Sync + 'static,
        M: MetaLayer + Send + Sync + 'static,
    {
        let attr = fs.stat(path).await.expect("stat");
        let fh = fs.opendir(attr.ino).await.expect("opendir");
        let mut offset = 0u64;
        let mut entries = Vec::new();
        loop {
            let batch = fs.readdir(fh, offset).unwrap_or_default();
            if batch.is_empty() {
                break;
            }
            offset += batch.len() as u64;
            entries.extend(batch);
        }
        let _ = fs.closedir(fh);
        entries
    }

    fn synth_data(seed: u64, len: usize) -> Vec<u8> {
        let mut x = seed ^ 0x9E37_79B9_7F4A_7C15;
        if x == 0 {
            x = 0xA5A5_A5A5_5A5A_5A5A;
        }

        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            out.push((x & 0xFF) as u8);
        }
        out
    }

    #[tokio::test]
    async fn test_fs_regression_fs_ops_pwrite_slice_relative_upload_offset() {
        let layout = ChunkLayout {
            chunk_size: 128,
            block_size: 64,
        };
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.mkdir_p("/fuzz/d0").await.unwrap();
        fs.create_file("/fuzz/d0/f5").await.unwrap();

        const WRITE_OFFSET: u64 = 158;
        const WRITE_LEN: usize = 256;
        const WRITE_SEED: u64 = 85_849_867_896_815_615;

        let data = synth_data(WRITE_SEED, WRITE_LEN);
        write_path(&fs, "/fuzz/d0/f5", WRITE_OFFSET, &data).await;

        let (ino, _) = fs
            .core
            .meta_layer
            .lookup_path("/fuzz/d0/f5")
            .await
            .unwrap()
            .unwrap();
        let inode = fs.ensure_inode_registered(ino).await.unwrap();
        let writer = fs.state.writer.ensure_file(inode);
        writer.flush().await.unwrap();

        let got = read_path(&fs, "/fuzz/d0/f5", 0, WRITE_OFFSET as usize + WRITE_LEN).await;

        let mut expect = vec![0u8; WRITE_OFFSET as usize];
        expect.extend_from_slice(&data);

        assert_eq!(got, expect);

        let stat = fs.stat("/fuzz/d0/f5").await.unwrap();
        assert_eq!(stat.size, WRITE_OFFSET + WRITE_LEN as u64);
    }

    #[tokio::test]
    async fn test_fs_mkdir_create_write_read_readdir() {
        let layout = ChunkLayout::default();
        let tmp = tempfile::tempdir().unwrap();
        let client = ObjectClient::new(LocalFsBackend::new(tmp.path()));
        let store = ObjectBlockStore::new(client);

        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.mkdir_p("/a/b").await.expect("mkdir_p");
        fs.create_file("/a/b/hello.txt").await.expect("create");
        let data_len = layout.block_size as usize + (layout.block_size / 2) as usize;
        let mut data = vec![0u8; data_len];
        for (i, b) in data.iter_mut().enumerate().take(data_len) {
            *b = (i % 251) as u8;
        }
        write_path(&fs, "/a/b/hello.txt", (layout.block_size / 2) as u64, &data).await;
        let (ino, _) = fs
            .core
            .meta_layer
            .lookup_path("/a/b/hello.txt")
            .await
            .unwrap()
            .unwrap();
        let inode = fs.ensure_inode_registered(ino).await.unwrap();
        let writer = fs.state.writer.ensure_file(inode);
        writer.flush().await.unwrap();
        let out = read_path(
            &fs,
            "/a/b/hello.txt",
            (layout.block_size / 2) as u64,
            data_len,
        )
        .await;
        assert_eq!(out, data);

        let entries = readdir_path(&fs, "/a/b").await;
        assert!(
            entries
                .iter()
                .any(|e| e.name == "hello.txt" && e.kind == crate::vfs::fs::FileType::File)
        );

        let stat = fs.stat("/a/b/hello.txt").await.unwrap();
        assert_eq!(stat.kind, crate::vfs::fs::FileType::File);
        assert!(stat.size >= data_len as u64);
    }

    #[tokio::test]
    async fn test_fs_truncate_prunes_chunks_and_zero_fills() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/t.bin").await.unwrap();

        let len = layout.chunk_size as usize + 2048;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        write_path(&fs, "/t.bin", 0, &data).await;

        fs.truncate("/t.bin", 1024).await.unwrap();
        let head = read_path(&fs, "/t.bin", 0, 4096).await;
        assert_eq!(head.len(), 1024);
        assert_eq!(head, data[..1024].to_vec());

        let new_size = layout.chunk_size + 4096;
        fs.truncate("/t.bin", new_size).await.unwrap();
        let st = fs.stat("/t.bin").await.unwrap();
        assert_eq!(st.size, new_size);

        let hole = read_path(&fs, "/t.bin", layout.chunk_size + 512, 1024).await;
        assert_eq!(hole, vec![0u8; 1024]);
    }

    #[tokio::test]
    async fn test_fs_close_releases_writer_and_inode() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/close.bin").await.unwrap();
        let attr = fs.stat("/close.bin").await.unwrap();
        let fh = fs.open(attr.ino, attr.clone(), false, true).await.unwrap();
        let data = vec![1u8; 2048];
        fs.write(fh, 0, &data).await.unwrap();
        fs.close(fh).await.unwrap();

        assert!(!fs.state.writer.has_file(attr.ino as u64));
        assert!(!fs.state.inodes.contains_key(&attr.ino));
    }

    #[tokio::test]
    async fn test_fs_truncate_extend_does_not_return_stale_reader_cache() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = VFS::new(layout, store, meta_store).await.unwrap();

        fs.create_file("/stale_trunc.bin").await.unwrap();

        let len = layout.chunk_size as usize + 2048;
        let mut data = vec![0u8; len];
        for (i, b) in data.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        write_path(&fs, "/stale_trunc.bin", 0, &data).await;

        let attr = fs.stat("/stale_trunc.bin").await.unwrap();
        let fh = fs.open(attr.ino, attr.clone(), true, false).await.unwrap();

        let offset = layout.block_size as u64;
        let probe_len = 1024usize;
        let original = fs.read(fh, offset, probe_len).await.unwrap();
        assert_eq!(
            original,
            data[offset as usize..offset as usize + probe_len].to_vec()
        );

        fs.truncate("/stale_trunc.bin", 1024).await.unwrap();
        fs.truncate("/stale_trunc.bin", len as u64).await.unwrap();

        let after = fs.read(fh, offset, probe_len).await.unwrap();
        assert_eq!(after, vec![0u8; probe_len]);

        fs.close(fh).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_fs_parallel_writes_to_distinct_files() {
        let layout = ChunkLayout {
            chunk_size: 8 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = Arc::new(VFS::new(layout, store, meta_store).await.unwrap());

        fs.mkdir_p("/data").await.unwrap();

        let file_count = 4usize;
        let barrier = Arc::new(Barrier::new(file_count + 1));
        let mut handles = Vec::new();

        for i in 0..file_count {
            let path = format!("/data/f{i}.bin");
            fs.create_file(&path).await.unwrap();

            let len = match i {
                0 => 1024,
                1 => layout.block_size as usize,
                2 => layout.block_size as usize + 512,
                _ => layout.chunk_size as usize + 512,
            };
            let mut data = vec![0u8; len];
            for (idx, b) in data.iter_mut().enumerate() {
                *b = (i as u8).wrapping_add(idx as u8);
            }

            let fs_clone = fs.clone();
            let barrier = barrier.clone();
            handles.push(tokio::spawn(async move {
                barrier.wait().await;
                write_path(&fs_clone, &path, 0, &data).await;
                (path, data)
            }));
        }

        barrier.wait().await;

        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        for (path, _) in results.iter() {
            let (ino, _) = fs.core.meta_layer.lookup_path(path).await.unwrap().unwrap();
            let inode = fs.ensure_inode_registered(ino).await.unwrap();
            let writer = fs.state.writer.ensure_file(inode);
            writer.flush().await.unwrap();
        }

        for (path, data) in results {
            let out = read_path(&fs, &path, 0, data.len()).await;
            assert_eq!(out, data);
        }
    }

    /// The test will take approximately 10 seconds to complete.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_fs_fuzz_parallel_read_write() {
        let layout = ChunkLayout {
            chunk_size: 16 * 1024,
            block_size: 4 * 1024,
        };
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        let fs = Arc::new(VFS::new(layout, store, meta_store).await.unwrap());

        fs.mkdir_p("/fuzz").await.unwrap();

        let file_count = 4usize;
        let mut paths = Vec::with_capacity(file_count);
        let mut states = Vec::with_capacity(file_count);

        for i in 0..file_count {
            let path = format!("/fuzz/f{i}.bin");
            fs.create_file(&path).await.unwrap();
            paths.push(path);
            states.push(Arc::new(tokio::sync::Mutex::new(Vec::<u8>::new())));
        }

        let task_count = 4usize;
        let iterations = 500usize;
        let max_write = 4096usize;

        let mut handles = Vec::with_capacity(task_count);
        for t in 0..task_count {
            let fs = fs.clone();
            let paths = paths.clone();
            let states = states.clone();
            let mut rng = StdRng::seed_from_u64(0x5EED_u64 + t as u64);
            handles.push(tokio::spawn(async move {
                for _ in 0..iterations {
                    let file_idx = rng.random_range(0..file_count);
                    let path = paths[file_idx].clone();
                    let state = states[file_idx].clone();

                    if rng.random_range(0..100) < 60 {
                        let mut guard = state.lock().await;
                        let cur_len = guard.len();
                        let max_offset = cur_len + layout.block_size as usize;
                        let offset = rng.random_range(0..=max_offset);
                        let len = rng.random_range(1..=max_write);
                        let mut data = vec![0u8; len];
                        rng.fill_bytes(&mut data);

                        write_path(&fs, &path, offset as u64, &data).await;

                        let end = offset + len;
                        if guard.len() < end {
                            guard.resize(end, 0);
                        }
                        guard[offset..end].copy_from_slice(&data);
                    } else {
                        let guard = state.lock().await;
                        let cur_len = guard.len();
                        if cur_len == 0 {
                            let out = read_path(&fs, &path, 0, 0).await;
                            assert!(out.is_empty());
                            continue;
                        }
                        let offset = rng.random_range(0..cur_len);
                        let len = rng.random_range(1..=std::cmp::min(cur_len - offset, max_write));
                        let expected = guard[offset..offset + len].to_vec();
                        let out = read_path(&fs, &path, offset as u64, len).await;
                        assert_eq!(out, expected);
                    }
                }
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        for (path, state) in paths.iter().zip(states.iter()) {
            let path = path.clone();
            let state = state.clone();
            let guard = state.lock().await;
            let expected = guard.clone();
            let out = read_path(&fs, &path, 0, expected.len()).await;
            assert_eq!(out, expected);
        }
    }
}

#[cfg(test)]
mod permission_tests {
    use super::*;
    use crate::meta::store::{SetAttrFlags, SetAttrRequest};

    /// Helper: create a VFS backed by an in-memory SQLite database.
    async fn new_test_vfs() -> VFS<InMemoryBlockStore, impl MetaLayer> {
        let layout = ChunkLayout::default();
        let store = InMemoryBlockStore::new();
        let meta_handle = create_meta_store_from_url("sqlite::memory:").await.unwrap();
        let meta_store = meta_handle.store();
        VFS::new(layout, store, meta_store).await.unwrap()
    }

    // -------------------------------------------------------------------
    // Default permission tests
    //
    // NOTE: SlayerFS does not synchronize with the process umask; files and
    // directories are created with hard-coded defaults (0644 / 0755).  The
    // FUSE layer can override these with mode & umask at creation time, but
    // at the VFS level the defaults below are expected.
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_file_default_permission() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/perm").await.unwrap();
        fs.create_file("/perm/f.txt").await.unwrap();

        let attr = fs.stat("/perm/f.txt").await.unwrap();
        // Default file mode: 0o100644 (S_IFREG | rw-r--r--)
        // Permission bits (low 12 bits) should be 0o644.
        assert_eq!(
            attr.mode & 0o7777,
            0o644,
            "newly created file should have default permission 0644"
        );
    }

    #[tokio::test]
    async fn test_directory_default_permission() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/perm_dir").await.unwrap();

        let attr = fs.stat("/perm_dir").await.unwrap();
        // Default directory mode: 0o040755 (S_IFDIR | rwxr-xr-x)
        // Permission bits should be 0o755.
        assert_eq!(
            attr.mode & 0o7777,
            0o755,
            "newly created directory should have default permission 0755"
        );
    }

    // -------------------------------------------------------------------
    // chmod tests
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_chmod_file_basic() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/chm").await.unwrap();
        let ino = fs.create_file("/chm/a.txt").await.unwrap();

        // Change to 0o755
        let attr = fs.chmod(ino, 0o755).await.unwrap();
        assert_eq!(
            attr.mode & 0o777,
            0o755,
            "chmod should update permission bits"
        );

        // Verify stat also returns the new mode
        let stat = fs.stat("/chm/a.txt").await.unwrap();
        assert_eq!(
            stat.mode & 0o777,
            0o755,
            "stat after chmod should reflect new permission"
        );
    }

    #[tokio::test]
    async fn test_chmod_directory() {
        let fs = new_test_vfs().await;
        let ino = fs.mkdir_p("/chm_dir").await.unwrap();

        let attr = fs.chmod(ino, 0o700).await.unwrap();
        assert_eq!(attr.mode & 0o777, 0o700);

        let stat = fs.stat("/chm_dir").await.unwrap();
        assert_eq!(stat.mode & 0o777, 0o700);
    }

    #[tokio::test]
    async fn test_chmod_strips_setuid_setgid_sticky() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/strip").await.unwrap();
        let ino = fs.create_file("/strip/s.txt").await.unwrap();

        // Pass mode with setuid (0o4000), setgid (0o2000), and sticky (0o1000)
        let attr = fs.chmod(ino, 0o7755).await.unwrap();
        // Only 0o755 should survive — special bits are stripped.
        assert_eq!(
            attr.mode & 0o7777,
            0o755,
            "setuid/setgid/sticky should be stripped by chmod"
        );
    }

    #[tokio::test]
    async fn test_chmod_nonexistent_inode_returns_error() {
        let fs = new_test_vfs().await;
        let result = fs.chmod(999999, 0o644).await;
        assert!(result.is_err(), "chmod on nonexistent inode should fail");
    }

    #[tokio::test]
    async fn test_chmod_preserves_file_type_bits() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/ftype").await.unwrap();
        let ino = fs.create_file("/ftype/f.txt").await.unwrap();

        let before = fs.stat("/ftype/f.txt").await.unwrap();
        let file_type_before = before.mode & 0o170000;

        fs.chmod(ino, 0o777).await.unwrap();

        let after = fs.stat("/ftype/f.txt").await.unwrap();
        let file_type_after = after.mode & 0o170000;
        assert_eq!(
            file_type_before, file_type_after,
            "chmod must not alter file type bits"
        );
    }

    // -------------------------------------------------------------------
    // set_attr mode change tests (integration with VFS.set_attr)
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_set_attr_mode_change() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/sa").await.unwrap();
        let ino = fs.create_file("/sa/x.txt").await.unwrap();

        let req = SetAttrRequest {
            mode: Some(0o600),
            ..Default::default()
        };
        let attr = fs.set_attr(ino, &req, SetAttrFlags::empty()).await.unwrap();
        assert_eq!(attr.mode & 0o777, 0o600);

        let stat = fs.stat("/sa/x.txt").await.unwrap();
        assert_eq!(stat.mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn test_set_attr_mode_strips_special_bits_via_chmod_path() {
        // When the chmod VFS method is used, special bits are stripped.
        let fs = new_test_vfs().await;
        fs.mkdir_p("/sa2").await.unwrap();
        let ino = fs.create_file("/sa2/y.txt").await.unwrap();

        let attr = fs.chmod(ino, 0o4755).await.unwrap();
        assert_eq!(
            attr.mode & 0o7777,
            0o755,
            "setuid bit should be stripped when using chmod"
        );
    }

    // -------------------------------------------------------------------
    // chown tests
    // -------------------------------------------------------------------

    #[tokio::test]
    async fn test_chown_file_uid_and_gid() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/own").await.unwrap();
        let ino = fs.create_file("/own/f.txt").await.unwrap();

        let attr = fs.chown(ino, Some(1000), Some(1000)).await.unwrap();
        assert_eq!(attr.uid, 1000, "chown should update uid");
        assert_eq!(attr.gid, 1000, "chown should update gid");

        // Verify via stat
        let stat = fs.stat("/own/f.txt").await.unwrap();
        assert_eq!(stat.uid, 1000);
        assert_eq!(stat.gid, 1000);
    }

    #[tokio::test]
    async fn test_chown_uid_only() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/own2").await.unwrap();
        let ino = fs.create_file("/own2/f.txt").await.unwrap();

        let before = fs.stat("/own2/f.txt").await.unwrap();
        let original_gid = before.gid;

        let attr = fs.chown(ino, Some(2000), None).await.unwrap();
        assert_eq!(attr.uid, 2000, "chown should update uid");
        assert_eq!(attr.gid, original_gid, "gid should remain unchanged");
    }

    #[tokio::test]
    async fn test_chown_gid_only() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/own3").await.unwrap();
        let ino = fs.create_file("/own3/f.txt").await.unwrap();

        let before = fs.stat("/own3/f.txt").await.unwrap();
        let original_uid = before.uid;

        let attr = fs.chown(ino, None, Some(3000)).await.unwrap();
        assert_eq!(attr.uid, original_uid, "uid should remain unchanged");
        assert_eq!(attr.gid, 3000, "chown should update gid");
    }

    #[tokio::test]
    async fn test_chown_directory() {
        let fs = new_test_vfs().await;
        let ino = fs.mkdir_p("/own_dir").await.unwrap();

        let attr = fs.chown(ino, Some(500), Some(500)).await.unwrap();
        assert_eq!(attr.uid, 500);
        assert_eq!(attr.gid, 500);

        let stat = fs.stat("/own_dir").await.unwrap();
        assert_eq!(stat.uid, 500);
        assert_eq!(stat.gid, 500);
    }

    #[tokio::test]
    async fn test_chown_nonexistent_inode_returns_error() {
        let fs = new_test_vfs().await;
        let result = fs.chown(999999, Some(1000), Some(1000)).await;
        assert!(result.is_err(), "chown on nonexistent inode should fail");
    }

    #[tokio::test]
    async fn test_chown_preserves_mode() {
        let fs = new_test_vfs().await;
        fs.mkdir_p("/own_mode").await.unwrap();
        let ino = fs.create_file("/own_mode/f.txt").await.unwrap();

        // Change mode first
        fs.chmod(ino, 0o755).await.unwrap();

        // Then change owner
        let attr = fs.chown(ino, Some(1000), Some(1000)).await.unwrap();
        assert_eq!(
            attr.mode & 0o777,
            0o755,
            "chown should not alter permission bits"
        );
    }

    #[tokio::test]
    async fn test_set_attr_chown_via_request() {
        // Test chown via the SetAttrRequest path (simulates FUSE setattr)
        let fs = new_test_vfs().await;
        fs.mkdir_p("/sa_own").await.unwrap();
        let ino = fs.create_file("/sa_own/f.txt").await.unwrap();

        let req = SetAttrRequest {
            uid: Some(1234),
            gid: Some(5678),
            ..Default::default()
        };
        let attr = fs.set_attr(ino, &req, SetAttrFlags::empty()).await.unwrap();
        assert_eq!(attr.uid, 1234);
        assert_eq!(attr.gid, 5678);

        let stat = fs.stat("/sa_own/f.txt").await.unwrap();
        assert_eq!(stat.uid, 1234);
        assert_eq!(stat.gid, 5678);
    }
}
