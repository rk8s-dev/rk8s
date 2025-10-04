use anyhow::Context;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use rustls::RootCertStore;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::pki_types::pem::PemObject;
use rustls::server::WebPkiClientVerifier;
use std::io::Cursor;
use std::sync::Arc;
use quinn::ServerConfig;

pub async fn build_quic_config(
    enable_tls: bool,
    vault_url: impl AsRef<str>,
    bootstrap_token: impl AsRef<str>,
) -> anyhow::Result<quinn::ServerConfig> {
    if !enable_tls {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
        let cert_der = CertificateDer::from(cert.serialize_der()?);
        let key = PrivatePkcs8KeyDer::from(cert.serialize_private_key_der());
        let certs = vec![cert_der];
        let server_config =
            ServerConfig::with_single_cert(certs, rustls::pki_types::PrivateKeyDer::Pkcs8(key))?;
        return Ok(server_config);
    }

    let vault_url = vault_url.as_ref();
    let bootstrap_token = bootstrap_token.as_ref();

    let req = IssueCertificateRequest {
        common_name: Some("rks-cluster".to_string()),
        alt_names: None,
        ip_sans: Some("192.168.73.128:50051,127.0.0.1:50051".to_string()),
        ttl: Some("12h".to_string()),
    };

    let client = libvault::api::async_client::AsyncClient::builder()
        .with_addr(&vault_url.to_string())
        .with_token(&bootstrap_token)
        .build()?;
    let resp = client
        .request_write::<_, _, IssueCertificateResponse>("/v1/pki/issue/control-plane", Some(req))
        .await?
        .into_data()
        .with_context(|| "Failed to fetch issue certificate response")?;

    let mut roots = RootCertStore::empty();
    let mut certs = Vec::new();

    let mut pem_reader = Cursor::new(resp.ca_chain.as_bytes());
    for cert in rustls_pemfile::certs(&mut pem_reader) {
        let cert = cert?;
        roots.add(cert.clone())?;
        certs.push(cert);
    }

    let roots = Arc::new(roots);
    let private_key = PrivateKeyDer::from_pem_slice(resp.private_key.as_bytes())?;

    let verifier = WebPkiClientVerifier::builder(roots).build()?;
    let rustls_config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, private_key)?;

    Ok(quinn::ServerConfig::with_crypto(Arc::new(rustls_config)))
}
