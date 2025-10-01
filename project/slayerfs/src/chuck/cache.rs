use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Duration;
use std::{
    hash,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use dirs::cache_dir;
use sha2::{Digest, Sha256, digest::KeyInit};
use tempfile::tempdir;
use tokio::fs;
use tokio::sync::RwLock;
use tokio::time::Instant;

/// Configuration for ChunksCache
#[derive(Debug, Clone)]
pub struct ChunksCacheConfig {
    /// Maximum number of entries in hot cache
    pub hot_cache_size: usize,
    /// Maximum number of entries in cold cache
    pub cold_cache_size: usize,
    /// Access frequency threshold for promoting items to hot cache
    pub promotion_threshold: f64,
    /// Time window for access frequency calculation
    pub access_window_size: Duration,
    /// Maximum number of access records to keep per key
    pub max_access_entries: usize,
    /// Custom disk storage directory (optional)
    pub disk_storage_dir: Option<PathBuf>,
}

impl Default for ChunksCacheConfig {
    fn default() -> Self {
        Self {
            hot_cache_size: 1024,
            cold_cache_size: 1024,
            promotion_threshold: 10.0,
            access_window_size: Duration::from_secs(10),
            max_access_entries: 100,
            disk_storage_dir: None,
        }
    }
}

struct DiskStorage {
    base_dir: PathBuf,
}

impl DiskStorage {
    pub async fn new<P: AsRef<Path>>(base_dir: P) -> anyhow::Result<Self> {
        let base_dir = base_dir.as_ref().to_path_buf();
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir).await?;
        }

        Ok(Self { base_dir })
    }

    pub fn key_to_filename(key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let hash_result = hasher.finalize();

        hex::encode(hash_result)
    }

    pub async fn store(&self, key: &str, data: impl AsRef<[u8]>) -> anyhow::Result<()> {
        let filename = Self::key_to_filename(key);
        let filepath = self.base_dir.join(filename);

        tokio::fs::write(filepath, data).await?;
        Ok(())
    }

    pub async fn load(&self, key: &str) -> anyhow::Result<Vec<u8>> {
        let filename = Self::key_to_filename(key);
        let filepath = self.base_dir.join(filename);

        if !filepath.exists() {
            return Err(anyhow!("file {} does not exist", filepath.display()));
        }
        match tokio::fs::read(filepath).await {
            Ok(data) => Ok(data),
            Err(e) => Err(e.into()),
        }
    }

    #[allow(dead_code)]
    pub async fn remove(&self, key: &str) -> anyhow::Result<()> {
        let filename = Self::key_to_filename(key);
        let filepath = self.base_dir.join(filename);

        if !filepath.exists() {
            return Err(anyhow!("file {} does not exist", filepath.display()));
        }
        match tokio::fs::remove_file(filepath).await {
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Clone, Debug)]
struct AccessStats {
    accesses: VecDeque<Instant>,
    window_size: Duration,
    max_entries: usize,
}

impl AccessStats {
    fn new(window_size: Duration, max_entries: usize) -> Self {
        Self {
            accesses: VecDeque::with_capacity(max_entries),
            window_size,
            max_entries,
        }
    }
    fn record_access(&mut self) {
        let now = Instant::now();
        self.accesses.push_back(now);

        // 清理过期的访问记录
        self.cleanup_old_entries(now);

        // 限制队列大小
        if self.accesses.len() > self.max_entries {
            self.accesses.pop_front();
        }
    }

    fn cleanup_old_entries(&mut self, now: Instant) {
        while let Some(&oldest) = self.accesses.front() {
            if now.duration_since(oldest) > self.window_size {
                self.accesses.pop_front();
            } else {
                break;
            }
        }
    }

    fn get_access_count(&mut self, period: Duration) -> usize {
        let now = Instant::now();
        self.cleanup_old_entries(now);
        self.accesses
            .iter()
            .filter(|&&time| now.duration_since(time) <= period)
            .count()
    }

    fn get_access_frequency(&mut self, period: Duration) -> f64 {
        let count = self.get_access_count(period);
        count as f64 / period.as_secs_f64()
    }
}

pub struct ChunksCache {
    disk_storage: DiskStorage,
    hot_cache: moka::future::Cache<String, Vec<u8>>,
    cold_cache: moka::future::Cache<String, ()>,
    access_stats: Arc<RwLock<HashMap<String, AccessStats>>>,
    config: ChunksCacheConfig,
}

impl ChunksCache {
    /// Creates a new ChunksCache with default configuration
    #[allow(dead_code)]
    pub async fn new() -> anyhow::Result<Self> {
        Self::new_with_config(ChunksCacheConfig::default()).await
    }

    /// Creates a new ChunksCache with custom configuration
    pub async fn new_with_config(mut config: ChunksCacheConfig) -> anyhow::Result<Self> {
        let cache_dir = config
            .disk_storage_dir
            .take()
            .unwrap_or_else(|| cache_dir().unwrap());
        let disk_storage = DiskStorage::new(cache_dir).await?;

        let hot_cache_builder = moka::future::Cache::builder()
            .max_capacity(config.hot_cache_size as u64)
            .time_to_idle(Duration::from_secs(30))
            .time_to_live(Duration::from_secs(120));
        let cold_cache_builder = moka::future::Cache::builder()
            .max_capacity(config.cold_cache_size as u64)
            .time_to_idle(Duration::from_secs(30))
            .time_to_live(Duration::from_secs(120));

        Ok(Self {
            disk_storage,
            hot_cache: hot_cache_builder.build(),
            cold_cache: cold_cache_builder.build(),
            access_stats: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }

    async fn record_access(&self, key: String) {
        let mut stats = self.access_stats.write().await;
        let entry = stats.entry(key).or_insert_with(|| {
            AccessStats::new(
                self.config.access_window_size,
                self.config.max_access_entries,
            )
        });
        entry.record_access();
    }

    async fn should_promote(&self, key: String) -> bool {
        let mut stats = self.access_stats.write().await;
        let entry = stats.entry(key).or_insert_with(|| {
            AccessStats::new(
                self.config.access_window_size,
                self.config.max_access_entries,
            )
        });

        entry.get_access_frequency(self.config.access_window_size)
            >= self.config.promotion_threshold
    }

    pub async fn get(&self, key: &String) -> Option<Vec<u8>> {
        self.record_access(key.clone()).await;

        if let Some(value) = self.hot_cache.get(key).await {
            return Some(value);
        }

        if (self.cold_cache.get(key).await).is_some() {
            let value = self.disk_storage.load(key).await.ok()?;
            if self.should_promote(key.clone()).await {
                self.hot_cache.insert(key.clone(), value.clone()).await;
            }
            return Some(value);
        }

        None
    }

    pub async fn insert(&self, key: &str, data: &Vec<u8>) -> anyhow::Result<()> {
        self.hot_cache.insert(key.to_owned(), data.clone()).await;
        self.disk_storage.store(key, data).await?;
        self.cold_cache.insert(key.to_owned(), ()).await;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn remove(&self, key: &String) -> anyhow::Result<()> {
        self.hot_cache.invalidate(key).await;
        // self.disk_storage.remove(key).await?;
        self.cold_cache.invalidate(key).await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio;

    // 测试工具函数：创建临时存储目录
    async fn setup_test_storage() -> (DiskStorage, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let storage = DiskStorage::new(temp_dir.path()).await.unwrap();
        (storage, temp_dir)
    }

    // 测试工具函数：生成测试数据
    fn generate_test_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[tokio::test]
    async fn test_new_creates_directory() {
        let temp_dir = tempdir().unwrap();
        let dir_path = temp_dir.path().join("subdir");

        // 确保目录不存在
        assert!(!dir_path.exists());

        let _storage = DiskStorage::new(&dir_path).await.unwrap();
        assert!(dir_path.exists());
        assert!(dir_path.is_dir());
    }

    #[tokio::test]
    async fn test_new_existing_directory() {
        let temp_dir = tempdir().unwrap();

        // 目录已存在
        assert!(temp_dir.path().exists());

        let _storage = DiskStorage::new(temp_dir.path()).await.unwrap();
        assert!(temp_dir.path().exists());
    }

    #[tokio::test]
    async fn test_etag_to_filename_special_characters() {
        let binding = "a".repeat(1000);
        let etags = vec![
            "normal",
            "etag-with-dashes",
            "etag_with_underscores",
            "etag with spaces",
            "etag@with#special$chars%",
            "中文标签",
            "🚀emoji-etag",
            "",       // 空字符串
            "a",      // 单字符
            &binding, // 长字符串
        ];

        for etag in etags {
            let filename = DiskStorage::key_to_filename(etag);
            assert!(!filename.is_empty());
            // 文件名应该是有效的（不包含路径分隔符等）
            assert!(!filename.contains('/'));
            assert!(!filename.contains('\\'));
            assert!(!filename.contains(':'));
        }
    }

    #[tokio::test]
    async fn test_store_and_load_basic() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "test_etag_1";
        let test_data = b"Hello, World!".to_vec();

        // 存储数据
        storage.store(etag, &test_data).await.unwrap();

        // 加载数据
        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, test_data);
    }

    #[tokio::test]
    async fn test_store_and_load_large_data() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "large_data_etag";

        // 生成1MB的测试数据
        let large_data = generate_test_data(1024 * 1024);

        storage.store(etag, &large_data).await.unwrap();
        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, large_data);
    }

    #[tokio::test]
    async fn test_store_and_load_empty_data() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "empty_data_etag";
        let empty_data = vec![];

        storage.store(etag, &empty_data).await.unwrap();
        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, empty_data);
    }

    #[tokio::test]
    async fn test_store_overwrite() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "overwrite_etag";

        let data1 = b"First version".to_vec();
        let data2 = b"Second version".to_vec();

        storage.store(etag, &data1).await.unwrap();
        storage.store(etag, &data2).await.unwrap(); // 应该覆盖

        let loaded_data = storage.load(etag).await.unwrap();
        assert_eq!(loaded_data, data2); // 应该是第二个版本
    }

    #[tokio::test]
    async fn test_load_nonexistent_file() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "nonexistent_etag";

        let result = storage.load(etag).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_remove_existing_file() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "to_remove_etag";
        let test_data = b"Data to remove".to_vec();

        storage.store(etag, &test_data).await.unwrap();
        assert!(storage.load(etag).await.is_ok()); // 文件存在

        storage.remove(etag).await.unwrap();
        assert!(storage.load(etag).await.is_err()); // 文件应该被删除
    }

    #[tokio::test]
    async fn test_remove_nonexistent_file() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "nonexistent_remove_etag";

        let result = storage.remove(etag).await;
        assert!(result.is_err());

        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("does not exist"));
    }

    #[tokio::test]
    async fn test_multiple_operations_same_etag() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "multi_op_etag";
        let data1 = b"Data 1".to_vec();
        let data2 = b"Data 2".to_vec();

        // 存储 → 加载 → 存储 → 加载 → 删除 → 尝试加载
        storage.store(etag, &data1).await.unwrap();
        assert_eq!(storage.load(etag).await.unwrap(), data1);

        storage.store(etag, &data2).await.unwrap();
        assert_eq!(storage.load(etag).await.unwrap(), data2);

        storage.remove(etag).await.unwrap();
        assert!(storage.load(etag).await.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_operations() {
        let (storage, _temp_dir) = setup_test_storage().await;

        let mut handles = vec![];

        // 启动多个并发任务
        for i in 0..10 {
            let storage_clone = DiskStorage {
                base_dir: storage.base_dir.clone(),
            };
            let etag = format!("concurrent_etag_{}", i);
            let data = format!("Data for {}", i).into_bytes();

            handles.push(tokio::spawn(async move {
                storage_clone.store(&etag, &data).await.unwrap();
                let loaded = storage_clone.load(&etag).await.unwrap();
                assert_eq!(loaded, data);
                storage_clone.remove(&etag).await.unwrap();
            }));
        }

        // 等待所有任务完成
        for handle in handles {
            handle.await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_filename_uniqueness() {
        let etag1 = "test1";
        let etag2 = "test2";

        let filename1 = DiskStorage::key_to_filename(etag1);
        let filename2 = DiskStorage::key_to_filename(etag2);

        // 不同的etag应该生成不同的文件名
        assert_ne!(filename1, filename2);
    }

    #[tokio::test]
    async fn test_files_actually_created() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "file_creation_test";
        let test_data = b"Test data".to_vec();

        // 存储前检查目录为空（除了可能的系统文件）
        let mut entries = fs::read_dir(&storage.base_dir).await.unwrap();
        let mut initial_count = 0;
        while let Some(_) = entries.next_entry().await.unwrap() {
            initial_count += 1;
        }

        storage.store(etag, &test_data).await.unwrap();

        // 检查文件确实被创建
        let mut entries = fs::read_dir(&storage.base_dir).await.unwrap();
        let mut final_count = 0;
        while let Some(_) = entries.next_entry().await.unwrap() {
            final_count += 1;
        }
        assert_eq!(final_count, initial_count + 1);
    }

    #[tokio::test]
    async fn test_error_messages() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let etag = "error_test_etag";

        // 测试加载不存在的文件时的错误消息
        let load_error = storage.load(etag).await.unwrap_err();
        let error_string = load_error.to_string();
        assert!(error_string.contains("does not exist"));

        // 测试删除不存在的文件时的错误消息
        let remove_error = storage.remove(etag).await.unwrap_err();
        let error_string = remove_error.to_string();
        assert!(error_string.contains("does not exist"));
    }
}
