use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Debug, Clone, FromRow, Default)]
pub struct Repo {
    pub id: Uuid,
    pub namespace: String,
    pub name: String,
    pub is_public: bool,
    pub last_pushed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Repo {
    pub fn new(namespace: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            namespace: namespace.into(),
            name: name.into(),
            is_public: false,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct RepoTag {
    pub repo_id: Uuid,
    pub tag: String,
    pub manifest_digest: String,
    pub manifest_size_bytes: i64,
    pub pushed_at: DateTime<Utc>,
}
