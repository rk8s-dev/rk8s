use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct Repo {
    pub id: String,
    pub name: String,
    pub is_public: i64,
}

impl Repo {
    pub fn new(name: &str) -> Self {
        let uuid = Uuid::new_v4().to_string();
        Self {
            id: uuid,
            name: name.to_owned(),
            is_public: 1,
        }
    }
}