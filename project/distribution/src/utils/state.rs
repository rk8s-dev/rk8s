use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

use crate::storage::{Storage, driver::filesystem::FilesystemStorage};
use crate::storage::user_storage::UserStorage;
use crate::utils::cli::Args;

#[derive(Clone, Debug)]
pub struct UploadSession {
    pub length: u64,
    pub uploaded: u64, // the last uploaded byte index
}

#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub storge_typ: String,
    pub root_dir: String,
    pub registry_url: String,
    pub db_url: String,
    pub password_salt: String,
    pub jwt_secret: String,
    pub jwt_lifetime_secs: i64,
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<RwLock<HashMap<String, UploadSession>>>,
    pub storage: Arc<dyn Storage>,
    pub user_storage: Arc<UserStorage>,
    pub config: Arc<Config>,
}

impl AppState {
    pub async fn new(config: Config) -> anyhow::Result<Self> {
        let storage_backend: Arc<dyn Storage + Send + Sync> = match config.storge_typ.as_str() {
            "FILESYSTEM" => Arc::new(FilesystemStorage::new(&config.root_dir)),
            _ => Arc::new(FilesystemStorage::new(&config.root_dir)),
        };
        
        let db_url = config.db_url.to_string();
        Ok(AppState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            storage: storage_backend,
            config: Arc::new(config),
            user_storage: Arc::new(UserStorage::new(&db_url).await?),
        })
    }

    pub async fn get_session(&self, id: &str) -> Option<UploadSession> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn create_session(&self) -> Result<String, String> {
        let mut sessions = self.sessions.write().await;
        let session_id = uuid::Uuid::new_v4().to_string();
        sessions.insert(
            session_id.clone(),
            UploadSession {
                length: 0,
                uploaded: 0,
            },
        );
        Ok(session_id)
    }

    pub async fn close_session(&self, id: &str) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(id);
    }

    pub async fn update_session(&self, id: &str, length: u64) {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(id) {
            session.length += length;
            if session.uploaded == 0 {
                session.uploaded += length - 1;
            } else {
                session.uploaded += length;
            }
        }
    }
}
