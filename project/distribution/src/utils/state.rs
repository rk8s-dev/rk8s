use crate::config::Config;
use crate::storage::{Storage, driver::filesystem::FilesystemStorage};
use sqlx::{Pool, Sqlite};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;
use crate::domain::repo::{RepoRepository, SqliteRepoRepository};
use crate::domain::user::{SqliteUserRepository, UserRepository};

#[derive(Clone, Debug)]
pub struct UploadSession {
    pub uploaded: u64, // the total bytes uploaded
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<RwLock<HashMap<String, UploadSession>>>,
    pub storage: Arc<dyn Storage>,
    pub user_storage: Arc<dyn UserRepository>,
    pub repo_storage: Arc<dyn RepoRepository>,
    pub config: Arc<Config>,
}

impl AppState {
    pub async fn new(config: Config, pool: Arc<Pool<Sqlite>>) -> Self {
        let storage_backend: Arc<dyn Storage + Send + Sync> = match config.storge_typ.as_str() {
            "FILESYSTEM" => Arc::new(FilesystemStorage::new(&config.root_dir)),
            _ => Arc::new(FilesystemStorage::new(&config.root_dir)),
        };

        AppState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            storage: storage_backend,
            config: Arc::new(config),
            user_storage: Arc::new(SqliteUserRepository::new(pool.clone())),
            repo_storage: Arc::new(SqliteRepoRepository::new(pool)),
        }
    }

    pub async fn get_session(&self, id: &str) -> Option<UploadSession> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn create_session(&self) -> String {
        let mut sessions = self.sessions.write().await;
        let session_id = uuid::Uuid::new_v4().to_string();
        sessions.insert(session_id.clone(), UploadSession { uploaded: 0 });
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
