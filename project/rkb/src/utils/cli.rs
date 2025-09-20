use anyhow::Context;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;

#[async_trait::async_trait]
pub trait RequestBuilderExt {
    async fn send_and_json<U>(self) -> anyhow::Result<U>
    where
        U: DeserializeOwned;
}

#[async_trait::async_trait]
impl RequestBuilderExt for RequestBuilder {
    async fn send_and_json<U>(self) -> anyhow::Result<U>
    where
        U: DeserializeOwned,
    {
        self.send()
            .await?
            .json::<U>()
            .await
            .with_context(|| "Failed to deserialize response")
    }
}

pub fn assert_not_sudo(name: impl AsRef<str>) -> anyhow::Result<()> {
    if nix::unistd::getuid().is_root() {
        anyhow::bail!("`rkb {}` should not be run with sudo", name.as_ref())
    }
    Ok(())
}
