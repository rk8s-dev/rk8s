use crate::meta::store::{DirEntry, FileAttr, MetaError, MetaStore};
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

/// Cache TTL configuration with differentiated expiration times for different cache types
#[derive(Clone, Debug)]
pub struct CacheTtlConfig {
    /// File attributes cache TTL (for stat operations)
    pub attr_ttl: Duration,

    /// Directory entry cache TTL (for lookup operations)
    pub dentry_ttl: Duration,

    /// Forward path resolution cache TTL (for resolve_path)
    pub path_ttl: Duration,

    /// Reverse path lookup cache TTL (for get_path)
    pub inode_to_path_ttl: Duration,

    /// Directory content cache TTL (for readdir operations)
    pub readdir_ttl: Duration,
}

/// Caching metadata client using Moka cache library with automatic TTL management
pub struct MetaClient {
    store: Arc<dyn MetaStore + Send + Sync>,

    /// inode -> file attributes (with auto TTL management)
    attr_cache: Cache<i64, FileAttr>,

    /// (parent_inode, name) -> child_inode (directory entry cache)
    dentry_cache: Cache<(i64, String), i64>,

    /// absolute_path -> inode (forward path resolution cache)
    path_cache: Cache<String, i64>,

    /// absolute_path (reverse path lookup cache)
    inode_to_path: Cache<i64, String>,

    /// inode -> Vec<DirEntry> (directory content cache)
    readdir_cache: Cache<i64, Vec<DirEntry>>,
}

impl MetaClient {
    /// Create MetaClient with custom capacity and TTL configuration
    pub fn with_capacity_and_config(
        store: Arc<dyn MetaStore + Send + Sync>,
        attr_capacity: usize,
        dentry_capacity: usize,
        path_capacity: usize,
        inode_to_path_capacity: usize,
        readdir_capacity: usize,
        config: CacheTtlConfig,
    ) -> Self {
        Self {
            store,
            attr_cache: Cache::builder()
                .max_capacity(attr_capacity as u64)
                .time_to_live(config.attr_ttl)
                .build(),

            dentry_cache: Cache::builder()
                .max_capacity(dentry_capacity as u64)
                .time_to_live(config.dentry_ttl)
                .build(),

            path_cache: Cache::builder()
                .max_capacity(path_capacity as u64)
                .time_to_live(config.path_ttl)
                .build(),

            inode_to_path: Cache::builder()
                .max_capacity(inode_to_path_capacity as u64)
                .time_to_live(config.inode_to_path_ttl)
                .build(),

            readdir_cache: Cache::builder()
                .max_capacity(readdir_capacity as u64)
                .time_to_live(config.readdir_ttl)
                .build(),
        }
    }

    /// Resolve absolute path to inode, using cache when possible
    pub async fn resolve_path(&self, path: &str) -> Result<i64, MetaError> {
        info!("MetaClient: Resolving path: {}", path);

        if path == "/" {
            return Ok(self.store.root_ino());
        }

        if let Some(ino) = self.path_cache.get(path).await {
            info!("MetaClient: Path cache HIT for '{}' -> inode {}", path, ino);
            return Ok(ino);
        }

        info!("MetaClient: Path cache MISS for '{}'", path);

        let mut current_ino = self.store.root_ino();
        let segments: Vec<&str> = path
            .trim_start_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();

        for seg in segments {
            let cache_key = (current_ino, seg.to_string());

            if let Some(child_ino) = self.dentry_cache.get(&cache_key).await {
                info!(
                    "MetaClient: Dentry cache HIT for ({}, '{}') -> inode {}",
                    current_ino, seg, child_ino
                );
                current_ino = child_ino;
                continue;
            }

            info!(
                "MetaClient: Dentry cache MISS for ({}, '{}')",
                current_ino, seg
            );

            let child_ino = self
                .store
                .lookup(current_ino, seg)
                .await?
                .ok_or_else(|| MetaError::NotFound(current_ino))?;

            info!(
                "MetaClient: Loaded from store: ({}, '{}') -> inode {}",
                current_ino, seg, child_ino
            );
            self.dentry_cache.insert(cache_key, child_ino).await;

            current_ino = child_ino;
        }

        info!(
            "MetaClient: Caching path '{}' -> inode {}",
            path, current_ino
        );
        self.path_cache.insert(path.to_string(), current_ino).await;
        self.inode_to_path
            .insert(current_ino, path.to_string())
            .await;

        Ok(current_ino)
    }

    /// Get file attributes with caching
    async fn cached_stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        info!("MetaClient: stat request for inode {}", ino);

        if let Some(attr) = self.attr_cache.get(&ino).await {
            info!("MetaClient: Attr cache HIT for inode {}", ino);
            return Ok(Some(attr));
        }

        info!("MetaClient: Attr cache MISS for inode {}", ino);

        let attr = self.store.stat(ino).await?;

        if let Some(ref a) = attr {
            info!("MetaClient: Caching attr for inode {}", ino);
            self.attr_cache.insert(ino, a.clone()).await;
        }

        Ok(attr)
    }

    /// Lookup child inode by name with caching
    async fn cached_lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        info!("MetaClient: lookup request for ({}, '{}')", parent, name);

        let cache_key = (parent, name.to_string());

        if let Some(ino) = self.dentry_cache.get(&cache_key).await {
            info!(
                "MetaClient: Dentry cache HIT for ({}, '{}') -> inode {}",
                parent, name, ino
            );
            return Ok(Some(ino));
        }

        info!("MetaClient: Dentry cache MISS for ({}, '{}')", parent, name);

        let result = self.store.lookup(parent, name).await?;

        if let Some(ino) = result {
            info!(
                "MetaClient: Caching dentry ({}, '{}') -> inode {}",
                parent, name, ino
            );
            self.dentry_cache.insert(cache_key, ino).await;
        }

        Ok(result)
    }

    /// Invalidate cached directory entry (parent, name) -> child_inode mapping
    pub async fn invalidate_dentry(&self, parent_ino: i64, name: &str) {
        info!(
            "MetaClient: Invalidating dentry cache for ({}, '{}')",
            parent_ino, name
        );
        self.dentry_cache
            .invalidate(&(parent_ino, name.to_string()))
            .await;
    }

    /// Invalidate cached file attributes for given inode
    pub async fn invalidate_attr(&self, ino: i64) {
        info!("MetaClient: Invalidating attr cache for inode {}", ino);
        self.attr_cache.invalidate(&ino).await;
    }
}

use crate::vfs::fs::FileType;
use async_trait::async_trait;

#[async_trait]
impl MetaStore for MetaClient {
    async fn stat(&self, ino: i64) -> Result<Option<FileAttr>, MetaError> {
        self.cached_stat(ino).await
    }

    async fn lookup(&self, parent: i64, name: &str) -> Result<Option<i64>, MetaError> {
        self.cached_lookup(parent, name).await
    }

    async fn lookup_path(&self, path: &str) -> Result<Option<(i64, FileType)>, MetaError> {
        let ino = match self.resolve_path(path).await {
            Ok(ino) => ino,
            Err(MetaError::NotFound(_)) => return Ok(None),
            Err(e) => return Err(e),
        };

        let attr = self
            .cached_stat(ino)
            .await?
            .ok_or(MetaError::NotFound(ino))?;

        Ok(Some((ino, attr.kind)))
    }

    async fn readdir(&self, ino: i64) -> Result<Vec<DirEntry>, MetaError> {
        info!("MetaClient: readdir request for inode {}", ino);

        // Check cache first
        if let Some(entries) = self.readdir_cache.get(&ino).await {
            info!(
                "MetaClient: Readdir cache HIT for inode {} ({} entries)",
                ino,
                entries.len()
            );
            return Ok(entries);
        }

        info!("MetaClient: Readdir cache MISS for inode {}", ino);

        // Load from store
        let entries = self.store.readdir(ino).await?;

        info!(
            "MetaClient: Caching readdir result for inode {} ({} entries)",
            ino,
            entries.len()
        );
        self.readdir_cache.insert(ino, entries.clone()).await;

        Ok(entries)
    }

    async fn mkdir(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        info!("MetaClient: mkdir operation for ({}, '{}')", parent, name);
        let ino = self.store.mkdir(parent, name.clone()).await?;
        info!(
            "MetaClient: mkdir created inode {}, invalidating caches",
            ino
        );
        self.invalidate_dentry(parent, &name).await;
        self.readdir_cache.invalidate(&parent).await;
        self.path_cache.invalidate_all();
        Ok(ino)
    }

    async fn rmdir(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        info!("MetaClient: rmdir operation for ({}, '{}')", parent, name);
        self.store.rmdir(parent, name).await?;
        info!("MetaClient: rmdir completed, invalidating caches");
        self.invalidate_dentry(parent, name).await;
        self.readdir_cache.invalidate(&parent).await;
        self.path_cache.invalidate_all();
        self.inode_to_path.invalidate_all();
        Ok(())
    }

    async fn create_file(&self, parent: i64, name: String) -> Result<i64, MetaError> {
        info!(
            "MetaClient: create_file operation for ({}, '{}')",
            parent, name
        );
        let ino = self.store.create_file(parent, name.clone()).await?;
        info!(
            "MetaClient: create_file created inode {}, invalidating caches",
            ino
        );
        self.invalidate_dentry(parent, &name).await;
        self.readdir_cache.invalidate(&parent).await;
        self.path_cache.invalidate_all();
        Ok(ino)
    }

    async fn unlink(&self, parent: i64, name: &str) -> Result<(), MetaError> {
        info!("MetaClient: unlink operation for ({}, '{}')", parent, name);
        self.store.unlink(parent, name).await?;
        info!("MetaClient: unlink completed, invalidating caches");
        self.invalidate_dentry(parent, name).await;
        self.readdir_cache.invalidate(&parent).await;
        self.path_cache.invalidate_all();
        self.inode_to_path.invalidate_all();
        Ok(())
    }

    async fn rename(
        &self,
        old_parent: i64,
        old_name: &str,
        new_parent: i64,
        new_name: String,
    ) -> Result<(), MetaError> {
        info!(
            "MetaClient: rename operation from ({}, '{}') to ({}, '{}')",
            old_parent, old_name, new_parent, new_name
        );
        let ino = self.store.lookup(old_parent, old_name).await?;
        let is_dir = if let Some(ino) = ino {
            if let Ok(Some(attr)) = self.cached_stat(ino).await {
                attr.kind == FileType::Dir
            } else {
                false
            }
        } else {
            false
        };

        self.store
            .rename(old_parent, old_name, new_parent, new_name.clone())
            .await?;

        info!(
            "MetaClient: rename completed (is_dir={}), invalidating caches",
            is_dir
        );
        self.invalidate_dentry(old_parent, old_name).await;
        self.invalidate_dentry(new_parent, &new_name).await;

        // Invalidate readdir cache for both old and new parent
        self.readdir_cache.invalidate(&old_parent).await;
        if old_parent != new_parent {
            self.readdir_cache.invalidate(&new_parent).await;
        }

        if let Some(ino) = ino {
            self.inode_to_path.invalidate(&ino).await;

            if is_dir {
                self.path_cache.invalidate_all();
                self.inode_to_path.invalidate_all();
            } else {
                self.path_cache.invalidate_all();
            }
        }

        Ok(())
    }

    async fn set_file_size(&self, ino: i64, size: u64) -> Result<(), MetaError> {
        info!(
            "MetaClient: set_file_size operation for inode {} to {} bytes",
            ino, size
        );
        self.store.set_file_size(ino, size).await?;
        info!("MetaClient: set_file_size completed, invalidating attr cache");
        self.invalidate_attr(ino).await;
        Ok(())
    }

    async fn get_parent(&self, ino: i64) -> Result<Option<i64>, MetaError> {
        self.store.get_parent(ino).await
    }

    async fn get_name(&self, ino: i64) -> Result<Option<String>, MetaError> {
        self.store.get_name(ino).await
    }

    async fn get_path(&self, ino: i64) -> Result<Option<String>, MetaError> {
        info!("MetaClient: get_path request for inode {}", ino);

        // Check cache first
        if let Some(path) = self.inode_to_path.get(&ino).await {
            info!(
                "MetaClient: Inode-to-path cache HIT for inode {} -> '{}'",
                ino, path
            );
            return Ok(Some(path));
        }

        info!("MetaClient: Inode-to-path cache MISS for inode {}", ino);

        // Call underlying store to get path
        let path = self.store.get_path(ino).await?;

        // Cache the result if found
        if let Some(ref p) = path {
            info!("MetaClient: Caching inode {} -> '{}'", ino, p);
            self.inode_to_path.insert(ino, p.clone()).await;
        }

        Ok(path)
    }

    fn root_ino(&self) -> i64 {
        self.store.root_ino()
    }

    async fn initialize(&self) -> Result<(), MetaError> {
        self.store.initialize().await
    }
}
