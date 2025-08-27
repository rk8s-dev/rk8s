use std::sync::Arc;
use sqlx::SqlitePool;
use crate::domain::repo_model::Repo;
use crate::error::AppError;

pub struct RepoStorage {
    pool: Arc<SqlitePool>,
}

impl RepoStorage {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
        }
    }

    pub async fn insert_repo(&self, repo: Repo) -> Result<(), AppError> {
        sqlx::query("INSERT INTO repos (?, ?, ?)")
            .bind(repo.id)
            .bind(repo.name)
            .bind(repo.is_public)
            .execute(self.pool.as_ref())
            .await?;
        Ok(())
    }

    pub async fn query_repo_by_name(&self, name: &str) -> Result<Repo, AppError> {
        sqlx::query_as::<_, Repo>("select * from repos where name = $1")
            .bind(name)
            .fetch_optional(self.pool.as_ref())
            .await?
            .ok_or(AppError::NotFound(format!("repository {name}")))
    }

    pub async fn is_repo_public(&self, name: &str) -> Result<bool, AppError> {
        let repo = self.query_repo_by_name(name).await?;
        Ok(repo.is_public == 1)
    }
}