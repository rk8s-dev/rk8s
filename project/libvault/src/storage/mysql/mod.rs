//! MySQL storage backend implementations.

use std::collections::HashMap;

use diesel::{
    mysql::MysqlConnection,
    r2d2::{self, ConnectionManager},
};
use serde_json::Value;

use crate::errors::RvError;

type MysqlDbPool = r2d2::Pool<ConnectionManager<MysqlConnection>>;

pub mod mysql_backend;

pub fn new_db_pool(conf: &HashMap<String, Value>) -> Result<MysqlDbPool, RvError> {
    let pool = establish_mysql_connection(conf);
    match pool {
        Ok(pool) => Ok(pool),
        Err(e) => Err(e),
    }
}

/**
 * The `establish_mysql_connection` function is used to establish a connection to a MySQL database.
 * The function takes a configuration object as an argument and returns a `Result` containing a `MysqlDbPool` or an `RvError`.
 */
fn establish_mysql_connection(conf: &HashMap<String, Value>) -> Result<MysqlDbPool, RvError> {
    let address = conf.get("address").and_then(|v| v.as_str()).ok_or(RvError::ErrDatabaseConnectionInfoInvalid)?;

    let database = conf.get("database").and_then(|v| v.as_str()).unwrap_or("vault");
    let username = conf.get("username").and_then(|v| v.as_str()).ok_or(RvError::ErrDatabaseConnectionInfoInvalid)?;
    let password = conf.get("password").and_then(|v| v.as_str()).ok_or(RvError::ErrDatabaseConnectionInfoInvalid)?;

    // let table = conf.get("table").and_then(|v| v.as_str()).unwrap_or("vault");
    // let tls_ca_file = conf.get("tls_ca_file").and_then(|v| v.as_str()).unwrap_or("");
    // let plaintext_credentials_transmission = conf.get("plaintext_credentials_transmission").and_then(|v| v.as_str()).unwrap_or("");
    // let max_parralel = conf.get("max_parralel").and_then(|v| v.as_i64()).unwrap_or(128) as i32;
    // let max_idle_connections = conf.get("max_idle_connections").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    // let max_connection_lifetime = conf.get("max_connection_lifetime").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    //
    // now this can not support ssl connection yet. Still need to improve it.
    let database_url = format!("mysql://{}:{}@{}/{}", username, password, address, database);

    let manager = ConnectionManager::<MysqlConnection>::new(database_url);
    match r2d2::Pool::builder().build(manager) {
        Ok(pool) => Ok(pool),
        Err(e) => {
            log::error!("Error: {e:?}");
            Err(RvError::ErrConnectionPoolCreate { source: (e) })
        }
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, env};

    use super::*;

    #[test]
    fn test_establish_mysql_connection() {
        let mysql_pwd = env::var("CARGO_TEST_MYSQL_PASSWORD").unwrap_or("password".into());
        let mut conf: HashMap<String, Value> = HashMap::new();
        conf.insert("address".to_string(), Value::String("127.0.0.1:3306".to_string()));
        conf.insert("username".to_string(), Value::String("root".to_string()));
        conf.insert("password".to_string(), Value::String(mysql_pwd));

        let pool = establish_mysql_connection(&conf);

        assert!(pool.is_ok());
    }
}
