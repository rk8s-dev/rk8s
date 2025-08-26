//! VFS 最小端到端示例：使用 LocalFsBlockStore 完成跨块写入与读取校验。

use crate::cadapter::client::ObjectClient;
use crate::cadapter::localfs::LocalFsBackend;
use crate::chuck::chunk::ChunkLayout;
use crate::chuck::reader::ChunkReader;
use crate::chuck::store::ObjectBlockStore;
use crate::chuck::writer::ChunkWriter;
use std::error::Error;
use std::path::Path;

/// 在指定的本地目录下，演示一次写入与读取（跨两个 block），并进行数据完整性校验。
pub async fn e2e_localfs_demo<P: AsRef<Path>>(root: P) -> Result<(), Box<dyn Error>> {
    // 1) 构建对象客户端与 BlockStore（本地目录 mock）
    let client = ObjectClient::new(LocalFsBackend::new(root));
    let mut store = ObjectBlockStore::new(client);

    // 2) 选择一个 chunk_id，使用默认布局
    let layout = ChunkLayout::default();
    let chunk_id: i64 = 1001;

    // 3) 构造写入数据：从半个 block 起写入 1.5 个 block 的数据
    let half = (layout.block_size / 2) as usize;
    let len = layout.block_size as usize + half; // 1.5 blocks
    let mut data = vec![0u8; len];
    for i in 0..len {
        data[i] = (i % 251) as u8;
    }

    // 4) 写入（落到两个 block 上）
    {
        let mut writer = ChunkWriter::new(layout, chunk_id, &mut store);
    let _slice = writer.write(half as u64, &data).await;
    }

    // 5) 读取并校验
    {
        let reader = ChunkReader::new(layout, chunk_id, &store);
    let out = reader.read(half as u64, len).await;
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
        e2e_localfs_demo(dir.path()).await.expect("e2e demo should succeed");
    }
}
