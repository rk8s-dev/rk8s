use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Debug, Clone, FromRow, Default)]
pub struct User {
    pub id: Uuid,
    pub github_id: i64,
    pub username: String,
    pub password: String,
    pub salt: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(
        github_id: i64,
        username: impl Into<String>,
        password: impl Into<String>,
        salt: impl Into<String>,
    ) -> Self {
        User {
            id: Uuid::new_v4(),
            github_id,
            username: username.into(),
            password: password.into(),
            salt: salt.into(),
            ..Default::default()
        }
    }
}
