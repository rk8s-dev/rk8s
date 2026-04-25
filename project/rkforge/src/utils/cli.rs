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

pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
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
        let user = user.trim();
        if !user.is_empty() {
            return User::Normal(user.to_string());
        }
    }

    if let Ok(user) = std::env::var("LOGNAME") {
        let user = user.trim();
        if !user.is_empty() {
            return User::Normal(user.to_string());
        }
    }

    // `logname` command will return this directly if it is usable, which is in coreutils.
    // `logname` -> `me`
    // `sudo logname` -> `me`
    if let Ok(output) = Command::new("logname").output()
        && output.status.success()
    {
        let user = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !user.is_empty() {
            return User::Normal(user);
        }
    }

    User::Root
}

fn home_dir_for_user(user: &User) -> anyhow::Result<PathBuf> {
    match user {
        User::Normal(name) => {
            let user = uzers::get_user_by_name(name)
                .with_context(|| format!("Failed to find user with name: {name}"))?;
            Ok(user.home_dir().to_path_buf())
        }
        User::Root => dirs::home_dir().with_context(|| "Failed to get home directory"),
    }
}

/// Get the original user's home directory.
pub fn original_user_home_dir() -> anyhow::Result<PathBuf> {
    home_dir_for_user(&original_user_name())
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

            let home_dir = home_dir_for_user(&User::Normal(name))?;

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

#[cfg(test)]
mod tests {
    use super::format_size;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(999), "999 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }
}
