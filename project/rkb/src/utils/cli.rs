use anyhow::Context;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::process::Command;
use uzers::os::unix::UserExt;

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

#[derive(Debug, Clone)]
pub enum User {
    Normal(String),
    Root,
}

/// Get the username of the user who invoked `sudo`.
pub fn original_user_name() -> User {
    if let Ok(user) = std::env::var("SUDO_USER") {
        return User::Normal(user);
    }

    if let Ok(user) = std::env::var("LOGNAME") {
        return User::Normal(user);
    }

    // `logname` command will return this directly if it is usable, which is in coreutils.
    // `logname` -> `me`
    // `sudo logname` -> `me`
    if let Ok(output) = Command::new("logname").output() {
        return User::Normal(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    User::Root
}

/// Get the original user config path.
pub fn original_user_config_path<'a>(
    app_name: impl AsRef<str>,
    config_name: impl Into<Option<&'a str>>,
) -> anyhow::Result<PathBuf> {
    let user = original_user_name();
    match user {
        User::Normal(name) => {
            let app_name = app_name.as_ref();
            let config_name = config_name.into().unwrap_or("default-config");

            let user = uzers::get_user_by_name(&name)
                .with_context(|| format!("Failed to find user with name: {name}"))?;
            let home_dir = user.home_dir();

            let config_path = if cfg!(target_os = "windows") {
                home_dir.join("AppData/Roaming")
            } else if cfg!(target_os = "macos") {
                home_dir.join("Library/Application Support")
            } else {
                home_dir.join(".config")
            }
            .join(app_name);

            Ok(config_path.join(format!("{config_name}.toml")))
        }
        User::Root => confy::get_configuration_file_path(app_name.as_ref(), config_name)
            .with_context(|| "Failed to get config path with confy"),
    }
}
