use axum::routing::connect;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use crate::error::AppError;
use crate::domain::user_model::User;

pub struct UserStorage {
    pool: SqlitePool,
}

impl UserStorage {
    pub async fn new(db_url: &str) -> Result<UserStorage, AppError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(12)
            .connect(db_url)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await?;
        Ok(Self {
            pool,
        })
    }

    pub async fn query_user_by_name(&self, name: &str) -> Result<User, AppError> {
        sqlx::query_as::<_, User>("select * from users where name = $1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(AppError::NotFound(format!("user {name}")))
    }

    pub async fn insert_user(&self, user: User) -> Result<(), AppError> {
        sqlx::query("INSERT INTO users (?, ?, ?) VALUES ($1, $2, $3)")
            .bind(user.id)
            .bind(user.name)
            .bind(user.password)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}