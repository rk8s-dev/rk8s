use crate::domain::user::User;
use crate::error::{AppError, BusinessError, MapToAppError};
use sqlx::PgPool;
use std::sync::Arc;

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
pub trait UserRepository: Send + Sync {
    async fn query_user_by_name(&self, name: &str) -> Result<User>;

    async fn create_user(&self, user: User) -> Result<()>;
}

#[derive(Debug)]
pub struct PgUserRepository {
    pub pool: Arc<PgPool>,
}

impl PgUserRepository {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl UserRepository for PgUserRepository {
    async fn query_user_by_name(&self, name: &str) -> Result<User> {
        sqlx::query_as::<_, User>("select * from users where username = $1")
            .bind(name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_to_internal()?
            .ok_or_else(|| BusinessError::BadRequest("user not found".to_string()).into())
    }

    async fn create_user(&self, user: User) -> Result<()> {
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
