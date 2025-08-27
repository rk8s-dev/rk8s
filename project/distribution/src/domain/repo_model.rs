use sqlx::FromRow;
use uuid::Uuid;
use crate::utils::validation::is_valid_digest;

#[derive(Debug, Clone, FromRow)]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub is_public: i64,
}

impl Repo {
    pub fn new(name: String) -> Self {
        let uuid = Uuid::new_v4().to_string();
        Self {
            id: uuid,
            name,
            is_public: 1,
        }
    }
}