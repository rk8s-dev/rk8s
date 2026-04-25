use crate::domain::repo::{Repo, RepoTag};
use crate::error::{AppError, BusinessError, MapToAppError};
use crate::utils::repo_identifier::RepoIdentifier;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, QueryBuilder};
use std::sync::Arc;
use uuid::Uuid;

type Result<T> = std::result::Result<T, AppError>;

#[async_trait::async_trait]
pub trait RepoRepository: Send + Sync {
    async fn create_or_get_repo(&self, identifier: &RepoIdentifier) -> Result<Repo>;

    async fn query_repo_by_identifier_optional(
        &self,
        identifier: &RepoIdentifier,
    ) -> Result<Option<Repo>>;

    async fn query_repo_by_identifier(&self, identifier: &RepoIdentifier) -> Result<Repo> {
        self.query_repo_by_identifier_optional(identifier)
            .await?
            .ok_or_else(|| BusinessError::BadRequest("repo not found".to_string()).into())
    }

    async fn query_all_visible_repos(&self, namespace: &str) -> Result<Vec<Repo>>;

    async fn query_repo_tags(&self, repo_ids: &[Uuid]) -> Result<Vec<RepoTag>>;

    async fn query_distinct_manifest_digests(&self) -> Result<Vec<String>>;

    async fn query_repo_tags_by_digest(
        &self,
        repo_id: Uuid,
        manifest_digest: &str,
    ) -> Result<Vec<RepoTag>>;

    async fn repo_tag_exists_for_digest(&self, manifest_digest: &str) -> Result<bool>;

    async fn upsert_repo_tag(
        &self,
        repo_id: Uuid,
        tag: &str,
        manifest_digest: &str,
        manifest_size_bytes: i64,
        pushed_at: DateTime<Utc>,
    ) -> Result<()>;

    async fn delete_repo_tag(&self, repo_id: Uuid, tag: &str) -> Result<()>;

    async fn delete_repo_tags_by_digest(&self, repo_id: Uuid, manifest_digest: &str) -> Result<()>;

    async fn mark_repo_pushed(&self, repo_id: Uuid, pushed_at: DateTime<Utc>) -> Result<()>;

    async fn refresh_repo_last_pushed_at(&self, repo_id: Uuid) -> Result<()>;

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
    async fn create_or_get_repo(&self, identifier: &RepoIdentifier) -> Result<Repo> {
        if let Some(repo) = sqlx::query_as::<_, Repo>(
            "INSERT INTO repos (namespace, name, is_public)
             VALUES ($1, $2, FALSE)
             ON CONFLICT (namespace, name) DO NOTHING
             RETURNING *",
        )
        .bind(&identifier.namespace)
        .bind(&identifier.name)
        .fetch_optional(self.pool.as_ref())
        .await
        .map_to_internal()?
        {
            return Ok(repo);
        }

        self.query_repo_by_identifier(identifier).await
    }

    async fn query_repo_by_identifier_optional(
        &self,
        identifier: &RepoIdentifier,
    ) -> Result<Option<Repo>> {
        sqlx::query_as::<_, Repo>("select * from repos where namespace = $1 and name = $2")
            .bind(&identifier.namespace)
            .bind(&identifier.name)
            .fetch_optional(self.pool.as_ref())
            .await
            .map_to_internal()
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

    async fn query_repo_tags(&self, repo_ids: &[Uuid]) -> Result<Vec<RepoTag>> {
        if repo_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT repo_id, tag, manifest_digest, manifest_size_bytes, pushed_at FROM repo_tags WHERE repo_id IN (",
        );
        let mut separated = builder.separated(", ");
        for repo_id in repo_ids {
            separated.push_bind(repo_id);
        }
        separated.push_unseparated(")");
        builder.push(" ORDER BY pushed_at DESC, tag ASC");

        builder
            .build_query_as::<RepoTag>()
            .fetch_all(self.pool.as_ref())
            .await
            .map_to_internal()
    }

    async fn query_distinct_manifest_digests(&self) -> Result<Vec<String>> {
        sqlx::query_scalar::<_, String>(
            "SELECT DISTINCT manifest_digest FROM repo_tags ORDER BY manifest_digest",
        )
        .fetch_all(self.pool.as_ref())
        .await
        .map_to_internal()
    }

    async fn query_repo_tags_by_digest(
        &self,
        repo_id: Uuid,
        manifest_digest: &str,
    ) -> Result<Vec<RepoTag>> {
        sqlx::query_as::<_, RepoTag>(
            "SELECT repo_id, tag, manifest_digest, manifest_size_bytes, pushed_at
             FROM repo_tags
             WHERE repo_id = $1 AND manifest_digest = $2
             ORDER BY pushed_at DESC, tag ASC",
        )
        .bind(repo_id)
        .bind(manifest_digest)
        .fetch_all(self.pool.as_ref())
        .await
        .map_to_internal()
    }

    async fn upsert_repo_tag(
        &self,
        repo_id: Uuid,
        tag: &str,
        manifest_digest: &str,
        manifest_size_bytes: i64,
        pushed_at: DateTime<Utc>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO repo_tags (repo_id, tag, manifest_digest, manifest_size_bytes, pushed_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (repo_id, tag)
             DO UPDATE SET
                 manifest_digest = EXCLUDED.manifest_digest,
                 manifest_size_bytes = EXCLUDED.manifest_size_bytes,
                 pushed_at = EXCLUDED.pushed_at",
        )
        .bind(repo_id)
        .bind(tag)
        .bind(manifest_digest)
        .bind(manifest_size_bytes)
        .bind(pushed_at)
        .execute(self.pool.as_ref())
        .await
        .map_to_internal()?;
        Ok(())
    }

    async fn repo_tag_exists_for_digest(&self, manifest_digest: &str) -> Result<bool> {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM repo_tags WHERE manifest_digest = $1)",
        )
        .bind(manifest_digest)
        .fetch_one(self.pool.as_ref())
        .await
        .map_to_internal()
    }

    async fn delete_repo_tag(&self, repo_id: Uuid, tag: &str) -> Result<()> {
        sqlx::query("DELETE FROM repo_tags WHERE repo_id = $1 AND tag = $2")
            .bind(repo_id)
            .bind(tag)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }

    async fn delete_repo_tags_by_digest(&self, repo_id: Uuid, manifest_digest: &str) -> Result<()> {
        sqlx::query("DELETE FROM repo_tags WHERE repo_id = $1 AND manifest_digest = $2")
            .bind(repo_id)
            .bind(manifest_digest)
            .execute(self.pool.as_ref())
            .await
            .map_to_internal()?;
        Ok(())
    }

    async fn mark_repo_pushed(&self, repo_id: Uuid, pushed_at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "UPDATE repos
             SET last_pushed_at = CASE
                 WHEN last_pushed_at IS NULL OR last_pushed_at < $2 THEN $2
                 ELSE last_pushed_at
             END
             WHERE id = $1",
        )
        .bind(repo_id)
        .bind(pushed_at)
        .execute(self.pool.as_ref())
        .await
        .map_to_internal()?;
        Ok(())
    }

    async fn refresh_repo_last_pushed_at(&self, repo_id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE repos
             SET last_pushed_at = (
                 SELECT MAX(pushed_at)
                 FROM repo_tags
                 WHERE repo_id = $1
             )
             WHERE id = $1",
        )
        .bind(repo_id)
        .execute(self.pool.as_ref())
        .await
        .map_to_internal()?;
        Ok(())
    }

    async fn change_visibility(&self, identifier: &RepoIdentifier, is_public: bool) -> Result<()> {
        let result =
            sqlx::query("UPDATE repos SET is_public = $1 WHERE namespace = $2 and name = $3")
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
