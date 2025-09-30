use std::{any::Any, collections::HashMap};

use serde::Deserialize;
use serde_json::Value;

use crate::{
    errors::RvError,
    storage::{Backend, BackendEntry},
    utils::{db::strip_db_name, DatabaseName},
};

pub struct SqlxBackend {
    pool: sqlx::AnyPool,
    table_name: String,
    db_scheme: String,
    lock: SqlxBackendLock,
}

#[derive(Debug, Clone, Default, sqlx::FromRow, Deserialize)]
pub struct SqlxBackendEntry {
    pub vault_key: String,
    pub vault_value: Vec<u8>,
}

pub struct SqlxBackendLock {
    pool: sqlx::AnyPool,
    db_scheme: String,
    timeout_secs: i32,
}

impl SqlxBackendLock {
    pub fn new(pool: &sqlx::AnyPool, db_scheme: &str, timeout_secs: i32) -> Self {
        Self { pool: pool.clone(), db_scheme: db_scheme.to_string(), timeout_secs }
    }

    async fn lock(&self, lock_name: &str) -> Result<bool, RvError> {
        let result: bool = match self.db_scheme.as_str() {
            "mysql" => {
                let count: Option<i32> = sqlx::query_scalar("SELECT GET_LOCK(SHA1(?), ?) as result")
                    .bind(lock_name)
                    .bind(self.timeout_secs)
                    .fetch_one(&self.pool)
                    .await?;
                count.unwrap_or(0) == 1
            }
            "postgres" => {
                let ret: Option<bool> = sqlx::query_scalar("SELECT pg_advisory_lock(hashtext('?'))")
                    .bind(lock_name)
                    .fetch_one(&self.pool)
                    .await?;
                ret.unwrap_or(false)
            }
            _ => {
                return Err(RvError::ErrDatabaseTypeInvalid);
            }
        };

        Ok(result)
    }

    async fn unlock(&self, lock_name: &str) -> Result<(), RvError> {
        match self.db_scheme.as_str() {
            "mysql" => {
                sqlx::query("SELECT RELEASE_LOCK(SHA1(?))").bind(lock_name).execute(&self.pool).await?;
            }
            "postgres" => {
                sqlx::query("SELECT pg_advisory_unlock(hashtext('?'))").bind(lock_name).execute(&self.pool).await?;
            }
            _ => {
                return Err(RvError::ErrDatabaseTypeInvalid);
            }
        };

        Ok(())
    }
}

#[maybe_async::must_be_async]
impl Backend for SqlxBackend {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        if prefix.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendPrefixInvalid);
        }

        let results: Vec<SqlxBackendEntry> =
            sqlx::query_as(&format!("SELECT vault_key, vault_value FROM {} WHERE vault_key LIKE ?", self.table_name))
                .bind(format!("{prefix}%"))
                .fetch_all(&self.pool)
                .await?;

        let mut keys: Vec<String> = Vec::new();
        for entry in results.iter() {
            let key = entry.vault_key.clone();
            let key = key.trim_start_matches(prefix);
            match key.find('/') {
                Some(i) => {
                    let key = &key[0..i + 1];
                    if !keys.contains(&key.to_string()) {
                        keys.push(key.to_string());
                    }
                }
                None => {
                    keys.push(key.to_string());
                }
            }
        }
        Ok(keys)
    }

    async fn get(&self, key: &str) -> Result<Option<BackendEntry>, RvError> {
        if key.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let result: Option<SqlxBackendEntry> =
            sqlx::query_as(&format!("SELECT vault_key, vault_value FROM {} WHERE vault_key = ?", self.table_name))
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;

        if let Some(entry) = result {
            return Ok(Some(BackendEntry { key: entry.vault_key, value: entry.vault_value }));
        }

        Ok(None)
    }

    async fn put(&self, entry: &BackendEntry) -> Result<(), RvError> {
        if entry.key.as_str().starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let _ = self.lock.lock(&entry.key).await?;

        let ret = sqlx::query(&format!(
            "INSERT INTO {} VALUES( ?, ? ) ON DUPLICATE KEY UPDATE vault_value=VALUES(vault_value)",
            self.table_name
        ))
        .bind(entry.key.as_str())
        .bind(entry.value.as_slice())
        .execute(&self.pool)
        .await;

        self.lock.unlock(&entry.key).await?;

        let _ = ret?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), RvError> {
        if key.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let _ = self.lock(key).await?;

        let ret = sqlx::query(&format!("DELETE FROM {} WHERE vault_key = ?", self.table_name))
            .bind(key)
            .execute(&self.pool)
            .await;

        self.lock.unlock(key).await?;

        let _ = ret?;

        Ok(())
    }

    async fn lock(&self, _lock_name: &str) -> Result<Box<dyn Any>, RvError> {
        Ok(Box::new(SqlxBackendLock::new(&self.pool, self.db_scheme.as_str(), 1)))
    }
}

impl SqlxBackend {
    async fn new_backend(conf: &HashMap<String, Value>) -> Result<Self, RvError> {
        let database_url =
            conf.get("database_url").and_then(|v| v.as_str()).ok_or(RvError::ErrDatabaseConnectionInfoInvalid)?;
        let table_name = conf.get("table").and_then(|v| v.as_str()).unwrap_or("vault");

        let db_name = DatabaseName::from_url(database_url)?;
        let db_scheme = db_name.scheme().to_string();

        let database_url_root = strip_db_name(database_url);

        let pool = sqlx::AnyPool::connect(&database_url_root).await?;

        match db_name {
            DatabaseName::MySql(database_name) => {
                let ret = sqlx::query(&format!("CREATE DATABASE IF NOT EXISTS `{database_name}`")).execute(&pool).await;
                if ret.is_err() {
                    log::warn!("sqlx create database error: {:?}", ret.err());
                }
                let ret = sqlx::query(&format!("CREATE TABLE IF NOT EXISTS `{database_name}`.`{table_name}` (vault_key varbinary(3072), vault_value mediumblob, PRIMARY KEY (vault_key))")).execute(&pool).await;
                if ret.is_err() {
                    log::warn!("sqlx create table error: {:?}", ret.err());
                }
            }
            _ => {
                return Err(RvError::ErrDatabaseTypeInvalid);
            }
        }

        pool.close().await;

        let pool = sqlx::AnyPool::connect_lazy(database_url)?;

        let lock = SqlxBackendLock::new(&pool, db_scheme.as_str(), 1);
        Ok(SqlxBackend { pool, table_name: table_name.to_string(), db_scheme, lock })
    }

    pub fn new(conf: &HashMap<String, Value>) -> Result<SqlxBackend, RvError> {
        let _database_url =
            conf.get("database_url").and_then(|v| v.as_str()).ok_or(RvError::ErrDatabaseConnectionInfoInvalid)?;

        sqlx::any::install_default_drivers();

        match tokio::runtime::Handle::try_current() {
            Ok(_handle) => std::thread::scope(|s| {
                let conf = conf.clone();
                let handle = s.spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async { Self::new_backend(&conf).await })
                });
                handle.join().unwrap()
            }),
            Err(_) => {
                let rt = tokio::runtime::Runtime::new()?;
                rt.block_on(async { Self::new_backend(conf).await })
            }
        }
    }
}

#[cfg(all(test, not(feature = "sync_handler"), feature = "storage_sqlx"))]
mod test {
    use std::sync::Arc;
    use std::{collections::HashMap, env};

    use serde_json::Value;
    use serial_test::serial;

    use super::SqlxBackend;

    use crate::errors::RvError;
    use crate::storage::test::{test_backend_curd, test_backend_list_prefix};
    use crate::test_utils::test_multi_routine;

    async fn sqlx_table_clear(backend: &SqlxBackend) -> Result<(), RvError> {
        let _ = sqlx::query("TRUNCATE TABLE vault").execute(&backend.pool).await?;
        Ok(())
    }

    #[serial]
    #[tokio::test]
    async fn test_sqlx_backend() {
        let sqlx_pwd = env::var("CARGO_TEST_MYSQL_PASSWORD").unwrap_or("".into());
        let mut conf: HashMap<String, Value> = HashMap::new();
        conf.insert("database_url".to_string(), Value::String(format!("mysql://root:{sqlx_pwd}@127.0.0.1:3306/vault")));
        conf.insert("table".to_string(), Value::String("vault".to_string()));

        let backend = SqlxBackend::new(&conf);

        assert!(backend.is_ok());

        let backend = backend.unwrap();

        assert!(sqlx_table_clear(&backend).await.is_ok());

        test_backend_curd(&backend).await;
        test_backend_list_prefix(&backend).await;
    }

    #[serial]
    #[tokio::test]
    async fn test_sqlx_backend_multi_routine() {
        let sqlx_pwd = env::var("CARGO_TEST_MYSQL_PASSWORD").unwrap_or("".into());
        let mut conf: HashMap<String, Value> = HashMap::new();
        conf.insert("database_url".to_string(), Value::String(format!("mysql://root:{sqlx_pwd}@127.0.0.1:3306/vault")));
        conf.insert("table".to_string(), Value::String("vault".to_string()));

        let backend = SqlxBackend::new(&conf).unwrap();

        test_multi_routine(Arc::new(backend));
    }
}
