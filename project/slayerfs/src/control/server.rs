use crate::control::protocol::{ControlRequest, ControlResponse};
use anyhow::Context;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixListener;
use tokio::task::JoinHandle;

#[async_trait]
pub trait ControlHandler: Send + Sync + 'static {
    async fn handle(&self, request: ControlRequest) -> ControlResponse;
}

pub struct ControlServer {
    socket_path: PathBuf,
    task: JoinHandle<()>,
}

impl ControlServer {
    pub async fn bind<H>(socket_path: PathBuf, handler: H) -> anyhow::Result<Self>
    where
        H: ControlHandler,
    {
        if let Some(parent) = socket_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("create {}", parent.display()))?;
        }

        if socket_path.exists() {
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)
            .with_context(|| format!("bind {}", socket_path.display()))?;
        let handler = Arc::new(handler);

        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };

                let handler = Arc::clone(&handler);
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let response = match stream.read_to_end(&mut buf).await {
                        Ok(_) => match serde_json::from_slice::<ControlRequest>(&buf) {
                            Ok(request) => handler.handle(request).await,
                            Err(err) => ControlResponse::Error {
                                code: "invalid_request".to_string(),
                                message: err.to_string(),
                            },
                        },
                        Err(err) => ControlResponse::Error {
                            code: "read_failed".to_string(),
                            message: err.to_string(),
                        },
                    };

                    if let Ok(payload) = serde_json::to_vec(&response) {
                        let _ = stream.write_all(&payload).await;
                    }
                    let _ = stream.shutdown().await;
                });
            }
        });

        Ok(Self { socket_path, task })
    }
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
