//! Minimal end-to-end VFS demo: LocalFsBlockStore write/read across blocks with verification.

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::ObjectBlockStore;
use crate::chuck::writer::ChunkWriter;
use std::error::Error;
use std::path::Path;

/// Demonstrate a cross-block write/read cycle rooted at the given directory and verify integrity.
pub async fn e2e_localfs_demo<P: AsRef<Path>>(root: P) -> Result<(), Box<dyn Error>> {
    // 1) Build the object client and BlockStore (local-directory mock)
    let client = ObjectClient::new(LocalFsBackend::new(root));
    let store = ObjectBlockStore::new(client);

    // 2) Choose a chunk_id and use the default layout
    let layout = ChunkLayout::default();
    let chunk_id: i64 = 1001;

    // 3) Prepare data that starts halfway through a block and spans 1.5 blocks
    let half = (layout.block_size / 2) as usize;
    let len = layout.block_size as usize + half; // 1.5 blocks
    let mut data = vec![0u8; len];
    for (i, b) in data.iter_mut().enumerate().take(len) {
        *b = (i % 251) as u8;
    }

    // 4) Write, touching two blocks
    {
        let writer = ChunkWriter::new(layout, chunk_id, &store);
        writer.write(half as u64, &data).await?;
    }

    // 5) Read and verify
    {
        let reader = ChunkReader::new(layout, chunk_id, &store);
        let out = reader.read(half as u64, len).await?;
        if out != data {
            return Err("data mismatch".into());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_e2e_localfs_demo() {
        let dir = tempfile::tempdir().unwrap();
        e2e_localfs_demo(dir.path())
            .await
            .expect("e2e demo should succeed");
    }
}
