use crate::RksMessage;
use anyhow::Context;
use quinn::{Connection, RecvStream, SendStream};
use std::ops::{Deref, DerefMut};

#[macro_export]
macro_rules! reply_and_bail {
    ($this:expr, $message:expr, $expected:pat) => {{
        let error_msg = $crate::invalid_rks_variant_error!($message, $expected);
        $this.send_msg(&error_msg).await?;
        anyhow::bail!(std::format!("bailed out after replied: {}", &error_msg));
    }};
}

#[macro_export]
macro_rules! reply_error_msg_and_bail {
    ($this:expr, $message:expr) => {{
        $this.send_msg($message).await?;
        anyhow::bail!("bailed out after replied");
    }};
    ($this:expr, $reply_msg:expr, $error_msg:expr) => {{
        $this.send_msg($reply_msg).await?;
        anyhow::bail!($error_msg);
    }};
}

#[macro_export]
macro_rules! log_error {
    ($maybe_error:expr) => {
        if let Err(e) = $maybe_error {
            $crate::_private::error!("{e}");
        }
    };
    ($maybe_error:expr, $error_msg:expr) => {
        if let Err(e) = $maybe_error {
            $crate::_private::error!("{e}");
            anyhow::bail!($error_msg);
        }
    };
}

#[macro_export]
macro_rules! log_error_and_bail {
    ($maybe_error:expr) => {
        if let Err(e) = $maybe_error {
            $crate::_private::error!("{e}");
            anyhow::bail!("{e}");
        }
    };
    ($maybe_error:expr, $error_msg:expr) => {
        if let Err(e) = $maybe_error {
            $crate::_private::error!("{e}");
            anyhow::bail!($error_msg);
        }
    };
}

#[derive(Debug, Clone)]
pub struct RksConnection(Connection);

impl Deref for RksConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RksConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl RksConnection {
    pub fn take_inner_cloned(&self) -> Connection {
        self.0.clone()
    }

    pub fn into_inner(self) -> Connection {
        self.0
    }
}

pub struct RksStream((SendStream, RecvStream));

impl RksConnection {
    pub fn new(conn: Connection) -> Self {
        Self(conn)
    }

    pub async fn open_bi(&self) -> anyhow::Result<RksStream> {
        let bi = self.0.open_bi().await?;
        Ok(RksStream(bi))
    }

    pub async fn accept_bi(&self) -> anyhow::Result<RksStream> {
        let bi = self.0.accept_bi().await?;
        Ok(RksStream(bi))
    }

    pub async fn send_msg(&self, msg: &RksMessage) -> anyhow::Result<()> {
        let mut stream = self.0.open_uni().await?;
        stream.send_msg(msg).await?;
        stream
            .finish()
            .with_context(|| "Failed to close send stream")
    }

    pub async fn fetch_msg(&self) -> anyhow::Result<RksMessage> {
        self.0.accept_uni().await?.fetch_msg().await
    }
}

impl RksStream {
    pub fn sender(&mut self) -> &mut SendStream {
        &mut self.0.0
    }

    pub fn receiver(&mut self) -> &mut RecvStream {
        &mut self.0.1
    }

    pub fn into_inner(self) -> (SendStream, RecvStream) {
        self.0
    }

    pub async fn fetch_msg(&mut self) -> anyhow::Result<RksMessage> {
        self.receiver().fetch_msg().await
    }

    pub async fn send_msg(&mut self, msg: &RksMessage) -> anyhow::Result<usize> {
        self.sender().send_msg(msg).await
    }

    pub async fn send_frame<T: serde::Serialize>(&mut self, value: &T) -> anyhow::Result<()> {
        send_frame(self.sender(), value).await
    }

    pub async fn recv_frame<T: serde::de::DeserializeOwned>(&mut self) -> anyhow::Result<T> {
        recv_frame(self.receiver()).await
    }
}

#[async_trait::async_trait]
pub trait SendStreamExt {
    async fn send_msg(&mut self, msg: &RksMessage) -> anyhow::Result<usize>;
}

#[async_trait::async_trait]
pub trait RecvStreamExt {
    async fn fetch_msg(&mut self) -> anyhow::Result<RksMessage>;
}

#[async_trait::async_trait]
impl SendStreamExt for SendStream {
    async fn send_msg(&mut self, msg: &RksMessage) -> anyhow::Result<usize> {
        let msg = serde_json::to_vec(msg)?;
        self.write_all(&msg)
            .await
            .with_context(|| "Failed to send a rks message")?;
        Ok(msg.len())
    }
}

#[async_trait::async_trait]
impl RecvStreamExt for RecvStream {
    async fn fetch_msg(&mut self) -> anyhow::Result<RksMessage> {
        let mut buf = Vec::new();
        let mut chunk = vec![0u8; 4096];

        while let Some(n) = self.read(&mut chunk).await? {
            buf.extend_from_slice(&chunk[..n]);
        }

        serde_json::from_slice::<RksMessage>(&buf)
            .with_context(|| "Failed to deserialize rks message")
    }
}

pub async fn send_frame<T: serde::Serialize>(
    stream: &mut SendStream,
    value: &T,
) -> anyhow::Result<()> {
    let payload = serde_json::to_vec(value)?;
    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;
    stream.write_all(&payload).await?;
    Ok(())
}

pub async fn recv_frame<T: serde::de::DeserializeOwned>(
    stream: &mut RecvStream,
) -> anyhow::Result<T> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).await?;
    Ok(serde_json::from_slice(&payload)?)
}
