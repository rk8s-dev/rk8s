use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: u64,

    /// Session UUID for unique identification
    #[sea_orm(column_type = "Text", unique)]
    pub session_uuid: String,

    #[sea_orm(column_type = "Blob")]
    pub payload: Vec<u8>,

    /// Session creation timestamp (Unix epoch in milliseconds)
    pub created_at: i64,

    /// Session last update timestamp (Unix epoch in milliseconds)
    pub updated_at: i64,

    /// Whether the session is marked as stale (for cleanup)
    #[sea_orm(column_type = "Boolean", default_value = "false")]
    pub stale: bool,

    /// Optional hostname of the client
    #[sea_orm(column_type = "Text", nullable)]
    pub hostname: Option<String>,

    /// Optional process ID of the client
    #[sea_orm(column_type = "Integer", nullable)]
    pub process_id: Option<i32>,

    /// Optional mount point
    #[sea_orm(column_type = "Text", nullable)]
    pub mount_point: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[allow(dead_code)]
impl Model {
    /// Create a new session model with current timestamp
    pub fn new(
        id: u64,
        session_uuid: String,
        payload: Vec<u8>,
        hostname: Option<String>,
        process_id: Option<i32>,
        mount_point: Option<String>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        Self {
            id,
            session_uuid,
            payload,
            created_at: now,
            updated_at: now,
            stale: false,
            hostname,
            process_id,
            mount_point,
        }
    }

    /// Update the session's last update timestamp
    pub fn touch(&mut self) {
        self.updated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        self.stale = false;
    }

    /// Mark the session as stale
    pub fn mark_stale(&mut self) {
        self.stale = true;
    }

    /// Check if the session is expired based on given timeout
    pub fn is_expired(&self, timeout_ms: i64) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        (now - self.updated_at) > timeout_ms
    }

    /// Get the session creation time as SystemTime
    pub fn created_time(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(self.created_at as u64)
    }

    /// Get the session last update time as SystemTime
    pub fn updated_time(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_millis(self.updated_at as u64)
    }
}
