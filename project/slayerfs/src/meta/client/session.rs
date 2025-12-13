use std::{sync::Arc, time::Duration};

use crate::meta::store::LockName;
use sea_orm::prelude::{DateTimeUtc, Uuid};
use serde::{Deserialize, Serialize};
use tokio::{select, sync::RwLock, time::sleep};
use tokio_util::sync::CancellationToken;
use tracing::error;

use crate::meta::{MetaStore, store::MetaError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub session_id: Uuid,
    pub expire: i64,
    pub session_info: SessionInfo,
}

impl Session {
    pub async fn new(session_id: Uuid, expire: i64, session_info: SessionInfo) -> Self {
        Session {
            session_id,
            expire,
            session_info,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Version string identifying the client or protocol version
    pub version: String,
    pub host_name: String,
    pub ip_addrs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mount_point: Option<String>,
    pub mount_time: DateTimeUtc,
    pub process_id: u32,
    /// Session creation timestamp
    pub created_at: DateTimeUtc,
}

pub struct SessionManager<M: MetaStore> {
    shutdown_token: CancellationToken,
    store: Arc<M>,
    pub session_id: RwLock<Option<Uuid>>,
    pub session: RwLock<Option<Session>>,
}

impl<M: MetaStore + 'static> SessionManager<M> {
    pub fn new(store: Arc<M>) -> Self {
        SessionManager {
            shutdown_token: CancellationToken::new(),
            store,
            session_id: RwLock::new(None),
            session: RwLock::new(None),
        }
    }

    pub async fn start(&self, session_info: SessionInfo) -> Result<(), MetaError> {
        let session = self.store.new_session(session_info).await?;
        let session_id = session.session_id;
        let mut session_guard = self.session.write().await;
        *session_guard = Some(session);

        let mut session_id_guard = self.session_id.write().await;
        *session_id_guard = Some(session_id);

        tokio::spawn(refresh_cycle(
            self.shutdown_token.clone(),
            self.store.clone(),
            session_id,
        ));
        tokio::spawn(cleanup_cycle(
            self.shutdown_token.clone(),
            self.store.clone(),
        ));
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.shutdown_token.cancel();
        // Attempt to read the session_id, handling poisoned lock and missing session
        match *self.session_id.read().await {
            Some(session_id) => match self.store.shutdown_session(session_id).await {
                Ok(_) => (),
                Err(err) => error!("Failed to clean session: {}", err),
            },
            None => {
                error!("No session_id found during shutdown; session may not have been started.");
            }
        }
    }
}

async fn refresh_cycle<M: MetaStore>(token: CancellationToken, store: Arc<M>, session_id: Uuid) {
    loop {
        select! {
            _ = token.cancelled() => {
                break;
            }
            _ = sleep(Duration::from_secs(10)) => {
                match store.refresh_session(session_id).await {
                    Ok(_) => (),
                    Err(err) => error!("Failed to refresh session: {}", err),
                }
            }
        }
    }
}

async fn cleanup_cycle<M: MetaStore>(token: CancellationToken, store: Arc<M>) {
    loop {
        select! {
            _ = token.cancelled() => {
                break;
            }
            _ = sleep(Duration::from_secs(10)) => {
                let ok = store.get_lock(LockName::CleanupSessionsLock).await;
                if ok {
                    match store.cleanup_sessions().await {
                        Ok(_) => (),
                        Err(err) => error!("Failed to cleanup session: {}", err),
                    }
                }
            }
        }
    }
}
