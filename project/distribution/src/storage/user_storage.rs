use crate::domain::user_model::User;
use crate::error::{AppError, BusinessError, MapToAppError};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Debug)]
pub struct UserStorage {
    pool: Arc<SqlitePool>,
}

impl UserStorage {
    pub fn new(pool: Arc<SqlitePool>) -> Self { 
        Self {
            pool,
        }
    }

    pub async fn query_user_by_name(&self, name: &str) -> Result<User, AppError> {
        sqlx::query_as::<_, User>("select * from users where username = $1")
            .bind(name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_to_internal()?
            .ok_or_else(|| AppError::from(BusinessError::ResourceNotFound(format!("user {name}"))))
    }

    pub async fn insert_user(&self, user: User) -> Result<(), AppError> {
        sqlx::query("INSERT INTO users (id, username, password) VALUES ($1, $2, $3)")
            .bind(user.id)
            .bind(user.username)
            .bind(user.password)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }
}