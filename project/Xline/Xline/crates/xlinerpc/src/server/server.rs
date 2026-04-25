//! HTTP/3 server infrastructure for xlinerpc
//!
//! This module provides utility functions for HTTP/3 server implementation.

use std::collections::HashSet;

use gm_quic::prelude::{BindUri, ParseBindUriError};

/// Extract the host portion from a URL string like "https://host:port/path".
/// Returns `None` if the URL cannot be parsed or has no host.
pub fn extract_host_from_url(url: &str) -> Option<&str> {
    // Strip the scheme prefix (e.g. "https://")
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("quic://"))?;
    // The host is everything before the first ':' (port) or '/' (path)
    let end = rest
        .find(':')
        .or_else(|| rest.find('/'))
        .unwrap_or(rest.len());
    let host = &rest[..end];
    if host.is_empty() { None } else { Some(host) }
}

/// Extract port numbers from a list of URL strings.
pub fn extract_ports_from_urls(urls: Vec<String>) -> anyhow::Result<HashSet<u16>> {
    let ports = urls
        .into_iter()
        .map(|url| parse_bind_uri(&url))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|uri| uri.as_inet_bind_uri().map(|x| x.port()))
        .collect();
    Ok(ports)
}

/// Parse a URL string into a BindUri.
///
/// Supports multiple URL schemes including `http://`, `https://`, and `quic://`.
/// If the URL doesn't parse directly as a BindUri, extracts the authority portion
/// and parses it as a socket address.
pub fn parse_bind_uri(s: &str) -> anyhow::Result<BindUri> {
    s.parse::<BindUri>().or_else(|_e: ParseBindUriError| {
        let endpoint = s
            .strip_prefix("http://")
            .or_else(|| s.strip_prefix("https://"))
            .or_else(|| s.strip_prefix("quic://"))
            .unwrap_or(s);
        let authority = endpoint.split('/').next().unwrap_or(endpoint);
        let addr = authority
            .parse::<std::net::SocketAddr>()
            .map_err(|e| anyhow::anyhow!("invalid listen url {s}: {e}"))?;
        Ok(BindUri::from(addr))
    })
}
