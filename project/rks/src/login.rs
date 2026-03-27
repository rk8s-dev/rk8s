use axum::Router;
use axum::extract::Query;
use axum::response::Html;
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use log::debug;
use serde::Deserialize;
use std::net::IpAddr;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot, watch};

#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

#[derive(Debug)]
struct CallbackResult {
    code: String,
    state: String,
}

#[derive(Debug, Deserialize)]
struct ExchangeResponse {
    token: String,
    username: String,
    #[allow(dead_code)]
    expires_at: String,
    #[serde(default)]
    registry_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    error: String,
    message: String,
}

async fn start_callback_server()
-> anyhow::Result<(u16, oneshot::Receiver<CallbackResult>, watch::Sender<()>)> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let (tx, rx) = oneshot::channel::<CallbackResult>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(());

    let app = Router::new().route(
        "/callback",
        get({
            let tx = tx.clone();
            move |Query(params): Query<CallbackParams>| {
                let tx = tx.clone();
                async move {
                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(CallbackResult {
                            code: params.code,
                            state: params.state,
                        });
                    }
                    Html(concat!(
                        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Login</title></head>",
                        "<body style=\"font-family:system-ui,sans-serif;display:flex;justify-content:center;",
                        "align-items:center;min-height:100vh;margin:0\">",
                        "<div style=\"text-align:center\">",
                        "<h2>Authentication Successful</h2>",
                        "<p>You can close this tab and return to the terminal.</p>",
                        "</div></body></html>",
                    ))
                }
            }
        }),
    );

    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.changed().await;
            })
            .await;
    });

    Ok((port, rx, shutdown_tx))
}

fn open_browser(url: &str) {
    let result = {
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(url)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        }

        #[cfg(target_os = "windows")]
        {
            let quoted = format!("\"{url}\"");
            std::process::Command::new("cmd")
                .args(["/C", "start", "", quoted.as_str()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported platform",
            ))
        }
    };

    if let Err(e) = result {
        debug!("failed to open browser: {e}");
    }
}

async fn exchange_auth_code(
    client: &reqwest::Client,
    server_url: &str,
    code: &str,
) -> anyhow::Result<ExchangeResponse> {
    let url = format!("{}/api/cli/exchange", server_url.trim_end_matches('/'));

    let res = client
        .post(&url)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await?;

    if res.status().is_success() {
        Ok(res.json::<ExchangeResponse>().await?)
    } else {
        match res.json::<ErrorBody>().await {
            Ok(err) => anyhow::bail!("{}: {}", err.error, err.message),
            Err(_) => anyhow::bail!("Exchange request failed"),
        }
    }
}

fn generate_user_code() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let bytes: [u8; 8] = rand::random();
    let raw: String = bytes
        .iter()
        .map(|&b| CHARS[(b as usize) % CHARS.len()] as char)
        .collect();
    format!("{}-{}", &raw[..4], &raw[4..])
}

fn generate_state() -> String {
    let bytes: [u8; 32] = rand::random();
    URL_SAFE_NO_PAD.encode(bytes)
}

fn is_private_ip(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let normalized = host.trim().trim_start_matches('[').trim_end_matches(']');
    match normalized.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
        Err(_) => false,
    }
}

fn parse_server_url(s: &str) -> anyhow::Result<String> {
    let s = s.trim();
    if s.is_empty() {
        anyhow::bail!("server URL must not be empty");
    }
    if s.contains("://") {
        return Ok(s.trim_end_matches('/').to_string());
    }
    let host_part = if let Some(rest) = s.strip_prefix('[') {
        rest.split(']').next().unwrap_or(s)
    } else {
        s.split(':').next().unwrap_or(s)
    };
    let scheme = if is_private_ip(host_part) {
        "http"
    } else {
        "https"
    };
    Ok(format!("{}://{}", scheme, s.trim_end_matches('/')))
}

/// Normalize a registry host string: lowercase, strip scheme/path/query.
/// Compatible with rkforge's `parse_registry_host` for the common cases.
pub fn normalize_registry_host(input: &str) -> anyhow::Result<String> {
    let input = input.trim();
    let host = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .unwrap_or(input);
    let host = host.split('/').next().unwrap_or(host);
    let host = host.split('?').next().unwrap_or(host);
    let host = host.split('#').next().unwrap_or(host);
    if host.is_empty() {
        anyhow::bail!("registry host must not be empty");
    }
    Ok(host.to_ascii_lowercase())
}

/// Perform browser-based login against an auth server.
/// Returns `(registry_host, pat_token, username)`.
pub async fn do_login(
    server: &str,
    skip_tls_verify: bool,
) -> anyhow::Result<(String, String, String)> {
    let server_url = parse_server_url(server)?;

    let state = generate_state();
    let user_code = generate_user_code();
    let (port, rx, _shutdown_tx) = start_callback_server().await?;

    let login_url = format!(
        "{}/api/cli/login?user_code={}&callback_port={}&state={}",
        server_url, user_code, port, state,
    );

    println!("Open this URL in your browser to authenticate:");
    println!("  {login_url}");
    println!();
    println!("Your one-time verification code: {user_code}");
    println!();
    println!("Opening browser...");
    open_browser(&login_url);

    println!("\nWaiting for authentication (timeout: 300s)...");
    let callback = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
        .await
        .map_err(|_| anyhow::anyhow!("Login timed out after 300 seconds"))?
        .map_err(|_| anyhow::anyhow!("Callback channel closed unexpectedly"))?;

    if callback.state != state {
        anyhow::bail!("State mismatch – possible CSRF attack");
    }

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(skip_tls_verify)
        .build()?;
    let res = exchange_auth_code(&client, &server_url, &callback.code).await?;

    let registry_url = res
        .registry_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("exchange response missing registry_url"))?;
    let registry = normalize_registry_host(registry_url)?;

    println!("Logged in as {} successfully!", res.username);
    Ok((registry, res.token, res.username))
}

#[cfg(test)]
mod tests {
    use super::parse_server_url;

    #[test]
    fn parse_server_url_uses_http_for_private_ipv6() {
        assert_eq!(parse_server_url("[::1]:7001").unwrap(), "http://[::1]:7001");
    }

    #[test]
    fn parse_server_url_preserves_explicit_scheme() {
        assert_eq!(
            parse_server_url("https://example.com:7001").unwrap(),
            "https://example.com:7001"
        );
    }
}
