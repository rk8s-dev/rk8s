use crate::config::auth::AuthConfig;
use crate::storage::parse_image_ref;
use anyhow::{Context, bail};
use oci_client::Client;
use oci_client::client::{ClientConfig, ClientProtocol};
use oci_client::secrets::RegistryAuth;
use oci_spec::distribution::Reference;
use reqwest::Url;
use std::net::Ipv6Addr;
use tracing::warn;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RegistryScheme {
    Http,
    Https,
}

impl RegistryScheme {
    pub fn as_str(self) -> &'static str {
        match self {
            RegistryScheme::Http => "http",
            RegistryScheme::Https => "https",
        }
    }

    pub fn to_client_protocol(self) -> ClientProtocol {
        match self {
            RegistryScheme::Http => ClientProtocol::Http,
            RegistryScheme::Https => ClientProtocol::Https,
        }
    }
}

pub fn parse_registry_host(value: impl AsRef<str>) -> anyhow::Result<String> {
    let value = value.as_ref().trim();
    if value.is_empty() {
        bail!("registry must not be empty, expected host[:port]");
    }
    if value.contains("://") {
        bail!(
            "registry must be host[:port] format (e.g., 'registry.example.com:5000'). \
Remove 'http://' or 'https://' prefix: {value}"
        );
    }
    if value.contains('/') || value.contains('?') || value.contains('#') {
        bail!("registry must be host[:port] without path/query/fragment: {value}");
    }

    let probe = if value.starts_with('[') {
        format!("https://{value}")
    } else if value.parse::<Ipv6Addr>().is_ok() {
        format!("https://[{value}]")
    } else {
        format!("https://{value}")
    };

    let parsed = Url::parse(&probe).with_context(|| {
        format!(
            "invalid registry `{value}`, expected host[:port]. \
For IPv6 with port, use `[addr]:port` (e.g., `[::1]:5000`)"
        )
    })?;
    let host = parsed
        .host_str()
        .with_context(|| format!("registry `{value}` is missing host"))?
        .to_ascii_lowercase();

    if !parsed.username().is_empty() || parsed.password().is_some() {
        bail!("registry must not contain username or password: {value}");
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        bail!("registry must be host[:port] without path/query/fragment: {value}");
    }

    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host
    };
    match parsed.port() {
        Some(port) => Ok(format!("{host}:{port}")),
        None => Ok(host),
    }
}

pub fn parse_registry_host_arg(value: &str) -> Result<String, String> {
    parse_registry_host(value).map_err(|e| e.to_string())
}

pub fn is_insecure_registry(registry: impl AsRef<str>, insecure_registries: &[String]) -> bool {
    insecure_registries
        .iter()
        .any(|entry| entry == registry.as_ref())
}

pub fn scheme_for_registry(
    registry: impl AsRef<str>,
    insecure_registries: &[String],
) -> RegistryScheme {
    if is_insecure_registry(registry, insecure_registries) {
        RegistryScheme::Http
    } else {
        RegistryScheme::Https
    }
}

pub fn api_url(scheme: RegistryScheme, registry: impl AsRef<str>, path: impl AsRef<str>) -> String {
    format!(
        "{}://{}/{}",
        scheme.as_str(),
        registry.as_ref(),
        path.as_ref().trim_start_matches('/')
    )
}

pub fn effective_skip_tls_verify(
    skip_tls_verify: bool,
    scheme: RegistryScheme,
    registry: impl AsRef<str>,
) -> bool {
    if skip_tls_verify && matches!(scheme, RegistryScheme::Http) {
        warn!(
            "Ignoring --skip-tls-verify for insecure registry {} (HTTP transport is in use)",
            registry.as_ref()
        );
    }
    skip_tls_verify && matches!(scheme, RegistryScheme::Https)
}

pub fn oci_client_for_registry(
    scheme: RegistryScheme,
    skip_tls_verify: bool,
    registry: impl AsRef<str>,
) -> Client {
    let accept_invalid_certificates = effective_skip_tls_verify(skip_tls_verify, scheme, registry);
    let client_config = ClientConfig {
        protocol: scheme.to_client_protocol(),
        accept_invalid_certificates,
        ..Default::default()
    };
    Client::new(client_config)
}

pub fn resolve_client_ref_auth(
    auth_config: &AuthConfig,
    registry: impl AsRef<str>,
    image_ref: impl AsRef<str>,
    skip_tls_verify: bool,
) -> anyhow::Result<(Client, Reference, RegistryAuth)> {
    let registry = parse_registry_host(registry.as_ref())?;
    let auth_method = match auth_config.find_entry_by_url(&registry) {
        Ok(entry) => RegistryAuth::Bearer(entry.pat.clone()),
        Err(_) => RegistryAuth::Anonymous,
    };
    let scheme = auth_config.registry_scheme(&registry);
    let client = oci_client_for_registry(scheme, skip_tls_verify, &registry);
    let image_ref = parse_image_ref(&registry, image_ref, None::<String>)?;
    Ok((client, image_ref, auth_method))
}

#[cfg(test)]
mod tests {
    use super::parse_registry_host;

    #[test]
    fn parse_registry_host_rejects_scheme() {
        assert!(parse_registry_host("https://example.com").is_err());
        assert!(parse_registry_host("http://example.com").is_err());
    }

    #[test]
    fn parse_registry_host_rejects_path() {
        assert!(parse_registry_host("example.com/api").is_err());
    }

    #[test]
    fn parse_registry_host_accepts_host_and_port() {
        assert_eq!(parse_registry_host("Example.Com").unwrap(), "example.com");
        assert_eq!(
            parse_registry_host("registry.example.com:5000").unwrap(),
            "registry.example.com:5000"
        );
    }

    #[test]
    fn parse_registry_host_accepts_ipv6() {
        assert_eq!(parse_registry_host("::1").unwrap(), "[::1]");
        assert_eq!(parse_registry_host("[::1]").unwrap(), "[::1]");
        assert_eq!(parse_registry_host("[::1]:5000").unwrap(), "[::1]:5000");
    }

    #[test]
    fn parse_registry_host_rejects_userinfo() {
        assert!(parse_registry_host("user:pass@example.com").is_err());
    }

    #[test]
    fn parse_registry_host_rejects_empty_or_port_only() {
        assert!(parse_registry_host("").is_err());
        assert!(parse_registry_host(":5000").is_err());
    }
}
