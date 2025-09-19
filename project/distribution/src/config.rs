use crate::utils::cli::Args;
use std::fmt::Display;
use std::path::Path;
use std::str::FromStr;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct Config {
    pub host: String,
    pub port: u16,

    pub storge_type: String,
    pub root_dir: String,
    pub registry_url: String,
    pub db_url: String,

    pub jwt_secret: String,
    pub jwt_lifetime_secs: i64,

    pub github_client_id: String,
    pub github_client_secret: String,
}

fn must_set<T: FromStr + Display>(
    name: &str,
    errors: &mut Vec<String>,
    default: Option<T>,
) -> Option<T> {
    let Ok(value) = std::env::var(name) else {
        return match default {
            Some(value) => {
                tracing::warn!("{name} is not set. Use default value: {value}");
                Some(value)
            }
            None => {
                errors.push(format!("{name} must be set"));
                None
            }
        };
    };

    let Ok(value) = value.parse::<T>() else {
        errors.push(format!(
            "failed to parse <{name}:{value}> into {}",
            std::any::type_name::<T>(),
        ));
        return None;
    };

    Some(value)
}

pub async fn validate_config(args: &Args) -> Config {
    let mut validation_errors = Vec::new();

    let root_dir = Path::new(&args.root);
    match tokio::fs::metadata(root_dir).await {
        Ok(meta) => {
            if !meta.is_dir() {
                validation_errors.push(format!(
                    "OCI_REGISTRY_ROOTDIR `{}` exists but is not a directory",
                    args.root,
                ));
            }
        }
        Err(_) => validation_errors.push(format!(
            "OCI_REGISTRY_ROOTDIR `{}` does not exist.",
            args.root,
        )),
    }

    let jwt_secret = must_set(
        "JWT_SECRET",
        &mut validation_errors,
        Some("secret".to_string()),
    )
    .unwrap();
    let jwt_lifetime_secs =
        must_set("JWT_LIFETIME_SECONDS", &mut validation_errors, Some(3600)).unwrap();
    let github_client_id = must_set("GITHUB_CLIENT_ID", &mut validation_errors, None);
    let github_client_secret = must_set("GITHUB_CLIENT_SECRET", &mut validation_errors, None);

    let db_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            let db_password = match std::env::var("POSTGRES_PASSWORD") {
                Ok(password) => password,
                Err(_) => {
                    validation_errors.push("POSTGRES_PASSWORD is not set".into());
                    "".into()
                }
            };
            format!(
                "postgres://{}:{}@{}:{}/{}",
                args.db_user, db_password, args.db_host, args.db_port, args.db_name
            )
        }
    };

    if !validation_errors.is_empty() {
        tracing::error!("{}", validation_errors.join("\n"));
        std::process::exit(1);
    }

    Config {
        host: args.host.clone(),
        port: args.port,
        storge_type: args.storage.clone(),
        root_dir: args.root.clone(),
        registry_url: args.url.clone(),
        db_url,
        jwt_secret,
        jwt_lifetime_secs,
        github_client_id: github_client_id.unwrap(),
        github_client_secret: github_client_secret.unwrap(),
    }
}
