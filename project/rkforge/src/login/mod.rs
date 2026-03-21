use crate::config::auth::AuthConfig;
use crate::registry::parse_registry_host;
use crate::rt::block_on;
use clap::Parser;

mod browser;
mod callback_server;
mod exchange;
mod oauth;
mod types;

fn is_private_ip(host: &str) -> bool {
    let host_lower = host.to_lowercase();
    if host_lower.starts_with("localhost") || host_lower.starts_with("127.") {
        return true;
    }
    // Check common private network ranges
    if host_lower.starts_with("10.") || host_lower.starts_with("192.168.") {
        return true;
    }
    // 172.16.0.0 - 172.31.255.255
    if host_lower.starts_with("172.")
        && let Some(second_octet) = host_lower.split('.').nth(1)
        && let Ok(n) = second_octet.parse::<u8>()
        && (16..=31).contains(&n)
    {
        return true;
    }
    false
}

fn parse_server_url(s: &str) -> Result<String, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("server URL must not be empty".into());
    }
    if s.contains("://") {
        return Ok(s.trim_end_matches('/').to_string());
    }
    // Extract host part (before port) for IP range check
    let host_part = s.split(':').next().unwrap_or(s);
    let scheme = if is_private_ip(host_part) {
        "http"
    } else {
        "https"
    };
    Ok(format!("{}://{}", scheme, s.trim_end_matches('/')))
}

#[derive(Debug, Parser)]
pub struct LoginArgs {
    /// Auth server URL (e.g. https://libra.tools or http://localhost:7001).
    /// When omitted, defaults to https://libra.tools.
    #[arg(value_parser = parse_server_url, default_value = "https://libra.tools")]
    server: String,

    /// Skip TLS certificate verification for HTTPS connections.
    #[arg(long)]
    skip_tls_verify: bool,
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

fn display_login_info(uri: &str, user_code: &str) {
    println!("Open this URL in your browser to authenticate:");
    println!("  {uri}");
    println!();
    println!("Your one-time verification code: {user_code}");
}

pub fn login(args: LoginArgs) -> anyhow::Result<()> {
    block_on(async move {
        // 1. Start local callback server on 127.0.0.1:<random port>
        let state = generate_state();
        let user_code = generate_user_code();
        let (port, rx, _shutdown_tx) = callback_server::start().await?;

        // 2. Build login URL and display QR + code
        let login_url = format!(
            "{}/api/cli/login?user_code={}&callback_port={}&state={}",
            args.server.trim_end_matches('/'),
            user_code,
            port,
            state,
        );

        display_login_info(&login_url, &user_code);
        println!();
        println!("Opening browser...");
        browser::open(&login_url);

        // 3. Wait for the browser redirect back to the local server
        println!("\nWaiting for authentication (timeout: 300s)...");
        let callback = tokio::time::timeout(std::time::Duration::from_secs(300), rx)
            .await
            .map_err(|_| anyhow::anyhow!("Login timed out after 300 seconds"))?
            .map_err(|_| anyhow::anyhow!("Callback channel closed unexpectedly"))?;

        // 4. Verify the state matches (CSRF protection)
        if callback.state != state {
            anyhow::bail!("State mismatch – possible CSRF attack");
        }

        // 5. Exchange the one-time auth_code for a JWT
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(args.skip_tls_verify)
            .build()?;
        let res = exchange::exchange_auth_code(&client, &args.server, &callback.code).await?;

        // 6. Save the JWT for the target registry (from server config)
        let registry = match res.registry_url.as_deref().filter(|s| !s.is_empty()) {
            Some(url) => parse_registry_host(url)
                .map_err(|e| anyhow::anyhow!("invalid registry_url from server: {e}"))?,
            None => AuthConfig::load()
                .and_then(|c| c.resolve_url(None::<&str>))
                .unwrap_or_else(|_| "127.0.0.1:8968".to_string()),
        };
        AuthConfig::login(res.token, &registry)?;
        println!("Logged in as {} successfully!", res.username);
        Ok(())
    })?
}

fn generate_state() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::rng().random();
    hex::encode(bytes)
}
