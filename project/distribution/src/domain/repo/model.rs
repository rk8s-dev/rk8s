use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Debug, Clone, FromRow, Default)]
pub struct Repo {
    pub id: Uuid,
    pub github_id: i64,
    pub name: String,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Repo {
    pub fn new(github_id: i64, name: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            github_id,
            name: name.to_owned(),
            is_public: false,
            ..Default::default()
        }
    }
}
