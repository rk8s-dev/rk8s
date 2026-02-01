// Example program: demonstrate vfs::sdk::VfsClient usage without FUSE

use slayerfs::{ChunkLayout, LocalClient};
use std::path::PathBuf;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Resolve object root directory from argv or default to a temp location
    let mut args = std::env::args().skip(1);
    let root: PathBuf = if let Some(p) = args.next() {
        PathBuf::from(p)
    } else {
        std::env::temp_dir().join("slayerfs-sdk-demo")
    };

    let layout = ChunkLayout::default();
    let cli = LocalClient::new_local(&root, layout)
        .await
        .expect("init LocalClient");

    // Prepare paths
    let dir = "/demo/ns";
    let dir2 = "/demo/renamed"; // will be auto-created by rename
    let file_a = "/demo/ns/a.txt";
    let file_b = "/demo/ns/b.txt";
    let file_a2 = "/demo/renamed/a_renamed.txt";
    let file_a_hard = "/demo/renamed/a_hardlink.txt";
    let file_a_symlink = "/demo/renamed/a_symlink";

    // Ensure directory and files
    cli.mkdir_p(dir).await.expect("mkdir_p");
    cli.create_file(file_a, false).await.expect("create a");
    cli.create_file(file_b, false).await.expect("create b");

    // A) Write cross-block content to a.txt starting at half block
    let half = (layout.block_size / 2) as usize;
    let len = layout.block_size as usize + half; // 1.5 blocks
    let mut data = vec![0u8; len];
    for (i, b) in data.iter_mut().enumerate().take(len) {
        *b = (i % 251) as u8;
    }
    cli.write_at(file_a, half as u64, &data)
        .await
        .expect("write A half+block");

    // B) Overwrite prefix of a.txt at offset 0
    let prefix: Vec<u8> = (0..128u8).collect();
    cli.write_at(file_a, 0, &prefix)
        .await
        .expect("overwrite prefix");

    // C) Cross-chunk write: near chunk boundary
    let near_end = layout.chunk_size - 10;
    let cross = vec![7u8; 100]; // crosses into next chunk
    cli.write_at(file_a, near_end, &cross)
        .await
        .expect("cross-chunk write");

    // D) Sparse write in b.txt leaving holes
    let off_b = (layout.block_size as u64) * 2 + 123; // after 2 blocks + 123 bytes
    let data_b = vec![9u8; 1024];
    cli.write_at(file_b, off_b, &data_b)
        .await
        .expect("sparse write B");

    // Reads and validations
    let out_a = cli
        .read_at(file_a, half as u64, len)
        .await
        .expect("read A segment");
    assert_eq!(out_a, data, "A segment readback");
    let out_prefix = cli
        .read_at(file_a, 0, prefix.len())
        .await
        .expect("read prefix");
    assert_eq!(out_prefix, prefix, "prefix overwritten should match");
    // Read across chunk boundary: should match what we wrote
    let out_cross = cli
        .read_at(file_a, near_end, cross.len())
        .await
        .expect("read cross-chunk");
    assert_eq!(out_cross, cross);

    // Zero-fill checks on B: read a window that partially overlaps the hole and data
    let read_start = off_b - 200; // before the written region
    let read_len = 200 + 1024 + 200; // include before and after
    let out_b = cli
        .read_at(file_b, read_start, read_len as usize)
        .await
        .expect("read around hole");
    // Expect first 200 bytes to be zero, next 1024 to be 9, last 200 to be zero
    assert!(out_b[..200].iter().all(|&b| b == 0));
    assert!(out_b[200..(200 + 1024)].iter().all(|&b| b == 9));
    assert!(out_b[(200 + 1024)..].iter().all(|&b| b == 0));

    // Read zero length returns empty
    let empty = cli.read_at(file_a, 0, 0).await.expect("read zero");
    assert!(empty.is_empty());

    // Directory listing before rename
    let ents = cli.readdir(dir).await.expect("readdir before rename");
    println!(
        "{} => {}",
        dir,
        ents.iter()
            .map(|e| e.name.clone())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // Cross-directory rename (auto-create parent), then stat
    cli.rename(file_a, file_a2)
        .await
        .expect("rename across dirs");
    let st_a2 = cli.stat(file_a2).await.expect("stat a_renamed");
    println!(
        "root={:?} file={} size={} kind={:?}",
        root, file_a2, st_a2.size, st_a2.kind
    );

    let hard_attr = cli
        .link(file_a2, file_a_hard)
        .await
        .expect("create hard link");
    println!(
        "hard link inode={} nlink={} -> {}",
        hard_attr.ino, hard_attr.nlink, file_a_hard
    );

    cli.symlink(file_a_symlink, file_a2)
        .await
        .expect("create symlink");
    let symlink_target = cli.readlink(file_a_symlink).await.expect("read symlink");
    println!("symlink {} -> {}", file_a_symlink, symlink_target);

    // Truncate shrink, verify size, and do an in-bound write/read near tail
    cli.truncate(file_a2, layout.block_size as u64)
        .await
        .expect("truncate shrink");
    let st_shrink = cli.stat(file_a2).await.expect("stat after shrink");
    assert_eq!(st_shrink.size, layout.block_size as u64);
    let tail_off = st_shrink.size - 256;
    let tail = vec![5u8; 256];
    cli.write_at(file_a2, tail_off, &tail)
        .await
        .expect("write tail after shrink");
    let tail_back = cli
        .read_at(file_a2, tail_off, 256)
        .await
        .expect("read tail after shrink");
    assert_eq!(tail_back, tail);

    // Truncate extend b.txt, then read in the extended area => zeros
    let new_size_b = (layout.block_size as u64) * 5;
    cli.truncate(file_b, new_size_b)
        .await
        .expect("truncate extend");
    let extended = cli
        .read_at(file_b, new_size_b - 128, 256)
        .await
        .expect("read extended hole");
    assert!(extended.iter().all(|&b| b == 0));

    // Try error scenarios
    // 1) rmdir non-empty directory
    if let Err(e) = cli.rmdir(dir).await {
        println!("expected rmdir non-empty error: {e}");
    }
    // 2) unlink directory path (should error)
    if let Err(e) = cli.unlink(dir).await {
        println!("expected unlink dir error: {e}");
    }

    // Cleanup: unlink files and remove directories
    cli.unlink(file_b).await.expect("unlink b");
    // After moving a.txt -> a_renamed.txt, remove in new location
    cli.unlink(file_a_symlink).await.expect("unlink symlink");
    cli.unlink(file_a_hard).await.expect("unlink hard link");
    cli.unlink(file_a2).await.expect("unlink a2");
    // Remove both directories; dir2 becomes empty after unlink
    cli.rmdir(dir2).await.expect("rmdir dir2");
    cli.rmdir(dir).await.expect("rmdir dir");

    println!("sdk demo: OK");
}
