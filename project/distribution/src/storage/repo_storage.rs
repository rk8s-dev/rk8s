use std::sync::Arc;
use sqlx::SqlitePool;
use crate::domain::repo_model::Repo;
use crate::error::{AppError, BusinessError, InternalError, MapToAppError};

#[derive(Debug)]
pub struct RepoStorage {
    pool: Arc<SqlitePool>,
}

impl RepoStorage {
    pub fn new(pool: Arc<SqlitePool>) -> Self {
        Self {
            pool,
        }
    }

    pub async fn ensure_repo_exists(&self, name: &str) -> Result<(), AppError> {
        if self.query_repo_by_name(name).await.is_err() {
            let repo = Repo::new(name);
            self.insert_repo(repo).await?;
        }
        Ok(())
    }
    pub async fn insert_repo(&self, repo: Repo) -> Result<(), AppError> {
        sqlx::query("INSERT INTO repos (id, name, is_public) VALUES ($1, $2, $3)")
            .bind(repo.id)
            .bind(repo.name)
            .bind(repo.is_public)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }

    pub async fn query_repo_by_name(&self, name: &str) -> Result<Repo, AppError> {
        sqlx::query_as::<_, Repo>("select * from repos where name = $1")
            .bind(name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_to_internal()?
            .ok_or_else(|| AppError::from(BusinessError::ResourceNotFound(format!("repository {name}"))))
    }

    pub async fn change_visibility(&self, name: &str, is_public: bool) -> Result<(), AppError> {
        self.query_repo_by_name(name).await?;
        sqlx::query("UPDATE repos set is_public = $1 WHERE name = $2")
            .bind(is_public)
            .bind(name)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }
}