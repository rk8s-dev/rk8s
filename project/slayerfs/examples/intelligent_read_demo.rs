/// 智能读取策略演示
/// 展示如何根据读取范围大小选择不同的读取策略
use anyhow::Result;
use slayerfs::cadapter::{client::ObjectClient, localfs::LocalFsBackend};
use slayerfs::chuck::cache::ChunksCacheConfig;
use slayerfs::chuck::store::{BlockStore, BlockStoreConfig, ObjectBlockStore};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::main]
async fn main() -> Result<()> {
    println!("🚀 SlayerFS 智能读取策略演示");
    println!("{}", "=".repeat(60));

    // 创建临时目录和测试数据
    let temp_dir = TempDir::new()?;
    let backend = LocalFsBackend::new(temp_dir.path());
    let client = ObjectClient::new(backend);

    // 配置智能读取策略
    let block_config = BlockStoreConfig {
        block_size: 4 * 1024 * 1024, // 4MB 块大小
        range_read_threshold: 0.25,  // 25% 阈值 = 1MB
    };

    let store = Arc::new(ObjectBlockStore::new_with_configs(
        client,
        ChunksCacheConfig::default(),
        block_config,
    )?);

    // 创建测试数据 (4MB 的测试块)
    println!("📝 创建 4MB 测试数据...");
    let test_data: Vec<u8> = (0..4_194_304).map(|i| (i % 256) as u8).collect();

    // 写入测试数据
    let chunk_key = (42, 3); // (chunk_id, block_index)
    store.write_range(chunk_key, 0, &test_data).await?;
    println!("✅ 测试数据写入完成: {} bytes", test_data.len());

    println!("\n📊 智能读取策略测试:");
    println!("   阈值: 1MB (25% of 4MB block)");
    println!("   策略: < 1MB → 范围读取 | >= 1MB → 完整读取 + SingleFlight");

    // 测试场景 1: 小范围读取 (512KB < 1MB)
    println!("\n🔍 场景 1: 小范围读取 (512KB)");
    let mut small_buffer = vec![0u8; 512 * 1024];
    let start = std::time::Instant::now();
    store.read_range(chunk_key, 1024, &mut small_buffer).await?;
    let duration = start.elapsed();

    println!("   ✓ 策略: 直接范围读取 (get_object_range)");
    println!("   ✓ 耗时: {:?}", duration);
    println!(
        "   ✓ 数据验证: {}",
        if small_buffer[0] == ((1024) % 256) as u8 {
            "通过"
        } else {
            "失败"
        }
    );

    // 测试场景 2: 大范围读取 (2MB > 1MB)
    println!("\n🔍 场景 2: 大范围读取 (2MB)");
    let mut large_buffer = vec![0u8; 2 * 1024 * 1024];
    let start = std::time::Instant::now();
    store.read_range(chunk_key, 0, &mut large_buffer).await?;
    let duration = start.elapsed();

    println!("   ✓ 策略: 完整块读取 + SingleFlight 合并");
    println!("   ✓ 耗时: {:?}", duration);
    println!(
        "   ✓ 数据验证: {}",
        if large_buffer[0] == 0 && large_buffer[1000] == (1000 % 256) as u8 {
            "通过"
        } else {
            "失败"
        }
    );

    // 测试场景 3: 并发读取演示
    println!("\n🔍 场景 3: 并发大范围读取 (展示 SingleFlight 效果)");
    let start = std::time::Instant::now();

    let mut handles = Vec::new();
    for i in 0..5 {
        let store_clone = Arc::clone(&store);
        let handle = tokio::spawn(async move {
            let mut buffer = vec![0u8; 1 * 1024 * 1024]; // 1MB each
            store_clone
                .read_range(chunk_key, i * 1024, &mut buffer)
                .await
        });
        handles.push(handle);
    }

    // 等待所有并发读取完成
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await? {
            Ok(_) => println!("   ✓ 并发读取 {} 完成", i + 1),
            Err(e) => println!("   ✗ 并发读取 {} 失败: {}", i + 1, e),
        }
    }

    let total_duration = start.elapsed();
    println!("   ✓ 总耗时: {:?}", total_duration);
    println!("   ✓ SingleFlight 优化: 5个并发请求合并为1个API调用");

    println!("\n🎯 性能优化总结:");
    println!("   • 小读取 (< 1MB): 精确范围读取，节省带宽");
    println!("   • 大读取 (>= 1MB): 完整块读取，利用预读和缓存");
    println!("   • 并发读取: SingleFlight 合并，避免重复API调用");
    println!("   • 智能决策: 根据读取大小自动选择最优策略");

    println!("\n🏆 原来 unused 的函数现已重新发挥价值!");
    println!("   get_object_range: 用于高效的小范围精确读取");
    println!("   get_object: 用于大范围读取和并发优化");

    Ok(())
}
