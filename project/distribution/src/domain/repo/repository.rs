use crate::domain::repo::Repo;
use crate::error::{AppError, BusinessError, MapToAppError};
use crate::utils::repo_identifier::RepoIdentifier;
use sqlx::PgPool;
use std::sync::Arc;

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
pub trait RepoRepository: Send + Sync {
    async fn create_repo(&self, repo: Repo) -> Result<()>;

    async fn ensure_repo_exists(&self, identifier: &RepoIdentifier) -> Result<()> {
        if self.query_repo_by_identifier(identifier).await.is_err() {
            let repo = Repo::new(&identifier.namespace, &identifier.name);
            self.create_repo(repo).await?;
        }
        Ok(())
    }

    async fn query_repo_by_identifier(&self, identifier: &RepoIdentifier) -> Result<Repo>;

    async fn query_all_visible_repos(&self, namespace: &str) -> Result<Vec<Repo>>;

    async fn change_visibility(&self, identifier: &RepoIdentifier, is_public: bool) -> Result<()>;
}

#[derive(Debug)]
pub struct PgRepoRepository {
    pub pool: Arc<PgPool>,
}

impl PgRepoRepository {
    pub fn new(pool: Arc<PgPool>) -> PgRepoRepository {
        PgRepoRepository { pool }
    }
}

#[async_trait::async_trait]
impl RepoRepository for PgRepoRepository {
    async fn create_repo(&self, repo: Repo) -> Result<()> {
        sqlx::query("INSERT INTO repos (id, namespace, name, is_public) VALUES ($1, $2, $3, $4)")
            .bind(repo.id)
            .bind(repo.namespace)
            .bind(repo.name)
            .bind(repo.is_public)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }

    async fn query_repo_by_identifier(&self, identifier: &RepoIdentifier) -> Result<Repo> {
        sqlx::query_as::<_, Repo>("select * from repos where namespace = $1 and name = $2")
            .bind(&identifier.namespace)
            .bind(&identifier.name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_to_internal()?
            .ok_or_else(|| BusinessError::BadRequest("repo not found".to_string()).into())
    }

    async fn query_all_visible_repos(&self, namespace: &str) -> Result<Vec<Repo>> {
        Ok(sqlx::query_as::<_, Repo>(
            "SELECT * FROM repos where is_public = true or namespace = $1",
        )
        .bind(namespace)
        .fetch_all(self.pool.as_ref())
        .await
        .map_to_internal()?)
    }

    async fn change_visibility(&self, identifier: &RepoIdentifier, is_public: bool) -> Result<()> {
        let result =
            sqlx::query("UPDATE repos SET is_public = $1, updated_at = NOW() WHERE namespace = $2 and name = $3")
                .bind(is_public)
                .bind(&identifier.namespace)
                .bind(&identifier.name)
                .execute(&*self.pool)
                .await
                .map_to_internal()?;
        match result.rows_affected() {
            0 => Err(BusinessError::BadRequest(
                format!("repository `{}` not found", identifier.full_name()).to_string(),
            )
            .into()),
            _ => Ok(()),
        }
    }
}
