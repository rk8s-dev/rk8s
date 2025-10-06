use crate::cli::TLSConnectionArgs;
use anyhow::Context;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use quinn::crypto::rustls::QuicServerConfig;
use rustls::RootCertStore;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use std::sync::Arc;

const DEFAULT_TTL: &str = "12h";

#[derive(Debug, Clone)]
pub struct TLSConnectionConfig {
    pub enable_tls: bool,
    pub vault_url: String,
    pub bootstrap_token: String,
}

impl From<TLSConnectionArgs> for TLSConnectionConfig {
    fn from(value: TLSConnectionArgs) -> Self {
        Self {
            enable_tls: value.enable_tls,
            vault_url: value.vault_url,
            bootstrap_token: value.bootstrap_token,
        }
    }
}

fn build_no_tls_config() -> anyhow::Result<quinn::ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let cert_der = CertificateDer::from(cert.serialize_der()?);
    let key = PrivatePkcs8KeyDer::from(cert.serialize_private_key_der());
    let certs = vec![cert_der];
    let server_config =
        quinn::ServerConfig::with_single_cert(certs, rustls::pki_types::PrivateKeyDer::Pkcs8(key))?;
    Ok(server_config)
}

pub async fn build_quic_config(
    config: &TLSConnectionConfig,
) -> anyhow::Result<quinn::ServerConfig> {
    if !config.enable_tls {
        return build_no_tls_config();
    }

    let req = IssueCertificateRequest {
        common_name: Some("rks-cluster".to_string()),
        alt_names: Some("rks.svc.cluster.local".to_string()),
        ip_sans: Some("192.168.73.128,127.0.0.1".to_string()),
        ttl: Some(DEFAULT_TTL.to_string()),
    };

    let IssuedCertMaterial {
        certs,
        trust_roots,
        private_key,
    } = into_cert_material(issue_certificate(config, "/v1/pki/issue/rks-node", req).await?)?;

    let verifier = WebPkiClientVerifier::builder(trust_roots).build()?;
    let rustls_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, private_key)?;

    let quic_crypto = QuicServerConfig::try_from(rustls_config)?;
    Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_crypto)))
}

async fn issue_certificate(
    config: &TLSConnectionConfig,
    path: &str,
    req: IssueCertificateRequest,
) -> anyhow::Result<IssueCertificateResponse> {
    let mut vault_url = config.vault_url.clone();
    if !vault_url.starts_with("http") {
        vault_url = format!("http://{}", vault_url);
    }

    let client = libvault::api::Client::builder()
        .with_addr(&vault_url)
        .with_token(&config.bootstrap_token)
        .build()?;

    client
        .request_write::<_, _, IssueCertificateResponse>(path, Some(req))
        .await?
        .into_data()
        .with_context(|| format!("Failed to fetch issue certificate response from {path}"))
}

fn trust_roots_from_certs(certs: &[CertificateDer<'static>]) -> anyhow::Result<Arc<RootCertStore>> {
    let mut roots = RootCertStore::empty();
    for cert in certs.iter().skip(1) {
        roots.add(cert.clone())?;
    }
    Ok(Arc::new(roots))
}

struct IssuedCertMaterial {
    certs: Vec<CertificateDer<'static>>,
    trust_roots: Arc<RootCertStore>,
    private_key: PrivateKeyDer<'static>,
}

fn into_cert_material(resp: IssueCertificateResponse) -> anyhow::Result<IssuedCertMaterial> {
    let certs = resp.to_certs()?;
    let trust_roots = trust_roots_from_certs(&certs)?;
    let private_key = PrivateKeyDer::from_pem_slice(resp.private_key.as_bytes())?;

    Ok(IssuedCertMaterial {
        certs,
        trust_roots,
        private_key,
    })
}
