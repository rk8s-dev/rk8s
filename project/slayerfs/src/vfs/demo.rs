//! Minimal end-to-end VFS demo: LocalFsBlockStore write/read across blocks with verification.

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::ObjectBlockStore;
use crate::chuck::writer::ChunkWriter;
use crate::meta::create_meta_store_from_url;
use std::convert::TryInto;
use std::error::Error;
use std::path::Path;

/// Demonstrate a cross-block write/read cycle rooted at the given directory and verify integrity.
pub async fn e2e_localfs_demo<P: AsRef<Path>>(root: P) -> Result<(), Box<dyn Error>> {
    // 1) Build the object client and BlockStore (local-directory mock)
    let client = ObjectClient::new(LocalFsBackend::new(root));
    let store = ObjectBlockStore::new(client);

    // 2) Choose a chunk_id and use the default layout
    let layout = ChunkLayout::default();
    let chunk_id: u64 = 1001;

    // 3) Build a lightweight metadata store (in-memory sqlite)
    let meta = create_meta_store_from_url("sqlite::memory:")
        .await
        .map_err(|e| format!("failed to init meta store: {e}"))?;

    // 4) Prepare data that starts halfway through a block and spans 1.5 blocks
    let half = (layout.block_size / 2) as usize;
    let len = layout.block_size as usize + half; // 1.5 blocks
    let mut data = vec![0u8; len];
    for (i, b) in data.iter_mut().enumerate().take(len) {
        *b = (i % 251) as u8;
    }

    // 5) Write, touching two blocks
    {
        let writer = ChunkWriter::new(layout, chunk_id, &store, &meta);
        writer
            .write(
                half.try_into().expect("chunk offset must fit in u32"),
                &data,
            )
            .await?;
    }

    // 6) Read and verify
    {
        let reader = ChunkReader::new(layout, chunk_id, &store, &meta);
        let out = reader
            .read(half.try_into().expect("chunk offset must fit in u32"), len)
            .await?;
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
