//! Metadata store factory
//!
//! Creates appropriate MetaStore implementation based on configuration

use std::path::Path;
use std::sync::Arc;

use crate::meta::client::{MetaClient, MetaClientOptions};
use crate::meta::config::{
    CacheConfig, CacheTtl, ClientOptions, Config, DatabaseConfig, DatabaseType,
};
use crate::meta::layer::MetaLayer;
use crate::meta::store::{MetaError, MetaStore};
use crate::meta::stores::{DatabaseMetaStore, EtcdMetaStore, RedisMetaStore};

/// Combined handles for raw stores and cached meta layers.
#[allow(dead_code)]
pub struct MetaHandle<M: MetaStore> {
    store: Arc<M>,
    #[allow(dead_code)]
    layer: Arc<MetaClient<M>>,
}

impl<M: MetaStore> MetaHandle<M> {
    // Revert `get` to consume `self` and return owned `Arc` references
    #[allow(dead_code)]
    fn get(self) -> (Arc<M>, Arc<MetaClient<M>>) {
        (self.store, self.layer)
    }

    pub fn store(&self) -> Arc<M> {
        Arc::clone(&self.store)
    }

    #[allow(dead_code)]
    pub fn layer(&self) -> Arc<MetaClient<M>> {
        Arc::clone(&self.layer)
    }
}

/// Factory for creating metadata handles (raw store + cached layer)
pub struct MetaStoreFactory<M: MetaStore> {
    _marker: std::marker::PhantomData<M>,
}

impl MetaStoreFactory<DatabaseMetaStore> {
    /// Create MetaStore from path (with MetaClient caching)
    #[allow(dead_code)]
    pub async fn create_from_path(
        backend_path: &Path,
    ) -> Result<MetaHandle<DatabaseMetaStore>, MetaError> {
        let config = Self::load_config(backend_path)?;
        Self::create_from_config(config).await
    }

    /// Create MetaStore from config (with MetaClient caching)
    pub async fn create_from_config(
        config: Config,
    ) -> Result<MetaHandle<DatabaseMetaStore>, MetaError> {
        let store = Self::create_store(&config).await?;
        let layer = Self::create_layer(&store, &config).await?;
        Ok(MetaHandle { store, layer })
    }

    /// Load configuration from a path
    fn load_config(backend_path: &Path) -> Result<Config, MetaError> {
        Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))
    }

    /// Create the raw MetaStore
    async fn create_store(config: &Config) -> Result<Arc<DatabaseMetaStore>, MetaError> {
        DatabaseMetaStore::from_config(config.clone())
            .await
            .map(Arc::new)
    }

    /// Create the MetaClient layer
    async fn create_layer(
        store: &Arc<DatabaseMetaStore>,
        config: &Config,
    ) -> Result<Arc<MetaClient<DatabaseMetaStore>>, MetaError> {
        let ttl = if config.cache.ttl.is_zero() {
            CacheTtl::for_backend(config.database.db_config.backend_type())
        } else {
            config.cache.ttl.clone()
        };

        let client_options = MetaClientOptions {
            read_only: config.client.read_only,
            no_background_jobs: config.client.no_background_jobs,
            case_insensitive: config.client.case_insensitive,
            session_heartbeat: config
                .client
                .session_heartbeat
                .unwrap_or_else(|| MetaClientOptions::default().session_heartbeat),
            ..MetaClientOptions::default()
        };

        let client = MetaClient::with_options(
            Arc::clone(store),
            config.cache.capacity.clone(),
            ttl,
            client_options,
        );

        client.initialize().await?;
        Ok(client)
    }
}

impl MetaStoreFactory<RedisMetaStore> {
    /// Create a Redis MetaStore from path (with MetaClient caching)
    #[allow(dead_code)]
    pub async fn create_from_path(
        backend_path: &Path,
    ) -> Result<MetaHandle<RedisMetaStore>, MetaError> {
        let config =
            Config::from_path(backend_path).map_err(|e| MetaError::Config(e.to_string()))?;
        Self::create_from_config(config).await
    }

    /// Create a Redis MetaStore from config (with MetaClient caching)
    pub async fn create_from_config(
        config: Config,
    ) -> Result<MetaHandle<RedisMetaStore>, MetaError> {
        let store = RedisMetaStore::from_config(config.clone())
            .await
            .map(Arc::new)?;
        let layer = Self::create_layer(&store, &config).await?;
        Ok(MetaHandle { store, layer })
    }

    async fn create_layer(
        store: &Arc<RedisMetaStore>,
        config: &Config,
    ) -> Result<Arc<MetaClient<RedisMetaStore>>, MetaError> {
        let ttl = if config.cache.ttl.is_zero() {
            CacheTtl::for_backend(config.database.db_config.backend_type())
        } else {
            config.cache.ttl.clone()
        };

        let client_options = MetaClientOptions {
            read_only: config.client.read_only,
            no_background_jobs: config.client.no_background_jobs,
            case_insensitive: config.client.case_insensitive,
            session_heartbeat: config
                .client
                .session_heartbeat
                .unwrap_or_else(|| MetaClientOptions::default().session_heartbeat),
            ..MetaClientOptions::default()
        };

        let client = MetaClient::with_options(
            Arc::clone(store),
            config.cache.capacity.clone(),
            ttl,
            client_options,
        );

        client.initialize().await?;
        Ok(client)
    }
}

impl MetaStoreFactory<EtcdMetaStore> {
    #[allow(dead_code)]
    pub async fn create_from_config(
        config: Config,
    ) -> Result<MetaHandle<EtcdMetaStore>, MetaError> {
        let store = Self::create_store(&config).await?;
        let layer = Self::create_layer(&store, &config).await?;
        Ok(MetaHandle { store, layer })
    }

    /// Create the raw MetaStore
    #[allow(dead_code)]
    async fn create_store(config: &Config) -> Result<Arc<EtcdMetaStore>, MetaError> {
        EtcdMetaStore::from_config(config.clone())
            .await
            .map(Arc::new)
    }

    /// Create the MetaClient layer
    #[allow(dead_code)]
    async fn create_layer(
        store: &Arc<EtcdMetaStore>,
        config: &Config,
    ) -> Result<Arc<MetaClient<EtcdMetaStore>>, MetaError> {
        let ttl = if config.cache.ttl.is_zero() {
            CacheTtl::for_backend(config.database.db_config.backend_type())
        } else {
            config.cache.ttl.clone()
        };

        let client_options = MetaClientOptions {
            read_only: config.client.read_only,
            no_background_jobs: config.client.no_background_jobs,
            case_insensitive: config.client.case_insensitive,
            session_heartbeat: config
                .client
                .session_heartbeat
                .unwrap_or_else(|| MetaClientOptions::default().session_heartbeat),
            ..MetaClientOptions::default()
        };

        let client = MetaClient::with_options(
            Arc::clone(store),
            config.cache.capacity.clone(),
            ttl,
            client_options,
        );

        client.initialize().await?;
        Ok(client)
    }
}
/// Convenience function to create MetaStore from path
#[allow(dead_code)]
pub async fn create_meta_store(
    backend_path: &Path,
) -> Result<MetaHandle<DatabaseMetaStore>, MetaError> {
    MetaStoreFactory::<DatabaseMetaStore>::create_from_path(backend_path).await
}

/// Convenience function to create MetaStore from a URL string.
#[allow(dead_code)]
pub async fn create_meta_store_from_url(
    url: &str,
) -> Result<MetaHandle<DatabaseMetaStore>, MetaError> {
    let config = Config {
        database: DatabaseConfig {
            db_config: DatabaseType::Sqlite {
                url: url.to_string(),
            },
        },
        cache: CacheConfig::default(),
        client: ClientOptions::default(),
    };
    MetaStoreFactory::<DatabaseMetaStore>::create_from_config(config).await
}

/// Convenience function to create a Redis MetaStore from a URL string.
#[allow(dead_code)]
pub async fn create_redis_meta_store_from_url(
    url: &str,
) -> Result<MetaHandle<RedisMetaStore>, MetaError> {
    let config = Config {
        database: DatabaseConfig {
            db_config: DatabaseType::Redis {
                url: url.to_string(),
            },
        },
        cache: CacheConfig::default(),
        client: ClientOptions::default(),
    };
    MetaStoreFactory::<RedisMetaStore>::create_from_config(config).await
}
