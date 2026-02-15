//! This module includes storage backends for all SQL types. Currently supported: SQLite, PostgreSQL.

#[cfg(feature = "storage_pg")]
pub mod postgresql;
#[cfg(feature = "storage_sqlite")]
pub mod sqlite;
