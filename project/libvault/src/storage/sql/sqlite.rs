use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::{
    collections::{HashMap, HashSet},
    env,
    path::PathBuf,
    time::Duration,
};
use tracing::debug;

use crate::{
    errors::RvError,
    storage::{Backend, BackendEntry},
};

const DEFAULT_SQLITE_FILENAME: &str = "vault.db";
const DEFAULT_SQLITE_TABLE: &str = "vault";
const DEFAULT_SQLITE_TIMEOUT: u64 = 7200;

#[derive(Debug, sqlx::FromRow)]
struct SqliteBackendEntry {
    vault_key: Vec<u8>,
    vault_value: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SqliteBackendConfig {
    filename: PathBuf,
    table: String,
    timeout: Duration,
    create_if_missing: bool,
}

impl<'de> Deserialize<'de> for SqliteBackendConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let default_cfg = Self::default();
        let deserializer_map: Map<String, Value> = <Map<String, Value>>::deserialize(deserializer)?;
        Ok(Self {
            filename: std::env::var("VAULT_SQLITE_FILENAME")
                .ok()
                .map(PathBuf::from)
                .unwrap_or(
                    deserializer_map
                        .get("filename")
                        .and_then(|filename| {
                            serde_json::from_value::<PathBuf>(filename.clone())
                                .map_err(|err| {
                                    log::warn!(
                                        "SQLite Backend: filename from_value failed: {err:?}"
                                    )
                                })
                                .ok()
                        })
                        .unwrap_or(default_cfg.filename),
                )
                .canonicalize()
                .map_err(D::Error::custom)?,
            table: deserializer_map
                .get("table")
                .and_then(|table| {
                    serde_json::from_value::<String>(table.clone())
                        .map_err(|err| {
                            log::warn!("SQLite Backend: table from_value failed: {err:?}")
                        })
                        .ok()
                })
                .unwrap_or(default_cfg.table),
            timeout: match std::env::var("VAULT_SQLITE_TIMEOUT")
                .map(Value::String)
                .ok()
                .or(deserializer_map.get("timeout").cloned())
            {
                Some(Value::String(duration)) => match duration.is_empty() {
                    true => default_cfg.timeout,
                    false => humantime::parse_duration(duration.trim())
                        .map(|timeout| {
                            match timeout.gt(&Duration::ZERO)
                                && timeout.lt(&Duration::from_secs(DEFAULT_SQLITE_TIMEOUT))
                            {
                                true => Ok(timeout),
                                false => Err(D::Error::custom(format!(
                                    "SQLite Backend: Timeout must be greater than 0s and less than {}s.",
                                    DEFAULT_SQLITE_TIMEOUT
                                ))),
                            }
                        })
                        .map_err(D::Error::custom)??,
                },
                Some(Value::Number(secs)) => Duration::from_secs(secs.as_u64().unwrap_or(5_u64)),
                _ => default_cfg.timeout,
            },
            create_if_missing: deserializer_map
                .get("create_if_missing")
                .and_then(|key| {
                    serde_json::from_value::<bool>(key.clone())
                        .map_err(|err| {
                            log::warn!("SQLite Backend: table from_value failed: {err:?}")
                        })
                        .ok()
                })
                .unwrap_or(default_cfg.create_if_missing),
        })
    }
}

impl Default for SqliteBackendConfig {
    fn default() -> Self {
        Self {
            filename: env::temp_dir().join(DEFAULT_SQLITE_FILENAME),
            table: DEFAULT_SQLITE_TABLE.to_string(),
            timeout: Duration::new(5, 0),
            create_if_missing: true,
        }
    }
}

pub struct SqliteBackend {
    pool: SqlitePool,
    table: String,
}

impl SqliteBackend {
    pub async fn new(conf: &HashMap<String, Value>) -> Result<Self, RvError> {
        let conf: SqliteBackendConfig = serde_json::from_value(serde_json::to_value(conf)?)?;
        let opts = SqliteConnectOptions::new()
            .filename(conf.filename)
            .busy_timeout(conf.timeout)
            .create_if_missing(conf.create_if_missing)
            .read_only(false);
        debug!("Sqlite connect options: {:?}", opts);

        let pool = SqlitePool::connect_with(opts).await?;
        sqlx::query(&format!(
            r#"CREATE TABLE IF NOT EXISTS `{}` (
    `vault_key` BLOB NOT NULL,
    `vault_value` BLOB,
    PRIMARY KEY (`vault_key`)
);"#,
            conf.table
        ))
        .execute(&pool)
        .await?;

        Ok(Self {
            pool,
            table: conf.table,
        })
    }
}

#[async_trait::async_trait]
impl Backend for SqliteBackend {
    async fn get(&self, key: &str) -> Result<Option<BackendEntry>, RvError> {
        let sql = format!(
            "SELECT vault_key, vault_value FROM `{}` WHERE vault_key = ?",
            &self.table
        );
        let ret: Option<SqliteBackendEntry> = sqlx::query_as(&sql)
            .bind(key.as_bytes())
            .fetch_optional(&self.pool)
            .await?;

        if let Some(item) = ret {
            Ok(Some(BackendEntry {
                key: key.to_string(),
                value: item.vault_value,
            }))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, entry: &BackendEntry) -> Result<(), RvError> {
        let sql = format!(
            "INSERT INTO `{}` (vault_key, vault_value) VALUES (?, ?) ON CONFLICT(vault_key) DO UPDATE SET vault_value = excluded.vault_value",
            &self.table
        );
        sqlx::query(&sql)
            .bind(entry.key.as_bytes())
            .bind(&entry.value)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), RvError> {
        let sql = format!("DELETE FROM {} WHERE vault_key = ?", &self.table);
        sqlx::query(&sql)
            .bind(key.as_bytes())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        let sql = format!(
            "SELECT vault_key, vault_value FROM {} WHERE vault_key LIKE ?",
            &self.table
        );
        let items: Vec<SqliteBackendEntry> = sqlx::query_as(&sql)
            .bind(format!("{}%", prefix).as_bytes())
            .fetch_all(&self.pool)
            .await?;

        let mut res = HashSet::new();

        for item in items {
            let key = String::from_utf8(item.vault_key)?;
            let key = key.strip_prefix(prefix).unwrap_or(&key);

            match key.find('/') {
                Some(i) => {
                    let key = &key[0..i + 1];
                    res.insert(key.to_string());
                }
                None => {
                    res.insert(key.to_string());
                }
            }
        }

        Ok(res.into_iter().collect())
    }
}
