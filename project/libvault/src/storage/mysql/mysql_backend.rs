use std::{any::Any, collections::HashMap};

use diesel::{
    prelude::*,
    r2d2::ConnectionManager,
    sql_types::{Integer, Text},
    MysqlConnection,
};
use r2d2::{Pool, PooledConnection};
use serde::Deserialize;
use serde_json::Value;

use super::new_db_pool;
use crate::{
    errors::RvError,
    schema::vault::{self, dsl::*, vault_key},
    storage::{Backend, BackendEntry},
};

pub type DbPool = Pool<ConnectionManager<MysqlConnection>>;
pub type DbConn = PooledConnection<ConnectionManager<MysqlConnection>>;

pub struct MysqlBackend {
    pool: DbPool,
}

#[derive(Insertable, Queryable, PartialEq, Debug, Deserialize)]
#[diesel(table_name = vault)]
pub struct MysqlBackendEntry {
    pub vault_key: String,
    pub vault_value: Vec<u8>,
}

pub struct MysqlBackendLock {
    conn: DbConn,
    lock_name: String,
}

#[derive(Debug, QueryableByName)]
#[diesel(check_for_backend(diesel::mysql::Mysql))]
struct GetLockResult {
    #[diesel(sql_type = Integer)]
    result: i32,
}

impl MysqlBackendLock {
    pub fn new(pool: &DbPool, lock_name: &str, timeout_secs: i32) -> Result<Self, RvError> {
        let mut conn = pool.get()?;

        if !Self::get_lock(&mut conn, lock_name, timeout_secs)? {
            return Err(RvError::ErrStorageBackendLockFailed);
        }

        Ok(Self { conn, lock_name: lock_name.to_string() })
    }

    fn get_lock(conn: &mut DbConn, lock_name: &str, timeout_secs: i32) -> Result<bool, RvError> {
        let count = diesel::sql_query("SELECT GET_LOCK(?, ?) as result")
            .bind::<Text, _>(lock_name)
            .bind::<Integer, _>(timeout_secs)
            .get_result::<GetLockResult>(conn)
            .optional()?
            .map(|res| res.result)
            .unwrap_or(0);

        Ok(count == 1)
    }

    fn release_lock(&mut self) -> Result<(), RvError> {
        let rows_affected =
            diesel::sql_query("SELECT RELEASE_LOCK(?)").bind::<Text, _>(&self.lock_name).execute(&mut self.conn)?;

        if rows_affected == 0 {
            log::error!("MysqlBackendLock failed to release lock: {}", self.lock_name);
            return Err(RvError::ErrStorageBackendUnlockFailed);
        }

        Ok(())
    }
}

impl Drop for MysqlBackendLock {
    fn drop(&mut self) {
        if let Err(err) = self.release_lock() {
            log::error!("MysqlBackendLock error releasing lock '{}': {:?}", self.lock_name, err);
        }
    }
}

#[maybe_async::maybe_async]
impl Backend for MysqlBackend {
    async fn list(&self, prefix: &str) -> Result<Vec<String>, RvError> {
        if prefix.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendPrefixInvalid);
        }

        let conn = &mut self.pool.get()?;

        let results: Result<Vec<MysqlBackendEntry>, _> =
            vault.filter(vault_key.like(format!("{}%", prefix))).load::<MysqlBackendEntry>(conn);

        match results {
            Ok(entries) => {
                let mut keys: Vec<String> = Vec::new();
                for entry in entries {
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
                return Ok(keys);
            }
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    async fn get(&self, key: &str) -> Result<Option<BackendEntry>, RvError> {
        if key.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let conn = &mut self.pool.get()?;

        let result: Result<MysqlBackendEntry, _> = vault.filter(vault_key.eq(key)).first::<MysqlBackendEntry>(conn);

        match result {
            Ok(entry) => return Ok(Some(BackendEntry { key: entry.vault_key, value: entry.vault_value })),
            Err(e) => {
                if e == diesel::NotFound {
                    return Ok(None);
                } else {
                    return Err(RvError::ErrDatabaseExecuteEntry { source: (e) });
                }
            }
        }
    }

    async fn put(&self, entry: &BackendEntry) -> Result<(), RvError> {
        if entry.key.as_str().starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let _ = self.lock(&entry.key);

        let conn = &mut self.pool.get()?;

        let new_entry = MysqlBackendEntry { vault_key: entry.key.clone(), vault_value: entry.value.clone() };

        match diesel::replace_into(vault).values(&new_entry).execute(conn) {
            Ok(_) => return Ok(()),
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    async fn delete(&self, key: &str) -> Result<(), RvError> {
        if key.starts_with("/") {
            return Err(RvError::ErrPhysicalBackendKeyInvalid);
        }

        let _ = self.lock(key);

        let conn = &mut self.pool.get()?;

        match diesel::delete(vault.filter(vault_key.eq(key))).execute(conn) {
            Ok(_) => return Ok(()),
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    async fn lock(&self, lock_name: &str) -> Result<Box<dyn Any>, RvError> {
        Ok(Box::new(MysqlBackendLock::new(&self.pool, lock_name, 1)?))
    }
}

impl MysqlBackend {
    pub fn new(conf: &HashMap<String, Value>) -> Result<MysqlBackend, RvError> {
        match new_db_pool(conf) {
            Ok(pool) => Ok(MysqlBackend { pool }),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use std::{collections::HashMap, env};

    use diesel::{prelude::*, MysqlConnection};
    use serde_json::Value;

    use super::MysqlBackend;

    use crate::errors::RvError;
    use crate::storage::test::{test_backend_curd, test_backend_list_prefix};
    use crate::test_utils::test_multi_routine;

    fn mysql_table_clear(backend: &MysqlBackend) -> Result<(), RvError> {
        let conn: &mut MysqlConnection = &mut backend.pool.get().unwrap();

        match diesel::sql_query("TRUNCATE TABLE vault").execute(conn) {
            Ok(_) => return Ok(()),
            Err(e) => return Err(RvError::ErrDatabaseExecuteEntry { source: (e) }),
        }
    }

    #[test]
    fn test_mysql_backend() {
        let mysql_pwd = env::var("CARGO_TEST_MYSQL_PASSWORD").unwrap_or("password".into());
        let mut conf: HashMap<String, Value> = HashMap::new();
        conf.insert("address".to_string(), Value::String("127.0.0.1:3306".to_string()));
        conf.insert("username".to_string(), Value::String("root".to_string()));
        conf.insert("password".to_string(), Value::String(mysql_pwd));

        let backend = MysqlBackend::new(&conf);

        assert!(backend.is_ok());

        let backend = backend.unwrap();

        assert!(mysql_table_clear(&backend).is_ok());

        test_backend_curd(&backend);
        test_backend_list_prefix(&backend);
    }

    #[test]
    fn test_mysql_backend_multi_routine() {
        let mysql_pwd = env::var("CARGO_TEST_MYSQL_PASSWORD").unwrap_or("password".into());
        let mut conf: HashMap<String, Value> = HashMap::new();
        conf.insert("address".to_string(), Value::String("127.0.0.1:3306".to_string()));
        conf.insert("username".to_string(), Value::String("root".to_string()));
        conf.insert("password".to_string(), Value::String(mysql_pwd));

        let backend = MysqlBackend::new(&conf).unwrap();

        test_multi_routine(Arc::new(backend));
    }
}
