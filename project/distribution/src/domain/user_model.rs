use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub password: String,
}

impl User {
    pub fn new(name: String, password: String) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        User { id, name, password }
    }
}