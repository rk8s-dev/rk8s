use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub username: String,
    pub password: String,
}

impl User {
    pub fn new(username: String, password: String) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        User { id, username, password }
    }
}