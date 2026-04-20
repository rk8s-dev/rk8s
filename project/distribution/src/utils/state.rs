use crate::config::Config;
use crate::domain::repo::{PgRepoRepository, RepoRepository};
use crate::domain::user::{PgUserRepository, UserRepository};
use crate::storage::Storage;
use crate::storage::driver::filesystem::FilesystemStorage;
use crate::storage::driver::s3::S3Storage;
use anyhow::{Context, bail};
use sqlx::PgPool;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{Mutex, RwLock};

#[derive(Clone, Debug)]
pub struct UploadSession {
    pub uploaded: u64,
    pub operation_lock: Arc<Mutex<()>>,
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<RwLock<HashMap<String, UploadSession>>>,
    pub storage: Arc<dyn Storage>,
    pub user_storage: Arc<dyn UserRepository>,
    pub repo_storage: Arc<dyn RepoRepository>,
    pub config: Arc<Config>,
    pub http_client: reqwest::Client,
}

impl AppState {
    pub async fn new(config: Config, pool: Arc<PgPool>) -> anyhow::Result<Self> {
        let storage_backend: Arc<dyn Storage + Send + Sync> = match config.storage_type.as_str() {
            "FILESYSTEM" => Arc::new(FilesystemStorage::new(&config.root_dir)),
            "S3" => {
                let s3_cfg = config
                    .s3_config
                    .as_ref()
                    .context("S3 config must be present when storage_type is S3")?;
                let s3 = S3Storage::new(s3_cfg)
                    .map_err(|e| anyhow::anyhow!("Failed to initialize S3 storage: {e}"))?;
                Arc::new(s3)
            }
            other => {
                bail!("Unsupported storage type: '{other}'. Valid values: FILESYSTEM, S3");
            }
        };

        Ok(AppState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            storage: storage_backend,
            config: Arc::new(config),
            http_client: reqwest::Client::new(),
            user_storage: Arc::new(PgUserRepository::new(pool.clone())),
            repo_storage: Arc::new(PgRepoRepository::new(pool)),
        })
    }

    pub async fn get_session(&self, id: &str) -> Option<UploadSession> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn get_session_lock(&self, id: &str) -> Option<Arc<Mutex<()>>> {
        let sessions = self.sessions.read().await;
        sessions
            .get(id)
            .map(|session| session.operation_lock.clone())
    }

    pub async fn create_session(&self) -> String {
        let mut sessions = self.sessions.write().await;
        let session_id = uuid::Uuid::new_v4().to_string();
        sessions.insert(
            session_id.clone(),
            UploadSession {
                uploaded: 0,
                operation_lock: Arc::new(Mutex::new(())),
            },
        );
        session_id
    }

    pub async fn close_session(&self, id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);
    }

    pub async fn update_session(&self, id: &str, chunk_length: u64) -> Option<u64> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.uploaded += chunk_length;
            Some(session.uploaded)
        } else {
            None
        }
    }
}
