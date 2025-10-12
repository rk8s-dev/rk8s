use crate::protocol::config::config;
use crate::vault::Vault;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use quinn::crypto::rustls::QuicServerConfig;
use rustls::RootCertStore;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use std::sync::Arc;

const DEFAULT_TTL: &str = "12h";

fn build_no_tls_config() -> anyhow::Result<quinn::ServerConfig> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;
    let cert_der = CertificateDer::from(cert.serialize_der()?);
    let key = PrivatePkcs8KeyDer::from(cert.serialize_private_key_der());
    let certs = vec![cert_der];
    let server_config = quinn::ServerConfig::with_single_cert(certs, PrivateKeyDer::Pkcs8(key))?;
    Ok(server_config)
}

pub async fn build_quic_config(
    vault: &Vault,
) -> anyhow::Result<(quinn::ServerConfig, Option<Vec<CertificateDer<'static>>>)> {
    if !config().tls_config.enable {
        return Ok((build_no_tls_config()?, None));
    }

    let req = IssueCertificateRequest {
        common_name: Some("rks-cluster".to_string()),
        alt_names: Some("rks.svc.cluster.local,localhost".to_string()),
        ip_sans: Some("192.168.73.128,127.0.0.1".to_string()),
        ttl: Some("15s".to_string()),
    };

    let IssuedCertMaterial {
        certs,
        trust_roots,
        private_key,
    } = into_cert_material(vault.issue_cert("rks", &req).await?)?;

    let verifier = WebPkiClientVerifier::builder(trust_roots)
        .allow_unauthenticated()
        .build()?;
    let rustls_config = rustls::ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs.clone(), private_key)?;

    let quic_crypto = QuicServerConfig::try_from(rustls_config)?;
    Ok((
        quinn::ServerConfig::with_crypto(Arc::new(quic_crypto)),
        Some(certs),
    ))
}

fn trust_roots_from_certs(certs: &[CertificateDer<'static>]) -> anyhow::Result<Arc<RootCertStore>> {
    let mut roots = RootCertStore::empty();
    for cert in certs.iter().skip(1) {
        roots.add(cert.clone())?;
    }
    Ok(Arc::new(roots))
}

struct IssuedCertMaterial {
    pub certs: Vec<CertificateDer<'static>>,
    pub trust_roots: Arc<RootCertStore>,
    pub private_key: PrivateKeyDer<'static>,
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
