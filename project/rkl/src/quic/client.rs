use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;
use anyhow::{anyhow, Context};
use quinn::{Connection, Endpoint};
use quinn::crypto::rustls::QuicClientConfig;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::PrivateKeyDer;
use rustls::RootCertStore;
use tokio::time;
use tracing::debug;
use common::RksMessage;
use libvault::modules::pki::types::{IssueCertificateRequest, IssueCertificateResponse};
use crate::quic::TLSConnectionConfig;
use crate::quic::verifier::SkipServerVerification;

pub struct UserQUICClient {
    conn: Connection,
}

impl UserQUICClient {
    pub async fn connect<S: AsRef<str>>(server_addr: S, config: TLSConnectionConfig) -> anyhow::Result<Self> {
        // Skip certificate verification
        let server_addr = server_addr.as_ref();

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

pub async fn build_quic_config(
    config: &TLSConnectionConfig,
) -> anyhow::Result<quinn::ClientConfig> {
    if !config.enable_tls {
        let mut tls = rustls::ClientConfig::builder()
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth();
        tls.dangerous()
            .set_certificate_verifier(Arc::new(SkipServerVerification));

        let quic_crypto = QuicClientConfig::try_from(tls)?;
        return Ok(quinn::ClientConfig::new(Arc::new(quic_crypto)));
    }

    let vault_url = &config.vault_url;
    let bootstrap_token = &config.bootstrap_token;

    let req = IssueCertificateRequest {
        common_name: Some("rkl-cluster".to_string()),
        alt_names: None,
        ip_sans: None,
        ttl: Some("12h".to_string()),
    };

    let client = libvault::api::async_client::AsyncClient::builder()
        .with_addr(&vault_url.to_string())
        .with_token(&bootstrap_token)
        .build()?;
    let resp = client
        .request_write::<_, _, IssueCertificateResponse>("/v1/pki/issue/data-plane", Some(req))
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

    let mut roots = Arc::new(roots);

    let private_key = PrivateKeyDer::from_pem_slice(resp.private_key.as_bytes())?;
    let rustls_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots.clone())
        .with_client_auth_cert(certs, private_key)?;

    let quic_crypto = QuicClientConfig::try_from(rustls_config)?;
    Ok(quinn::ClientConfig::new(Arc::new(quic_crypto)))
}