use crate::quic::TLSConnectionConfig;
use crate::quic::verifier::SkipServerVerification;
use anyhow::{Context, anyhow};
use common::RksMessage;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use quinn::crypto::rustls::QuicClientConfig;
use quinn::{Connection, Endpoint};
use rustls::RootCertStore;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tracing::debug;

const DEFAULT_TTL: &str = "12h";

pub struct UserQUICClient {
    conn: Connection,
}

impl UserQUICClient {
    pub async fn connect<S: AsRef<str>>(
        server_addr: S,
        config: impl Into<TLSConnectionConfig>,
    ) -> anyhow::Result<Self> {
        // Skip certificate verification
        let server_addr = server_addr.as_ref();
        let config = config.into();

        CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("failed to install default CryptoProvider");

        let client_cfg = build_quic_config(&config).await?;

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())?;
        endpoint.set_default_client_config(client_cfg);

        // establish connection with retry
        let conn = loop {
            match endpoint.connect(server_addr.parse()?, "localhost") {
                core::result::Result::Ok(connecting) => match connecting.await {
                    core::result::Result::Ok(conn) => break conn,
                    Err(e) => {
                        eprintln!("[user] connect failed: {e}, retrying 2s");
                        time::sleep(Duration::from_secs(2)).await;
                    }
                },
                Err(e) => {
                    eprintln!("[user] endpoint connect error: {e}, retrying 2s");
                    time::sleep(Duration::from_secs(2)).await;
                }
            }
        };
        let cli = UserQUICClient { conn };
        cli.send_uni(&RksMessage::UserRequest("Hello".to_string()))
            .await?;
        println!("RKL connected to RKS at {server_addr}");
        anyhow::Ok(cli)
    }

    pub async fn wait_response(&self) -> anyhow::Result<RksMessage> {
        match self.conn.accept_uni().await {
            core::result::Result::Ok(mut recv_stream) => {
                let mut buf = vec![0u8; 4096];
                match recv_stream.read(&mut buf).await {
                    core::result::Result::Ok(Some(n)) => {
                        if let core::result::Result::Ok(msg) =
                            bincode::deserialize::<RksMessage>(&buf[..n])
                        {
                            debug!("Get From Server: {:?}", msg);
                            return anyhow::Ok(msg);
                        }
                        Err(anyhow!("Empty response"))
                    }

                    core::result::Result::Ok(None) => Err(anyhow!("Empty response")),
                    Err(e) => Err(anyhow!("read response error: {}", e)),
                }
            }
            Err(e) => Err(anyhow!("connection error: {e}")),
        }
    }

    pub async fn send_uni(&self, msg: &RksMessage) -> anyhow::Result<()> {
        let mut uni = self.conn.open_uni().await?;
        let data = bincode::serialize(msg)?;
        uni.write_all(&data).await?;
        uni.finish()?;
        anyhow::Ok(())
    }
}

fn build_no_tls_config() -> anyhow::Result<quinn::ClientConfig> {
    let mut tls = rustls::ClientConfig::builder()
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    tls.dangerous()
        .set_certificate_verifier(Arc::new(SkipServerVerification));

    let quic_crypto = QuicClientConfig::try_from(tls)?;
    Ok(quinn::ClientConfig::new(Arc::new(quic_crypto)))
}

pub async fn build_quic_config(
    config: &TLSConnectionConfig,
) -> anyhow::Result<quinn::ClientConfig> {
    if !config.enable_tls {
        return build_no_tls_config();
    }

    let request = IssueCertificateRequest {
        common_name: Some("rkl-cluster".to_string()),
        alt_names: Some("rkl.svc.cluster.local".to_string()),
        ip_sans: None,
        ttl: Some(DEFAULT_TTL.to_string()),
    };
    let IssuedCertMaterial {
        certs,
        trust_roots,
        private_key,
    } = into_cert_material(issue_certificate(config, "/v1/pki/issue/rkl-node", request).await?)?;

    let rustls_config = rustls::ClientConfig::builder()
        .with_root_certificates(trust_roots.clone())
        .with_client_auth_cert(certs, private_key)?;

    let quic_crypto = QuicClientConfig::try_from(rustls_config)?;
    Ok(quinn::ClientConfig::new(Arc::new(quic_crypto)))
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

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_TTL, IssuedCertMaterial, TLSConnectionConfig, build_quic_config,
        into_cert_material, issue_certificate, trust_roots_from_certs,
    };
    use anyhow::anyhow;
    use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
    use quinn::Endpoint;
    use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
    use rustls::RootCertStore;
    use rustls::crypto::CryptoProvider;
    use rustls::server::WebPkiClientVerifier;
    use std::net::SocketAddr;
    use std::sync::Arc;

    pub async fn build_quic_config_server(
        config: &TLSConnectionConfig,
    ) -> anyhow::Result<quinn::ServerConfig> {
        let IssuedCertMaterial {
            certs,
            trust_roots,
            private_key,
        } = into_cert_material(
            issue_certificate(
                config,
                "/v1/pki/issue/rks-node",
                IssueCertificateRequest {
                    common_name: Some("rks-cluster".to_string()),
                    alt_names: Some("rks.svc.cluster.local".to_string()),
                    ip_sans: Some("192.168.73.128,127.0.0.1".to_string()),
                    ttl: Some(DEFAULT_TTL.to_string()),
                },
            )
            .await?,
        )?;

        let verifier = WebPkiClientVerifier::builder(trust_roots).build()?;
        let rustls_config = rustls::ServerConfig::builder()
            .with_client_cert_verifier(verifier)
            .with_single_cert(certs, private_key)?;

        let quic_crypto = QuicServerConfig::try_from(rustls_config)?;
        Ok(quinn::ServerConfig::with_crypto(Arc::new(quic_crypto)))
    }

    fn build_trust_roots(resp: &IssueCertificateResponse) -> anyhow::Result<Arc<RootCertStore>> {
        trust_roots_from_certs(&resp.to_certs()?)
    }

    fn is_missing_client_cert(err: &quinn::ConnectionError) -> bool {
        match err {
            quinn::ConnectionError::ApplicationClosed(info) => {
                let reason = &*info.reason;
                reason == b"missing client cert" || reason == b"peer sent no certificates"
            }
            quinn::ConnectionError::ConnectionClosed(close) => {
                let reason = &*close.reason;
                reason == b"missing client cert" || reason == b"peer sent no certificates"
            }
            quinn::ConnectionError::TransportError(transport) => {
                let reason: &[u8] = transport.reason.as_ref();
                reason == b"missing client cert" || reason == b"peer sent no certificates"
            }
            _ => false,
        }
    }

    fn ensure_missing_client_cert(err: &quinn::ConnectionError) -> anyhow::Result<()> {
        if is_missing_client_cert(err) {
            Ok(())
        } else {
            Err(anyhow!("unexpected TLS error: {:?}", err))
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_quic_communication() -> anyhow::Result<()> {
        CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("failed to install default CryptoProvider");

        let config = TLSConnectionConfig {
            enable_tls: true,
            vault_url: "127.0.0.1:8200".to_string(),
            bootstrap_token: "9af76dde-4000-a567-4df5-85ea649743fa".to_string(),
        };

        let client_config = build_quic_config(&config).await?;
        let server_config = build_quic_config_server(&config).await?;

        let server_addr: SocketAddr = "127.0.0.1:0".parse()?;
        let mut server_endpoint = Endpoint::server(server_config, server_addr)?;
        let listen_addr = server_endpoint.local_addr()?;

        let server_task = tokio::spawn(async move {
            if let Some(connecting) = server_endpoint.accept().await {
                let connection = connecting.await?;

                let mut recv = connection.accept_uni().await?;
                let mut buf = vec![0u8; 1024];
                let mut received = Vec::new();
                loop {
                    match recv.read(&mut buf).await? {
                        Some(n) => {
                            received.extend_from_slice(&buf[..n]);
                        }
                        None => break,
                    }
                }

                if received != b"ping" {
                    return Err(anyhow!(
                        "unexpected payload from client: {}",
                        String::from_utf8_lossy(&received)
                    ));
                }

                let mut send = connection.open_uni().await?;
                send.write_all(b"pong").await?;
                send.finish()?;
                let _ = connection.closed().await;
                Ok::<(), anyhow::Error>(())
            } else {
                Err(anyhow!("server endpoint did not accept any connection"))
            }
        });

        let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        client_endpoint.set_default_client_config(client_config);

        let connection = client_endpoint.connect(listen_addr, "127.0.0.1")?.await?;

        let mut send = connection.open_uni().await?;
        send.write_all(b"ping").await?;
        send.finish()?;

        let mut recv = connection.accept_uni().await?;
        let mut buf = vec![0u8; 1024];
        let mut response = Vec::new();
        loop {
            match recv.read(&mut buf).await? {
                Some(n) => {
                    response.extend_from_slice(&buf[..n]);
                }
                None => break,
            }
        }

        connection.close(0u32.into(), b"done");
        let _ = connection.closed().await;
        drop(connection);
        client_endpoint.wait_idle().await;

        assert_eq!(response, b"pong");

        server_task.await??;

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_quic_rejects_client_without_certificate() -> anyhow::Result<()> {
        CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("failed to install default CryptoProvider");

        let config = TLSConnectionConfig {
            enable_tls: true,
            vault_url: "127.0.0.1:8200".to_string(),
            bootstrap_token: "9af76dde-4000-a567-4df5-85ea649743fa".to_string(),
        };

        let server_config = build_quic_config_server(&config).await?;

        let server_addr: SocketAddr = "127.0.0.1:0".parse()?;
        let mut server_endpoint = Endpoint::server(server_config, server_addr)?;
        let listen_addr = server_endpoint.local_addr()?;

        let ca_response = issue_certificate(
            &config,
            "/v1/pki/issue/rks-node",
            IssueCertificateRequest {
                common_name: Some("rks-cluster".to_string()),
                alt_names: Some("rks.svc.cluster.local".to_string()),
                ip_sans: Some("192.168.73.128,127.0.0.1".to_string()),
                ttl: Some(DEFAULT_TTL.to_string()),
            },
        )
        .await?;
        let trust_roots = build_trust_roots(&ca_response)?;

        let client_tls = rustls::ClientConfig::builder()
            .with_root_certificates(trust_roots)
            .with_no_client_auth();
        let client_quic = QuicClientConfig::try_from(client_tls)?;
        let client_config = quinn::ClientConfig::new(Arc::new(client_quic));

        let server_handle = tokio::spawn(async move {
            let connecting = server_endpoint
                .accept()
                .await
                .ok_or_else(|| anyhow!("server did not receive a connection attempt"))?;
            match connecting.await {
                Ok(_) => Err(anyhow!(
                    "handshake unexpectedly succeeded without client cert"
                )),
                Err(err) => Ok::<quinn::ConnectionError, anyhow::Error>(err),
            }
        });

        let client_handle = tokio::spawn(async move {
            let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
            client_endpoint.set_default_client_config(client_config);

            let error = match client_endpoint.connect(listen_addr, "127.0.0.1")?.await {
                Ok(connection) => connection.closed().await,
                Err(err) => err,
            };

            client_endpoint.wait_idle().await;

            Ok::<quinn::ConnectionError, anyhow::Error>(error)
        });

        let server_error = server_handle.await??;
        let client_error = client_handle.await??;

        ensure_missing_client_cert(&server_error)?;
        ensure_missing_client_cert(&client_error)?;

        Ok(())
    }

    #[tokio::test]
    #[ignore]
    async fn test_quic_rejects_untrusted_server_certificate() -> anyhow::Result<()> {
        CryptoProvider::install_default(rustls::crypto::ring::default_provider())
            .expect("failed to install default CryptoProvider");

        let config = TLSConnectionConfig {
            enable_tls: true,
            vault_url: "127.0.0.1:8200".to_string(),
            bootstrap_token: "9af76dde-4000-a567-4df5-85ea649743fa".to_string(),
        };

        let server_config = build_quic_config_server(&config).await?;

        let server_addr: SocketAddr = "127.0.0.1:0".parse()?;
        let mut server_endpoint = Endpoint::server(server_config, server_addr)?;
        let listen_addr = server_endpoint.local_addr()?;

        let IssuedCertMaterial {
            certs, private_key, ..
        } = into_cert_material(
            issue_certificate(
                &config,
                "/v1/pki/issue/rkl-node",
                IssueCertificateRequest {
                    common_name: Some("rkl-cluster".to_string()),
                    alt_names: Some("rkl.svc.cluster.local".to_string()),
                    ip_sans: None,
                    ttl: Some(DEFAULT_TTL.to_string()),
                },
            )
            .await?,
        )?;

        let bad_tls = rustls::ClientConfig::builder()
            .with_root_certificates(Arc::new(RootCertStore::empty()))
            .with_client_auth_cert(certs, private_key)?;
        let bad_quic = QuicClientConfig::try_from(bad_tls)?;
        let bad_client_config = quinn::ClientConfig::new(Arc::new(bad_quic));

        let server_task = tokio::spawn(async move {
            if let Some(connecting) = server_endpoint.accept().await {
                match connecting.await {
                    Ok(_) => Err(anyhow!("handshake unexpectedly succeeded")),
                    Err(_) => Ok::<(), anyhow::Error>(()),
                }
            } else {
                Err(anyhow!(
                    "server endpoint did not observe a connection attempt"
                ))
            }
        });

        let mut client_endpoint = Endpoint::client("0.0.0.0:0".parse()?)?;
        client_endpoint.set_default_client_config(bad_client_config);

        let handshake = client_endpoint.connect(listen_addr, "127.0.0.1")?;
        let result = handshake.await;
        assert!(
            result.is_err(),
            "handshake succeeded even though the server certificate was untrusted"
        );

        client_endpoint.wait_idle().await;

        server_task.await??;

        Ok(())
    }
}
