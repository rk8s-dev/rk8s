use crate::config::auth::AuthConfig;
use crate::registry::parse_registry_host;
use crate::rt::block_on;
use clap::Parser;
use std::net::IpAddr;

mod browser;
mod callback_server;
mod exchange;
mod oauth;
mod types;

fn is_private_ip(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    // Accept bracketed IPv6 host literals like "[::1]".
    let normalized = host.trim().trim_start_matches('[').trim_end_matches(']');
    match normalized.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
        Err(_) => false,
    }
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

        // 2. Build login URL and display the URL + one-time code
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

        // 6. Save the JWT for the target registry (must be provided by auth server)
        let registry_url = res
            .registry_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("exchange response missing registry_url"))?;
        let registry = parse_registry_host(registry_url)
            .map_err(|e| anyhow::anyhow!("invalid registry_url from server: {e}"))?;
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
