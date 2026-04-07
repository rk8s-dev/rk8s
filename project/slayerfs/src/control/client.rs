use crate::control::protocol::{ControlRequest, ControlResponse};
use anyhow::Context;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub async fn send_request(
    socket_path: impl AsRef<Path>,
    request: &ControlRequest,
) -> anyhow::Result<ControlResponse> {
    let socket_path = socket_path.as_ref();
    let mut stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("connect {}", socket_path.display()))?;

    let payload = serde_json::to_vec(request)?;
    stream.write_all(&payload).await?;
    stream.shutdown().await?;

    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    serde_json::from_slice(&buf).context("decode control response")
}
