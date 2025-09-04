use crate::domain::repo::Repo;
use crate::error::{AppError, BusinessError, MapToAppError};
use sqlx::SqlitePool;
use std::sync::Arc;

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
pub trait RepoRepository: Send + Sync {
    async fn query_repo_by_name(&self, name: &str) -> Result<Repo>;

    async fn create_repo(&self, repo: Repo) -> Result<()>;

    async fn ensure_repo_exists(&self, name: &str) -> Result<()> {
        if self.query_repo_by_name(name).await.is_err() {
            let repo = Repo::new(name);
            self.create_repo(repo).await?;
        }
        Ok(())
    }

    async fn change_visibility(&self, name: &str, is_public: bool) -> Result<()>;
}

#[derive(Debug)]
pub struct SqliteRepoRepository {
    pub pool: Arc<SqlitePool>,
}

impl SqliteRepoRepository {
    pub fn new(pool: Arc<SqlitePool>) -> SqliteRepoRepository {
        SqliteRepoRepository { pool }
    }
}

#[async_trait::async_trait]
impl RepoRepository for SqliteRepoRepository {
    async fn query_repo_by_name(&self, name: &str) -> Result<Repo> {
        sqlx::query_as::<_, Repo>("select * from repos where name = $1")
            .bind(name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_to_internal()?
            .ok_or_else(|| BusinessError::BadRequest("repo not found".to_string()).into())
    }

    async fn create_repo(&self, repo: Repo) -> Result<()> {
        sqlx::query("INSERT INTO repos (id, name, is_public) VALUES ($1, $2, $3)")
            .bind(repo.id)
            .bind(repo.name)
            .bind(repo.is_public)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }

    async fn change_visibility(&self, name: &str, is_public: bool) -> Result<()> {
        let result = sqlx::query(
            "UPDATE repos SET is_public = ?, updated_at = datetime('now') WHERE name = ?",
        )
        .bind(is_public)
        .bind(name)
        .execute(&*self.pool)
        .await
        .map_to_internal()?;
        match result.rows_affected() {
            0 => Err(BusinessError::BadRequest(
                format!("repository `{name}` not found").to_string(),
            )
            .into()),
            _ => Ok(()),
        }
    }
}
